//! ORACLE — Autonomous Prediction Market AI Agent
//!
//! Entry point. Loads configuration, initialises structured logging,
//! restores state from disk (or creates fresh), and runs the main
//! scan→estimate→bet loop with graceful shutdown.

use anyhow::Result;
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, warn};

use oracle::dashboard::routes::{AppState, BalancePoint, CycleLogEntry, DashboardState, ErrorLogEntry, EvaluationProgress, TradeLogEntry};
use oracle::dashboard::spawn_dashboard;

use oracle::config;
use oracle::engine::accountant::{Accountant, CycleCosts, CycleReport};
use oracle::engine::auto_exit::{AutoExitConfig, AutoExitEngine, CloseResult};
use oracle::engine::enricher::Enricher;
use oracle::engine::executor::Executor;
use oracle::engine::scanner::MarketRouter;
use oracle::llm::anthropic::AnthropicClient;
use oracle::llm::openai::OpenAiClient;
use oracle::llm::openrouter::OpenRouterClient;
use oracle::llm::LlmEstimator;
use oracle::platforms::betfair::BetfairClient;
use oracle::platforms::manifold::ManifoldClient;
use oracle::platforms::metaculus::MetaculusClient;
use oracle::storage;
use oracle::strategy::edge::{EdgeConfig, EdgeDetector};
use oracle::strategy::kelly::{KellyCalculator, KellyConfig};
use oracle::strategy::risk::{RiskConfig, RiskManager};
use oracle::strategy::StrategyOrchestrator;
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
        initial_bankroll = %cfg.agent.initial_bankroll,
        currency = %cfg.agent.currency,
        "ORACLE starting up"
    );

    // -- Restore or create state -----------------------------------------

    let mut state = match storage::load_state(None)? {
        Some(mut s) => {
            // A restart is an explicit user action — if the bankroll is still
            // above the survival threshold, reset a persisted Died status so
            // the agent can trade again. If it's truly out of money the first
            // cycle's deduct_cost will mark it dead again immediately.
            if s.status == AgentStatus::Died && s.bankroll > s.survival_threshold {
                warn!(
                    bankroll = %s.bankroll,
                    threshold = %s.survival_threshold,
                    "Restarting from Died state — bankroll is above threshold, resetting to Alive"
                );
                s.status = AgentStatus::Alive;
            }
            info!(
                bankroll = %s.bankroll,
                cycles = s.cycle_count,
                trades = s.trades_placed,
                "Resumed from saved state"
            );
            s
        }
        None => {
            let s = AgentState::new(cfg.agent.initial_bankroll);
            info!(bankroll = %s.bankroll, "Fresh start");
            s
        }
    };
    state.survival_threshold = cfg.agent.survival_threshold;

    // Seed Mana bankroll from config if this is a fresh state or an existing state
    // that predates the mana_bankroll field (backward compat: default is 0).
    if let Some(configured_mana) = cfg.platforms.manifold.mana_bankroll {
        if state.mana_bankroll == Decimal::ZERO {
            state.mana_bankroll = configured_mana;
            debug!(mana_bankroll = %configured_mana, "Mana bankroll initialised from config");
        }
    }

    // -- Dashboard -------------------------------------------------------

    // Shared state for the web dashboard (Arc so both the server and the
    // main loop can hold a reference).
    let dashboard_state: AppState = Arc::new(DashboardState::new(state.clone()));

    if cfg.dashboard.enabled {
        if let Err(e) = spawn_dashboard(Arc::clone(&dashboard_state), cfg.dashboard.port).await {
            tracing::warn!(error = %e, "Dashboard disabled — could not start");
        }
    }

    // -- Initialise components -------------------------------------------

    // Platform clients
    let manifold = if cfg.platforms.manifold.enabled {
        let api_key = cfg.platforms.manifold.api_key_env.as_deref()
            .and_then(|env| std::env::var(env).ok());
        Some(ManifoldClient::new(api_key)?)
    } else {
        None
    };

    let metaculus = if cfg.platforms.metaculus.enabled {
        let api_key = cfg.platforms.metaculus.api_key_env.as_deref()
            .and_then(|env| std::env::var(env).ok());
        Some(MetaculusClient::new(api_key)?)
    } else {
        None
    };

    let betfair = if cfg.platforms.betfair.enabled {
        match BetfairClient::new() {
            Ok(client) => {
                info!("Betfair Exchange enabled");
                Some(client)
            }
            Err(e) => {
                warn!(error = %e, "Betfair init failed (credentials missing?), continuing without");
                None
            }
        }
    } else {
        None
    };

    // Market router (takes ownership of platform clients)
    let router = match betfair {
        Some(bf) => MarketRouter::with_betfair_config(cfg.scanner.clone(), bf, manifold, metaculus),
        None => MarketRouter::with_config(cfg.scanner.clone(), manifold, metaculus),
    };

    // Data enricher
    let fred_key = cfg.data_sources.fred_api_key_env.as_deref()
        .and_then(|env| std::env::var(env).ok());
    let news_key = cfg.data_sources.news_api_key_env.as_deref()
        .and_then(|env| std::env::var(env).ok());
    let sports_key = cfg.data_sources.api_sports_key_env.as_deref()
        .and_then(|env| std::env::var(env).ok());
    let mut enricher = Enricher::with_config(cfg.enricher.clone(), fred_key, news_key, sports_key)?;

    // LLM estimator
    let llm_api_key = std::env::var(&cfg.llm.api_key_env).unwrap_or_default();

    let llm: Box<dyn LlmEstimator> = if llm_api_key.is_empty() {
        warn!("No LLM API key configured — running in dry-run/scan-only mode");
        Box::new(AnthropicClient::new("dummy".into(), Some("dummy".to_string()), None)?)
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
                    Some(cfg.llm.batch_size),
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
            "openai" => {
                info!(model = %cfg.llm.model, "Using OpenAI LLM provider");
                Box::new(OpenAiClient::new(
                    llm_api_key,
                    Some(cfg.llm.model.clone()),
                    Some(cfg.llm.max_tokens),
                )?)
            }
            other => {
                anyhow::bail!(
                    "Unknown LLM provider '{}' in config.toml. \
                     Valid values are: openrouter, anthropic, openai",
                    other
                );
            }
        }
    };

    // Store active model name and trading mode in dashboard for display
    *dashboard_state.active_model.write().await = llm.model_name().to_string();
    *dashboard_state.trading_mode.write().await = cfg.agent.trading_mode.clone();

    // Strategy orchestrator (edge detection → Kelly sizing → risk approval)
    let dec_006 = rust_decimal_macros::dec!(0.06);
    let dec_008 = rust_decimal_macros::dec!(0.08);
    let dec_010 = rust_decimal_macros::dec!(0.10);
    let dec_012 = rust_decimal_macros::dec!(0.12);
    let mut orchestrator = StrategyOrchestrator::new(
        EdgeDetector::new(EdgeConfig {
            weather_threshold: *cfg.risk.category_thresholds.get("weather").unwrap_or(&dec_006),
            sports_threshold: *cfg.risk.category_thresholds.get("sports").unwrap_or(&dec_008),
            economics_threshold: *cfg.risk.category_thresholds.get("economics").unwrap_or(&dec_010),
            politics_threshold: *cfg.risk.category_thresholds.get("politics").unwrap_or(&dec_012),
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

    // Executor — create platform clients based on trading_mode
    let (executor_manifold, executor_betfair, dry_run) =
        match cfg.agent.trading_mode.as_str() {
            "paper" => {
                info!("Trading mode: PAPER (Manifold play-money)");
                let api_key = cfg.platforms.manifold.api_key_env.as_deref()
                    .and_then(|env| std::env::var(env).ok());
                let client = ManifoldClient::new(api_key).ok();
                if client.is_none() {
                    warn!("Manifold client unavailable — MANIFOLD_API_KEY may be missing");
                }
                (client, None, false)
            }
            "live" => {
                info!("Trading mode: LIVE (Betfair real-money)");
                let betfair = BetfairClient::new().ok();
                if betfair.is_none() {
                    warn!("Betfair client unavailable — credentials may be missing");
                }
                (None, betfair, false)
            }
            _ => {
                info!("Trading mode: DRY RUN (no execution)");
                (None, None, true)
            }
        };
    let executor = Executor::with_betfair(executor_manifold, executor_betfair, dry_run);

    // Auto-exit engine — create fresh clients (executor took ownership of the first set)
    let auto_exit_config = AutoExitConfig {
        enabled: cfg.strategy.enable_auto_exit,
        take_profit_percent: cfg.strategy.take_profit_percent,
        stop_loss_percent: cfg.strategy.stop_loss_percent,
        max_hold_hours: cfg.strategy.max_hold_hours,
        min_close_stake: cfg.strategy.min_close_stake,
        dry_run: cfg.strategy.auto_exit_dry_run || dry_run,
    };
    let ae_manifold = if cfg.agent.trading_mode == "paper" {
        let api_key = cfg.platforms.manifold.api_key_env.as_deref()
            .and_then(|env| std::env::var(env).ok());
        ManifoldClient::new(api_key).ok()
    } else {
        None
    };
    let ae_betfair = if cfg.agent.trading_mode == "live" {
        BetfairClient::new().ok()
    } else {
        None
    };
    let auto_exit_engine = AutoExitEngine::new(ae_manifold, ae_betfair, auto_exit_config);

    // -- Main loop -------------------------------------------------------

    let scan_interval = Duration::from_secs(cfg.agent.scan_interval_secs);

    // If we have a recorded last-cycle time, wait out whatever remains of the
    // current interval before firing the first tick. This prevents an immediate
    // re-scan when the agent is restarted shortly after a recent cycle.
    let initial_delay = state.last_cycle_time
        .map(|t| {
            let elapsed_secs = (chrono::Utc::now() - t).num_seconds().max(0) as u64;
            scan_interval.saturating_sub(Duration::from_secs(elapsed_secs))
        })
        .unwrap_or(Duration::ZERO); // No prior run → fire first cycle immediately

    let next_tick = tokio::time::Instant::now() + initial_delay;
    let mut interval = tokio::time::interval_at(next_tick, scan_interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);

    if initial_delay.is_zero() {
        info!(
            interval_secs = cfg.agent.scan_interval_secs,
            "Entering main loop. Press Ctrl+C to stop."
        );
    } else {
        info!(
            interval_secs = cfg.agent.scan_interval_secs,
            wait_secs = initial_delay.as_secs(),
            "Entering main loop — waiting for remainder of last interval. Press Ctrl+C to stop."
        );
    }

    loop {
        tokio::select! {
            _ = interval.tick() => {
                if !state.is_alive() {
                    info!("Agent is dead. Shutting down.");
                    break;
                }

                // Check if any previously placed bets have resolved.
                if !state.open_bets.is_empty() {
                    let resolutions = executor.check_manifold_resolutions(&state.open_bets).await;
                    if !resolutions.is_empty() {
                        let mut resolved_ids = std::collections::HashSet::new();
                        for r in &resolutions {
                            // Manifold PnL is in Mana — update mana state only,
                            // never the AUD bankroll or survival check.
                            state.record_mana_resolution(r.pnl, r.won);
                            info!(
                                market_id = %r.market_id,
                                pnl_mana = %r.pnl,
                                won = r.won,
                                mana_bankroll = %state.mana_bankroll,
                                "Manifold bet resolved"
                            );
                            resolved_ids.insert(r.bet_id.clone());
                        }
                        state.open_bets.retain(|b| !resolved_ids.contains(&b.order_id));
                        // Persist updated state after resolutions
                        if let Err(e) = storage::save_state(&state, None) {
                            error!(error = %e, "Failed to save state after resolution");
                        }
                    }
                }

                // Auto-exit: check open positions for take-profit / stop-loss / time limits.
                if !state.open_bets.is_empty() {
                    let close_results = auto_exit_engine.check_and_close(&state.open_bets).await;
                    if !close_results.is_empty() {
                        process_auto_exits(
                            &close_results,
                            &mut state,
                            &dashboard_state,
                        ).await;
                        if let Err(e) = storage::save_state(&state, None) {
                            error!(error = %e, "Failed to save state after auto-exit");
                        }
                    }
                }

                // Use live mana_bankroll from state so Kelly sizing reflects
                // actual Mana balance after wins/losses, not the static config value.
                let mana_for_sizing = if state.mana_bankroll > Decimal::ZERO {
                    Some(state.mana_bankroll)
                } else {
                    None
                };
                match run_cycle(
                    &router, &mut enricher, &*llm, &mut orchestrator,
                    &executor, &mut state, Some(&dashboard_state), mana_for_sizing,
                ).await {
                    Ok(report) => {
                        log_cycle_report(&report);
                        update_dashboard(&dashboard_state, &state, &report).await;
                        *dashboard_state.progress.write().await = EvaluationProgress::Idle;
                        state.last_cycle_time = Some(chrono::Utc::now());
                        if let Err(e) = storage::save_state(&state, None) {
                            error!(error = %e, "Failed to save state");
                        }
                        if state.status == AgentStatus::Died {
                            info!("Agent died. Final bankroll: ${}", state.bankroll.round_dp(2));
                            break;
                        }
                    }
                    Err(e) => {
                        error!(error = %e, "Cycle failed — continuing to next");
                        {
                            let mut log = dashboard_state.error_log.write().await;
                            log.push(ErrorLogEntry {
                                timestamp: chrono::Utc::now().to_rfc3339(),
                                cycle_number: state.cycle_count + 1,
                                error: e.to_string(),
                            });
                            if log.len() > 50 {
                                let excess = log.len() - 50;
                                log.drain(0..excess);
                            }
                        }
                        *dashboard_state.progress.write().await = EvaluationProgress::Idle;
                        state.cycle_count += 1;
                        state.last_cycle_time = Some(chrono::Utc::now());
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
        bankroll = %format!("${}", state.bankroll.round_dp(2)),
        cycles = state.cycle_count,
        trades = state.trades_placed,
        pnl = %format!("${}", state.total_pnl.round_dp(2)),
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
    dash: Option<&AppState>,
    mana_bankroll: Option<Decimal>,
) -> Result<CycleReport> {
    info!(cycle = state.cycle_count + 1, "Starting cycle");

    // 1. Scan markets
    if let Some(d) = dash { *d.progress.write().await = EvaluationProgress::Scanning; }
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
            total_committed: Decimal::ZERO,
            total_commission: Decimal::ZERO,
        };
        let mut report = Accountant::reconcile(state, &exec, &costs);
        report.markets_scanned = markets_scanned;
        return Ok(report);
    }

    // 2. Enrich with data
    if let Some(d) = dash { *d.progress.write().await = EvaluationProgress::Enriching { markets_total: markets_scanned }; }
    let enriched = enricher.enrich_batch(&markets).await?;

    // 3. LLM estimation
    let estimates: Vec<_> = if llm.model_name() != "dummy" {
        let market_contexts: Vec<_> = enriched.iter()
            .map(|(m, c)| (m.clone(), c.clone()))
            .collect();
        if let Some(d) = dash { *d.progress.write().await = EvaluationProgress::Estimating { markets_total: markets_scanned, markets_done: 0 }; }
        let ests = llm.batch_estimate(&market_contexts).await?;
        if let Some(d) = dash { *d.progress.write().await = EvaluationProgress::Estimating { markets_total: markets_scanned, markets_done: markets_scanned }; }
        enriched.iter().zip(ests).map(|((m, _), e)| (m.clone(), e)).collect()
    } else {
        Vec::new() // No LLM key — skip estimation
    };

    // 4-5. Edge detection → Kelly sizing → risk approval (via orchestrator)
    if let Some(d) = dash { *d.progress.write().await = EvaluationProgress::Selecting { markets_total: markets_scanned }; }
    orchestrator.reset_cycle();
    let (approved_bets, decisions) = orchestrator.select_bets(&estimates, state, mana_bankroll);
    // decisions contains KellyRejected + RiskRejected + Selected — all edges
    // above threshold — so its length equals the raw edge count.
    let edges_found = decisions.len();

    // 6. Execute
    if let Some(d) = dash { *d.progress.write().await = EvaluationProgress::Executing { bets_total: approved_bets.len() }; }
    let execution = executor.execute_batch(&approved_bets).await?;

    // 7. Track open bets (for resolution checking on next cycles)
    for trade in &execution.executed {
        if trade.platform != "dry-run" {
            state.open_bets.push(trade.receipt.clone());
        }
    }

    // 8. Reconcile
    if let Some(d) = dash { *d.progress.write().await = EvaluationProgress::Reconciling; }
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
        committed = %format!("${}", report.total_committed.round_dp(2)),
        costs = %format!("${}", report.cycle_costs.total().round_dp(4)),
        bankroll = %format!("${}", report.bankroll_after.round_dp(2)),
        status = ?report.status,
        "Cycle complete"
    );
}

/// Push cycle results into the shared dashboard state.
async fn update_dashboard(dash: &AppState, state: &AgentState, report: &CycleReport) {
    // Mirror the latest agent state snapshot
    *dash.agent.write().await = state.clone();

    // Append cycle log entry (cap at 100 on the write side)
    {
        let mut log = dash.cycle_log.write().await;
        log.push(CycleLogEntry {
            cycle_number: report.cycle_number,
            timestamp: report.timestamp.to_rfc3339(),
            markets_scanned: report.markets_scanned,
            edges_found: report.edges_found,
            bets_placed: report.bets_placed,
            bets_failed: report.bets_failed,
            cycle_cost: report.cycle_costs.total().to_f64().unwrap_or(0.0),
            bankroll_after: report.bankroll_after.to_f64().unwrap_or(0.0),
            status: format!("{}", report.status),
        });
        if log.len() > 100 {
            let excess = log.len() - 100;
            log.drain(0..excess);
        }
    }

    // Append executed trades to recent trades log (cap at 200)
    if !report.executed_trades.is_empty() {
        let mut trades = dash.recent_trades.write().await;
        for t in &report.executed_trades {
            trades.push(TradeLogEntry {
                timestamp: t.receipt.timestamp.to_rfc3339(),
                market_id: t.market_id.clone(),
                platform: t.platform.clone(),
                side: format!("{}", t.side),
                amount: t.amount.to_f64().unwrap_or(0.0),
                currency: t.receipt.currency.clone(),
                edge_pct: t.edge_pct,
                confidence: t.confidence,
                close_reason: None,
                final_pnl: None,
            });
        }
        if trades.len() > 200 {
            let excess = trades.len() - 200;
            trades.drain(0..excess);
        }
    }

    // Append balance history point (cap at 500 on the write side)
    {
        let mut history = dash.balance_history.write().await;
        history.push(BalancePoint {
            timestamp: report.timestamp.to_rfc3339(),
            bankroll: report.bankroll_after.to_f64().unwrap_or(0.0),
        });
        if history.len() > 500 {
            let excess = history.len() - 500;
            history.drain(0..excess);
        }
    }
}

/// Process auto-exit close results: update state, record P&L, and push to dashboard.
async fn process_auto_exits(
    results: &[CloseResult],
    state: &mut oracle::types::AgentState,
    dash: &AppState,
) {
    let mut closed_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

    for result in results {
        if !result.success {
            continue;
        }

        // Record the closed position's P&L in agent state.
        // Manifold is Mana (paper money) — never touch the AUD survival bankroll.
        let won = result.realized_pnl >= Decimal::ZERO;
        if result.platform == "manifold" {
            state.record_mana_resolution(result.realized_pnl, won);
        } else {
            state.record_resolution(result.realized_pnl, won);
        }
        closed_ids.insert(result.bet_id.clone());

        // Push a "closed" trade entry to the dashboard
        {
            let mut trades = dash.recent_trades.write().await;
            trades.push(TradeLogEntry {
                timestamp: chrono::Utc::now().to_rfc3339(),
                market_id: result.market_id.clone(),
                platform: format!("{}-closed", result.platform),
                side: "CLOSE".to_string(),
                amount: result.realized_pnl.to_f64().unwrap_or(0.0),
                currency: if result.platform == "betfair" { "AUD".to_string() } else { "Mana".to_string() },
                edge_pct: 0.0,
                confidence: 0.0,
                close_reason: Some(result.reason.to_string()),
                final_pnl: Some(result.realized_pnl.to_f64().unwrap_or(0.0)),
            });
            if trades.len() > 200 {
                let excess = trades.len() - 200;
                trades.drain(0..excess);
            }
        }
    }

    // Remove successfully closed bets from open_bets
    if !closed_ids.is_empty() {
        state.open_bets.retain(|b| !closed_ids.contains(&b.order_id));
        // Mirror updated state to dashboard
        *dash.agent.write().await = state.clone();
    }
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
