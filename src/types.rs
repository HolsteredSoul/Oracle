//! Shared types for the ORACLE agent.
//!
//! These types form the data model used across all modules.
//! They are designed to be stable so that platform, strategy,
//! and engine modules can depend on them without circular references.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

// ---------------------------------------------------------------------------
// Market
// ---------------------------------------------------------------------------

/// A prediction market on any platform.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Market {
    pub id: String,
    /// Platform identifier: "forecastex" | "metaculus" | "manifold"
    pub platform: String,
    pub question: String,
    pub description: String,
    pub category: MarketCategory,
    /// Current YES price (0.0–1.0)
    pub current_price_yes: f64,
    /// Current NO price (0.0–1.0)
    pub current_price_no: f64,
    /// 24-hour volume in USD equivalent
    pub volume_24h: f64,
    /// Available liquidity in USD equivalent
    pub liquidity: f64,
    /// Market resolution deadline
    pub deadline: DateTime<Utc>,
    pub resolution_criteria: String,
    pub url: String,
    /// Cross-platform probability references
    pub cross_refs: CrossReferences,
}

impl fmt::Display for Market {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {} (YES: {:.0}¢ | NO: {:.0}¢ | vol: ${:.0} | {})",
            self.platform,
            self.question,
            self.current_price_yes * 100.0,
            self.current_price_no * 100.0,
            self.volume_24h,
            self.category,
        )
    }
}

impl Market {
    /// The mid-price between YES and NO (useful as a quick reference).
    pub fn mid_price(&self) -> f64 {
        (self.current_price_yes + (1.0 - self.current_price_no)) / 2.0
    }

    /// Spread between YES and (1 - NO) prices — a measure of market efficiency.
    pub fn spread(&self) -> f64 {
        (self.current_price_yes - (1.0 - self.current_price_no)).abs()
    }

    /// Whether the market is still active (deadline in the future).
    pub fn is_active(&self) -> bool {
        self.deadline > Utc::now()
    }

    /// Time remaining until resolution deadline.
    pub fn time_remaining(&self) -> chrono::Duration {
        self.deadline - Utc::now()
    }

    /// Helper to build a test/sample market with sensible defaults.
    #[cfg(test)]
    pub fn sample() -> Self {
        Market {
            id: "test-001".to_string(),
            platform: "forecastex".to_string(),
            question: "Will CPI exceed 3% in Q1 2026?".to_string(),
            description: "Resolves YES if the BLS reports CPI > 3% for Q1 2026.".to_string(),
            category: MarketCategory::Economics,
            current_price_yes: 0.45,
            current_price_no: 0.55,
            volume_24h: 5000.0,
            liquidity: 12000.0,
            deadline: Utc::now() + chrono::Duration::days(30),
            resolution_criteria: "Based on BLS CPI report".to_string(),
            url: "https://forecastex.example.com/test-001".to_string(),
            cross_refs: CrossReferences {
                metaculus_prob: Some(0.52),
                metaculus_forecasters: Some(314),
                manifold_prob: Some(0.48),
                forecastex_price: Some(0.45),
            },
        }
    }
}

/// Cross-platform reference probabilities.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CrossReferences {
    pub metaculus_prob: Option<f64>,
    pub metaculus_forecasters: Option<u32>,
    pub manifold_prob: Option<f64>,
    pub forecastex_price: Option<f64>,
}

impl fmt::Display for CrossReferences {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::new();
        if let Some(p) = self.metaculus_prob {
            let n = self.metaculus_forecasters.unwrap_or(0);
            parts.push(format!("Metaculus: {:.0}% (n={n})", p * 100.0));
        }
        if let Some(p) = self.manifold_prob {
            parts.push(format!("Manifold: {:.0}%", p * 100.0));
        }
        if let Some(p) = self.forecastex_price {
            parts.push(format!("ForecastEx: {:.0}¢", p * 100.0));
        }
        if parts.is_empty() {
            write!(f, "No cross-references")
        } else {
            write!(f, "{}", parts.join(" | "))
        }
    }
}

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Bet direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Side {
    Yes,
    No,
}

impl Side {
    /// The opposite side.
    pub fn opposite(&self) -> Self {
        match self {
            Side::Yes => Side::No,
            Side::No => Side::Yes,
        }
    }
}

impl fmt::Display for Side {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Side::Yes => write!(f, "YES"),
            Side::No => write!(f, "NO"),
        }
    }
}

/// Market category for routing to appropriate data providers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MarketCategory {
    Weather,
    Sports,
    Economics,
    Politics,
    Culture,
    Other,
}

impl MarketCategory {
    /// All known categories (useful for iteration).
    pub const ALL: &'static [MarketCategory] = &[
        MarketCategory::Weather,
        MarketCategory::Sports,
        MarketCategory::Economics,
        MarketCategory::Politics,
        MarketCategory::Culture,
        MarketCategory::Other,
    ];
}

impl fmt::Display for MarketCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MarketCategory::Weather => write!(f, "Weather"),
            MarketCategory::Sports => write!(f, "Sports"),
            MarketCategory::Economics => write!(f, "Economics"),
            MarketCategory::Politics => write!(f, "Politics"),
            MarketCategory::Culture => write!(f, "Culture"),
            MarketCategory::Other => write!(f, "Other"),
        }
    }
}

