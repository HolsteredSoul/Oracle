//! Metaculus read-only integration.
//!
//! Fetches community forecasts for cross-referencing with ForecastEx
//! and Manifold market prices. No monetary bets — used purely as a
//! Bayesian anchor and calibration signal.
//!
//! API: `https://www.metaculus.com/api2/questions/`
//! Auth: Not required for reading.
//! Pagination: Offset-based (`?limit=N&offset=M`), max 100 per page.
//! Timestamps: ISO 8601 strings (e.g. "2026-02-09T18:45:09.861028Z").

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::Deserialize;
use tracing::{debug, info, warn};

use super::PredictionPlatform;
use crate::types::{
    CrossReferences, LiquidityInfo, Market, MarketCategory, Position, Side, TradeReceipt,
};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

const BASE_URL: &str = "https://www.metaculus.com/api2/questions";
const PLATFORM_NAME: &str = "metaculus";

/// Maximum questions per API page (Metaculus caps at 100).
const PAGE_LIMIT: u32 = 100;

/// Maximum pages to fetch per scan (100 × 5 = 500 questions max).
const MAX_PAGES: u32 = 5;

/// Minimum forecasters for a question to be useful as a signal.
const MIN_FORECASTERS: u32 = 5;

// ---------------------------------------------------------------------------
// API response types (Metaculus JSON → Rust)
// ---------------------------------------------------------------------------

/// Top-level paginated response from `/api2/questions/`.
#[derive(Debug, Deserialize)]
struct MetaculusPage {
    count: u32,
    next: Option<String>,
    results: Vec<MetaculusPost>,
}

/// A Metaculus "post" — the top-level object wrapping a question.
/// The API nests the actual question data inside a `question` field.
#[derive(Debug, Deserialize)]
struct MetaculusPost {
    id: u64,
    title: String,
    #[serde(default)]
    slug: String,

    /// ISO 8601 creation time.
    #[serde(default)]
    created_at: Option<String>,

    status: String, // "open", "closed", "resolved", etc.
    #[serde(default)]
    resolved: bool,

    /// Number of distinct forecasters.
    #[serde(default)]
    nr_forecasters: u32,
    /// Total number of forecasts submitted.
    #[serde(default)]
    forecasts_count: u32,

    /// Nested question object with type, timing, and aggregations.
    #[serde(default)]
    question: Option<MetaculusQuestion>,

    /// Project/topic tags for categorisation.
    #[serde(default)]
    projects: Option<MetaculusProjects>,
}

/// The inner question object within a Metaculus post.
#[derive(Debug, Deserialize)]
struct MetaculusQuestion {
    /// "binary", "numeric", "multiple_choice", etc.
    #[serde(rename = "type")]
    question_type: String,

    /// When forecasting closes.
    #[serde(default)]
    scheduled_close_time: Option<String>,
    /// When the question resolves.
    #[serde(default)]
    scheduled_resolve_time: Option<String>,

    /// Resolution criteria text.
    #[serde(default)]
    resolution_criteria: Option<String>,
    /// Fine print / edge cases.
    #[serde(default)]
    fine_print: Option<String>,
    /// Question description.
    #[serde(default)]
    description: Option<String>,

    /// Community prediction aggregations.
    #[serde(default)]
    aggregations: Option<MetaculusAggregations>,
}

/// Aggregation methods — we use `recency_weighted` as the primary signal.
#[derive(Debug, Deserialize)]
struct MetaculusAggregations {
    recency_weighted: Option<MetaculusAggregation>,
}

/// A single aggregation containing history and latest snapshot.
#[derive(Debug, Deserialize)]
struct MetaculusAggregation {
    latest: Option<MetaculusAggSnapshot>,
}

/// A snapshot of community prediction at a point in time.
#[derive(Debug, Deserialize)]
struct MetaculusAggSnapshot {
    /// Median probability (for binary: single f64).
    #[serde(default)]
    centers: Option<serde_json::Value>,
    /// Mean probability.
    #[serde(default)]
    means: Option<serde_json::Value>,
    /// Number of forecasters included in this aggregation.
    #[serde(default)]
    forecaster_count: Option<u32>,
}

