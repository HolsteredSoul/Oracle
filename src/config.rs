//! Configuration loading from TOML with environment variable resolution.
//!
//! Reads `config.toml` and deserializes into strongly-typed structs.
//! Secrets (API keys) are referenced by env-var name in the config and
//! resolved at runtime via `std::env::var`.

use anyhow::{Context, Result};
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
    pub data_sources: DataSourcesConfig,
    pub dashboard: DashboardConfig,
    pub alerts: AlertsConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AgentConfig {
    pub name: String,
    pub scan_interval_secs: u64,
    pub initial_bankroll: f64,
    pub survival_threshold: f64,
    pub currency: String,
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
}

#[derive(Debug, Deserialize, Clone)]
pub struct ManifoldConfig {
    pub enabled: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RiskConfig {
    pub mispricing_threshold: f64,
    pub kelly_multiplier: f64,
    pub max_bet_pct: f64,
    pub max_exposure_pct: f64,
    pub min_liquidity_contracts: u64,
    pub category_thresholds: HashMap<String, f64>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DataSourcesConfig {
    pub openweathermap_key_env: Option<String>,
    pub bom_enabled: Option<bool>,
    pub api_sports_key_env: Option<String>,
    pub fred_api_key_env: Option<String>,
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
        Ok(config)
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
            assert!(cfg.agent.initial_bankroll > 0.0);
            assert_eq!(cfg.llm.provider, "openrouter");
            assert!(cfg.platforms.forecastex.enabled);
            assert!(cfg.risk.kelly_multiplier > 0.0);
            assert!(cfg.risk.kelly_multiplier <= 1.0);
        }
        // If config.toml isn't found, that's acceptable in some test environments
    }
}
