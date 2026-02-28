//! ORACLE — Autonomous Prediction Market AI Agent
//!
//! Entry point. Loads configuration, initialises structured logging,
//! restores state from disk (or creates fresh), and runs the main
//! scan→estimate→bet loop with graceful shutdown.

use anyhow::Result;
use std::time::Duration;
use tracing::{error, info, warn};

use oracle::config;
use oracle::engine::accountant::{Accountant, CycleCosts, CycleReport};
use oracle::engine::enricher::Enricher;
use oracle::engine::executor::Executor;
use oracle::engine::scanner::MarketRouter;
use oracle::llm::anthropic::AnthropicClient;
use oracle::llm::openrouter::OpenRouterClient;
use oracle::llm::LlmEstimator;
use oracle::platforms::manifold::ManifoldClient;
use oracle::platforms::metaculus::MetaculusClient;
use oracle::storage;
use oracle::strategy::edge::{EdgeConfig, EdgeDetector};
use oracle::strategy::kelly::{KellyCalculator, KellyConfig};
use oracle::strategy::risk::{RiskConfig, RiskManager};
use oracle::strategy::{DecisionRecord, StrategyOrchestrator};
use oracle::types::{AgentState, AgentStatus};

const BANNER: &str = r#"
  ___  ____      _    ____ _     _____
 / _ \|  _ \    / \  / ___| |   | ____|
| | | | |_) |  / _ \| |   | |   |  _|
| |_| |  _ <  / ___ \ |___| |___| |___
 \___/|_| \_\/_/   \_\____|_____|_____|

  Optimized Risk-Adjusted Cross-platform Leveraged Engine
  v0.1.0 — Autonomous Agent
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

    // -- Restore or create state -----------------------------------------

    let mut state = match storage::load_state(None)? {
        Some(s) => {
            info!(
                bankroll = s.bankroll,
                cycles = s.cycle_count,
                trades = s.trades_placed,
                "Resumed from saved state"
            );
            s
        }
        None => {
            let s = AgentState::new(cfg.agent.initial_bankroll);
            info!(bankroll = s.bankroll, "Fresh start");
            s
        }
    };

    // -- Initialise components -------------------------------------------

    // Platform clients
    let manifold = if cfg.platforms.manifold.enabled {
        Some(ManifoldClient::new(None)?)
    } else {
        None
    };

    let metaculus = if cfg.platforms.metaculus.enabled {
        Some(MetaculusClient::new()?)
    } else {
        None
    };

    // Market router (takes ownership of platform clients)
    let router = MarketRouter::new(manifold, metaculus);

    // Data enricher
    let fred_key = cfg.data_sources.fred_api_key_env.as_deref()
        .and_then(|env| std::env::var(env).ok());
    let news_key = std::env::var("NEWS_API_KEY").ok();
    let sports_key = cfg.data_sources.api_sports_key_env.as_deref()
        .and_then(|env| std::env::var(env).ok());
    let mut enricher = Enricher::new(fred_key, news_key, sports_key)?;

    // LLM estimator
    let llm_api_key = std::env::var(&cfg.llm.api_key_env).unwrap_or_default();

    let llm: Box<dyn LlmEstimator> = if llm_api_key.is_empty() {
        warn!("No LLM API key configured — running in dry-run/scan-only mode");
        Box::new(AnthropicClient::new("dummy".into(), Some(cfg.llm.model.clone()), None)?)
    } else {
        match cfg.llm.provider.as_str() {
            "openrouter" => {
                info!(
                    model = %cfg.llm.model,
                    fallback = ?cfg.llm.fallback_model,
                    "Using OpenRouter LLM provider"
                );
                Box::new(OpenRouterClient::new(
                    llm_api_key,
                    Some(cfg.llm.model.clone()),
                    cfg.llm.fallback_model.clone(),
                    Some(cfg.llm.max_tokens),
                )?)
            }
            "anthropic" => {
                info!(model = %cfg.llm.model, "Using Anthropic LLM provider");
                Box::new(AnthropicClient::new(
                    llm_api_key,
                    Some(cfg.llm.model.clone()),
                    Some(cfg.llm.max_tokens),
                )?)
            }
            other => {
                warn!(provider = other, "Unknown LLM provider, defaulting to OpenRouter");
                Box::new(OpenRouterClient::new(
                    llm_api_key,
                    Some(cfg.llm.model.clone()),
                    cfg.llm.fallback_model.clone(),
                    Some(cfg.llm.max_tokens),
                )?)
            }
        }
    };

    // Strategy orchestrator (edge detection → Kelly sizing → risk approval)
    let mut orchestrator = StrategyOrchestrator::new(
        EdgeDetector::new(EdgeConfig {
            weather_threshold: *cfg.risk.category_thresholds.get("weather").unwrap_or(&0.06),
            sports_threshold: *cfg.risk.category_thresholds.get("sports").unwrap_or(&0.08),
            economics_threshold: *cfg.risk.category_thresholds.get("economics").unwrap_or(&0.10),
            politics_threshold: *cfg.risk.category_thresholds.get("politics").unwrap_or(&0.12),
            ..EdgeConfig::default()
        }),
        KellyCalculator::new(KellyConfig {
            multiplier: cfg.risk.kelly_multiplier,
            max_bet_pct: cfg.risk.max_bet_pct,
            ..KellyConfig::default()
        }),
        RiskManager::new(RiskConfig {
            max_exposure_pct: cfg.risk.max_exposure_pct,
            ..RiskConfig::default()
        }),
    );

    // Executor (dry-run until IB ForecastEx is integrated in Phase 2A)
    // Manifold execution requires a separate client with API key
    let executor = Executor::new(None, true);

    // -- Main loop -------------------------------------------------------

    let scan_interval = Duration::from_secs(cfg.agent.scan_interval_secs);
    let mut interval = tokio::time::interval(scan_interval);
    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);

    info!(
        interval_secs = cfg.agent.scan_interval_secs,
        "Entering main loop. Press Ctrl+C to stop."
    );

    loop {
        tokio::select! {
            _ = interval.tick() => {
                if !state.is_alive() {
                    info!("Agent is dead. Shutting down.");
                    break;
                }

                match run_cycle(
                    &router, &mut enricher, &*llm, &mut orchestrator,
                    &executor, &mut state,
                ).await {
                    Ok(report) => {
                        log_cycle_report(&report);
                        // Persist state after each cycle
                        if let Err(e) = storage::save_state(&state, None) {
                            error!(error = %e, "Failed to save state");
                        }
                        if state.status == AgentStatus::Died {
                            info!("Agent died. Final bankroll: ${:.2}", state.bankroll);
                            break;
                        }
                    }
                    Err(e) => {
                        error!(error = %e, "Cycle failed — continuing to next");
                        state.cycle_count += 1;
                    }
                }
            }
            _ = &mut shutdown => {
                info!("Shutdown signal received.");
                break;
            }
        }
    }

    // Save final state
    storage::save_state(&state, None)?;
    info!(
        bankroll = format!("${:.2}", state.bankroll),
        cycles = state.cycle_count,
        trades = state.trades_placed,
        pnl = format!("${:.2}", state.total_pnl),
        "ORACLE shut down cleanly."
    );

    Ok(())
}