/// Project/topic container.
#[derive(Debug, Deserialize)]
struct MetaculusProjects {
    #[serde(default)]
    category: Option<Vec<MetaculusCategory>>,
}

/// A single category tag.
#[derive(Debug, Deserialize)]
struct MetaculusCategory {
    #[serde(default)]
    name: String,
    #[serde(default)]
    slug: String,
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

/// Metaculus read-only platform client.
pub struct MetaculusClient {
    http: Client,
}

impl MetaculusClient {
    /// Create a new Metaculus client. No auth required.
    pub fn new() -> Result<Self> {
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("ORACLE/0.1.0 (prediction-market-agent)")
            .build()
            .context("Failed to build HTTP client for Metaculus")?;

        Ok(Self { http })
    }

    // -- Internal helpers ------------------------------------------------

    /// Fetch a single page of binary, open questions.
    async fn fetch_page(&self, offset: u32) -> Result<MetaculusPage> {
        let url = format!(
            "{BASE_URL}/?limit={PAGE_LIMIT}&offset={offset}\
             &status=open&type=binary&order_by=-nr_forecasters\
             &has_group=false"
        );

        debug!(url = %url, "Fetching Metaculus page");

        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .context("Metaculus API request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Metaculus API error {status}: {body}");
        }

        let page: MetaculusPage = resp
            .json()
            .await
            .context("Failed to parse Metaculus response")?;

        Ok(page)
    }

    /// Extract the community median probability from a post's aggregations.
    /// Returns `None` if the prediction hasn't been revealed yet or has
    /// too few forecasters.
    fn extract_probability(post: &MetaculusPost) -> Option<f64> {
        let question = post.question.as_ref()?;
        let agg = question.aggregations.as_ref()?;
        let rw = agg.recency_weighted.as_ref()?;
        let latest = rw.latest.as_ref()?;

        // `centers` is the median. For binary questions it's a single f64.
        let prob = match &latest.centers {
            Some(serde_json::Value::Number(n)) => n.as_f64(),
            Some(serde_json::Value::Array(arr)) if !arr.is_empty() => {
                arr[0].as_f64()
            }
            _ => None,
        }?;

        // Sanity check
        if prob >= 0.0 && prob <= 1.0 {
            Some(prob)
        } else {
            None
        }
    }

    /// Extract the community mean probability if available.
    fn extract_mean(post: &MetaculusPost) -> Option<f64> {
        let question = post.question.as_ref()?;
        let agg = question.aggregations.as_ref()?;
        let rw = agg.recency_weighted.as_ref()?;
        let latest = rw.latest.as_ref()?;

        match &latest.means {
            Some(serde_json::Value::Number(n)) => n.as_f64(),
            Some(serde_json::Value::Array(arr)) if !arr.is_empty() => {
                arr[0].as_f64()
            }
            _ => None,
        }
    }

    /// Get the forecaster count from the aggregation snapshot, falling
    /// back to the post-level `nr_forecasters`.
    fn forecaster_count(post: &MetaculusPost) -> u32 {
        post.question
            .as_ref()
            .and_then(|q| q.aggregations.as_ref())
            .and_then(|a| a.recency_weighted.as_ref())
            .and_then(|rw| rw.latest.as_ref())
            .and_then(|s| s.forecaster_count)
            .unwrap_or(post.nr_forecasters)
    }

