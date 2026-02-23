//! Dashboard API route handlers.
//!
//! All endpoints return JSON. State is shared via `Arc<DashboardState>`.

use axum::{extract::State, http::StatusCode, Json};
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::types::{AgentState, AgentStatus};

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

/// Shared state accessible by all route handlers.
pub struct DashboardState {
    pub agent: RwLock<AgentState>,
    pub cycle_log: RwLock<Vec<CycleLogEntry>>,
    pub balance_history: RwLock<Vec<BalancePoint>>,
    pub recent_trades: RwLock<Vec<TradeLogEntry>>,
}

impl DashboardState {
    pub fn new(initial_state: AgentState) -> Self {
        let initial_balance = initial_state.bankroll;
        Self {
            agent: RwLock::new(initial_state),
            cycle_log: RwLock::new(Vec::new()),
            balance_history: RwLock::new(vec![BalancePoint {
                timestamp: chrono::Utc::now().to_rfc3339(),
                bankroll: initial_balance,
            }]),
            recent_trades: RwLock::new(Vec::new()),
        }
    }
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct StatusResponse {
    pub status: String,
    pub bankroll: f64,
    pub peak_bankroll: f64,
    pub total_pnl: f64,
    pub cycle_count: u64,
    pub trades_placed: u64,
    pub trades_won: u64,
    pub trades_lost: u64,
    pub win_rate: f64,
    pub total_api_costs: f64,
    pub total_ib_commissions: f64,
    pub total_costs: f64,
    pub uptime_secs: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CycleLogEntry {
    pub cycle_number: u64,
    pub timestamp: String,
    pub markets_scanned: usize,
    pub edges_found: usize,
    pub bets_placed: usize,
    pub bets_failed: usize,
    pub cycle_cost: f64,
    pub bankroll_after: f64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BalancePoint {
    pub timestamp: String,
    pub bankroll: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TradeLogEntry {
    pub timestamp: String,
    pub market_id: String,
    pub platform: String,
    pub side: String,
    pub amount: f64,
    pub edge_pct: f64,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CostsResponse {
    pub total_api_costs: f64,
    pub total_ib_commissions: f64,
    pub total_costs: f64,
    pub cost_breakdown: CostBreakdown,
}

#[derive(Debug, Clone, Serialize)]
pub struct CostBreakdown {
    pub llm: f64,
    pub data: f64,
    pub ib_commissions: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetricsResponse {
    pub win_rate: f64,
    pub trades_placed: u64,
    pub trades_won: u64,
    pub trades_lost: u64,
    pub total_pnl: f64,
    pub roi_pct: f64,
    pub cycles_run: u64,
}

// ---------------------------------------------------------------------------
// Route handlers
// ---------------------------------------------------------------------------

pub type AppState = Arc<DashboardState>;

/// GET /api/status
pub async fn get_status(State(state): State<AppState>) -> Json<StatusResponse> {
    let agent = state.agent.read().await;
    let uptime = (chrono::Utc::now() - agent.start_time).num_seconds();
    let win_rate = if agent.trades_placed > 0 {
        agent.trades_won as f64 / agent.trades_placed as f64
    } else {
        0.0
    };

    Json(StatusResponse {
        status: format!("{}", agent.status),
        bankroll: agent.bankroll,
        peak_bankroll: agent.peak_bankroll,
        total_pnl: agent.total_pnl,
        cycle_count: agent.cycle_count,
        trades_placed: agent.trades_placed,
        trades_won: agent.trades_won,
        trades_lost: agent.trades_lost,
        win_rate,
        total_api_costs: agent.total_api_costs,
        total_ib_commissions: agent.total_ib_commissions,
        total_costs: agent.total_costs(),
        uptime_secs: uptime,
    })
}

/// GET /api/cycles
pub async fn get_cycles(State(state): State<AppState>) -> Json<Vec<CycleLogEntry>> {
    let log = state.cycle_log.read().await;
    // Return last 100 cycles
    let start = log.len().saturating_sub(100);
    Json(log[start..].to_vec())
}

/// GET /api/balance-history
pub async fn get_balance_history(State(state): State<AppState>) -> Json<Vec<BalancePoint>> {
    let history = state.balance_history.read().await;
    Json(history.clone())
}

/// GET /api/trades
pub async fn get_trades(State(state): State<AppState>) -> Json<Vec<TradeLogEntry>> {
    let trades = state.recent_trades.read().await;
    let start = trades.len().saturating_sub(100);
    Json(trades[start..].to_vec())
}

/// GET /api/costs
pub async fn get_costs(State(state): State<AppState>) -> Json<CostsResponse> {
    let agent = state.agent.read().await;
    Json(CostsResponse {
        total_api_costs: agent.total_api_costs,
        total_ib_commissions: agent.total_ib_commissions,
        total_costs: agent.total_costs(),
        cost_breakdown: CostBreakdown {
            llm: agent.total_api_costs * 0.8,    // Approximate split
            data: agent.total_api_costs * 0.2,
            ib_commissions: agent.total_ib_commissions,
        },
    })
}

/// GET /api/metrics
pub async fn get_metrics(State(state): State<AppState>) -> Json<MetricsResponse> {
    let agent = state.agent.read().await;
    let initial = agent.bankroll - agent.total_pnl + agent.total_costs();
    let roi = if initial > 0.0 {
        ((agent.bankroll - initial) / initial) * 100.0
    } else {
        0.0
    };

    Json(MetricsResponse {
        win_rate: if agent.trades_placed > 0 {
            agent.trades_won as f64 / agent.trades_placed as f64
        } else {
            0.0
        },
        trades_placed: agent.trades_placed,
        trades_won: agent.trades_won,
        trades_lost: agent.trades_lost,
        total_pnl: agent.total_pnl,
        roi_pct: roi,
        cycles_run: agent.cycle_count,
    })
}

/// GET /health
pub async fn health() -> StatusCode {
    StatusCode::OK
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dashboard_state_creation() {
        let state = DashboardState::new(AgentState::new(100.0));
        // Just verify it constructs without panic
        assert!(true);
    }

    #[test]
    fn test_status_response_serializes() {
        let resp = StatusResponse {
            status: "ALIVE".into(),
            bankroll: 100.0,
            peak_bankroll: 110.0,
            total_pnl: 10.0,
            cycle_count: 5,
            trades_placed: 3,
            trades_won: 2,
            trades_lost: 1,
            win_rate: 0.667,
            total_api_costs: 0.50,
            total_ib_commissions: 1.00,
            total_costs: 1.50,
            uptime_secs: 3600,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("ALIVE"));
        assert!(json.contains("100"));
    }

    #[test]
    fn test_cycle_log_entry_serializes() {
        let entry = CycleLogEntry {
            cycle_number: 1,
            timestamp: "2026-02-21T12:00:00Z".into(),
            markets_scanned: 50,
            edges_found: 3,
            bets_placed: 2,
            bets_failed: 0,
            cycle_cost: 0.05,
            bankroll_after: 99.95,
            status: "ALIVE".into(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("50"));
    }

    #[test]
    fn test_balance_point_serializes() {
        let point = BalancePoint {
            timestamp: "2026-02-21T12:00:00Z".into(),
            bankroll: 105.50,
        };
        let json = serde_json::to_string(&point).unwrap();
        assert!(json.contains("105.5"));
    }

    #[test]
    fn test_costs_response_serializes() {
        let resp = CostsResponse {
            total_api_costs: 1.0,
            total_ib_commissions: 2.0,
            total_costs: 3.0,
            cost_breakdown: CostBreakdown {
                llm: 0.8,
                data: 0.2,
                ib_commissions: 2.0,
            },
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("ib_commissions"));
    }

    #[tokio::test]
    async fn test_get_status_handler() {
        let state = Arc::new(DashboardState::new(AgentState::new(100.0)));
        let Json(resp) = get_status(State(state)).await;
        assert!((resp.bankroll - 100.0).abs() < 1e-10);
        assert!(resp.status.contains("ALIVE"));
    }

    #[tokio::test]
    async fn test_get_cycles_empty() {
        let state = Arc::new(DashboardState::new(AgentState::new(100.0)));
        let Json(cycles) = get_cycles(State(state)).await;
        assert!(cycles.is_empty());
    }

    #[tokio::test]
    async fn test_get_balance_history_initial() {
        let state = Arc::new(DashboardState::new(AgentState::new(50.0)));
        let Json(history) = get_balance_history(State(state)).await;
        assert_eq!(history.len(), 1);
        assert!((history[0].bankroll - 50.0).abs() < 1e-10);
    }

    #[tokio::test]
    async fn test_get_metrics_no_trades() {
        let state = Arc::new(DashboardState::new(AgentState::new(100.0)));
        let Json(metrics) = get_metrics(State(state)).await;
        assert_eq!(metrics.win_rate, 0.0);
        assert_eq!(metrics.trades_placed, 0);
    }
}
