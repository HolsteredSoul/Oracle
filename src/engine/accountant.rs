//! Accountant — cost tracking, P&L, and survival checks.
//!
//! Reconciles each scan cycle: deducts costs, records trade outcomes,
//! updates bankroll, and checks if the agent is still alive.

use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal::prelude::*;
use rust_decimal_macros::dec;
use tracing::{info, warn};

use crate::engine::executor::ExecutionReport;
use crate::types::{AgentState, AgentStatus};

// ---------------------------------------------------------------------------
// Cycle cost breakdown
// ---------------------------------------------------------------------------

/// All costs incurred during a single scan cycle.
#[derive(Debug, Clone)]
pub struct CycleCosts {
    /// LLM API cost (token usage).
    pub llm_cost: Decimal,
    /// Data provider API costs (FRED, NewsAPI, etc.).
    pub data_cost: Decimal,
    /// IB commissions on executed trades.
    pub ib_commissions: Decimal,
    /// Any other costs.
    pub other: Decimal,
}

impl Default for CycleCosts {
    fn default() -> Self {
        Self {
            llm_cost: Decimal::ZERO,
            data_cost: Decimal::ZERO,
            ib_commissions: Decimal::ZERO,
            other: Decimal::ZERO,
        }
    }
}

impl CycleCosts {
    pub fn total(&self) -> Decimal {
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
    pub total_committed: Decimal,
    pub cycle_costs: CycleCosts,
    pub bankroll_before: Decimal,
    pub bankroll_after: Decimal,
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
                bankroll = %state.bankroll,
                total_costs = %state.total_costs(),
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

    fn make_state(bankroll: Decimal) -> AgentState {
        AgentState::new(bankroll)
    }

    fn make_execution(count: usize, total: Decimal) -> ExecutionReport {
        let per_trade = if count > 0 {
            total / Decimal::from(count)
        } else {
            Decimal::ZERO
        };

        let executed: Vec<ExecutedTrade> = (0..count)
            .map(|i| ExecutedTrade {
                market_id: format!("m{i}"),
                platform: "dry-run".to_string(),
                side: Side::Yes,
                amount: per_trade,
                receipt: TradeReceipt::dry_run(&format!("m{i}"), per_trade),
            })
            .collect();

        ExecutionReport {
            executed,
            failed: Vec::new(),
            total_committed: total,
            total_commission: Decimal::ZERO,
        }
    }

    #[test]
    fn test_reconcile_deducts_costs() {
        let mut state = make_state(dec!(100));
        let execution = make_execution(0, Decimal::ZERO);
        let costs = CycleCosts {
            llm_cost: dec!(0.05),
            data_cost: dec!(0.01),
            ib_commissions: Decimal::ZERO,
            other: Decimal::ZERO,
        };

        let report = Accountant::reconcile(&mut state, &execution, &costs);

        assert_eq!(state.bankroll, dec!(99.94));
        assert_eq!(report.cycle_number, 1);
        assert_eq!(report.bankroll_before, dec!(100));
        assert_eq!(report.bankroll_after, dec!(99.94));
    }

    #[test]
    fn test_reconcile_tracks_trades() {
        let mut state = make_state(dec!(1000));
        let execution = make_execution(3, dec!(150));
        let costs = CycleCosts::default();

        Accountant::reconcile(&mut state, &execution, &costs);

        assert_eq!(state.trades_placed, 3);
        assert_eq!(state.cycle_count, 1);
    }

    #[test]
    fn test_reconcile_updates_peak() {
        let mut state = make_state(dec!(100));
        state.bankroll = dec!(120); // Grew
        state.peak_bankroll = dec!(100);

        let execution = make_execution(0, Decimal::ZERO);
        let costs = CycleCosts::default();

        Accountant::reconcile(&mut state, &execution, &costs);

        assert_eq!(state.peak_bankroll, dec!(120));
    }

    #[test]
    fn test_reconcile_agent_dies() {
        let mut state = make_state(dec!(0.05));
        let execution = make_execution(0, Decimal::ZERO);
        let costs = CycleCosts {
            llm_cost: dec!(0.10),
            ..Default::default()
        };

        let report = Accountant::reconcile(&mut state, &execution, &costs);

        assert_eq!(state.status, AgentStatus::Died);
        assert_eq!(report.status, AgentStatus::Died);
    }

    #[test]
    fn test_reconcile_accumulates_api_costs() {
        let mut state = make_state(dec!(100));
        let execution = make_execution(0, Decimal::ZERO);

        let costs1 = CycleCosts { llm_cost: dec!(0.05), data_cost: dec!(0.01), ..Default::default() };
        Accountant::reconcile(&mut state, &execution, &costs1);

        let costs2 = CycleCosts { llm_cost: dec!(0.03), data_cost: dec!(0.02), ..Default::default() };
        Accountant::reconcile(&mut state, &execution, &costs2);

        assert_eq!(state.total_api_costs, dec!(0.11));
    }

    #[test]
    fn test_cycle_costs_total() {
        let costs = CycleCosts {
            llm_cost: dec!(0.05),
            data_cost: dec!(0.01),
            ib_commissions: dec!(0.50),
            other: dec!(0.10),
        };
        assert_eq!(costs.total(), dec!(0.66));
    }

    #[test]
    fn test_cycle_costs_default() {
        let costs = CycleCosts::default();
        assert_eq!(costs.total(), Decimal::ZERO);
    }
}
