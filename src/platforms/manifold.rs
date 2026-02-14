//! Manifold Markets play-money integration.
//!
//! Used for paper-trading validation and as a fast-moving sentiment signal.
//! All bets are in Mana (play currency, no cash-out).
//!
//! API docs: https://docs.manifold.markets/api
//! Base URL: https://api.manifold.markets/v0/
//! Rate limit: 500 requests/minute per IP
//! Auth: Not required for reads; `Authorization: Key {key}` for writes.

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
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

const BASE_URL: &str = "https://api.manifold.markets/v0";
const PLATFORM_NAME: &str = "manifold";

/// Maximum markets to fetch per search query (API max is 1000).
const DEFAULT_FETCH_LIMIT: u32 = 200;

/// Minimum unique bettors for a market to be considered meaningful.
const MIN_BETTORS: u32 = 3;

// ---------------------------------------------------------------------------
// API response types (Manifold JSON → Rust)
// ---------------------------------------------------------------------------

/// Manifold `LiteMarket` — the shape returned by `/v0/search-markets`
/// and `/v0/markets`. We only deserialize the fields we need.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifoldLiteMarket {
    id: String,
    question: String,
    #[serde(default)]
    slug: String,
    url: String,

    /// "BINARY", "MULTIPLE_CHOICE", etc.
    outcome_type: String,
    /// "cpmm-1", "dpm-2", "cpmm-multi-1"
    mechanism: String,

    /// Current implied probability (0.0–1.0) for binary markets.
    #[serde(default)]
    probability: f64,

    /// Pool shares: { "YES": f64, "NO": f64 }
    #[serde(default)]
    pool: Option<serde_json::Value>,

    /// Total mana in liquidity pool (CPMM markets).
    #[serde(default)]
    total_liquidity: Option<f64>,

    /// Lifetime volume in mana.
    #[serde(default)]
    volume: f64,
    /// Rolling 24-hour volume in mana.
    #[serde(default)]
    volume24_hours: f64,

    /// Number of distinct users who have bet.
    #[serde(default)]
    unique_bettor_count: u32,

    /// Whether the market has been resolved.
    #[serde(default)]
    is_resolved: bool,

    /// Market close timestamp (ms since epoch). May be absent.
    #[serde(default)]
    close_time: Option<i64>,

    /// Created timestamp (ms since epoch).
    #[serde(default)]
    created_time: i64,

    /// Token type: "MANA" or "CASH".
    #[serde(default)]
    token: Option<String>,

    /// Creator info (for logging/debugging).
    #[serde(default)]
    creator_username: Option<String>,

    /// Topics / group slugs tagged on this market.
    #[serde(default)]
    group_slugs: Option<Vec<String>>,
}

/// Response from `/v0/bet` POST (place a bet).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifoldBetResponse {
    #[serde(default)]
    bet_id: Option<String>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    amount: f64,
    #[serde(default)]
    shares: f64,
    #[serde(default)]
    prob_after: f64,
    #[serde(default)]
    fees: Option<serde_json::Value>,
    #[serde(default)]
    created_time: Option<i64>,
}

/// Response from `/v0/me` GET (authenticated user info).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifoldUser {
    #[serde(default)]
    balance: f64,
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

/// Manifold Markets platform client.
pub struct ManifoldClient {
    http: Client,
    /// Optional API key for authenticated endpoints (betting, balance).
    api_key: Option<String>,
}

impl ManifoldClient {
    /// Create a new Manifold client.
    ///
    /// `api_key` is optional — only needed for placing bets and checking
    /// balance. Scanning markets is fully public.
    pub fn new(api_key: Option<String>) -> Result<Self> {
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("ORACLE/0.1.0 (prediction-market-agent)")
            .build()
            .context("Failed to build HTTP client for Manifold")?;

        Ok(Self { http, api_key })
    }

    // -- Internal helpers ------------------------------------------------