/// Attempt to parse a string into a MarketCategory (case-insensitive).
impl std::str::FromStr for MarketCategory {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "weather" => Ok(MarketCategory::Weather),
            "sports" | "sport" => Ok(MarketCategory::Sports),
            "economics" | "economic" | "econ" => Ok(MarketCategory::Economics),
            "politics" | "political" => Ok(MarketCategory::Politics),
            "culture" | "cultural" | "entertainment" => Ok(MarketCategory::Culture),
            "other" => Ok(MarketCategory::Other),
            _ => Err(anyhow::anyhow!("Unknown market category: {s}")),
        }
    }
}

/// Agent lifecycle status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentStatus {
    Alive,
    Died,
    Paused,
}

impl fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentStatus::Alive => write!(f, "🟢 ALIVE"),
            AgentStatus::Died => write!(f, "🔴 DIED"),
            AgentStatus::Paused => write!(f, "🟡 PAUSED"),
        }
    }
}

// ---------------------------------------------------------------------------
// Trade & Position types
// ---------------------------------------------------------------------------

/// Receipt returned after a trade is executed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeReceipt {
    pub order_id: String,
    pub market_id: String,
    pub platform: String,
    pub side: Side,
    pub amount: f64,
    pub fill_price: f64,
    pub fees: f64,
    pub timestamp: DateTime<Utc>,
}

impl fmt::Display for TradeReceipt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {} {} ${:.2} @ {:.2}¢ (fees: ${:.4}) [{}]",
            self.platform,
            self.side,
            self.market_id,
            self.amount,
            self.fill_price * 100.0,
            self.fees,
            self.order_id,
        )
    }
}

impl TradeReceipt {
    /// Net cost of this trade (amount + fees).
    pub fn net_cost(&self) -> f64 {
        self.amount + self.fees
    }
}

/// An open position on a platform.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub market_id: String,
    pub platform: String,
    pub side: Side,
    pub size: f64,
    pub entry_price: f64,
    pub current_value: f64,
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let pnl = self.unrealized_pnl();
        let pnl_sign = if pnl >= 0.0 { "+" } else { "" };
        write!(
            f,
            "[{}] {} {} size={:.2} entry={:.2}¢ val=${:.2} ({pnl_sign}{pnl:.2})",
            self.platform,
            self.side,
            self.market_id,
            self.size,
            self.entry_price * 100.0,
            self.current_value,
        )
    }
}

impl Position {
    /// Unrealized P&L (current_value - size * entry_price).
    pub fn unrealized_pnl(&self) -> f64 {
        self.current_value - (self.size * self.entry_price)
    }
}

/// Order book / liquidity information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidityInfo {
    pub bid_depth: f64,
    pub ask_depth: f64,
    pub volume_24h: f64,
}

impl fmt::Display for LiquidityInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "bid_depth=${:.0} ask_depth=${:.0} vol_24h=${:.0}",
            self.bid_depth, self.ask_depth, self.volume_24h,
        )
    }
}

impl LiquidityInfo {
    /// Total depth (bid + ask).
    pub fn total_depth(&self) -> f64 {
        self.bid_depth + self.ask_depth
    }

    /// Whether this market has sufficient liquidity given a threshold.
    pub fn is_sufficient(&self, min_depth: f64) -> bool {
        self.total_depth() >= min_depth
    }
}

// ---------------------------------------------------------------------------
// Strategy types
// ---------------------------------------------------------------------------

/// A fully computed bet decision ready for execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetDecision {
    pub market: Market,
    pub side: Side,
    /// LLM fair-value estimate (probability)
    pub fair_value: f64,
    /// |fair_value - market_price|
    pub edge: f64,
    /// Raw Kelly fraction
    pub kelly_fraction: f64,
    /// Final bet amount after caps and risk limits
    pub bet_amount: f64,
    /// LLM self-reported confidence (0–1)
    pub confidence: f64,
    /// LLM reasoning summary
    pub rationale: String,
    /// List of data sources used for the estimate
    pub data_sources_used: Vec<String>,
}

impl fmt::Display for BetDecision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} | fair={:.0}% mkt={:.0}% edge={:.1}% | kelly={:.1}% bet=${:.2} | conf={:.0}%",
            self.side,
            self.market.question,
            self.fair_value * 100.0,
            match self.side {
                Side::Yes => self.market.current_price_yes,
                Side::No => self.market.current_price_no,
            } * 100.0,
            self.edge * 100.0,
            self.kelly_fraction * 100.0,
            self.bet_amount,
            self.confidence * 100.0,
        )
    }
}

impl BetDecision {
    /// Expected value of this bet: edge × bet_amount.
    pub fn expected_value(&self) -> f64 {
        self.edge * self.bet_amount
    }

    /// The market price on the side we're betting.
    pub fn market_price(&self) -> f64 {
        match self.side {
            Side::Yes => self.market.current_price_yes,
            Side::No => self.market.current_price_no,
        }
    }
}

