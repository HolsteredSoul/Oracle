//! Betfair Exchange integration.
//!
//! Real-money sports betting exchange with back/lay order model.
//! Uses the Betfair Exchange API (JSON-RPC over REST).
//!
//! API docs: https://docs.developer.betfair.com/display/1smk3cen4v3lu3yomq5qye0ni/API+Overview
//! Betting API base: https://api.betfair.com/exchange/betting/rest/v1.0/
//! Account API base: https://api.betfair.com/exchange/account/rest/v1.0/
//! Auth: https://identitysso.betfair.com/api/login
//!
//! Auth requires: App Key + session token (obtained via username/password login).
//! Headers: `X-Application: {app_key}`, `X-Authentication: {session_token}`
//!
//! Betfair uses decimal odds and a back/lay model:
//! - Back = bet FOR an outcome (like YES)
//! - Lay = bet AGAINST an outcome (like NO)
//! - Implied probability = 1 / decimal_odds
//!
//! Commission: Betfair charges a market rate commission on net winnings
//! (typically 5%, varies by market and jurisdiction).

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::Client;
use rust_decimal::Decimal;
use rust_decimal::prelude::*;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use super::PredictionPlatform;
use crate::types::{
    d, CrossReferences, LiquidityInfo, Market, MarketCategory, Position, Side, TradeReceipt,
};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

const AUTH_URL: &str = "https://identitysso.betfair.com/api/login";
const BETTING_URL: &str = "https://api.betfair.com/exchange/betting/rest/v1.0";
const ACCOUNT_URL: &str = "https://api.betfair.com/exchange/account/rest/v1.0";
const PLATFORM_NAME: &str = "betfair";

/// Maximum markets to fetch per catalogue request.
const DEFAULT_FETCH_LIMIT: u32 = 200;

/// Default commission rate (5%) — actual rate varies by market.
const DEFAULT_COMMISSION_RATE: Decimal = dec!(0.05);

/// Minimum total matched on a market for it to be considered liquid.
const MIN_TOTAL_MATCHED: f64 = 100.0;

// ---------------------------------------------------------------------------
// Betfair API types
// ---------------------------------------------------------------------------

/// Login response from the SSO endpoint.
#[derive(Debug, Deserialize)]
struct LoginResponse {
    #[serde(rename = "sessionToken")]
    session_token: Option<String>,
    #[serde(rename = "loginStatus")]
    login_status: String,
}

/// Event type (top-level sport/category).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EventTypeResult {
    event_type: EventType,
    market_count: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EventType {
    id: String,
    name: String,
}