    /// Fetch binary, open, MANA markets sorted by the given criterion.
    async fn search_markets(
        &self,
        term: &str,
        sort: &str,
        limit: u32,
    ) -> Result<Vec<ManifoldLiteMarket>> {
        let url = format!(
            "{BASE_URL}/search-markets?term={}&filter=open&contractType=BINARY&sort={}&limit={}",
            urlencoding::encode(term),
            sort,
            limit,
        );

        debug!(url = %url, "Fetching Manifold markets");

        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .context("Manifold API request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Manifold API error {status}: {body}");
        }

        let markets: Vec<ManifoldLiteMarket> = resp
            .json()
            .await
            .context("Failed to parse Manifold search-markets response")?;

        Ok(markets)
    }

    /// Convert a Manifold API timestamp (ms since epoch) to `DateTime<Utc>`.
    fn ms_to_datetime(ms: i64) -> DateTime<Utc> {
        Utc.timestamp_millis_opt(ms).single().unwrap_or_else(Utc::now)
    }

    /// Classify a market question into a `MarketCategory` using keyword
    /// heuristics. Manifold doesn't provide structured categories on
    /// `LiteMarket`, so we infer from question text and topic slugs.
    fn classify(market: &ManifoldLiteMarket) -> MarketCategory {
        let q = market.question.to_lowercase();
        let slugs: Vec<String> = market
            .group_slugs
            .as_ref()
            .map(|s| s.iter().map(|g| g.to_lowercase()).collect())
            .unwrap_or_default();

        let has_slug = |pattern: &str| slugs.iter().any(|s| s.contains(pattern));
        let has_word = |pattern: &str| q.contains(pattern);

        // Weather
        if has_slug("weather") || has_slug("climate")
            || has_word("temperature") || has_word("hurricane")
            || has_word("tornado") || has_word("rainfall")
            || has_word("snowfall") || has_word("weather")
            || has_word("drought") || has_word("heat wave")
        {
            return MarketCategory::Weather;
        }

        // Sports
        if has_slug("sports") || has_slug("nba") || has_slug("nfl")
            || has_slug("mlb") || has_slug("soccer") || has_slug("football")
            || has_slug("tennis") || has_slug("olympics")
            || has_word("championship") || has_word("playoffs")
            || has_word("finals") || has_word("super bowl")
            || has_word("world cup") || has_word("win the ")
            || has_word("mvp") || has_word("premier league")
        {
            return MarketCategory::Sports;
        }

        // Economics
        if has_slug("economics") || has_slug("finance") || has_slug("crypto")
            || has_slug("stocks") || has_slug("markets")
            || has_word("gdp") || has_word("inflation") || has_word("cpi")
            || has_word("interest rate") || has_word("fed ") || has_word("federal reserve")
            || has_word("recession") || has_word("unemployment")
            || has_word("stock market") || has_word("s&p 500") || has_word("s&p500")
            || has_word("bitcoin") || has_word("crypto") || has_word("tariff")
        {
            return MarketCategory::Economics;
        }

        // Politics
        if has_slug("politics") || has_slug("elections") || has_slug("us-politics")
            || has_slug("world-politics") || has_slug("geopolitics")
            || has_word("president") || has_word("congress") || has_word("senate")
            || has_word("election") || has_word("vote") || has_word("democrat")
            || has_word("republican") || has_word("trump") || has_word("biden")
            || has_word("governor") || has_word("supreme court")
            || has_word("legislation") || has_word("impeach")
        {
            return MarketCategory::Politics;
        }

        // Culture
        if has_slug("entertainment") || has_slug("movies") || has_slug("music")
            || has_slug("tv") || has_slug("celebrity") || has_slug("oscars")
            || has_word("oscar") || has_word("grammy") || has_word("emmy")
            || has_word("box office") || has_word("netflix") || has_word("spotify")
            || has_word("album") || has_word("movie")
        {
            return MarketCategory::Culture;
        }

        MarketCategory::Other
    }

    /// Convert a `ManifoldLiteMarket` to the ORACLE `Market` type.
    fn to_oracle_market(m: ManifoldLiteMarket) -> Market {
        let category = Self::classify(&m);
        let prob = m.probability.clamp(0.0, 1.0);
        let deadline = m
            .close_time
            .map(Self::ms_to_datetime)
            .unwrap_or_else(|| Utc::now() + chrono::Duration::days(365));

        // Extract pool YES/NO for liquidity estimate
        let (pool_yes, pool_no) = match &m.pool {
            Some(serde_json::Value::Object(map)) => {
                let yes = map.get("YES").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let no = map.get("NO").and_then(|v| v.as_f64()).unwrap_or(0.0);
                (yes, no)
            }
            _ => (0.0, 0.0),
        };
        let liquidity = m.total_liquidity.unwrap_or(pool_yes + pool_no);

        Market {
            id: m.id,
            platform: PLATFORM_NAME.to_string(),
            question: m.question,
            description: String::new(), // LiteMarket doesn't include description
            category,
            current_price_yes: prob,
            current_price_no: 1.0 - prob,
            volume_24h: m.volume24_hours,
            liquidity,
            deadline,
            resolution_criteria: String::new(), // Not in LiteMarket
            url: m.url,
            cross_refs: CrossReferences {
                manifold_prob: Some(prob),
                ..CrossReferences::default()
            },
        }
    }
}