/// LLM probability estimate for a market.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Estimate {
    pub probability: f64,
    /// LLM self-reported confidence (0–1)
    pub confidence: f64,
    /// Chain-of-thought reasoning summary
    pub reasoning: String,
    pub tokens_used: u32,
    pub cost: f64,
}

impl fmt::Display for Estimate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "P={:.1}% conf={:.0}% (tokens={} cost=${:.4})",
            self.probability * 100.0,
            self.confidence * 100.0,
            self.tokens_used,
            self.cost,
        )
    }
}

impl Estimate {
    /// Whether this estimate is within valid bounds [0.01, 0.99].
    /// Estimates outside this range indicate overconfidence.
    pub fn is_valid(&self) -> bool {
        self.probability >= 0.01 && self.probability <= 0.99
    }

    /// Whether the estimate is suspiciously close to a given market price.
    /// This may indicate the LLM is echoing the market rather than
    /// providing an independent estimate.
    pub fn is_echo(&self, market_price: f64, tolerance: f64) -> bool {
        (self.probability - market_price).abs() < tolerance
    }
}

// ---------------------------------------------------------------------------
// Data context
// ---------------------------------------------------------------------------

/// Enrichment data fetched from external APIs for a market.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataContext {
    pub category: MarketCategory,
    /// Full API response (preserved for audit)
    pub raw_data: serde_json::Value,
    /// Human-readable summary for LLM prompt
    pub summary: String,
    /// When this data was fetched
    pub freshness: DateTime<Utc>,
    /// Data source name
    pub source: String,
    /// API call cost in USD
    pub cost: f64,
    /// Cross-reference probabilities
    pub metaculus_forecast: Option<f64>,
    pub metaculus_forecasters: Option<u32>,
    pub manifold_price: Option<f64>,
}

impl fmt::Display for DataContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let age = Utc::now() - self.freshness;
        let age_str = if age.num_minutes() < 60 {
            format!("{}m ago", age.num_minutes())
        } else {
            format!("{}h ago", age.num_hours())
        };
        write!(
            f,
            "[{}] {} ({}, cost=${:.4})",
            self.category, self.source, age_str, self.cost,
        )
    }
}

impl DataContext {
    /// Whether the data is stale (older than the given duration).
    pub fn is_stale(&self, max_age: chrono::Duration) -> bool {
        Utc::now() - self.freshness > max_age
    }

    /// Build an empty/placeholder context (useful for markets with no
    /// data enrichment available).
    pub fn empty(category: MarketCategory) -> Self {
        DataContext {
            category,
            raw_data: serde_json::Value::Null,
            summary: "No enrichment data available.".to_string(),
            freshness: Utc::now(),
            source: "none".to_string(),
            cost: 0.0,
            metaculus_forecast: None,
            metaculus_forecasters: None,
            manifold_price: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Agent state
// ---------------------------------------------------------------------------

/// Persistent agent state, saved to SQLite after each cycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentState {
    pub bankroll: f64,
    pub total_pnl: f64,
    pub cycle_count: u64,
    pub trades_placed: u64,
    pub trades_won: u64,
    pub trades_lost: u64,
    pub total_api_costs: f64,
    pub total_ib_commissions: f64,
    pub start_time: DateTime<Utc>,
    pub peak_bankroll: f64,
    pub status: AgentStatus,
}

impl fmt::Display for AgentState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} | bankroll=${:.2} | PnL=${:.2} | cycles={} | trades={} (W{}/L{}) | win_rate={:.1}% | drawdown={:.1}% | costs=${:.2}",
            self.status,
            self.bankroll,
            self.total_pnl,
            self.cycle_count,
            self.trades_placed,
            self.trades_won,
            self.trades_lost,
            self.win_rate(),
            self.drawdown() * 100.0,
            self.total_costs(),
        )
    }
}

impl AgentState {
    /// Create a new agent state with the given initial bankroll.
    pub fn new(initial_bankroll: f64) -> Self {
        Self {
            bankroll: initial_bankroll,
            total_pnl: 0.0,
            cycle_count: 0,
            trades_placed: 0,
            trades_won: 0,
            trades_lost: 0,
            total_api_costs: 0.0,
            total_ib_commissions: 0.0,
            start_time: Utc::now(),
            peak_bankroll: initial_bankroll,
            status: AgentStatus::Alive,
        }
    }

    /// Win rate as a percentage. Returns 0.0 if no resolved trades.
    pub fn win_rate(&self) -> f64 {
        let resolved = self.trades_won + self.trades_lost;
        if resolved == 0 {
            0.0
        } else {
            (self.trades_won as f64 / resolved as f64) * 100.0
        }
    }

    /// Current drawdown from peak as a fraction (0.0 = at peak).
    pub fn drawdown(&self) -> f64 {
        if self.peak_bankroll <= 0.0 {
            0.0
        } else {
            1.0 - (self.bankroll / self.peak_bankroll)
        }
    }

    /// Total combined costs (API + IB commissions).
    pub fn total_costs(&self) -> f64 {
        self.total_api_costs + self.total_ib_commissions
    }

    /// Number of resolved trades (won + lost).
    pub fn trades_resolved(&self) -> u64 {
        self.trades_won + self.trades_lost
    }