    /// Classify a Metaculus question using its category slugs.
    fn classify(post: &MetaculusPost) -> MarketCategory {
        let slugs: Vec<String> = post
            .projects
            .as_ref()
            .and_then(|p| p.category.as_ref())
            .map(|cats| cats.iter().map(|c| c.slug.to_lowercase()).collect())
            .unwrap_or_default();

        let title = post.title.to_lowercase();
        let has_slug = |pattern: &str| slugs.iter().any(|s| s.contains(pattern));
        let has_word = |pattern: &str| title.contains(pattern);

        // Weather / Climate
        if has_slug("weather") || has_slug("climate") || has_slug("environment")
            || has_word("temperature") || has_word("hurricane")
            || has_word("earthquake") || has_word("flood")
            || has_word("drought") || has_word("wildfire")
            || has_word("weather") || has_word("rainfall")
        {
            return MarketCategory::Weather;
        }

        // Sports
        if has_slug("sports")
            || has_word("championship") || has_word("olympics")
            || has_word("world cup") || has_word("medal")
            || has_word("playoffs") || has_word("finals")
        {
            return MarketCategory::Sports;
        }

        // Economics
        if has_slug("economics") || has_slug("finance") || has_slug("business")
            || has_word("gdp") || has_word("inflation") || has_word("cpi")
            || has_word("interest rate") || has_word("fed ")
            || has_word("recession") || has_word("unemployment")
            || has_word("stock") || has_word("market cap")
            || has_word("tariff") || has_word("trade war")
        {
            return MarketCategory::Economics;
        }

        // Politics
        if has_slug("politics") || has_slug("elections") || has_slug("geopolitics")
            || has_slug("law") || has_slug("governance")
            || has_word("president") || has_word("congress")
            || has_word("election") || has_word("vote")
            || has_word("supreme court") || has_word("legislation")
            || has_word("trump") || has_word("biden")
            || has_word("senate") || has_word("governor")
            || has_word("war") || has_word("invasion")
        {
            return MarketCategory::Politics;
        }

        // Culture
        if has_slug("entertainment") || has_slug("arts") || has_slug("media")
            || has_word("oscar") || has_word("grammy")
            || has_word("box office") || has_word("album")
        {
            return MarketCategory::Culture;
        }

        MarketCategory::Other
    }

    /// Parse an ISO 8601 datetime string, falling back to `Utc::now()`.
    fn parse_datetime(s: &str) -> DateTime<Utc> {
        s.parse::<DateTime<Utc>>().unwrap_or_else(|_| Utc::now())
    }

    /// Convert a `MetaculusPost` to the ORACLE `Market` type.
    fn to_oracle_market(post: MetaculusPost) -> Option<Market> {
        let question = post.question.as_ref()?;

        // Only binary questions
        if question.question_type != "binary" {
            return None;
        }

        let prob = Self::extract_probability(&post)?;
        let forecasters = Self::forecaster_count(&post);
        let category = Self::classify(&post);

        let deadline = question
            .scheduled_resolve_time
            .as_deref()
            .or(question.scheduled_close_time.as_deref())
            .map(Self::parse_datetime)
            .unwrap_or_else(|| Utc::now() + chrono::Duration::days(365));

        let description = question
            .description
            .clone()
            .unwrap_or_default();

        let resolution_criteria = question
            .resolution_criteria
            .clone()
            .unwrap_or_default();

        let url = format!("https://www.metaculus.com/questions/{}/", post.id);

        Some(Market {
            id: post.id.to_string(),
            platform: PLATFORM_NAME.to_string(),
            question: post.title,
            description,
            category,
            current_price_yes: prob,
            current_price_no: 1.0 - prob,
            // Metaculus doesn't have volume/liquidity in the monetary sense.
            // We use forecaster count as a proxy for "information depth".
            volume_24h: 0.0,
            liquidity: forecasters as f64,
            deadline,
            resolution_criteria,
            url,
            cross_refs: CrossReferences {
                metaculus_prob: Some(prob),
                metaculus_forecasters: Some(forecasters),
                ..CrossReferences::default()
            },
        })
    }
}

