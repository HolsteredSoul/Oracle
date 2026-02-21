//! Accountant — cost tracking, P&L, and survival checks.
//!
//! Reconciles each scan cycle: deducts costs, records trade outcomes,
//! updates bankroll, and checks if the agent is still alive.

use chrono::Utc;
use tracing::{info, warn};

use crate::engine::executor::ExecutionReport;
use crate::types::{AgentState, AgentStatus};

// ---------------------------------------------------------------------------
// Cycle cost breakdown
// ---------------------------------------------------------------------------

/// All costs incurred during a single scan cycle.
#[derive(Debug, Clone, Default)]
pub struct CycleCosts {
    /// LLM API cost (token usage).
    pub llm_cost: f64,
    /// Data provider API costs (FRED, NewsAPI, etc.).
    pub data_cost: f64,
    /// IB commissions on executed trades.
    pub ib_commissions: f64,
    /// Any other costs.
    pub other: f64,
}

impl CycleCosts {
    pub fn total(&self) -> f64 {
        self.llm_cost + self.data_cost + self.ib_commissions + self.other
    }
}

// ---------------------------------------------------------------------------
// Cycle report
// ---------------------------------------------------------------------------

/// Summary of a complete scan→estimate→execute cycle.
#[derive(Debug, Clone)]
pub struct CycleReport {
    pub cycle_number: u64,
    pub markets_scanned: usize,
    pub edges_found: usize,
    pub bets_placed: usize,
    pub bets_failed: usize,
    pub total_committed: f64,
    pub cycle_costs: CycleCosts,
    pub bankroll_before: f64,
    pub bankroll_after: f64,
    pub status: AgentStatus,
    pub timestamp: chrono::DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Accountant
// ---------------------------------------------------------------------------

pub struct Accountant;

impl Accountant {
    /// Reconcile a cycle: deduct costs, update state, check survival.
    pub fn reconcile(
        state: &mut AgentState,
        execution: &ExecutionReport,
        costs: &CycleCosts,
    ) -> CycleReport {
        let bankroll_before = state.bankroll;

        // Deduct costs (this also triggers death if bankroll <= 0)
        let api_cost = costs.llm_cost + costs.data_cost + costs.other;
        let alive = state.deduct_cost(api_cost, costs.ib_commissions);

        // Record trades
        state.trades_placed += execution.executed.len() as u64;
        state.cycle_count += 1;

        // Update peak
        state.update_peak();

        if !alive {
            warn!(
                bankroll = state.bankroll,
                total_costs = state.total_costs(),
                "Agent has died — bankroll depleted"
            );
        }

        let report = CycleReport {
            cycle_number: state.cycle_count,
            markets_scanned: 0, // Caller fills this in
            edges_found: 0,     // Caller fills this in
            bets_placed: execution.executed.len(),
            bets_failed: execution.failed.len(),
            total_committed: execution.total_committed,
            cycle_costs: costs.clone(),
            bankroll_before,
            bankroll_after: state.bankroll,
            status: state.status.clone(),
            timestamp: Utc::now(),
        };

        info!(
            cycle = report.cycle_number,
            bankroll = format!("${:.2}", state.bankroll),
            costs = format!("${:.4}", costs.total()),
            bets = report.bets_placed,
            status = ?state.status,
            "Cycle reconciled"
        );

        report
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::executor::{ExecutedTrade, ExecutionReport};
    use crate::types::{Side, TradeReceipt};

    fn make_state(bankroll: f64) -> AgentState {
        AgentState::new(bankroll)
    }

    fn make_execution(count: usize, total: f64) -> ExecutionReport {
        let executed: Vec<ExecutedTrade> = (0..count)
            .map(|i| ExecutedTrade {
                market_id: format!("m{i}"),
                platform: "dry-run".to_string(),
                side: Side::Yes,
                amount: total / count as f64,
                receipt: TradeReceipt::dry_run(&format!("m{i}"), total / count as f64),
            })
            .collect();

        ExecutionReport {
            executed,
            failed: Vec::new(),
            total_committed: total,
            total_commission: 0.0,
        }
    }

    #[test]
    fn test_reconcile_deducts_costs() {
        let mut state = make_state(100.0);
        let execution = make_execution(0, 0.0);
        let costs = CycleCosts {
            llm_cost: 0.05,
            data_cost: 0.01,
            ib_commissions: 0.0,
            other: 0.0,
        };

        let report = Accountant::reconcile(&mut state, &execution, &costs);

        assert!((state.bankroll - 99.94).abs() < 1e-10);
        assert_eq!(report.cycle_number, 1);
        assert!((report.bankroll_before - 100.0).abs() < 1e-10);
        assert!((report.bankroll_after - 99.94).abs() < 1e-10);
    }

    #[test]
    fn test_reconcile_tracks_trades() {
        let mut state = make_state(1000.0);
        let execution = make_execution(3, 150.0);
        let costs = CycleCosts::default();

        Accountant::reconcile(&mut state, &execution, &costs);

        assert_eq!(state.trades_placed, 3);
        assert_eq!(state.cycle_count, 1);
    }

    #[test]
    fn test_reconcile_updates_peak() {
        let mut state = make_state(100.0);
        state.bankroll = 120.0; // Grew
        state.peak_bankroll = 100.0;

        let execution = make_execution(0, 0.0);
        let costs = CycleCosts::default();

        Accountant::reconcile(&mut state, &execution, &costs);

        assert_eq!(state.peak_bankroll, 120.0);
    }

    #[test]
    fn test_reconcile_agent_dies() {
        let mut state = make_state(0.05);
        let execution = make_execution(0, 0.0);
        let costs = CycleCosts {
            llm_cost: 0.10,
            ..Default::default()
        };

        let report = Accountant::reconcile(&mut state, &execution, &costs);

        assert_eq!(state.status, AgentStatus::Died);
        assert_eq!(report.status, AgentStatus::Died);
    }

    #[test]
    fn test_reconcile_accumulates_api_costs() {
        let mut state = make_state(100.0);
        let execution = make_execution(0, 0.0);

        let costs1 = CycleCosts { llm_cost: 0.05, data_cost: 0.01, ..Default::default() };
        Accountant::reconcile(&mut state, &execution, &costs1);

        let costs2 = CycleCosts { llm_cost: 0.03, data_cost: 0.02, ..Default::default() };
        Accountant::reconcile(&mut state, &execution, &costs2);

        assert!((state.total_api_costs - 0.11).abs() < 1e-10);
    }

    #[test]
    fn test_cycle_costs_total() {
        let costs = CycleCosts {
            llm_cost: 0.05,
            data_cost: 0.01,
            ib_commissions: 0.50,
            other: 0.10,
        };
        assert!((costs.total() - 0.66).abs() < 1e-10);
    }

    #[test]
    fn test_cycle_costs_default() {
        let costs = CycleCosts::default();
        assert_eq!(costs.total(), 0.0);
    }
}