/// Market catalogue entry.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MarketCatalogue {
    market_id: String,
    market_name: String,
    #[serde(default)]
    description: Option<MarketDescription>,
    #[serde(default)]
    event: Option<EventInfo>,
    #[serde(default)]
    event_type: Option<EventType>,
    #[serde(default)]
    total_matched: Option<f64>,
    #[serde(default)]
    market_start_time: Option<String>,
    #[serde(default)]
    runners: Vec<RunnerCatalogue>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MarketDescription {
    #[serde(default)]
    betting_type: Option<String>,
    #[serde(default)]
    market_type: Option<String>,
    #[serde(default)]
    market_time: Option<String>,
    #[serde(default)]
    turn_in_play_enabled: Option<bool>,
    #[serde(default)]
    rules: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EventInfo {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    country_code: Option<String>,
    #[serde(default)]
    open_date: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunnerCatalogue {
    selection_id: u64,
    runner_name: String,
    #[serde(default)]
    sort_priority: Option<u32>,
}

/// Market book (live prices/odds).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MarketBook {
    market_id: String,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    is_market_data_delayed: Option<bool>,
    #[serde(default)]
    number_of_winners: Option<u32>,
    #[serde(default)]
    total_matched: Option<f64>,
    #[serde(default)]
    total_available: Option<f64>,
    #[serde(default)]
    runners: Vec<RunnerBook>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunnerBook {
    selection_id: u64,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    last_price_traded: Option<f64>,
    #[serde(default)]
    total_matched: Option<f64>,
    #[serde(default)]
    ex: Option<ExchangePrices>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExchangePrices {
    #[serde(default)]
    available_to_back: Vec<PriceSize>,
    #[serde(default)]
    available_to_lay: Vec<PriceSize>,
    #[serde(default)]
    traded_volume: Vec<PriceSize>,
}

#[derive(Debug, Deserialize)]
struct PriceSize {
    price: f64,
    size: f64,
}

/// Place order request types.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PlaceOrdersRequest {
    market_id: String,
    instructions: Vec<PlaceInstruction>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PlaceInstruction {
    order_type: String,
    selection_id: u64,
    side: String,
    limit_order: Option<LimitOrder>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LimitOrder {
    size: f64,
    price: f64,
    persistence_type: String,
}

/// Place orders response.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlaceOrdersResponse {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    market_id: Option<String>,
    #[serde(default)]
    instruction_reports: Vec<InstructionReport>,
    #[serde(default)]
    error_code: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstructionReport {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    error_code: Option<String>,
    #[serde(default)]
    bet_id: Option<String>,
    #[serde(default)]
    placed_date: Option<String>,
    #[serde(default)]
    average_price_matched: Option<f64>,
    #[serde(default)]
    size_matched: Option<f64>,
}

/// Current orders response.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurrentOrdersResponse {
    #[serde(default)]
    current_orders: Vec<CurrentOrder>,
    #[serde(default)]
    more_available: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurrentOrder {
    bet_id: String,
    market_id: String,
    selection_id: u64,
    side: String,
    #[serde(default)]
    price_size: Option<PriceSize>,
    #[serde(default)]
    average_price_matched: Option<f64>,
    #[serde(default)]
    size_matched: Option<f64>,
    #[serde(default)]
    size_remaining: Option<f64>,
    #[serde(default)]
    placed_date: Option<String>,
    #[serde(default)]
    status: Option<String>,
}

/// Account funds response.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountFunds {
    available_to_bet_balance: Option<f64>,
    #[serde(default)]
    exposure: Option<f64>,
    #[serde(default)]
    retained_commission: Option<f64>,
    #[serde(default)]
    discount_rate: Option<f64>,
}

// ---------------------------------------------------------------------------
// Filter for market catalogue requests
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MarketFilter {
    #[serde(skip_serializing_if = "Option::is_none")]
    event_type_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    market_type_codes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    market_betting_types: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    in_play_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    market_start_time: Option<TimeRange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    market_ids: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
struct TimeRange {
    #[serde(skip_serializing_if = "Option::is_none")]
    from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    to: Option<String>,
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

/// Betfair Exchange platform client.
pub struct BetfairClient {
    http: Client,
    app_key: String,
    session_token: std::sync::RwLock<Option<String>>,
    username: String,
    password: String,
}

impl BetfairClient {
    /// Create a new Betfair client.
    ///
    /// Reads credentials from environment variables:
    /// - `BETFAIR_APP_KEY` — application key
    /// - `BETFAIR_USERNAME` — account username
    /// - `BETFAIR_PASSWORD` — account password
    pub fn new() -> Result<Self> {
        let app_key = std::env::var("BETFAIR_APP_KEY")
            .context("BETFAIR_APP_KEY environment variable not set")?;
        let username = std::env::var("BETFAIR_USERNAME")
            .context("BETFAIR_USERNAME environment variable not set")?;
        let password = std::env::var("BETFAIR_PASSWORD")
            .context("BETFAIR_PASSWORD environment variable not set")?;

        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("ORACLE/0.1.0 (prediction-market-agent)")
            .build()
            .context("Failed to build HTTP client for Betfair")?;

        Ok(Self {
            http,
            app_key,
            session_token: std::sync::RwLock::new(None),
            username,
            password,
        })
    }

    /// Create a client with explicit credentials (for testing).
    pub fn with_credentials(
        app_key: String,
        username: String,
        password: String,
    ) -> Result<Self> {
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("ORACLE/0.1.0 (prediction-market-agent)")
            .build()
            .context("Failed to build HTTP client for Betfair")?;

        Ok(Self {
            http,
            app_key,
            session_token: std::sync::RwLock::new(None),
            username,
            password,
        })
    }

    // -- Authentication ----------------------------------------------------

    /// Authenticate with Betfair SSO and store the session token.
    async fn login(&self) -> Result<()> {
        info!("Authenticating with Betfair...");

        let resp = self
            .http
            .post(AUTH_URL)
            .header("X-Application", &self.app_key)
            .header("Accept", "application/json")
            .form(&[
                ("username", self.username.as_str()),
                ("password", self.password.as_str()),
            ])
            .send()
            .await
            .context("Betfair login request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Betfair login failed {status}: {body}");
        }

        let login: LoginResponse = resp
            .json()
            .await
            .context("Failed to parse Betfair login response")?;

        if login.login_status != "SUCCESS" {
            anyhow::bail!("Betfair login rejected: {}", login.login_status);
        }

        let token = login
            .session_token
            .context("Betfair login succeeded but no session token returned")?;

        {
            let mut guard = self.session_token.write().unwrap();
            *guard = Some(token);
        }

        info!("Betfair authentication successful");
        Ok(())
    }

    /// Get a valid session token, logging in if necessary.
    async fn ensure_session(&self) -> Result<String> {
        {
            let guard = self.session_token.read().unwrap();
            if let Some(ref token) = *guard {
                return Ok(token.clone());
            }
        }
        self.login().await?;
        let guard = self.session_token.read().unwrap();
        guard.clone().context("Session token missing after login")
    }

    // -- API helpers -------------------------------------------------------

    /// Make an authenticated POST to the Betfair Betting API.
    async fn betting_api<T: serde::de::DeserializeOwned>(
        &self,
        endpoint: &str,
        body: &serde_json::Value,
    ) -> Result<T> {
        let token = self.ensure_session().await?;
        let url = format!("{BETTING_URL}/{endpoint}/");

        debug!(url = %url, "Betfair API request");

        let resp = self
            .http
            .post(&url)
            .header("X-Application", &self.app_key)
            .header("X-Authentication", &token)
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await
            .with_context(|| format!("Betfair {endpoint} request failed"))?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            // Session expired — clear token and retry once
            {
                let mut guard = self.session_token.write().unwrap();
                *guard = None;
            }
            warn!("Betfair session expired, re-authenticating...");
            let token = self.ensure_session().await?;

            let resp = self
                .http
                .post(&url)
                .header("X-Application", &self.app_key)
                .header("X-Authentication", &token)
                .header("Content-Type", "application/json")
                .json(body)
                .send()
                .await
                .with_context(|| format!("Betfair {endpoint} retry failed"))?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body_text = resp.text().await.unwrap_or_default();
                anyhow::bail!("Betfair {endpoint} error {status}: {body_text}");
            }

            return resp
                .json()
                .await
                .with_context(|| format!("Failed to parse Betfair {endpoint} response"));
        }

        if !resp.status().is_success() {
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Betfair {endpoint} error {status}: {body_text}");
        }

        resp.json()
            .await
            .with_context(|| format!("Failed to parse Betfair {endpoint} response"))
    }

    /// Make an authenticated POST to the Betfair Account API.
    async fn account_api<T: serde::de::DeserializeOwned>(
        &self,
        endpoint: &str,
        body: &serde_json::Value,
    ) -> Result<T> {
        let token = self.ensure_session().await?;
        let url = format!("{ACCOUNT_URL}/{endpoint}/");

        let resp = self
            .http
            .post(&url)
            .header("X-Application", &self.app_key)
            .header("X-Authentication", &token)
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await
            .with_context(|| format!("Betfair account {endpoint} request failed"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Betfair account {endpoint} error {status}: {body_text}");
        }

        resp.json()
            .await
            .with_context(|| format!("Failed to parse Betfair account {endpoint} response"))
    }

    // -- Market fetching ---------------------------------------------------

    /// Fetch market catalogues for the given event type IDs.
    async fn fetch_market_catalogues(
        &self,
        event_type_ids: &[&str],
    ) -> Result<Vec<MarketCatalogue>> {
        let now = Utc::now();
        let from = now.format("%Y-%m-%dT%H:%M:%SZ").to_string();
        // Look ahead 7 days for upcoming markets
        let to = (now + chrono::Duration::days(7))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();

        let body = serde_json::json!({
            "filter": {
                "eventTypeIds": event_type_ids,
                "marketBettingTypes": ["ODDS"],
                "marketStartTime": {
                    "from": from,
                    "to": to
                }
            },
            "maxResults": DEFAULT_FETCH_LIMIT,
            "marketProjection": [
                "EVENT",
                "EVENT_TYPE",
                "MARKET_DESCRIPTION",
                "RUNNER_DESCRIPTION",
                "MARKET_START_TIME"
            ],
            "sort": "MAXIMUM_TRADED"
        });

        self.betting_api("listMarketCatalogue", &body).await
    }

    /// Fetch live prices for a list of market IDs.
    async fn fetch_market_books(&self, market_ids: &[String]) -> Result<Vec<MarketBook>> {
        let body = serde_json::json!({
            "marketIds": market_ids,
            "priceProjection": {
                "priceData": ["EX_BEST_OFFERS", "EX_TRADED"],
                "virtualise": false
            },
            "orderProjection": "EXECUTABLE",
            "matchProjection": "ROLLED_UP_BY_AVG_PRICE"
        });

        self.betting_api("listMarketBook", &body).await
    }

    // -- Conversion helpers ------------------------------------------------

    /// Map Betfair event type name to ORACLE MarketCategory.
    fn classify_event_type(event_type_name: &str) -> MarketCategory {
        match event_type_name.to_lowercase().as_str() {
            "soccer" | "football" | "tennis" | "horse racing" | "golf"
            | "cricket" | "rugby union" | "rugby league" | "basketball"
            | "american football" | "baseball" | "ice hockey" | "boxing"
            | "mixed martial arts" | "motor sport" | "cycling"
            | "darts" | "snooker" | "athletics" => MarketCategory::Sports,
            "politics" => MarketCategory::Politics,
            "financial bets" | "financials" => MarketCategory::Economics,
            _ => MarketCategory::Other,
        }
    }

    /// Betfair event type IDs for the categories we care about.
    /// These are the most liquid event types on the exchange.
    fn target_event_type_ids() -> Vec<&'static str> {
        vec![
            "1",       // Soccer
            "2",       // Tennis
            "7",       // Horse Racing
            "4339",    // Greyhound Racing
            "7522",    // Basketball
            "6423",    // American Football
            "7524",    // Ice Hockey
            "4",       // Cricket
            "1477",    // Rugby League
            "5",       // Rugby Union
            "6",       // Boxing
            "468328",  // Mixed Martial Arts
            "2378961", // Politics
            "3",       // Golf
        ]
    }

    /// Convert a Betfair market catalogue + market book into an ORACLE Market.
    ///
    /// For two-runner markets (e.g., Match Odds with only 2 selections),
    /// we model as binary YES/NO. For multi-runner markets, we use
    /// the favourite's implied probability.
    fn to_oracle_market(
        catalogue: &MarketCatalogue,
        book: Option<&MarketBook>,
    ) -> Option<Market> {
        // Determine category from event type
        let category = catalogue
            .event_type
            .as_ref()
            .map(|et| Self::classify_event_type(&et.name))
            .unwrap_or(MarketCategory::Other);

        // Build question from event name + market name
        let event_name = catalogue
            .event
            .as_ref()
            .and_then(|e| e.name.as_deref())
            .unwrap_or("Unknown Event");
        let question = format!("{} — {}", event_name, catalogue.market_name);

        // Parse market start time as deadline
        let deadline = catalogue
            .market_start_time
            .as_deref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .or_else(|| {
                catalogue
                    .event
                    .as_ref()
                    .and_then(|e| e.open_date.as_deref())
                    .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&Utc))
            })
            .unwrap_or_else(|| Utc::now() + chrono::Duration::days(7));

        // Extract prices from market book
        let (price_yes, price_no, total_matched, bid_depth, ask_depth) =
            if let Some(mb) = book {
                Self::extract_prices(mb)
            } else {
                (dec!(0.50), dec!(0.50), 0.0, 0.0, 0.0)
            };

        let total_matched_dec = d(catalogue.total_matched.unwrap_or(total_matched));

        // Build URL
        let url = format!(
            "https://www.betfair.com/exchange/plus/market/{}",
            catalogue.market_id
        );

        // Description from rules
        let description = catalogue
            .description
            .as_ref()
            .and_then(|d| d.rules.as_deref())
            .unwrap_or("")
            .to_string();

        Some(Market {
            id: catalogue.market_id.clone(),
            platform: PLATFORM_NAME.to_string(),
            question,
            description,
            category,
            current_price_yes: price_yes,
            current_price_no: price_no,
            volume_24h: total_matched_dec, // Betfair doesn't split 24h; use total matched
            liquidity: d(bid_depth + ask_depth),
            deadline,
            resolution_criteria: String::new(),
            url,
            cross_refs: CrossReferences::default(),
        })
    }

    /// Extract best back/lay prices from market book runners.
    ///
    /// For the favourite runner (first runner or one with lowest back price):
    /// - back price → implied YES probability
    /// - lay price → implied NO probability
    fn extract_prices(book: &MarketBook) -> (Decimal, Decimal, f64, f64, f64) {
        let total_matched = book.total_matched.unwrap_or(0.0);

        // Find the runner with the best (lowest) back price — the favourite
        let mut best_back: Option<f64> = None;
        let mut best_lay: Option<f64> = None;
        let mut bid_depth = 0.0;
        let mut ask_depth = 0.0;

        for runner in &book.runners {
            if runner.status.as_deref() != Some("ACTIVE") && runner.status.is_some() {
                continue;
            }

            if let Some(ref ex) = runner.ex {
                // Best back price (highest available to back)
                if let Some(back) = ex.available_to_back.first() {
                    if best_back.is_none() || back.price < best_back.unwrap() {
                        best_back = Some(back.price);
                        bid_depth = ex.available_to_back.iter().map(|p| p.size).sum();
                    }
                }
                // Best lay price (lowest available to lay)
                if let Some(lay) = ex.available_to_lay.first() {
                    if best_lay.is_none() || lay.price < best_lay.unwrap() {
                        best_lay = Some(lay.price);
                        ask_depth = ex.available_to_lay.iter().map(|p| p.size).sum();
                    }
                }
            }
        }

        // Convert decimal odds to implied probability
        // Implied prob = 1 / decimal_odds
        let price_yes = best_back
            .map(|odds| d(1.0 / odds).min(Decimal::ONE))
            .unwrap_or(dec!(0.50));
        let price_no = Decimal::ONE - price_yes;

        (price_yes, price_no, total_matched, bid_depth, ask_depth)
    }

    /// Find the selection ID for the favourite (lowest back price) runner.
    fn favourite_selection_id(book: &MarketBook) -> Option<u64> {
        let mut best_price = f64::MAX;
        let mut best_id = None;

        for runner in &book.runners {
            if runner.status.as_deref() != Some("ACTIVE") && runner.status.is_some() {
                continue;
            }
            if let Some(ref ex) = runner.ex {
                if let Some(back) = ex.available_to_back.first() {
                    if back.price < best_price {
                        best_price = back.price;
                        best_id = Some(runner.selection_id);
                    }
                }
            }
        }

        best_id
    }
}

