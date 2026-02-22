//! Polymarket integration.
//!
//! Uses the Gamma API for market discovery (no auth required) and the
//! CLOB API for order placement (requires wallet private key + HMAC auth).
//!
//! Gamma API: https://gamma-api.polymarket.com
//! CLOB API: https://clob.polymarket.com
//!
//! Market data is free and unauthenticated. Trading requires a Polygon
//! wallet with USDC and EIP-712 order signing.

use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use tracing::{debug, info, warn};

use crate::platforms::PredictionPlatform;
use crate::types::{
    CrossReferences, LiquidityInfo, Market, MarketCategory, Position, Side, TradeReceipt,
};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

const GAMMA_API_URL: &str = "https://gamma-api.polymarket.com";
const CLOB_API_URL: &str = "https://clob.polymarket.com";
const DEFAULT_LIMIT: u32 = 100;
const MIN_VOLUME_24H: f64 = 1000.0;
const MIN_LIQUIDITY: f64 = 500.0;

// ---------------------------------------------------------------------------
// Gamma API response types (market discovery)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)]
pub struct GammaMarket {
    #[serde(default)]
    pub id: Option<u64>,
    #[serde(default)]
    pub question: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, rename = "conditionId")]
    pub condition_id: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default, rename = "endDate")]
    pub end_date: Option<String>,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub closed: bool,
    /// Outcome prices as JSON string: "[\"0.65\",\"0.35\"]"
    #[serde(default, rename = "outcomePrices")]
    pub outcome_prices: Option<String>,
    #[serde(default, rename = "clobTokenIds")]
    pub clob_token_ids: Option<String>,
    #[serde(default)]
    pub volume: Option<f64>,
    #[serde(default, rename = "volumeNum")]
    pub volume_num: Option<f64>,
    #[serde(default)]
    pub liquidity: Option<f64>,
    #[serde(default)]
    pub tags: Option<Vec<GammaTag>>,
    #[serde(default, rename = "bestBid")]
    pub best_bid: Option<f64>,
    #[serde(default, rename = "bestAsk")]
    pub best_ask: Option<f64>,
    #[serde(default)]
    pub spread: Option<f64>,
    #[serde(default, rename = "lastTradePrice")]
    pub last_trade_price: Option<f64>,
}

#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)]
pub struct GammaTag {
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub slug: String,
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

pub struct PolymarketClient {
    http: Client,
    min_volume: f64,
    min_liquidity: f64,
}

impl PolymarketClient {
    pub fn new() -> Result<Self> {
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .context("Failed to build Polymarket HTTP client")?;

        Ok(Self {
            http,
            min_volume: MIN_VOLUME_24H,
            min_liquidity: MIN_LIQUIDITY,
        })
    }

    /// Fetch active markets from the Gamma API (no auth required).
    pub async fn fetch_gamma_markets(&self) -> Result<Vec<GammaMarket>> {
        let url = format!("{GAMMA_API_URL}/markets");
        debug!("Fetching Polymarket markets from Gamma API");

        let resp = self.http
            .get(&url)
            .query(&[
                ("active", "true"),
                ("closed", "false"),
                ("limit", &DEFAULT_LIMIT.to_string()),
            ])
            .send()
            .await
            .context("Gamma API request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Gamma API error {status}: {body}");
        }

        let markets: Vec<GammaMarket> = resp.json().await
            .context("Failed to parse Gamma markets response")?;

        info!(count = markets.len(), "Fetched raw Gamma markets");
        Ok(markets)
    }

    /// Convert a Gamma market into our internal Market type.
    pub fn convert_market(gm: &GammaMarket) -> Option<Market> {
        if gm.condition_id.is_empty() || gm.question.is_empty() {
            return None;
        }

        // Parse outcome prices: "[\"0.65\",\"0.35\"]" or "0.65, 0.35"
        let (price_yes, price_no) = Self::parse_outcome_prices(
            gm.outcome_prices.as_deref().unwrap_or(""),
        ).unwrap_or((0.5, 0.5));

        // Parse deadline
        let deadline = gm.end_date.as_deref()
            .and_then(|d| {
                chrono::DateTime::parse_from_rfc3339(d).ok()
                    .map(|dt| dt.with_timezone(&chrono::Utc))
            })
            .or_else(|| {
                gm.end_date.as_deref().and_then(|d| {
                    chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()
                        .map(|nd| nd.and_hms_opt(23, 59, 59).unwrap().and_utc())
                })
            })
            .unwrap_or_else(|| chrono::Utc::now() + chrono::Duration::days(365));

        let volume = gm.volume.or(gm.volume_num).unwrap_or(0.0);
        let liquidity = gm.liquidity.unwrap_or(0.0);

        // Categorize from tags
        let category = gm.tags.as_ref()
            .map(|tags| Self::categorize_from_tags(tags, &gm.question))
            .unwrap_or_else(|| Self::categorize_from_question(&gm.question));

        let url = format!("https://polymarket.com/event/{}", gm.slug);

        Some(Market {
            id: gm.condition_id.clone(),
            platform: "polymarket".to_string(),
            question: gm.question.clone(),
            description: gm.description.clone(),
            category,
            current_price_yes: price_yes,
            current_price_no: price_no,
            volume_24h: volume,
            liquidity,
            deadline,
            resolution_criteria: gm.description.clone(),
            url,
            cross_refs: CrossReferences::default(),
        })
    }