// ---------------------------------------------------------------------------
// PredictionPlatform trait implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl PredictionPlatform for ManifoldClient {
    /// Fetch active binary markets from Manifold.
    ///
    /// Uses `/v0/search-markets` with multiple queries to cover different
    /// sort criteria and maximize market discovery. Deduplicates by ID.
    async fn fetch_markets(&self) -> Result<Vec<Market>> {
        info!("Scanning Manifold Markets for active binary markets...");

        // Fetch from multiple sort perspectives to get broad coverage
        let sorts = [
            ("", "liquidity"),         // highest-liquidity markets
            ("", "24-hour-vol"),       // most actively traded right now
            ("", "newest"),            // recently created
            ("", "most-popular"),      // most bettors
        ];

        let mut seen = std::collections::HashSet::new();
        let mut all_markets = Vec::new();

        for (term, sort) in &sorts {
            match self
                .search_markets(term, sort, DEFAULT_FETCH_LIMIT)
                .await
            {
                Ok(batch) => {
                    let count_before = all_markets.len();
                    for m in batch {
                        // Skip non-binary, resolved, low-quality, or CASH markets
                        if m.outcome_type != "BINARY" {
                            continue;
                        }
                        if m.is_resolved {
                            continue;
                        }
                        if m.unique_bettor_count < MIN_BETTORS {
                            continue;
                        }
                        // Skip CASH token markets (we only use MANA for paper)
                        if m.token.as_deref() == Some("CASH") {
                            continue;
                        }
                        if seen.insert(m.id.clone()) {
                            all_markets.push(Self::to_oracle_market(m));
                        }
                    }
                    debug!(
                        sort = sort,
                        new = all_markets.len() - count_before,
                        total = all_markets.len(),
                        "Manifold batch fetched"
                    );
                }
                Err(e) => {
                    warn!(sort = sort, error = %e, "Manifold search query failed, continuing");
                }
            }
        }

        info!(
            total = all_markets.len(),
            "Manifold scan complete"
        );

        Ok(all_markets)
    }

    /// Place a play-money bet on Manifold.
    ///
    /// Requires an API key. Amount is in Mana.
    async fn place_bet(
        &self,
        market_id: &str,
        side: Side,
        amount: f64,
    ) -> Result<TradeReceipt> {
        let api_key = self
            .api_key
            .as_ref()
            .context("Manifold API key required for placing bets")?;

        let outcome = match side {
            Side::Yes => "YES",
            Side::No => "NO",
        };

        let body = serde_json::json!({
            "amount": amount,
            "outcome": outcome,
            "contractId": market_id,
        });

        let resp = self
            .http
            .post(&format!("{BASE_URL}/bet"))
            .header("Authorization", format!("Key {api_key}"))
            .json(&body)
            .send()
            .await
            .context("Manifold bet request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Manifold bet failed {status}: {body}");
        }

        let bet: ManifoldBetResponse = resp
            .json()
            .await
            .context("Failed to parse Manifold bet response")?;

        let order_id = bet
            .bet_id
            .or(bet.id)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        let timestamp = bet
            .created_time
            .map(Self::ms_to_datetime)
            .unwrap_or_else(Utc::now);

        info!(
            order_id = %order_id,
            market_id = %market_id,
            side = %side,
            amount = amount,
            prob_after = bet.prob_after,
            "Manifold bet placed"
        );

        Ok(TradeReceipt {
            order_id,
            market_id: market_id.to_string(),
            platform: PLATFORM_NAME.to_string(),
            side,
            amount: bet.amount,
            fill_price: bet.prob_after,
            fees: 0.0, // Manifold doesn't charge explicit fees on bets
            timestamp,
        })
    }

    /// Get current positions on Manifold.
    ///
    /// TODO: Implement using `/v0/bets` filtered by user ID. Requires
    /// tracking the authenticated user's ID and aggregating open positions.
    async fn get_positions(&self) -> Result<Vec<Position>> {
        // Phase 6 will implement full position tracking.
        // For now return empty — scanning is the priority.
        Ok(Vec::new())
    }

    /// Get Mana balance for the authenticated user.
    async fn get_balance(&self) -> Result<f64> {
        let api_key = self
            .api_key
            .as_ref()
            .context("Manifold API key required for balance check")?;

        let resp = self
            .http
            .get(&format!("{BASE_URL}/me"))
            .header("Authorization", format!("Key {api_key}"))
            .send()
            .await
            .context("Manifold balance request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Manifold balance check failed {status}: {body}");
        }

        let user: ManifoldUser = resp
            .json()
            .await
            .context("Failed to parse Manifold user response")?;

        Ok(user.balance)
    }

    /// Check liquidity for a specific Manifold market.
    async fn check_liquidity(&self, market_id: &str) -> Result<LiquidityInfo> {
        let url = format!("{BASE_URL}/market/{market_id}");

        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .context("Manifold market detail request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Manifold market detail failed {status}: {body}");
        }

        let market: ManifoldLiteMarket = resp
            .json()
            .await
            .context("Failed to parse Manifold market detail")?;

        let (pool_yes, pool_no) = match &market.pool {
            Some(serde_json::Value::Object(map)) => {
                let yes = map.get("YES").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let no = map.get("NO").and_then(|v| v.as_f64()).unwrap_or(0.0);
                (yes, no)
            }
            _ => (0.0, 0.0),
        };

        Ok(LiquidityInfo {
            bid_depth: pool_yes,
            ask_depth: pool_no,
            volume_24h: market.volume24_hours,
        })
    }

    /// Manifold is play-money only — not a real-money execution venue.
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
    use chrono::Datelike;

    // -- Classification tests --

    fn make_test_market(question: &str, slugs: Vec<&str>) -> ManifoldLiteMarket {
        ManifoldLiteMarket {
            id: "test-id".to_string(),
            question: question.to_string(),
            slug: String::new(),
            url: "https://manifold.markets/test".to_string(),
            outcome_type: "BINARY".to_string(),
            mechanism: "cpmm-1".to_string(),
            probability: 0.5,
            pool: Some(serde_json::json!({"YES": 100.0, "NO": 100.0})),
            total_liquidity: Some(200.0),
            volume: 1000.0,
            volume24_hours: 50.0,
            unique_bettor_count: 10,
            is_resolved: false,
            close_time: Some(Utc::now().timestamp_millis() + 86_400_000),
            created_time: Utc::now().timestamp_millis(),
            token: Some("MANA".to_string()),
            creator_username: None,
            group_slugs: Some(slugs.into_iter().map(String::from).collect()),
        }
    }

    #[test]
    fn test_classify_weather_by_question() {
        let m = make_test_market("Will there be a major hurricane in 2026?", vec![]);
        assert_eq!(ManifoldClient::classify(&m), MarketCategory::Weather);
    }

    #[test]
    fn test_classify_weather_by_slug() {
        let m = make_test_market("Some obscure question", vec!["weather", "science"]);
        assert_eq!(ManifoldClient::classify(&m), MarketCategory::Weather);
    }

    #[test]
    fn test_classify_sports() {
        let m = make_test_market("Will the Oklahoma City Thunder win 2026 NBA Finals", vec!["nba"]);
        assert_eq!(ManifoldClient::classify(&m), MarketCategory::Sports);
    }

    #[test]
    fn test_classify_economics_cpi() {
        let m = make_test_market("Will US CPI exceed 3% in Q2 2026?", vec![]);
        assert_eq!(ManifoldClient::classify(&m), MarketCategory::Economics);
    }

    #[test]
    fn test_classify_economics_by_slug() {
        let m = make_test_market("Some finance thing", vec!["finance", "markets"]);
        assert_eq!(ManifoldClient::classify(&m), MarketCategory::Economics);
    }

    #[test]
    fn test_classify_politics() {
        let m = make_test_market("Will Trump finish his second term?", vec![]);
        assert_eq!(ManifoldClient::classify(&m), MarketCategory::Politics);
    }

    #[test]
    fn test_classify_politics_by_slug() {
        let m = make_test_market("Some policy question", vec!["us-politics"]);
        assert_eq!(ManifoldClient::classify(&m), MarketCategory::Politics);
    }

    #[test]
    fn test_classify_culture() {
        let m = make_test_market("Will the Oscar for Best Picture go to...", vec![]);
        assert_eq!(ManifoldClient::classify(&m), MarketCategory::Culture);
    }

    #[test]
    fn test_classify_other() {
        let m = make_test_market("Will AGI be developed before 2030?", vec!["technology"]);
        assert_eq!(ManifoldClient::classify(&m), MarketCategory::Other);
    }

    // -- Conversion tests --

    #[test]
    fn test_to_oracle_market_fields() {
        let m = make_test_market("Will US CPI exceed 3% in Q2 2026?", vec!["economics"]);
        let oracle = ManifoldClient::to_oracle_market(m);

        assert_eq!(oracle.id, "test-id");
        assert_eq!(oracle.platform, "manifold");
        assert_eq!(oracle.category, MarketCategory::Economics);
        assert!((oracle.current_price_yes - 0.5).abs() < 1e-10);
        assert!((oracle.current_price_no - 0.5).abs() < 1e-10);
        assert!((oracle.volume_24h - 50.0).abs() < 1e-10);
        assert!((oracle.liquidity - 200.0).abs() < 1e-10);
        assert_eq!(oracle.cross_refs.manifold_prob, Some(0.5));
        assert!(oracle.cross_refs.metaculus_prob.is_none());
    }

    #[test]
    fn test_to_oracle_market_probability_clamped() {
        let mut m = make_test_market("Test", vec![]);
        m.probability = 1.5; // Invalid, should clamp
        let oracle = ManifoldClient::to_oracle_market(m);
        assert!((oracle.current_price_yes - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_to_oracle_market_missing_pool() {
        let mut m = make_test_market("Test", vec![]);
        m.pool = None;
        m.total_liquidity = None;
        let oracle = ManifoldClient::to_oracle_market(m);
        assert!((oracle.liquidity - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_to_oracle_market_missing_close_time() {
        let mut m = make_test_market("Test", vec![]);
        m.close_time = None;
        let oracle = ManifoldClient::to_oracle_market(m);
        // Should default to ~1 year from now
        assert!(oracle.deadline > Utc::now() + chrono::Duration::days(300));
    }

    // -- Timestamp tests --

    #[test]
    fn test_ms_to_datetime() {
        let dt = ManifoldClient::ms_to_datetime(1_700_000_000_000);
        assert_eq!(dt.year(), 2023);
    }

    #[test]
    fn test_ms_to_datetime_zero() {
        let dt = ManifoldClient::ms_to_datetime(0);
        assert_eq!(dt.year(), 1970);
    }

    // -- Client construction --

    #[test]
    fn test_new_client_no_key() {
        let client = ManifoldClient::new(None);
        assert!(client.is_ok());
        let client = client.unwrap();
        assert!(client.api_key.is_none());
        assert!(!client.is_executable());
        assert_eq!(client.name(), "manifold");
    }

    #[test]
    fn test_new_client_with_key() {
        let client = ManifoldClient::new(Some("test-key-123".to_string()));
        assert!(client.is_ok());
        assert!(client.unwrap().api_key.is_some());
    }
}