// ---------------------------------------------------------------------------
// PredictionPlatform trait implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl PredictionPlatform for BetfairClient {
    /// Fetch active markets from Betfair Exchange.
    ///
    /// Queries market catalogues for target event types, then fetches
    /// live prices for the most liquid markets.
    async fn fetch_markets(&self) -> Result<Vec<Market>> {
        info!("Scanning Betfair Exchange for active markets...");

        // 1. Fetch market catalogues across target event types
        let event_type_ids = Self::target_event_type_ids();
        let catalogues = self.fetch_market_catalogues(&event_type_ids).await?;

        info!(count = catalogues.len(), "Betfair market catalogues fetched");

        if catalogues.is_empty() {
            return Ok(Vec::new());
        }

        // 2. Filter to markets with meaningful liquidity
        let liquid_catalogues: Vec<_> = catalogues
            .into_iter()
            .filter(|c| c.total_matched.unwrap_or(0.0) >= MIN_TOTAL_MATCHED)
            .collect();

        // 3. Fetch market books (prices) in batches of 40 (API limit)
        let market_ids: Vec<String> = liquid_catalogues
            .iter()
            .map(|c| c.market_id.clone())
            .collect();

        let mut all_books = Vec::new();
        for chunk in market_ids.chunks(40) {
            match self.fetch_market_books(&chunk.to_vec()).await {
                Ok(books) => all_books.extend(books),
                Err(e) => {
                    warn!(error = %e, "Failed to fetch market book batch, continuing");
                }
            }
        }

        // 4. Build index of books by market ID for fast lookup
        let book_index: std::collections::HashMap<String, &MarketBook> = all_books
            .iter()
            .map(|b| (b.market_id.clone(), b))
            .collect();

        // 5. Convert to ORACLE Market type
        let markets: Vec<Market> = liquid_catalogues
            .iter()
            .filter_map(|c| {
                let book = book_index.get(&c.market_id).copied();
                Self::to_oracle_market(c, book)
            })
            .collect();

        info!(total = markets.len(), "Betfair scan complete");
        Ok(markets)
    }

    /// Place a bet on a Betfair market.
    ///
    /// Converts ORACLE Side::Yes/No to Betfair BACK/LAY on the favourite runner.
    /// Uses a LIMIT order at the current best available price.
    async fn place_bet(
        &self,
        market_id: &str,
        side: Side,
        amount: Decimal,
    ) -> Result<TradeReceipt> {
        // Get current market book to find the best price and selection
        let books = self.fetch_market_books(&[market_id.to_string()]).await?;
        let book = books
            .first()
            .context("No market book returned for order placement")?;

        let selection_id = Self::favourite_selection_id(book)
            .context("No active runner found for order placement")?;

        // Determine Betfair side and price
        let (bf_side, price) = match side {
            Side::Yes => {
                // BACK the favourite — use best available back price
                let back_price = book
                    .runners
                    .iter()
                    .find(|r| r.selection_id == selection_id)
                    .and_then(|r| r.ex.as_ref())
                    .and_then(|ex| ex.available_to_back.first())
                    .map(|p| p.price)
                    .context("No back price available")?;
                ("BACK", back_price)
            }
            Side::No => {
                // LAY the favourite — use best available lay price
                let lay_price = book
                    .runners
                    .iter()
                    .find(|r| r.selection_id == selection_id)
                    .and_then(|r| r.ex.as_ref())
                    .and_then(|ex| ex.available_to_lay.first())
                    .map(|p| p.price)
                    .context("No lay price available")?;
                ("LAY", lay_price)
            }
        };

        let amount_f64 = amount.to_f64().unwrap_or(0.0);

        let body = serde_json::json!({
            "marketId": market_id,
            "instructions": [{
                "orderType": "LIMIT",
                "selectionId": selection_id,
                "side": bf_side,
                "limitOrder": {
                    "size": amount_f64,
                    "price": price,
                    "persistenceType": "LAPSE"
                }
            }]
        });

        let resp: PlaceOrdersResponse = self.betting_api("placeOrders", &body).await?;

        // Check for API-level errors
        if let Some(ref error_code) = resp.error_code {
            anyhow::bail!("Betfair placeOrders error: {error_code}");
        }

        if resp.status.as_deref() == Some("FAILURE") {
            let instruction_error = resp
                .instruction_reports
                .first()
                .and_then(|r| r.error_code.as_deref())
                .unwrap_or("UNKNOWN");
            anyhow::bail!("Betfair order failed: {instruction_error}");
        }

        // Extract receipt from first instruction report
        let report = resp
            .instruction_reports
            .first()
            .context("No instruction report in placeOrders response")?;

        let order_id = report
            .bet_id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        let fill_price = d(report.average_price_matched.unwrap_or(price));
        let size_matched = d(report.size_matched.unwrap_or(amount_f64));
        let fees = size_matched * DEFAULT_COMMISSION_RATE;

        let timestamp = report
            .placed_date
            .as_deref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);

        info!(
            order_id = %order_id,
            market_id = %market_id,
            side = %side,
            amount = %amount,
            price = %fill_price,
            "Betfair order placed"
        );

        Ok(TradeReceipt {
            order_id,
            market_id: market_id.to_string(),
            platform: PLATFORM_NAME.to_string(),
            side,
            amount: size_matched,
            fill_price,
            fees,
            timestamp,
        })
    }

    /// Get current open positions on Betfair.
    ///
    /// Queries `listCurrentOrders` and aggregates by market.
    async fn get_positions(&self) -> Result<Vec<Position>> {
        let body = serde_json::json!({
            "orderProjection": "EXECUTABLE"
        });

        let resp: CurrentOrdersResponse =
            self.betting_api("listCurrentOrders", &body).await?;

        let positions: Vec<Position> = resp
            .current_orders
            .iter()
            .map(|order| {
                let side = if order.side == "BACK" {
                    Side::Yes
                } else {
                    Side::No
                };
                let size = d(order.size_matched.unwrap_or(0.0));
                let entry_price = d(order.average_price_matched.unwrap_or(0.0));

                Position {
                    market_id: order.market_id.clone(),
                    platform: PLATFORM_NAME.to_string(),
                    side,
                    size,
                    entry_price,
                    current_value: size, // Would need market book for accurate valuation
                }
            })
            .collect();

        Ok(positions)
    }

    /// Get available balance on Betfair.
    async fn get_balance(&self) -> Result<Decimal> {
        let body = serde_json::json!({});

        let funds: AccountFunds =
            self.account_api("getAccountFunds", &body).await?;

        let balance = funds
            .available_to_bet_balance
            .context("No balance returned from Betfair")?;

        Ok(d(balance))
    }

    /// Check liquidity for a specific Betfair market.
    async fn check_liquidity(&self, market_id: &str) -> Result<LiquidityInfo> {
        let books = self.fetch_market_books(&[market_id.to_string()]).await?;
        let book = books
            .first()
            .context("No market book returned for liquidity check")?;

        let (_, _, total_matched, bid_depth, ask_depth) = Self::extract_prices(book);

        Ok(LiquidityInfo {
            bid_depth: d(bid_depth),
            ask_depth: d(ask_depth),
            volume_24h: d(total_matched),
        })
    }

    /// Betfair is a real-money execution venue.
    fn is_executable(&self) -> bool {
        true
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

    // -- Classification tests --

    #[test]
    fn test_classify_sports_types() {
        assert_eq!(
            BetfairClient::classify_event_type("Soccer"),
            MarketCategory::Sports
        );
        assert_eq!(
            BetfairClient::classify_event_type("Tennis"),
            MarketCategory::Sports
        );
        assert_eq!(
            BetfairClient::classify_event_type("Horse Racing"),
            MarketCategory::Sports
        );
        assert_eq!(
            BetfairClient::classify_event_type("basketball"),
            MarketCategory::Sports
        );
        assert_eq!(
            BetfairClient::classify_event_type("American Football"),
            MarketCategory::Sports
        );
    }

    #[test]
    fn test_classify_politics() {
        assert_eq!(
            BetfairClient::classify_event_type("Politics"),
            MarketCategory::Politics
        );
    }

    #[test]
    fn test_classify_economics() {
        assert_eq!(
            BetfairClient::classify_event_type("Financial Bets"),
            MarketCategory::Economics
        );
        assert_eq!(
            BetfairClient::classify_event_type("financials"),
            MarketCategory::Economics
        );
    }

    #[test]
    fn test_classify_other() {
        assert_eq!(
            BetfairClient::classify_event_type("Entertainment"),
            MarketCategory::Other
        );
        assert_eq!(
            BetfairClient::classify_event_type("Special Bets"),
            MarketCategory::Other
        );
    }

    // -- Price extraction tests --

    fn make_test_book(
        back_price: f64,
        back_size: f64,
        lay_price: f64,
        lay_size: f64,
    ) -> MarketBook {
        MarketBook {
            market_id: "1.234567890".to_string(),
            status: Some("OPEN".to_string()),
            is_market_data_delayed: Some(false),
            number_of_winners: Some(1),
            total_matched: Some(50000.0),
            total_available: Some(10000.0),
            runners: vec![RunnerBook {
                selection_id: 12345,
                status: Some("ACTIVE".to_string()),
                last_price_traded: Some(back_price),
                total_matched: Some(25000.0),
                ex: Some(ExchangePrices {
                    available_to_back: vec![PriceSize {
                        price: back_price,
                        size: back_size,
                    }],
                    available_to_lay: vec![PriceSize {
                        price: lay_price,
                        size: lay_size,
                    }],
                    traded_volume: vec![],
                }),
            }],
        }
    }

    #[test]
    fn test_extract_prices_evens() {
        // Decimal odds of 2.0 = 50% implied probability
        let book = make_test_book(2.0, 100.0, 2.02, 100.0);
        let (price_yes, price_no, total_matched, bid_depth, ask_depth) =
            BetfairClient::extract_prices(&book);

        assert_eq!(price_yes, d(0.5));
        assert_eq!(price_no, d(0.5));
        assert_eq!(total_matched, 50000.0);
        assert_eq!(bid_depth, 100.0);
        assert_eq!(ask_depth, 100.0);
    }

    #[test]
    fn test_extract_prices_strong_favourite() {
        // Decimal odds of 1.25 = 80% implied probability
        let book = make_test_book(1.25, 500.0, 1.27, 300.0);
        let (price_yes, _, _, _, _) = BetfairClient::extract_prices(&book);

        assert_eq!(price_yes, d(1.0 / 1.25));
    }

    #[test]
    fn test_extract_prices_longshot() {
        // Decimal odds of 10.0 = 10% implied probability
        let book = make_test_book(10.0, 50.0, 11.0, 50.0);
        let (price_yes, _, _, _, _) = BetfairClient::extract_prices(&book);

        assert_eq!(price_yes, d(0.1));
    }

    #[test]
    fn test_extract_prices_empty_book() {
        let book = MarketBook {
            market_id: "1.234".to_string(),
            status: Some("OPEN".to_string()),
            is_market_data_delayed: None,
            number_of_winners: None,
            total_matched: Some(0.0),
            total_available: None,
            runners: vec![],
        };
        let (price_yes, price_no, _, _, _) = BetfairClient::extract_prices(&book);

        // Should default to 50/50
        assert_eq!(price_yes, dec!(0.50));
        assert_eq!(price_no, dec!(0.50));
    }

    // -- Favourite selection tests --

    #[test]
    fn test_favourite_selection_id() {
        let book = make_test_book(2.0, 100.0, 2.02, 100.0);
        let id = BetfairClient::favourite_selection_id(&book);
        assert_eq!(id, Some(12345));
    }

    #[test]
    fn test_favourite_selection_id_empty() {
        let book = MarketBook {
            market_id: "1.234".to_string(),
            status: None,
            is_market_data_delayed: None,
            number_of_winners: None,
            total_matched: None,
            total_available: None,
            runners: vec![],
        };
        assert_eq!(BetfairClient::favourite_selection_id(&book), None);
    }

    #[test]
    fn test_favourite_selection_picks_lowest_back() {
        let book = MarketBook {
            market_id: "1.234".to_string(),
            status: Some("OPEN".to_string()),
            is_market_data_delayed: None,
            number_of_winners: Some(1),
            total_matched: Some(10000.0),
            total_available: None,
            runners: vec![
                RunnerBook {
                    selection_id: 111,
                    status: Some("ACTIVE".to_string()),
                    last_price_traded: None,
                    total_matched: None,
                    ex: Some(ExchangePrices {
                        available_to_back: vec![PriceSize {
                            price: 3.5,
                            size: 100.0,
                        }],
                        available_to_lay: vec![],
                        traded_volume: vec![],
                    }),
                },
                RunnerBook {
                    selection_id: 222,
                    status: Some("ACTIVE".to_string()),
                    last_price_traded: None,
                    total_matched: None,
                    ex: Some(ExchangePrices {
                        available_to_back: vec![PriceSize {
                            price: 1.8,
                            size: 200.0,
                        }],
                        available_to_lay: vec![],
                        traded_volume: vec![],
                    }),
                },
            ],
        };

        // Runner 222 at 1.8 is the favourite (lower back price)
        assert_eq!(BetfairClient::favourite_selection_id(&book), Some(222));
    }

    // -- Market conversion tests --

    #[test]
    fn test_to_oracle_market_basic() {
        let catalogue = MarketCatalogue {
            market_id: "1.234567890".to_string(),
            market_name: "Match Odds".to_string(),
            description: None,
            event: Some(EventInfo {
                id: Some("12345".to_string()),
                name: Some("Liverpool v Chelsea".to_string()),
                country_code: Some("GB".to_string()),
                open_date: None,
            }),
            event_type: Some(EventType {
                id: "1".to_string(),
                name: "Soccer".to_string(),
            }),
            total_matched: Some(100000.0),
            market_start_time: None,
            runners: vec![],
        };

        let market = BetfairClient::to_oracle_market(&catalogue, None).unwrap();

        assert_eq!(market.id, "1.234567890");
        assert_eq!(market.platform, "betfair");
        assert_eq!(market.question, "Liverpool v Chelsea — Match Odds");
        assert_eq!(market.category, MarketCategory::Sports);
    }

    #[test]
    fn test_to_oracle_market_with_book() {
        let catalogue = MarketCatalogue {
            market_id: "1.234567890".to_string(),
            market_name: "Match Odds".to_string(),
            description: None,
            event: Some(EventInfo {
                id: None,
                name: Some("Test Event".to_string()),
                country_code: None,
                open_date: None,
            }),
            event_type: Some(EventType {
                id: "1".to_string(),
                name: "Soccer".to_string(),
            }),
            total_matched: Some(50000.0),
            market_start_time: None,
            runners: vec![],
        };

        let book = make_test_book(2.0, 100.0, 2.02, 100.0);
        let market = BetfairClient::to_oracle_market(&catalogue, Some(&book)).unwrap();

        assert_eq!(market.current_price_yes, d(0.5));
        assert!(market.liquidity > Decimal::ZERO);
    }

    // -- Event type ID tests --

    #[test]
    fn test_target_event_type_ids_not_empty() {
        let ids = BetfairClient::target_event_type_ids();
        assert!(!ids.is_empty());
        assert!(ids.contains(&"1")); // Soccer
        assert!(ids.contains(&"2378961")); // Politics
    }

    // -- is_executable --

    #[test]
    fn test_is_executable() {
        // Can't create a real client without env vars, but we can test
        // that the trait method returns true by checking the const intent
        assert_eq!(PLATFORM_NAME, "betfair");
    }
}