    /// Number of trades still pending resolution.
    pub fn trades_pending(&self) -> u64 {
        self.trades_placed - self.trades_resolved()
    }

    /// Whether the agent is still alive and trading.
    pub fn is_alive(&self) -> bool {
        self.status == AgentStatus::Alive
    }

    /// Update peak bankroll if current is higher.
    pub fn update_peak(&mut self) {
        if self.bankroll > self.peak_bankroll {
            self.peak_bankroll = self.bankroll;
        }
    }

    /// Deduct a cost from the bankroll and track it. Returns false if
    /// the agent has died (bankroll <= 0).
    pub fn deduct_cost(&mut self, api_cost: f64, ib_commission: f64) -> bool {
        self.total_api_costs += api_cost;
        self.total_ib_commissions += ib_commission;
        self.bankroll -= api_cost + ib_commission;
        if self.bankroll <= 0.0 {
            self.status = AgentStatus::Died;
            false
        } else {
            true
        }
    }

    /// Record a resolved trade outcome.
    pub fn record_resolution(&mut self, pnl: f64, won: bool) {
        self.total_pnl += pnl;
        self.bankroll += pnl;
        if won {
            self.trades_won += 1;
        } else {
            self.trades_lost += 1;
        }
        self.update_peak();
    }

    /// Uptime duration since agent start.
    pub fn uptime(&self) -> chrono::Duration {
        Utc::now() - self.start_time
    }
}

// ---------------------------------------------------------------------------
// Cycle report
// ---------------------------------------------------------------------------

/// Summary of a single scan-estimate-bet cycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CycleReport {
    pub cycle_number: u64,
    pub timestamp: DateTime<Utc>,
    pub markets_scanned: u64,
    pub edges_found: u64,
    pub bets_placed: u64,
    pub cycle_cost: f64,
    pub cycle_pnl: f64,
    pub bankroll_after: f64,
}

impl fmt::Display for CycleReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Cycle #{}: scanned={} edges={} bets={} cost=${:.4} pnl=${:.2} balance=${:.2}",
            self.cycle_number,
            self.markets_scanned,
            self.edges_found,
            self.bets_placed,
            self.cycle_cost,
            self.cycle_pnl,
            self.bankroll_after,
        )
    }
}

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Domain-specific error types for ORACLE.
#[derive(Debug, thiserror::Error)]
pub enum OracleError {
    #[error("Platform error ({platform}): {message}")]
    Platform { platform: String, message: String },

    #[error("LLM error ({model}): {message}")]
    Llm { model: String, message: String },

    #[error("Data provider error ({data_source}): {message}")]
    DataProvider { data_source: String, message: String },

    #[error("Strategy error: {0}")]
    Strategy(String),

    #[error("Risk limit exceeded: {0}")]
    RiskLimit(String),

    #[error("Insufficient balance: need ${needed:.2}, have ${available:.2}")]
    InsufficientBalance { needed: f64, available: f64 },

    #[error("Market not found: {0}")]
    MarketNotFound(String),

    #[error("Invalid estimate: {0}")]
    InvalidEstimate(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Storage error: {0}")]
    Storage(String),
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Side tests --

    #[test]
    fn test_side_display() {
        assert_eq!(format!("{}", Side::Yes), "YES");
        assert_eq!(format!("{}", Side::No), "NO");
    }

    #[test]
    fn test_side_opposite() {
        assert_eq!(Side::Yes.opposite(), Side::No);
        assert_eq!(Side::No.opposite(), Side::Yes);
    }

    #[test]
    fn test_side_serialization_roundtrip() {
        let yes_json = serde_json::to_string(&Side::Yes).unwrap();
        let no_json = serde_json::to_string(&Side::No).unwrap();
        assert_eq!(yes_json, "\"Yes\"");
        assert_eq!(no_json, "\"No\"");

        let yes: Side = serde_json::from_str(&yes_json).unwrap();
        let no: Side = serde_json::from_str(&no_json).unwrap();
        assert_eq!(yes, Side::Yes);
        assert_eq!(no, Side::No);
    }

    // -- MarketCategory tests --

    #[test]
    fn test_category_display() {
        assert_eq!(format!("{}", MarketCategory::Weather), "Weather");
        assert_eq!(format!("{}", MarketCategory::Economics), "Economics");
        assert_eq!(format!("{}", MarketCategory::Other), "Other");
    }

    #[test]
    fn test_category_from_str() {
        assert_eq!("weather".parse::<MarketCategory>().unwrap(), MarketCategory::Weather);
        assert_eq!("SPORTS".parse::<MarketCategory>().unwrap(), MarketCategory::Sports);
        assert_eq!("econ".parse::<MarketCategory>().unwrap(), MarketCategory::Economics);
        assert_eq!("political".parse::<MarketCategory>().unwrap(), MarketCategory::Politics);
        assert_eq!("entertainment".parse::<MarketCategory>().unwrap(), MarketCategory::Culture);
        assert!("nonsense".parse::<MarketCategory>().is_err());
    }

    #[test]
    fn test_category_serialization_roundtrip() {
        for cat in MarketCategory::ALL {
            let json = serde_json::to_string(cat).unwrap();
            let parsed: MarketCategory = serde_json::from_str(&json).unwrap();
            assert_eq!(*cat, parsed);
        }
    }

    #[test]
    fn test_category_all() {
        assert_eq!(MarketCategory::ALL.len(), 6);
    }

    // -- AgentStatus tests --

    #[test]
    fn test_agent_status_display() {
        assert_eq!(format!("{}", AgentStatus::Alive), "🟢 ALIVE");
        assert_eq!(format!("{}", AgentStatus::Died), "🔴 DIED");
        assert_eq!(format!("{}", AgentStatus::Paused), "🟡 PAUSED");
    }

    #[test]
    fn test_agent_status_serialization_roundtrip() {
        for status in [AgentStatus::Alive, AgentStatus::Died, AgentStatus::Paused] {
            let json = serde_json::to_string(&status).unwrap();
            let parsed: AgentStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, parsed);
        }
    }