// ---------------------------------------------------------------------------
// PredictionPlatform trait implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl PredictionPlatform for MetaculusClient {
    /// Fetch active binary questions with community predictions.
    ///
    /// Paginates through the API, ordered by forecaster count descending
    /// to prioritise the most-predicted (and thus most reliable) questions.
    async fn fetch_markets(&self) -> Result<Vec<Market>> {
        info!("Scanning Metaculus for active binary questions...");

        let mut all_markets = Vec::new();
        let mut offset = 0u32;

        for page_num in 0..MAX_PAGES {
            match self.fetch_page(offset).await {
                Ok(page) => {
                    let batch_count = page.results.len();
                    debug!(
                        page = page_num,
                        results = batch_count,
                        total_available = page.count,
                        "Metaculus page fetched"
                    );

                    for post in page.results {
                        // Skip low-signal questions
                        if post.nr_forecasters < MIN_FORECASTERS {
                            continue;
                        }
                        if post.resolved {
                            continue;
                        }

                        if let Some(market) = Self::to_oracle_market(post) {
                            all_markets.push(market);
                        }
                    }

                    // Stop if no more pages
                    if page.next.is_none() || batch_count == 0 {
                        break;
                    }

                    offset += PAGE_LIMIT;
                }
                Err(e) => {
                    warn!(page = page_num, error = %e, "Metaculus page fetch failed, stopping pagination");
                    break;
                }
            }
        }

        info!(
            total = all_markets.len(),
            "Metaculus scan complete"
        );

        Ok(all_markets)
    }

    /// Metaculus is read-only — betting is not supported.
    async fn place_bet(
        &self,
        _market_id: &str,
        _side: Side,
        _amount: f64,
    ) -> Result<TradeReceipt> {
        anyhow::bail!("Metaculus is read-only — betting not supported")
    }

    /// Metaculus has no positions (read-only).
    async fn get_positions(&self) -> Result<Vec<Position>> {
        Ok(Vec::new())
    }

    /// Metaculus has no monetary balance.
    async fn get_balance(&self) -> Result<f64> {
        Ok(0.0)
    }

    /// Metaculus doesn't have traditional liquidity. We return forecaster
    /// count as a proxy for information quality.
    async fn check_liquidity(&self, _market_id: &str) -> Result<LiquidityInfo> {
        // Could fetch individual question here, but for cross-referencing
        // purposes the data from fetch_markets() is sufficient.
        Ok(LiquidityInfo {
            bid_depth: 0.0,
            ask_depth: 0.0,
            volume_24h: 0.0,
        })
    }

    /// Metaculus is read-only — not an execution venue.
    fn is_executable(&self) -> bool {
        false
    }

    fn name(&self) -> &str {
        PLATFORM_NAME
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Helpers --

    fn make_test_post(
        title: &str,
        category_slugs: Vec<&str>,
        prob: Option<f64>,
        forecasters: u32,
    ) -> MetaculusPost {
        let aggregations = prob.map(|p| MetaculusAggregations {
            recency_weighted: Some(MetaculusAggregation {
                latest: Some(MetaculusAggSnapshot {
                    centers: Some(serde_json::json!(p)),
                    means: Some(serde_json::json!(p - 0.01)),
                    forecaster_count: Some(forecasters),
                }),
            }),
        });

        let question = Some(MetaculusQuestion {
            question_type: "binary".to_string(),
            scheduled_close_time: Some("2027-01-01T00:00:00Z".to_string()),
            scheduled_resolve_time: Some("2027-01-01T00:00:00Z".to_string()),
            resolution_criteria: Some("Test criteria".to_string()),
            fine_print: None,
            description: Some("Test description".to_string()),
            aggregations,
        });

        let categories = category_slugs
            .into_iter()
            .map(|s| MetaculusCategory {
                name: s.to_string(),
                slug: s.to_string(),
            })
            .collect();

        MetaculusPost {
            id: 12345,
            title: title.to_string(),
            slug: "test-question".to_string(),
            created_at: Some("2026-01-01T00:00:00Z".to_string()),
            status: "open".to_string(),
            resolved: false,
            nr_forecasters: forecasters,
            forecasts_count: forecasters * 2,
            question,
            projects: Some(MetaculusProjects {
                category: Some(categories),
            }),
        }
    }

    // -- Classification tests --

    #[test]
    fn test_classify_politics_by_slug() {
        let post = make_test_post("Some question", vec!["politics"], Some(0.5), 10);
        assert_eq!(MetaculusClient::classify(&post), MarketCategory::Politics);
    }

    #[test]
    fn test_classify_politics_by_title() {
        let post = make_test_post(
            "Will Trump win the 2028 presidential election?",
            vec![],
            Some(0.3),
            50,
        );
        assert_eq!(MetaculusClient::classify(&post), MarketCategory::Politics);
    }

    #[test]
    fn test_classify_economics_by_slug() {
        let post = make_test_post("Rate question", vec!["economics", "finance"], Some(0.6), 20);
        assert_eq!(MetaculusClient::classify(&post), MarketCategory::Economics);
    }

    #[test]
    fn test_classify_economics_by_title() {
        let post = make_test_post(
            "Will US CPI exceed 3% in Q2 2026?",
            vec![],
            Some(0.4),
            30,
        );
        assert_eq!(MetaculusClient::classify(&post), MarketCategory::Economics);
    }

    #[test]
    fn test_classify_weather_by_slug() {
        let post = make_test_post("Climate question", vec!["climate", "environment"], Some(0.7), 15);
        assert_eq!(MetaculusClient::classify(&post), MarketCategory::Weather);
    }

    #[test]
    fn test_classify_weather_by_title() {
        let post = make_test_post(
            "Will California experience a 6.5+ magnitude earthquake before 2030?",
            vec!["science"],
            Some(0.69),
            8,
        );
        assert_eq!(MetaculusClient::classify(&post), MarketCategory::Weather);
    }

    #[test]
    fn test_classify_sports() {
        let post = make_test_post(
            "Will any athlete win a gold medal at the 2026 Winter Olympics?",
            vec!["sports"],
            Some(0.9),
            25,
        );
        assert_eq!(MetaculusClient::classify(&post), MarketCategory::Sports);
    }

    #[test]
    fn test_classify_culture() {
        let post = make_test_post("Oscar best picture", vec!["entertainment"], Some(0.2), 10);
        assert_eq!(MetaculusClient::classify(&post), MarketCategory::Culture);
    }

    #[test]
    fn test_classify_other() {
        let post = make_test_post(
            "Will AGI be achieved before 2030?",
            vec!["technology", "ai"],
            Some(0.15),
            200,
        );
        assert_eq!(MetaculusClient::classify(&post), MarketCategory::Other);
    }

    // -- Probability extraction tests --

    #[test]
    fn test_extract_probability_number() {
        let post = make_test_post("Test", vec![], Some(0.73), 10);
        assert_eq!(MetaculusClient::extract_probability(&post), Some(0.73));
    }

    #[test]
    fn test_extract_probability_array() {
        // Some responses wrap centers in an array
        let mut post = make_test_post("Test", vec![], Some(0.5), 10);
        if let Some(ref mut q) = post.question {
            if let Some(ref mut agg) = q.aggregations {
                if let Some(ref mut rw) = agg.recency_weighted {
                    if let Some(ref mut latest) = rw.latest {
                        latest.centers = Some(serde_json::json!([0.65]));
                    }
                }
            }
        }
        assert_eq!(MetaculusClient::extract_probability(&post), Some(0.65));
    }

    #[test]
    fn test_extract_probability_none_when_no_aggregation() {
        let mut post = make_test_post("Test", vec![], None, 1);
        post.question.as_mut().unwrap().aggregations = None;
        assert_eq!(MetaculusClient::extract_probability(&post), None);
    }

    #[test]
    fn test_extract_probability_none_when_no_question() {
        let mut post = make_test_post("Test", vec![], Some(0.5), 10);
        post.question = None;
        assert_eq!(MetaculusClient::extract_probability(&post), None);
    }

    #[test]
    fn test_extract_probability_rejects_invalid() {
        let mut post = make_test_post("Test", vec![], Some(0.5), 10);
        if let Some(ref mut q) = post.question {
            if let Some(ref mut agg) = q.aggregations {
                if let Some(ref mut rw) = agg.recency_weighted {
                    if let Some(ref mut latest) = rw.latest {
                        latest.centers = Some(serde_json::json!(1.5));
                    }
                }
            }
        }
        assert_eq!(MetaculusClient::extract_probability(&post), None);
    }

    // -- Mean extraction --

    #[test]
    fn test_extract_mean() {
        let post = make_test_post("Test", vec![], Some(0.73), 10);
        // Mean is set to prob - 0.01 in our test helper
        assert!((MetaculusClient::extract_mean(&post).unwrap() - 0.72).abs() < 1e-10);
    }

    // -- Forecaster count --

    #[test]
    fn test_forecaster_count_from_aggregation() {
        let post = make_test_post("Test", vec![], Some(0.5), 10);
        assert_eq!(MetaculusClient::forecaster_count(&post), 10);
    }

    #[test]
    fn test_forecaster_count_fallback() {
        let mut post = make_test_post("Test", vec![], None, 42);
        post.question.as_mut().unwrap().aggregations = None;
        assert_eq!(MetaculusClient::forecaster_count(&post), 42);
    }

    // -- Conversion tests --

    #[test]
    fn test_to_oracle_market_basic() {
        let post = make_test_post(
            "Will US GDP growth exceed 2% in 2026?",
            vec!["economics"],
            Some(0.65),
            50,
        );
        let market = MetaculusClient::to_oracle_market(post).unwrap();

        assert_eq!(market.id, "12345");
        assert_eq!(market.platform, "metaculus");
        assert_eq!(market.category, MarketCategory::Economics);
        assert!((market.current_price_yes - 0.65).abs() < 1e-10);
        assert!((market.current_price_no - 0.35).abs() < 1e-10);
        assert_eq!(market.cross_refs.metaculus_prob, Some(0.65));
        assert_eq!(market.cross_refs.metaculus_forecasters, Some(50));
        assert!(market.cross_refs.manifold_prob.is_none());
        assert_eq!(market.liquidity, 50.0); // forecasters as proxy
        assert!(market.url.contains("12345"));
    }

    #[test]
    fn test_to_oracle_market_none_when_no_probability() {
        let post = make_test_post("Hidden question", vec![], None, 1);
        assert!(MetaculusClient::to_oracle_market(post).is_none());
    }

    #[test]
    fn test_to_oracle_market_none_when_non_binary() {
        let mut post = make_test_post("Numeric question", vec![], Some(0.5), 10);
        post.question.as_mut().unwrap().question_type = "numeric".to_string();
        assert!(MetaculusClient::to_oracle_market(post).is_none());
    }

    #[test]
    fn test_to_oracle_market_none_when_no_question() {
        let mut post = make_test_post("No question", vec![], Some(0.5), 10);
        post.question = None;
        assert!(MetaculusClient::to_oracle_market(post).is_none());
    }

    // -- Datetime parsing --

    #[test]
    fn test_parse_datetime_valid() {
        let dt = MetaculusClient::parse_datetime("2026-06-15T12:00:00Z");
        assert_eq!(dt.year(), 2026);
        assert_eq!(dt.month(), 6);
    }

    use chrono::Datelike;

    #[test]
    fn test_parse_datetime_invalid_fallback() {
        let dt = MetaculusClient::parse_datetime("not-a-date");
        // Should fall back to ~now
        assert_eq!(dt.year(), Utc::now().year());
    }

    // -- Client construction --

    #[test]
    fn test_new_client() {
        let client = MetaculusClient::new();
        assert!(client.is_ok());
        let client = client.unwrap();
        assert!(!client.is_executable());
        assert_eq!(client.name(), "metaculus");
    }
}
