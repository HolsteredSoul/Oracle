//! ORACLE — Autonomous Prediction Market AI Agent
//!
//! Entry point. Loads configuration, initialises structured logging,
//! prints the startup banner, and enters the main idle/scan loop
//! with graceful shutdown on Ctrl+C.

use anyhow::Result;
use std::time::Duration;
use tracing::info;

use oracle::config;

const BANNER: &str = r#"
  ___  ____      _    ____ _     _____
 / _ \|  _ \    / \  / ___| |   | ____|
| | | | |_) |  / _ \| |   | |   |  _|
| |_| |  _ <  / ___ \ |___| |___| |___
 \___/|_| \_\/_/   \_\____|_____|_____|

  Optimized Risk-Adjusted Cross-platform Leveraged Engine
  v0.1.0 — Phase 0: Scaffolding
"#;

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env file if present (non-fatal if missing)
    let _ = dotenv::dotenv();

    // Load configuration from TOML
    let cfg = config::AppConfig::load("config.toml")?;

    // Initialise structured logging
    init_logging(&cfg);

    // Print startup banner
    println!("{BANNER}");
    info!(
        agent_name = %cfg.agent.name,
        scan_interval_secs = cfg.agent.scan_interval_secs,
        initial_bankroll = cfg.agent.initial_bankroll,
        currency = %cfg.agent.currency,
        "ORACLE starting up"
    );

    info!(
        llm_provider = %cfg.llm.provider,
        llm_model = %cfg.llm.model,
        "LLM configuration loaded"
    );

    info!(
        forecastex_enabled = cfg.platforms.forecastex.enabled,
        metaculus_enabled = cfg.platforms.metaculus.enabled,
        manifold_enabled = cfg.platforms.manifold.enabled,
        "Platform configuration loaded"
    );

    info!(
        mispricing_threshold = cfg.risk.mispricing_threshold,
        kelly_multiplier = cfg.risk.kelly_multiplier,
        max_bet_pct = cfg.risk.max_bet_pct,
        max_exposure_pct = cfg.risk.max_exposure_pct,
        "Risk parameters loaded"
    );

    // Main loop with graceful shutdown
    let scan_interval = Duration::from_secs(cfg.agent.scan_interval_secs);
    let mut interval = tokio::time::interval(scan_interval);
    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);

    info!("Entering main loop (interval: {}s). Press Ctrl+C to stop.", cfg.agent.scan_interval_secs);

    loop {
        tokio::select! {
            _ = interval.tick() => {
                // Phase 0: idle heartbeat. Real scan/estimate/bet logic comes in later phases.
                info!(cycle = "heartbeat", "Cycle tick — no active strategy yet (Phase 0)");
            }
            _ = &mut shutdown => {
                info!("Shutdown signal received. Saving state and exiting gracefully...");
                // TODO (Phase 6): persist AgentState to SQLite before exiting
                break;
            }
        }
    }

    info!("ORACLE shut down cleanly.");
    Ok(())
}

/// Initialise the `tracing` subscriber.
///
/// - In development (RUST_LOG set), uses human-readable pretty format.
/// - In production, uses structured JSON logging to stdout.
fn init_logging(cfg: &config::AppConfig) {
    use tracing_subscriber::{fmt, EnvFilter};

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("oracle=info"));

    // Check if the user wants JSON logging (production)
    let json_logging = std::env::var("ORACLE_LOG_JSON").is_ok();

    if json_logging {
        fmt()
            .json()
            .with_env_filter(env_filter)
            .with_target(true)
            .with_thread_ids(true)
            .with_file(true)
            .with_line_number(true)
            .init();
    } else {
        fmt()
            .with_env_filter(env_filter)
            .with_target(true)
            .init();
    }

    let _ = cfg; // cfg reserved for future log-config options
}