/// Run a single scan→enrich→estimate→edge→size→risk→execute cycle.
async fn run_cycle(
    router: &MarketRouter,
    enricher: &mut Enricher,
    llm: &dyn LlmEstimator,
    orchestrator: &mut StrategyOrchestrator,
    executor: &Executor,
    state: &mut AgentState,
) -> Result<CycleReport> {
    info!(cycle = state.cycle_count + 1, "Starting cycle");

    // 1. Scan markets
    let markets = router.scan_all().await?;
    let markets_scanned = markets.len();
    info!(count = markets_scanned, "Markets scanned");

    if markets.is_empty() {
        let costs = CycleCosts {
            data_cost: enricher.total_cost(),
            ..Default::default()
        };
        let exec = oracle::engine::executor::ExecutionReport {
            executed: Vec::new(),
            failed: Vec::new(),
            total_committed: 0.0,
            total_commission: 0.0,
        };
        let mut report = Accountant::reconcile(state, &exec, &costs);
        report.markets_scanned = markets_scanned;
        return Ok(report);
    }

    // 2. Enrich with data
    let enriched = enricher.enrich_batch(&markets).await?;

    // 3. LLM estimation
    let estimates: Vec<_> = if llm.model_name() != "dummy" {
        let market_contexts: Vec<_> = enriched.iter()
            .map(|(m, c)| (m.clone(), c.clone()))
            .collect();
        let ests = llm.batch_estimate(&market_contexts).await?;
        enriched.iter().zip(ests).map(|((m, _), e)| (m.clone(), e)).collect()
    } else {
        Vec::new() // No LLM key — skip estimation
    };

    // 4-5. Edge detection → Kelly sizing → risk approval (via orchestrator)
    orchestrator.reset_cycle();
    let (approved_bets, decisions) = orchestrator.select_bets(&estimates, state);
    // decisions contains KellyRejected + RiskRejected + Selected — all edges
    // above threshold — so its length equals the raw edge count.
    let edges_found = decisions.len();

    // 6. Execute
    let execution = executor.execute_batch(&approved_bets).await?;

    // 7. Reconcile
    let costs = CycleCosts {
        llm_cost: estimates.iter().map(|(_, e)| e.cost).sum(),
        data_cost: enricher.total_cost(),
        ..Default::default()
    };

    let mut report = Accountant::reconcile(state, &execution, &costs);
    report.markets_scanned = markets_scanned;
    report.edges_found = edges_found;

    Ok(report)
}

/// Log a human-readable cycle summary.
fn log_cycle_report(report: &CycleReport) {
    info!(
        cycle = report.cycle_number,
        scanned = report.markets_scanned,
        edges = report.edges_found,
        bets = report.bets_placed,
        failed = report.bets_failed,
        committed = format!("${:.2}", report.total_committed),
        costs = format!("${:.4}", report.cycle_costs.total()),
        bankroll = format!("${:.2}", report.bankroll_after),
        status = ?report.status,
        "Cycle complete"
    );
}

/// Initialise the `tracing` subscriber.
fn init_logging(cfg: &config::AppConfig) {
    use tracing_subscriber::{fmt, EnvFilter};

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("oracle=info"));

    let json_logging = std::env::var("ORACLE_LOG_JSON").is_ok();

    if json_logging {
        fmt()
            .json()
            .with_env_filter(env_filter)
            .with_target(true)
            .with_thread_ids(true)
            .init();
    } else {
        fmt()
            .with_env_filter(env_filter)
            .with_target(true)
            .init();
    }

    let _ = cfg;
}