    /// Parse outcome prices from Gamma's string format.
    /// Handles: "[\"0.65\",\"0.35\"]", "0.65, 0.35", etc.
    pub fn parse_outcome_prices(s: &str) -> Option<(f64, f64)> {
        let cleaned = s.replace(['[', ']', '"', '\\'], "");
        let parts: Vec<&str> = cleaned.split(',').map(|p| p.trim()).collect();
        if parts.len() >= 2 {
            let yes = parts[0].parse::<f64>().ok()?;
            let no = parts[1].parse::<f64>().ok()?;
            Some((yes, no))
        } else {
            None
        }
    }

    /// Categorize from Gamma tags.
    fn categorize_from_tags(tags: &[GammaTag], question: &str) -> MarketCategory {
        let tag_slugs: Vec<&str> = tags.iter().map(|t| t.slug.as_str()).collect();
        let tag_labels: Vec<String> = tags.iter().map(|t| t.label.to_lowercase()).collect();

        if tag_slugs.iter().any(|s| ["politics", "elections", "us-politics", "government"].contains(s))
            || tag_labels.iter().any(|l| l.contains("politic") || l.contains("election"))
        {
            return MarketCategory::Politics;
        }
        if tag_slugs.iter().any(|s| ["sports", "nba", "nfl", "soccer", "football", "mlb", "nhl", "tennis", "mma", "cricket", "f1"].contains(s))
            || tag_labels.iter().any(|l| l.contains("sport"))
        {
            return MarketCategory::Sports;
        }
        if tag_slugs.iter().any(|s| ["crypto", "finance", "economics", "business", "fed", "inflation", "stocks"].contains(s))
            || tag_labels.iter().any(|l| l.contains("econom") || l.contains("crypto") || l.contains("financ"))
        {
            return MarketCategory::Economics;
        }
        if tag_slugs.iter().any(|s| ["weather", "climate"].contains(s)) {
            return MarketCategory::Weather;
        }
        if tag_slugs.iter().any(|s| ["entertainment", "culture", "tech", "science", "ai"].contains(s))
            || tag_labels.iter().any(|l| l.contains("culture") || l.contains("entertainment"))
        {
            return MarketCategory::Culture;
        }

        // Fallback to question-based
        Self::categorize_from_question(question)
    }

    /// Categorize from question text (fallback).
    fn categorize_from_question(question: &str) -> MarketCategory {
        let q = question.to_lowercase();
        if q.contains("election") || q.contains("president") || q.contains("congress")
            || q.contains("senate") || q.contains("vote") || q.contains("trump")
            || q.contains("biden") || q.contains("governor") || q.contains("democrat")
            || q.contains("republican") || q.contains("political")
        {
            MarketCategory::Politics
        } else if q.contains("nba") || q.contains("nfl") || q.contains("mlb")
            || q.contains("soccer") || q.contains("tennis") || q.contains("cricket")
            || q.contains("championship") || q.contains("super bowl") || q.contains("world cup")
            || q.contains("win the") || q.contains("playoff")
        {
            MarketCategory::Sports
        } else if q.contains("bitcoin") || q.contains("ethereum") || q.contains("crypto")
            || q.contains("fed ") || q.contains("rate cut") || q.contains("inflation")
            || q.contains("gdp") || q.contains("stock") || q.contains("s&p")
            || q.contains("recession") || q.contains("interest rate")
        {
            MarketCategory::Economics
        } else if q.contains("temperature") || q.contains("hurricane")
            || q.contains("rainfall") || q.contains("weather") || q.contains("climate")
        {
            MarketCategory::Weather
        } else if q.contains("oscar") || q.contains("grammy") || q.contains("box office")
            || q.contains("movie") || q.contains("album") || q.contains("netflix")
        {
            MarketCategory::Culture
        } else {
            MarketCategory::Other
        }
    }