    // -- AgentState tests --

    #[test]
    fn test_agent_state_new() {
        let state = AgentState::new(100.0);
        assert_eq!(state.bankroll, 100.0);
        assert_eq!(state.total_pnl, 0.0);
        assert_eq!(state.cycle_count, 0);
        assert_eq!(state.status, AgentStatus::Alive);
        assert_eq!(state.win_rate(), 0.0);
        assert_eq!(state.drawdown(), 0.0);
        assert!(state.is_alive());
        assert_eq!(state.total_costs(), 0.0);
        assert_eq!(state.trades_resolved(), 0);
        assert_eq!(state.trades_pending(), 0);
    }

    #[test]
    fn test_agent_state_win_rate() {
        let mut state = AgentState::new(100.0);
        state.trades_won = 7;
        state.trades_lost = 3;
        assert!((state.win_rate() - 70.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_agent_state_drawdown() {
        let mut state = AgentState::new(100.0);
        state.peak_bankroll = 200.0;
        state.bankroll = 150.0;
        assert!((state.drawdown() - 0.25).abs() < f64::EPSILON);
    }

    #[test]
    fn test_agent_state_drawdown_zero_peak() {
        let mut state = AgentState::new(0.0);
        state.peak_bankroll = 0.0;
        assert_eq!(state.drawdown(), 0.0);
    }

    #[test]
    fn test_agent_state_deduct_cost_alive() {
        let mut state = AgentState::new(100.0);
        assert!(state.deduct_cost(0.10, 0.25));
        assert!((state.bankroll - 99.65).abs() < 1e-10);
        assert!((state.total_api_costs - 0.10).abs() < 1e-10);
        assert!((state.total_ib_commissions - 0.25).abs() < 1e-10);
        assert!(state.is_alive());
    }

    #[test]
    fn test_agent_state_deduct_cost_death() {
        let mut state = AgentState::new(0.50);
        assert!(!state.deduct_cost(0.30, 0.25));
        assert_eq!(state.status, AgentStatus::Died);
        assert!(!state.is_alive());
    }

    #[test]
    fn test_agent_state_record_resolution_win() {
        let mut state = AgentState::new(100.0);
        state.trades_placed = 1;
        state.record_resolution(15.0, true);
        assert_eq!(state.trades_won, 1);
        assert_eq!(state.trades_lost, 0);
        assert!((state.bankroll - 115.0).abs() < 1e-10);
        assert!((state.total_pnl - 15.0).abs() < 1e-10);
        assert!((state.peak_bankroll - 115.0).abs() < 1e-10);
    }

    #[test]
    fn test_agent_state_record_resolution_loss() {
        let mut state = AgentState::new(100.0);
        state.trades_placed = 1;
        state.record_resolution(-8.0, false);
        assert_eq!(state.trades_won, 0);
        assert_eq!(state.trades_lost, 1);
        assert!((state.bankroll - 92.0).abs() < 1e-10);
        assert!((state.peak_bankroll - 100.0).abs() < 1e-10); // peak unchanged
    }

    #[test]
    fn test_agent_state_trades_pending() {
        let mut state = AgentState::new(100.0);
        state.trades_placed = 10;
        state.trades_won = 4;
        state.trades_lost = 3;
        assert_eq!(state.trades_resolved(), 7);
        assert_eq!(state.trades_pending(), 3);
    }

    #[test]
    fn test_agent_state_serialization_roundtrip() {
        let state = AgentState::new(50.0);
        let json = serde_json::to_string(&state).unwrap();
        let parsed: AgentState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.bankroll, 50.0);
        assert_eq!(parsed.status, AgentStatus::Alive);
    }

    #[test]
    fn test_agent_state_display() {
        let state = AgentState::new(100.0);
        let display = format!("{state}");
        assert!(display.contains("ALIVE"));
        assert!(display.contains("100.00"));
    }

    // -- Market tests --

    #[test]
    fn test_market_serialization_roundtrip() {
        let market = Market::sample();
        let json = serde_json::to_string(&market).unwrap();
        let deserialized: Market = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, "test-001");
        assert_eq!(deserialized.platform, "forecastex");
        assert_eq!(deserialized.category, MarketCategory::Economics);
        assert_eq!(deserialized.cross_refs.metaculus_forecasters, Some(314));
    }

    #[test]
    fn test_market_mid_price() {
        let market = Market::sample(); // yes=0.45, no=0.55
        // mid = (0.45 + (1.0 - 0.55)) / 2 = (0.45 + 0.45) / 2 = 0.45
        assert!((market.mid_price() - 0.45).abs() < 1e-10);
    }

    #[test]
    fn test_market_spread() {
        let market = Market::sample();
        // spread = |0.45 - (1.0 - 0.55)| = |0.45 - 0.45| = 0.0
        assert!((market.spread() - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_market_is_active() {
        let market = Market::sample(); // deadline = now + 30 days
        assert!(market.is_active());
    }

    #[test]
    fn test_market_display() {
        let market = Market::sample();
        let display = format!("{market}");
        assert!(display.contains("forecastex"));
        assert!(display.contains("CPI"));
    }

    // -- CrossReferences tests --

    #[test]
    fn test_cross_refs_default() {
        let refs = CrossReferences::default();
        assert!(refs.metaculus_prob.is_none());
        assert!(refs.manifold_prob.is_none());
        assert!(refs.forecastex_price.is_none());
    }

    #[test]
    fn test_cross_refs_display_full() {
        let refs = CrossReferences {
            metaculus_prob: Some(0.60),
            metaculus_forecasters: Some(200),
            manifold_prob: Some(0.55),
            forecastex_price: Some(0.50),
        };
        let display = format!("{refs}");
        assert!(display.contains("Metaculus"));
        assert!(display.contains("Manifold"));
        assert!(display.contains("ForecastEx"));
    }

    #[test]
    fn test_cross_refs_display_empty() {
        let refs = CrossReferences::default();
        assert_eq!(format!("{refs}"), "No cross-references");
    }

    #[test]
    fn test_cross_refs_serialization_roundtrip() {
        let refs = CrossReferences {
            metaculus_prob: Some(0.65),
            metaculus_forecasters: Some(100),
            manifold_prob: None,
            forecastex_price: Some(0.70),
        };
        let json = serde_json::to_string(&refs).unwrap();
        let parsed: CrossReferences = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.metaculus_prob, Some(0.65));
        assert!(parsed.manifold_prob.is_none());
    }

    // -- Estimate tests --

    #[test]
    fn test_estimate_is_valid() {
        let e = Estimate {
            probability: 0.50,
            confidence: 0.8,
            reasoning: "test".to_string(),
            tokens_used: 100,
            cost: 0.01,
        };
        assert!(e.is_valid());
    }

    #[test]
    fn test_estimate_invalid_too_high() {
        let e = Estimate {
            probability: 1.0,
            confidence: 0.99,
            reasoning: "overconfident".to_string(),
            tokens_used: 100,
            cost: 0.01,
        };
        assert!(!e.is_valid());
    }

    #[test]
    fn test_estimate_invalid_too_low() {
        let e = Estimate {
            probability: 0.0,
            confidence: 0.99,
            reasoning: "overconfident".to_string(),
            tokens_used: 100,
            cost: 0.01,
        };
        assert!(!e.is_valid());
    }

    #[test]
    fn test_estimate_boundary_valid() {
        let low = Estimate {
            probability: 0.01,
            confidence: 0.5,
            reasoning: "".to_string(),
            tokens_used: 0,
            cost: 0.0,
        };
        let high = Estimate {
            probability: 0.99,
            confidence: 0.5,
            reasoning: "".to_string(),
            tokens_used: 0,
            cost: 0.0,
        };
        assert!(low.is_valid());
        assert!(high.is_valid());
    }

    #[test]
    fn test_estimate_echo_detection() {
        let e = Estimate {
            probability: 0.451,
            confidence: 0.7,
            reasoning: "".to_string(),
            tokens_used: 100,
            cost: 0.01,
        };
        // Market price 0.45, tolerance 0.02 → within tolerance → echo
        assert!(e.is_echo(0.45, 0.02));
        // Market price 0.45, tolerance 0.0005 → outside → not echo
        assert!(!e.is_echo(0.45, 0.0005));
    }

    #[test]
    fn test_estimate_display() {
        let e = Estimate {
            probability: 0.73,
            confidence: 0.85,
            reasoning: "strong signal".to_string(),
            tokens_used: 250,
            cost: 0.005,
        };
        let display = format!("{e}");
        assert!(display.contains("73.0%"));
        assert!(display.contains("85%"));
    }

    #[test]
    fn test_estimate_serialization_roundtrip() {
        let e = Estimate {
            probability: 0.62,
            confidence: 0.90,
            reasoning: "Based on BOM data".to_string(),
            tokens_used: 350,
            cost: 0.008,
        };
        let json = serde_json::to_string(&e).unwrap();
        let parsed: Estimate = serde_json::from_str(&json).unwrap();
        assert!((parsed.probability - 0.62).abs() < 1e-10);
        assert_eq!(parsed.tokens_used, 350);
    }

    // -- TradeReceipt tests --

    #[test]
    fn test_trade_receipt_net_cost() {
        let receipt = TradeReceipt {
            order_id: "ORD-001".to_string(),
            market_id: "MKT-001".to_string(),
            platform: "forecastex".to_string(),
            side: Side::Yes,
            amount: 5.0,
            fill_price: 0.45,
            fees: 0.25,
            timestamp: Utc::now(),
        };
        assert!((receipt.net_cost() - 5.25).abs() < 1e-10);
    }

    #[test]
    fn test_trade_receipt_display() {
        let receipt = TradeReceipt {
            order_id: "ORD-001".to_string(),
            market_id: "MKT-001".to_string(),
            platform: "forecastex".to_string(),
            side: Side::Yes,
            amount: 5.0,
            fill_price: 0.45,
            fees: 0.25,
            timestamp: Utc::now(),
        };
        let display = format!("{receipt}");
        assert!(display.contains("YES"));
        assert!(display.contains("forecastex"));
    }

    #[test]
    fn test_trade_receipt_serialization_roundtrip() {
        let receipt = TradeReceipt {
            order_id: "ORD-002".to_string(),
            market_id: "MKT-002".to_string(),
            platform: "manifold".to_string(),
            side: Side::No,
            amount: 10.0,
            fill_price: 0.60,
            fees: 0.0,
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&receipt).unwrap();
        let parsed: TradeReceipt = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.order_id, "ORD-002");
        assert_eq!(parsed.side, Side::No);
    }

    // -- Position tests --

    #[test]
    fn test_position_unrealized_pnl_profit() {
        let pos = Position {
            market_id: "MKT-001".to_string(),
            platform: "forecastex".to_string(),
            side: Side::Yes,
            size: 10.0,
            entry_price: 0.40,
            current_value: 5.0,
        };
        // PnL = 5.0 - (10.0 * 0.40) = 5.0 - 4.0 = 1.0
        assert!((pos.unrealized_pnl() - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_position_unrealized_pnl_loss() {
        let pos = Position {
            market_id: "MKT-001".to_string(),
            platform: "forecastex".to_string(),
            side: Side::No,
            size: 10.0,
            entry_price: 0.60,
            current_value: 4.0,
        };
        // PnL = 4.0 - (10.0 * 0.60) = 4.0 - 6.0 = -2.0
        assert!((pos.unrealized_pnl() - (-2.0)).abs() < 1e-10);
    }

    #[test]
    fn test_position_display() {
        let pos = Position {
            market_id: "MKT-001".to_string(),
            platform: "forecastex".to_string(),
            side: Side::Yes,
            size: 10.0,
            entry_price: 0.40,
            current_value: 5.0,
        };
        let display = format!("{pos}");
        assert!(display.contains("YES"));
        assert!(display.contains("+1")); // positive PnL
    }

    #[test]
    fn test_position_serialization_roundtrip() {
        let pos = Position {
            market_id: "MKT-001".to_string(),
            platform: "forecastex".to_string(),
            side: Side::Yes,
            size: 10.0,
            entry_price: 0.40,
            current_value: 5.0,
        };
        let json = serde_json::to_string(&pos).unwrap();
        let parsed: Position = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.market_id, "MKT-001");
        assert_eq!(parsed.side, Side::Yes);
    }

    // -- LiquidityInfo tests --

    #[test]
    fn test_liquidity_total_depth() {
        let liq = LiquidityInfo {
            bid_depth: 5000.0,
            ask_depth: 3000.0,
            volume_24h: 20000.0,
        };
        assert!((liq.total_depth() - 8000.0).abs() < 1e-10);
    }

    #[test]
    fn test_liquidity_is_sufficient() {
        let liq = LiquidityInfo {
            bid_depth: 5000.0,
            ask_depth: 3000.0,
            volume_24h: 20000.0,
        };
        assert!(liq.is_sufficient(8000.0));
        assert!(liq.is_sufficient(7999.0));
        assert!(!liq.is_sufficient(8001.0));
    }

    #[test]
    fn test_liquidity_display() {
        let liq = LiquidityInfo {
            bid_depth: 5000.0,
            ask_depth: 3000.0,
            volume_24h: 20000.0,
        };
        let display = format!("{liq}");
        assert!(display.contains("5000"));
        assert!(display.contains("3000"));
    }

    #[test]
    fn test_liquidity_serialization_roundtrip() {
        let liq = LiquidityInfo {
            bid_depth: 1234.56,
            ask_depth: 789.01,
            volume_24h: 50000.0,
        };
        let json = serde_json::to_string(&liq).unwrap();
        let parsed: LiquidityInfo = serde_json::from_str(&json).unwrap();
        assert!((parsed.bid_depth - 1234.56).abs() < 1e-10);
    }

    // -- BetDecision tests --

    #[test]
    fn test_bet_decision_expected_value() {
        let decision = BetDecision {
            market: Market::sample(),
            side: Side::Yes,
            fair_value: 0.55,
            edge: 0.10,
            kelly_fraction: 0.05,
            bet_amount: 5.0,
            confidence: 0.80,
            rationale: "Strong CPI signal".to_string(),
            data_sources_used: vec!["fred".to_string(), "metaculus".to_string()],
        };
        assert!((decision.expected_value() - 0.50).abs() < 1e-10);
    }

    #[test]
    fn test_bet_decision_market_price() {
        let decision = BetDecision {
            market: Market::sample(), // yes=0.45, no=0.55
            side: Side::Yes,
            fair_value: 0.55,
            edge: 0.10,
            kelly_fraction: 0.05,
            bet_amount: 5.0,
            confidence: 0.80,
            rationale: "test".to_string(),
            data_sources_used: vec![],
        };
        assert!((decision.market_price() - 0.45).abs() < 1e-10);

        let no_decision = BetDecision {
            side: Side::No,
            ..decision
        };
        assert!((no_decision.market_price() - 0.55).abs() < 1e-10);
    }

    #[test]
    fn test_bet_decision_display() {
        let decision = BetDecision {
            market: Market::sample(),
            side: Side::Yes,
            fair_value: 0.55,
            edge: 0.10,
            kelly_fraction: 0.05,
            bet_amount: 5.0,
            confidence: 0.80,
            rationale: "test".to_string(),
            data_sources_used: vec![],
        };
        let display = format!("{decision}");
        assert!(display.contains("YES"));
        assert!(display.contains("55%")); // fair value
    }

    #[test]
    fn test_bet_decision_serialization_roundtrip() {
        let decision = BetDecision {
            market: Market::sample(),
            side: Side::Yes,
            fair_value: 0.55,
            edge: 0.10,
            kelly_fraction: 0.05,
            bet_amount: 5.0,
            confidence: 0.80,
            rationale: "Based on FRED data".to_string(),
            data_sources_used: vec!["fred".to_string()],
        };
        let json = serde_json::to_string(&decision).unwrap();
        let parsed: BetDecision = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.side, Side::Yes);
        assert!((parsed.fair_value - 0.55).abs() < 1e-10);
    }

    // -- DataContext tests --

    #[test]
    fn test_data_context_empty() {
        let ctx = DataContext::empty(MarketCategory::Weather);
        assert_eq!(ctx.category, MarketCategory::Weather);
        assert_eq!(ctx.cost, 0.0);
        assert!(ctx.metaculus_forecast.is_none());
    }

    #[test]
    fn test_data_context_is_stale() {
        let mut ctx = DataContext::empty(MarketCategory::Sports);
        // Set freshness to 2 hours ago
        ctx.freshness = Utc::now() - chrono::Duration::hours(2);
        assert!(ctx.is_stale(chrono::Duration::hours(1)));
        assert!(!ctx.is_stale(chrono::Duration::hours(3)));
    }

    #[test]
    fn test_data_context_display() {
        let ctx = DataContext {
            category: MarketCategory::Economics,
            raw_data: serde_json::json!({"cpi": 3.2}),
            summary: "CPI at 3.2%".to_string(),
            freshness: Utc::now(),
            source: "fred".to_string(),
            cost: 0.001,
            metaculus_forecast: Some(0.55),
            metaculus_forecasters: Some(200),
            manifold_price: None,
        };
        let display = format!("{ctx}");
        assert!(display.contains("Economics"));
        assert!(display.contains("fred"));
    }

    #[test]
    fn test_data_context_serialization_roundtrip() {
        let ctx = DataContext {
            category: MarketCategory::Weather,
            raw_data: serde_json::json!({"temp": 25.3, "wind": 15}),
            summary: "Warm and windy".to_string(),
            freshness: Utc::now(),
            source: "bom".to_string(),
            cost: 0.0,
            metaculus_forecast: None,
            metaculus_forecasters: None,
            manifold_price: Some(0.60),
        };
        let json = serde_json::to_string(&ctx).unwrap();
        let parsed: DataContext = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.category, MarketCategory::Weather);
        assert_eq!(parsed.manifold_price, Some(0.60));
    }

    // -- CycleReport tests --

    #[test]
    fn test_cycle_report_display() {
        let report = CycleReport {
            cycle_number: 42,
            timestamp: Utc::now(),
            markets_scanned: 150,
            edges_found: 5,
            bets_placed: 2,
            cycle_cost: 0.15,
            cycle_pnl: 3.50,
            bankroll_after: 103.50,
        };
        let display = format!("{report}");
        assert!(display.contains("#42"));
        assert!(display.contains("150"));
    }

    #[test]
    fn test_cycle_report_serialization_roundtrip() {
        let report = CycleReport {
            cycle_number: 1,
            timestamp: Utc::now(),
            markets_scanned: 50,
            edges_found: 3,
            bets_placed: 1,
            cycle_cost: 0.12,
            cycle_pnl: -0.12,
            bankroll_after: 99.88,
        };
        let json = serde_json::to_string(&report).unwrap();
        let parsed: CycleReport = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.cycle_number, 1);
        assert!((parsed.bankroll_after - 99.88).abs() < 1e-10);
    }

    // -- OracleError tests --

    #[test]
    fn test_oracle_error_display() {
        let e = OracleError::Platform {
            platform: "forecastex".to_string(),
            message: "connection timeout".to_string(),
        };
        assert_eq!(format!("{e}"), "Platform error (forecastex): connection timeout");

        let e = OracleError::InsufficientBalance {
            needed: 10.0,
            available: 5.0,
        };
        assert!(format!("{e}").contains("10.00"));
        assert!(format!("{e}").contains("5.00"));
    }
}
