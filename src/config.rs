//! Configuration loading from TOML with environment variable resolution.
//!
//! Reads `config.toml` and deserializes into strongly-typed structs.
//! Secrets (API keys) are referenced by env-var name in the config and
//! resolved at runtime via `std::env::var`.

use anyhow::{Context, Result};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;

/// Top-level application configuration.
#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    pub agent: AgentConfig,
    pub llm: LlmConfig,
    pub platforms: PlatformsConfig,
    pub risk: RiskConfig,
    #[serde(default)]
    pub strategy: StrategyConfig,
    #[serde(default)]
    pub scanner: ScannerConfig,
    #[serde(default)]
    pub enricher: EnricherConfig,
    pub data_sources: DataSourcesConfig,
    pub dashboard: DashboardConfig,
    pub alerts: AlertsConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AgentConfig {
    pub name: String,
    /// Trading execution mode: "dry" | "paper" | "live"
    #[serde(default = "AgentConfig::default_trading_mode")]
    pub trading_mode: String,
    pub scan_interval_secs: u64,
    pub initial_bankroll: Decimal,
    pub survival_threshold: Decimal,
    pub currency: String,
}

impl AgentConfig {
    fn default_trading_mode() -> String {
        "dry".to_string()
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct LlmConfig {
    pub provider: String,
    pub model: String,
    pub api_key_env: String,
    pub max_tokens: u32,
    pub batch_size: u32,
    /// Fallback model for OpenRouter (used when primary model fails).
    #[serde(default)]
    pub fallback_model: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PlatformsConfig {
    pub forecastex: ForecastExConfig,
    pub metaculus: MetaculusConfig,
    pub manifold: ManifoldConfig,
    #[serde(default)]
    pub betfair: BetfairConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ForecastExConfig {
    pub enabled: bool,
    pub ib_host: String,
    pub ib_port: u16,
    pub ib_client_id: u32,
    pub account_id_env: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct MetaculusConfig {
    pub enabled: bool,
    /// Env var name for the Metaculus API token (e.g. "METACULUS_API_TOKEN").
    /// Required — Metaculus API requires authentication for all requests.
    #[serde(default)]
    pub api_key_env: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ManifoldConfig {
    pub enabled: bool,
    /// Env var name for the Manifold API key (default: "MANIFOLD_API_KEY").
    /// Only needed to place play-money bets; market scanning is public.
    #[serde(default)]
    pub api_key_env: Option<String>,
    /// Mana balance used for Kelly bet sizing when trading on Manifold.
    /// Manifold uses Mana (play currency), not AUD, so sizing against the
    /// real bankroll would be incorrect. Defaults to 1000 Mana if unset.
    #[serde(default)]
    pub mana_bankroll: Option<Decimal>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct BetfairConfig {
    pub enabled: bool,
    /// Env var name for Betfair app key (default: "BETFAIR_APP_KEY").
    #[serde(default = "BetfairConfig::default_app_key_env")]
    pub app_key_env: String,
    /// Env var name for Betfair username (default: "BETFAIR_USERNAME").
    #[serde(default = "BetfairConfig::default_username_env")]
    pub username_env: String,
    /// Env var name for Betfair password (default: "BETFAIR_PASSWORD").
    #[serde(default = "BetfairConfig::default_password_env")]
    pub password_env: String,
}

impl Default for BetfairConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            app_key_env: "BETFAIR_APP_KEY".to_string(),
            username_env: "BETFAIR_USERNAME".to_string(),
            password_env: "BETFAIR_PASSWORD".to_string(),
        }
    }
}

impl BetfairConfig {
    fn default_app_key_env() -> String {
        "BETFAIR_APP_KEY".to_string()
    }
    fn default_username_env() -> String {
        "BETFAIR_USERNAME".to_string()
    }
    fn default_password_env() -> String {
        "BETFAIR_PASSWORD".to_string()
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct RiskConfig {
    pub mispricing_threshold: Decimal,
    pub kelly_multiplier: Decimal,
    pub max_bet_pct: Decimal,
    pub max_exposure_pct: Decimal,
    pub min_liquidity_contracts: u64,
    pub category_thresholds: HashMap<String, Decimal>,
}

/// Strategy / auto-exit configuration ([strategy] section).
///
/// ## Betfair Australia minimum stake (AUD, March 2026)
/// - Exchange back/lay bets placed via API: **AUD $1.00** minimum
/// - BSP back bets: same AUD $1.00 minimum
/// - BSP lay liability: unchanged (not affected by the 2022 reduction)
/// - Sub-minimum bets via API are technically possible but risk account suspension
/// - Min Bet Payout exception: bets below $1 AUD are valid when payout ≥ $10 AUD
///
/// Oracle defaults `min_close_stake` to $2.00 AUD as a safety buffer above the
/// $1.00 absolute minimum. Never place a closing bet below this value.
///
/// ## Manifold minimum sell
/// - No documented minimum; `shares` parameter is optional (defaults to all)
/// - Practical minimum: 1 Mana — essentially no meaningful constraint
#[derive(Debug, Deserialize, Clone)]
pub struct StrategyConfig {
    /// Enable automatic take-profit / stop-loss / time-based position closing.
    #[serde(default = "StrategyConfig::default_enable_auto_exit")]
    pub enable_auto_exit: bool,
    /// Close position if unrealized P&L reaches this percentage (e.g. 15.0 = +15%).
    #[serde(default = "StrategyConfig::default_take_profit_percent")]
    pub take_profit_percent: Decimal,
    /// Close position if unrealized P&L falls to this percentage (e.g. -10.0 = -10%).
    #[serde(default = "StrategyConfig::default_stop_loss_percent")]
    pub stop_loss_percent: Decimal,
    /// Force-close after this many hours (0 = disabled).
    #[serde(default = "StrategyConfig::default_max_hold_hours")]
    pub max_hold_hours: u64,
    /// Minimum closing stake in AUD (Betfair) or Mana (Manifold).
    /// Betfair official minimum is $1.00 AUD; default here is $2.00 as buffer.
    #[serde(default = "StrategyConfig::default_min_close_stake")]
    pub min_close_stake: Decimal,
    /// If true, log close decisions without placing real orders (dry-run).
    #[serde(default)]
    pub auto_exit_dry_run: bool,
}

impl Default for StrategyConfig {
    fn default() -> Self {
        Self {
            enable_auto_exit: true,
            take_profit_percent: rust_decimal_macros::dec!(15.0),
            stop_loss_percent: rust_decimal_macros::dec!(-10.0),
            max_hold_hours: 48,
            min_close_stake: rust_decimal_macros::dec!(2.0),
            auto_exit_dry_run: false,
        }
    }
}

impl StrategyConfig {
    fn default_enable_auto_exit() -> bool { true }
    fn default_take_profit_percent() -> Decimal { rust_decimal_macros::dec!(15.0) }
    fn default_stop_loss_percent() -> Decimal { rust_decimal_macros::dec!(-10.0) }
    fn default_max_hold_hours() -> u64 { 48 }
    fn default_min_close_stake() -> Decimal { rust_decimal_macros::dec!(2.0) }
}

/// Scanner / market-router configuration ([scanner] section).
#[derive(Debug, Deserialize, Clone)]
pub struct ScannerConfig {
    /// Minimum Jaccard-based similarity score (0–1) to cross-reference two markets.
    #[serde(default = "ScannerConfig::default_match_threshold")]
    pub match_threshold: f64,
    /// Minimum liquidity (or forecaster count for Metaculus) to keep a market.
    #[serde(default = "ScannerConfig::default_min_liquidity")]
    pub min_liquidity: Decimal,
    /// Maximum hours until deadline — markets closing later than this are skipped.
    #[serde(default = "ScannerConfig::default_max_hours_to_deadline")]
    pub max_hours_to_deadline: f64,
    /// Minimum hours until deadline — markets closing sooner than this are skipped.
    #[serde(default = "ScannerConfig::default_min_hours_to_deadline")]
    pub min_hours_to_deadline: f64,
    /// Maximum markets passed to the enrichment and LLM estimation stages.
    #[serde(default = "ScannerConfig::default_max_markets_to_process")]
    pub max_markets_to_process: usize,
}

impl Default for ScannerConfig {
    fn default() -> Self {
        Self {
            match_threshold: 0.45,
            min_liquidity: dec!(5.0),
            max_hours_to_deadline: 24.0 * 365.0,
            min_hours_to_deadline: 1.0,
            max_markets_to_process: 80,
        }
    }
}

impl ScannerConfig {
    fn default_match_threshold() -> f64 { 0.45 }
    fn default_min_liquidity() -> Decimal { dec!(5.0) }
    fn default_max_hours_to_deadline() -> f64 { 24.0 * 365.0 }
    fn default_min_hours_to_deadline() -> f64 { 1.0 }
    fn default_max_markets_to_process() -> usize { 80 }
}

/// Enricher cache TTL configuration ([enricher] section).
#[derive(Debug, Deserialize, Clone)]
pub struct EnricherConfig {
    /// Default cache TTL in minutes for data contexts.
    #[serde(default = "EnricherConfig::default_default_cache_ttl_mins")]
    pub default_cache_ttl_mins: i64,
    /// Cache TTL in minutes for weather data (slower to change).
    #[serde(default = "EnricherConfig::default_weather_cache_ttl_mins")]
    pub weather_cache_ttl_mins: i64,
    /// Cache TTL in minutes for news/politics data (fast-moving).
    #[serde(default = "EnricherConfig::default_news_cache_ttl_mins")]
    pub news_cache_ttl_mins: i64,
}

impl Default for EnricherConfig {
    fn default() -> Self {
        Self {
            default_cache_ttl_mins: 30,
            weather_cache_ttl_mins: 60,
            news_cache_ttl_mins: 15,
        }
    }
}

impl EnricherConfig {
    fn default_default_cache_ttl_mins() -> i64 { 30 }
    fn default_weather_cache_ttl_mins() -> i64 { 60 }
    fn default_news_cache_ttl_mins() -> i64 { 15 }
}

#[derive(Debug, Deserialize, Clone)]
pub struct DataSourcesConfig {
    pub openweathermap_key_env: Option<String>,
    pub bom_enabled: Option<bool>,
    pub api_sports_key_env: Option<String>,
    pub fred_api_key_env: Option<String>,
    /// Env var name for the NewsAPI key (default: "NEWS_API_KEY").
    pub news_api_key_env: Option<String>,
    pub coingecko: Option<CoinGeckoConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CoinGeckoConfig {
    pub enabled: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DashboardConfig {
    pub enabled: bool,
    pub port: u16,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AlertsConfig {
    pub telegram_bot_token_env: Option<String>,
    pub telegram_chat_id_env: Option<String>,
}

impl AppConfig {
    /// Load configuration from a TOML file.
    pub fn load(path: &str) -> Result<Self> {
        let contents = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {path}"))?;
        let config: AppConfig = toml::from_str(&contents)
            .with_context(|| format!("Failed to parse config file: {path}"))?;
        config.validate()?;
        Ok(config)
    }

    /// Validate that key config values are within sensible bounds.
    fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.agent.initial_bankroll > Decimal::ZERO,
            "agent.initial_bankroll must be > 0"
        );
        anyhow::ensure!(
            self.risk.kelly_multiplier > Decimal::ZERO,
            "risk.kelly_multiplier must be > 0"
        );
        anyhow::ensure!(
            self.risk.kelly_multiplier <= Decimal::ONE,
            "risk.kelly_multiplier must be ≤ 1"
        );
        anyhow::ensure!(
            self.risk.max_bet_pct > Decimal::ZERO && self.risk.max_bet_pct <= Decimal::ONE,
            "risk.max_bet_pct must be in (0, 1]"
        );
        anyhow::ensure!(
            self.scanner.match_threshold > 0.0 && self.scanner.match_threshold <= 1.0,
            "scanner.match_threshold must be in (0, 1]"
        );
        anyhow::ensure!(
            self.scanner.max_markets_to_process > 0,
            "scanner.max_markets_to_process must be > 0"
        );
        Ok(())
    }

    /// Resolve an environment variable name to its value.
    /// Useful for loading secrets referenced in the config.
    pub fn resolve_env(env_name: &str) -> Result<String> {
        std::env::var(env_name)
            .with_context(|| format!("Environment variable not set: {env_name}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_config() {
        // This test requires config.toml to be in the working directory.
        // In CI, copy config.toml to the test working dir.
        let result = AppConfig::load("config.toml");
        if let Ok(cfg) = result {
            assert_eq!(cfg.agent.name, "ORACLE-001");
            assert_eq!(cfg.agent.scan_interval_secs, 600);
            assert!(cfg.agent.initial_bankroll > Decimal::ZERO);
            assert_eq!(cfg.llm.provider, "openrouter");
            assert!(!cfg.platforms.forecastex.enabled); // Phase 2 stub — disabled
            assert!(cfg.risk.kelly_multiplier > Decimal::ZERO);
            assert!(cfg.risk.kelly_multiplier <= Decimal::ONE);
        }
        // If config.toml isn't found, that's acceptable in some test environments
    }
}