    /// Filter markets by volume and liquidity thresholds.
    pub fn filter_markets(&self, markets: Vec<Market>) -> Vec<Market> {
        markets.into_iter().filter(|m| {
            m.volume_24h >= self.min_volume
                && m.liquidity >= self.min_liquidity
                && m.deadline > chrono::Utc::now()
                && m.current_price_yes > 0.02
                && m.current_price_yes < 0.98
        }).collect()
    }
}

// ---------------------------------------------------------------------------
// PredictionPlatform trait implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl PredictionPlatform for PolymarketClient {
    async fn fetch_markets(&self) -> Result<Vec<Market>> {
        let gamma_markets = self.fetch_gamma_markets().await?;

        let markets: Vec<Market> = gamma_markets.iter()
            .filter_map(|gm| Self::convert_market(gm))
            .collect();

        let filtered = self.filter_markets(markets);
        info!(count = filtered.len(), "Polymarket markets after filtering");
        Ok(filtered)
    }

    async fn place_bet(
        &self,
        market_id: &str,
        side: Side,
        amount: f64,
    ) -> Result<TradeReceipt> {
        // TODO: Implement CLOB order placement.
        // Requires:
        //   1. Polygon wallet private key
        //   2. EIP-712 order signing
        //   3. HMAC-SHA256 L2 authentication
        //   4. Token approval for USDC + CTF contracts
        //
        // For now, return a dry-run receipt. When ready, either:
        //   - Use polymarket-client-sdk crate (adds ~50 deps but handles all signing)
        //   - Implement raw EIP-712 signing with alloy (lighter but more work)
        warn!(
            market_id = %market_id,
            side = ?side,
            amount,
            "Polymarket execution not yet wired — returning dry-run receipt"
        );
        Ok(TradeReceipt::dry_run(market_id, amount))
    }

    async fn get_positions(&self) -> Result<Vec<Position>> {
        // TODO: Query CLOB API /positions with L2 auth
        Ok(Vec::new())
    }

    async fn get_balance(&self) -> Result<f64> {
        // TODO: Query Polygon RPC for USDC balance of wallet
        Ok(0.0)
    }

    async fn check_liquidity(&self, market_id: &str) -> Result<LiquidityInfo> {
        // TODO: Query CLOB order book for bid/ask depth
        Ok(LiquidityInfo {
            bid_depth: 0.0,
            ask_depth: 0.0,
            volume_24h: 0.0,
        })
    }

    fn is_executable(&self) -> bool {
        false // Until CLOB signing is wired up
    }

    fn name(&self) -> &str {
        "polymarket"
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_outcome_prices_json_format() {
        let (yes, no) = PolymarketClient::parse_outcome_prices("[\"0.65\",\"0.35\"]").unwrap();
        assert!((yes - 0.65).abs() < 1e-10);
        assert!((no - 0.35).abs() < 1e-10);
    }

    #[test]
    fn test_parse_outcome_prices_simple_format() {
        let (yes, no) = PolymarketClient::parse_outcome_prices("0.72, 0.28").unwrap();
        assert!((yes - 0.72).abs() < 1e-10);
        assert!((no - 0.28).abs() < 1e-10);
    }

    #[test]
    fn test_parse_outcome_prices_empty() {
        assert!(PolymarketClient::parse_outcome_prices("").is_none());
    }

    #[test]
    fn test_parse_outcome_prices_single() {
        assert!(PolymarketClient::parse_outcome_prices("0.50").is_none());
    }

    #[test]
    fn test_categorize_politics() {
        assert!(matches!(
            PolymarketClient::categorize_from_question("Will Trump win the 2025 election?"),
            MarketCategory::Politics
        ));
        assert!(matches!(
            PolymarketClient::categorize_from_question("Will the Senate pass the bill?"),
            MarketCategory::Politics
        ));
    }

    #[test]
    fn test_categorize_sports() {
        assert!(matches!(
            PolymarketClient::categorize_from_question("Who will win the NBA championship?"),
            MarketCategory::Sports
        ));
        assert!(matches!(
            PolymarketClient::categorize_from_question("Will the Super Bowl be in February?"),
            MarketCategory::Sports
        ));
    }

    #[test]
    fn test_categorize_economics() {
        assert!(matches!(
            PolymarketClient::categorize_from_question("Will Bitcoin exceed $100k?"),
            MarketCategory::Economics
        ));
        assert!(matches!(
            PolymarketClient::categorize_from_question("Will the Fed cut rates in March?"),
            MarketCategory::Economics
        ));
    }

    #[test]
    fn test_categorize_weather() {
        assert!(matches!(
            PolymarketClient::categorize_from_question("Will the hurricane hit Florida?"),
            MarketCategory::Weather
        ));
    }

    #[test]
    fn test_categorize_culture() {
        assert!(matches!(
            PolymarketClient::categorize_from_question("Will the movie win an Oscar?"),
            MarketCategory::Culture
        ));
    }

    #[test]
    fn test_categorize_other() {
        assert!(matches!(
            PolymarketClient::categorize_from_question("Will aliens be discovered in 2026?"),
            MarketCategory::Other
        ));
    }

    #[test]
    fn test_categorize_from_tags() {
        let politics_tags = vec![GammaTag { label: "Politics".into(), slug: "politics".into() }];
        assert!(matches!(
            PolymarketClient::categorize_from_tags(&politics_tags, "Some question"),
            MarketCategory::Politics
        ));

        let sports_tags = vec![GammaTag { label: "NBA".into(), slug: "nba".into() }];
        assert!(matches!(
            PolymarketClient::categorize_from_tags(&sports_tags, "Some question"),
            MarketCategory::Sports
        ));
    }

    #[test]
    fn test_convert_market_empty_condition() {
        let gm = GammaMarket {
            id: Some(1),
            question: "Test?".into(),
            description: String::new(),
            condition_id: String::new(), // empty!
            slug: "test".into(),
            end_date: None,
            active: true,
            closed: false,
            outcome_prices: Some("[\"0.5\",\"0.5\"]".into()),
            clob_token_ids: None,
            volume: Some(5000.0),
            volume_num: None,
            liquidity: Some(2000.0),
            tags: None,
            best_bid: None,
            best_ask: None,
            spread: None,
            last_trade_price: None,
        };
        assert!(PolymarketClient::convert_market(&gm).is_none());
    }

    #[test]
    fn test_convert_market_valid() {
        let gm = GammaMarket {
            id: Some(1),
            question: "Will Bitcoin hit $100k?".into(),
            description: "Resolves YES if...".into(),
            condition_id: "0xabc123".into(),
            slug: "bitcoin-100k".into(),
            end_date: Some("2026-12-31".into()),
            active: true,
            closed: false,
            outcome_prices: Some("[\"0.72\",\"0.28\"]".into()),
            clob_token_ids: Some("[\"token1\",\"token2\"]".into()),
            volume: Some(50000.0),
            volume_num: None,
            liquidity: Some(10000.0),
            tags: Some(vec![GammaTag { label: "Crypto".into(), slug: "crypto".into() }]),
            best_bid: Some(0.71),
            best_ask: Some(0.73),
            spread: Some(0.02),
            last_trade_price: Some(0.72),
        };

        let market = PolymarketClient::convert_market(&gm).unwrap();
        assert_eq!(market.id, "0xabc123");
        assert_eq!(market.platform, "polymarket");
        assert!((market.current_price_yes - 0.72).abs() < 1e-10);
        assert!(matches!(market.category, MarketCategory::Economics));
        assert!(market.url.contains("bitcoin-100k"));
    }

    #[test]
    fn test_filter_markets() {
        let client = PolymarketClient {
            http: Client::new(),
            min_volume: 1000.0,
            min_liquidity: 500.0,
        };

        let markets = vec![
            Market {
                id: "good".into(),
                platform: "polymarket".into(),
                question: "Good market".into(),
                description: String::new(),
                category: MarketCategory::Politics,
                current_price_yes: 0.50,
                current_price_no: 0.50,
                volume_24h: 5000.0,
                liquidity: 2000.0,
                deadline: chrono::Utc::now() + chrono::Duration::days(30),
                resolution_criteria: String::new(),
                url: String::new(),
                cross_refs: Default::default(),
            },
            Market {
                id: "low_volume".into(),
                platform: "polymarket".into(),
                question: "Low volume".into(),
                description: String::new(),
                category: MarketCategory::Politics,
                current_price_yes: 0.50,
                current_price_no: 0.50,
                volume_24h: 100.0, // too low
                liquidity: 2000.0,
                deadline: chrono::Utc::now() + chrono::Duration::days(30),
                resolution_criteria: String::new(),
                url: String::new(),
                cross_refs: Default::default(),
            },
            Market {
                id: "nearly_resolved".into(),
                platform: "polymarket".into(),
                question: "Nearly resolved".into(),
                description: String::new(),
                category: MarketCategory::Politics,
                current_price_yes: 0.99, // too close to 1.0
                current_price_no: 0.01,
                volume_24h: 5000.0,
                liquidity: 2000.0,
                deadline: chrono::Utc::now() + chrono::Duration::days(30),
                resolution_criteria: String::new(),
                url: String::new(),
                cross_refs: Default::default(),
            },
        ];

        let filtered = client.filter_markets(markets);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, "good");
    }

    #[test]
    fn test_client_construction() {
        let client = PolymarketClient::new().unwrap();
        assert_eq!(client.name(), "polymarket");
        assert!(!client.is_executable()); // until CLOB is wired
    }
}
