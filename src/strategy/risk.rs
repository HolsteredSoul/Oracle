//! Risk manager.
//!
//! Enforces position limits, category exposure caps, drawdown-adjusted
//! Kelly multiplier, and aggregate exposure limits. Acts as the final
//! gate before trade execution.

use std::collections::HashMap;

use super::kelly::SizedBet;
use crate::types::{AgentState, MarketCategory};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Risk management configuration.
#[derive(Debug, Clone)]
pub struct RiskConfig {
    /// Maximum total exposure as fraction of bankroll.
    pub max_exposure_pct: f64,
    /// Maximum exposure per category as fraction of bankroll.
    pub max_category_exposure_pct: f64,
    /// Maximum number of open positions.
    pub max_positions: usize,
    /// Maximum bets per single scan cycle.
    pub max_bets_per_cycle: usize,
    /// Drawdown threshold to start reducing bets (fraction from peak).
    pub drawdown_warning_pct: f64,
    /// Drawdown threshold to halt all betting (fraction from peak).
    pub drawdown_halt_pct: f64,
}

impl Default for RiskConfig {
    fn default() -> Self {
        Self {
            max_exposure_pct: 0.60,           // 60% of bankroll
            max_category_exposure_pct: 0.25,  // 25% per category
            max_positions: 20,
            max_bets_per_cycle: 5,
            drawdown_warning_pct: 0.20,       // 20% from peak
            drawdown_halt_pct: 0.40,          // 40% from peak
        }
    }
}

// ---------------------------------------------------------------------------
// Risk manager
// ---------------------------------------------------------------------------

/// Reason a bet was rejected by the risk manager.
#[derive(Debug, Clone)]
pub enum RejectionReason {
    ExposureLimitExceeded { current: f64, limit: f64 },
    CategoryLimitExceeded { category: MarketCategory, current: f64, limit: f64 },
    MaxPositionsReached { current: usize, limit: usize },
    MaxBetsPerCycleReached { current: usize, limit: usize },
    DrawdownHalt { drawdown_pct: f64 },
}

impl std::fmt::Display for RejectionReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ExposureLimitExceeded { current, limit } =>
                write!(f, "Total exposure {current:.0}% exceeds {limit:.0}% limit"),
            Self::CategoryLimitExceeded { category, current, limit } =>
                write!(f, "{category:?} exposure {current:.0}% exceeds {limit:.0}% limit"),
            Self::MaxPositionsReached { current, limit } =>
                write!(f, "{current} positions at {limit} limit"),
            Self::MaxBetsPerCycleReached { current, limit } =>
                write!(f, "{current} bets this cycle at {limit} limit"),
            Self::DrawdownHalt { drawdown_pct } =>
                write!(f, "Drawdown halt: {drawdown_pct:.1}% from peak"),
        }
    }
}

pub struct RiskManager {
    config: RiskConfig,
    /// Currently tracked exposure per category (updated as bets are approved).
    category_exposure: HashMap<MarketCategory, f64>,
    /// Total current exposure.
    total_exposure: f64,
    /// Number of open positions.
    position_count: usize,
    /// Bets approved this cycle.
    cycle_bets: usize,
}

impl RiskManager {
    pub fn new(config: RiskConfig) -> Self {
        Self {
            config,
            category_exposure: HashMap::new(),
            total_exposure: 0.0,
            position_count: 0,
            cycle_bets: 0,
        }
    }

    /// Reset cycle counter (call at start of each scan cycle).
    pub fn reset_cycle(&mut self) {
        self.cycle_bets = 0;
    }

    /// Update current exposure state from agent state.
    /// Call this at the start of each cycle with current positions.
    pub fn update_exposure(
        &mut self,
        total_exposure: f64,
        category_exposure: HashMap<MarketCategory, f64>,
        position_count: usize,
    ) {
        self.total_exposure = total_exposure;
        self.category_exposure = category_exposure;
        self.position_count = position_count;
    }

    /// Check if a sized bet passes all risk checks.
    ///
    /// Returns Ok(drawdown-adjusted bet amount) or Err(reason).
    pub fn approve(
        &self,
        bet: &SizedBet,
        state: &AgentState,
    ) -> Result<f64, RejectionReason> {
        let bankroll = state.bankroll;

        // 1. Drawdown check
        let drawdown = self.drawdown_from_peak(state);
        if drawdown >= self.config.drawdown_halt_pct {
            return Err(RejectionReason::DrawdownHalt {
                drawdown_pct: drawdown * 100.0,
            });
        }

        // 2. Max positions
        if self.position_count >= self.config.max_positions {
            return Err(RejectionReason::MaxPositionsReached {
                current: self.position_count,
                limit: self.config.max_positions,
            });
        }

        // 3. Max bets per cycle
        if self.cycle_bets >= self.config.max_bets_per_cycle {
            return Err(RejectionReason::MaxBetsPerCycleReached {
                current: self.cycle_bets,
                limit: self.config.max_bets_per_cycle,
            });
        }

        // 4. Total exposure check
        let new_total = self.total_exposure + bet.bet_amount;
        let max_exposure = bankroll * self.config.max_exposure_pct;
        if new_total > max_exposure {
            return Err(RejectionReason::ExposureLimitExceeded {
                current: (new_total / bankroll) * 100.0,
                limit: self.config.max_exposure_pct * 100.0,
            });
        }

        // 5. Category exposure check
        let category = &bet.edge.market.category;
        let current_cat = self.category_exposure.get(category).copied().unwrap_or(0.0);
        let new_cat = current_cat + bet.bet_amount;
        let max_cat = bankroll * self.config.max_category_exposure_pct;
        if new_cat > max_cat {
            return Err(RejectionReason::CategoryLimitExceeded {
                category: category.clone(),
                current: (new_cat / bankroll) * 100.0,
                limit: self.config.max_category_exposure_pct * 100.0,
            });
        }

        // 6. Drawdown-adjusted sizing
        let adjusted_amount = self.drawdown_adjust(bet.bet_amount, drawdown);

        Ok(adjusted_amount)
    }

    /// Record that a bet was approved (updates internal counters).
    pub fn record_approval(&mut self, bet: &SizedBet, amount: f64) {
        self.total_exposure += amount;
        let cat = &bet.edge.market.category;
        *self.category_exposure.entry(cat.clone()).or_insert(0.0) += amount;
        self.position_count += 1;
        self.cycle_bets += 1;
    }

    /// Compute drawdown from peak as a fraction (0.0 = at peak, 0.5 = 50% below).
    fn drawdown_from_peak(&self, state: &AgentState) -> f64 {
        if state.peak_bankroll <= 0.0 {
            return 0.0;
        }
        let dd = 1.0 - (state.bankroll / state.peak_bankroll);
        dd.max(0.0)
    }

    /// Reduce bet size proportionally to drawdown severity.
    ///
    /// At no drawdown → full bet. At drawdown_warning → 50% bet.
    /// Linear interpolation between.
    fn drawdown_adjust(&self, amount: f64, drawdown: f64) -> f64 {
        if drawdown <= 0.0 {
            return amount;
        }

        let warning = self.config.drawdown_warning_pct;
        if drawdown >= warning {
            // Scale from 50% at warning to 10% at halt
            let halt = self.config.drawdown_halt_pct;
            let ratio = ((halt - drawdown) / (halt - warning)).clamp(0.0, 1.0);
            amount * (0.1 + 0.4 * ratio)
        } else {
            // Scale from 100% at 0 to 50% at warning
            let ratio = 1.0 - (drawdown / warning) * 0.5;
            amount * ratio
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strategy::edge::Edge;
    use crate::strategy::kelly::SizedBet;
    use crate::types::*;
    use chrono::{Duration, Utc};

    fn make_agent_state(bankroll: f64, peak: f64) -> AgentState {
        AgentState {
            bankroll,
            total_pnl: 0.0,
            cycle_count: 10,
            trades_placed: 5,
            trades_won: 3,
            trades_lost: 2,
            total_api_costs: 1.0,
            total_ib_commissions: 0.5,
            start_time: Utc::now() - Duration::days(7),
            peak_bankroll: peak,
            status: AgentStatus::Alive,
        }
    }

    fn make_sized_bet(category: MarketCategory, amount: f64) -> SizedBet {
        SizedBet {
            edge: Edge {
                market: Market {
                    id: "test".into(),
                    platform: "manifold".into(),
                    question: "Test?".into(),
                    description: String::new(),
                    category,
                    current_price_yes: 0.50,
                    current_price_no: 0.50,
                    volume_24h: 100.0,
                    liquidity: 500.0,
                    deadline: Utc::now() + Duration::days(30),
                    resolution_criteria: String::new(),
                    url: String::new(),
                    cross_refs: Default::default(),
                },
                estimate: Estimate {
                    probability: 0.65,
                    confidence: 0.8,
                    reasoning: String::new(),
                    tokens_used: 100,
                    cost: 0.01,
                },
                side: Side::Yes,
                edge: 0.15,
                signed_edge: 0.15,
            },
            kelly_fraction: 0.10,
            bet_fraction: 0.05,
            bet_amount: amount,
            expected_value: amount * 0.15,
        }
    }

    #[test]
    fn test_approve_basic() {
        let rm = RiskManager::new(RiskConfig::default());
        let state = make_agent_state(1000.0, 1000.0);
        let bet = make_sized_bet(MarketCategory::Weather, 50.0);
        let result = rm.approve(&bet, &state);
        assert!(result.is_ok());
        assert!(result.unwrap() > 0.0);
    }

    #[test]
    fn test_reject_exposure_limit() {
        let mut rm = RiskManager::new(RiskConfig::default());
        rm.total_exposure = 550.0; // Already at 55% of $1000
        let state = make_agent_state(1000.0, 1000.0);
        let bet = make_sized_bet(MarketCategory::Weather, 60.0); // Would push to 61%
        let result = rm.approve(&bet, &state);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RejectionReason::ExposureLimitExceeded { .. }));
    }

    #[test]
    fn test_reject_category_limit() {
        let mut rm = RiskManager::new(RiskConfig::default());
        rm.category_exposure.insert(MarketCategory::Weather, 240.0); // Already at 24%
        let state = make_agent_state(1000.0, 1000.0);
        let bet = make_sized_bet(MarketCategory::Weather, 20.0); // Would push to 26%
        let result = rm.approve(&bet, &state);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RejectionReason::CategoryLimitExceeded { .. }));
    }

    #[test]
    fn test_reject_max_positions() {
        let mut rm = RiskManager::new(RiskConfig::default());
        rm.position_count = 20;
        let state = make_agent_state(1000.0, 1000.0);
        let bet = make_sized_bet(MarketCategory::Weather, 50.0);
        let result = rm.approve(&bet, &state);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RejectionReason::MaxPositionsReached { .. }));
    }

    #[test]
    fn test_reject_max_bets_per_cycle() {
        let mut rm = RiskManager::new(RiskConfig::default());
        rm.cycle_bets = 5;
        let state = make_agent_state(1000.0, 1000.0);
        let bet = make_sized_bet(MarketCategory::Weather, 50.0);
        let result = rm.approve(&bet, &state);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RejectionReason::MaxBetsPerCycleReached { .. }));
    }

    #[test]
    fn test_reject_drawdown_halt() {
        let rm = RiskManager::new(RiskConfig::default());
        let state = make_agent_state(550.0, 1000.0); // 45% drawdown, above 40% halt
        let bet = make_sized_bet(MarketCategory::Weather, 10.0);
        let result = rm.approve(&bet, &state);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RejectionReason::DrawdownHalt { .. }));
    }

    #[test]
    fn test_drawdown_reduces_bet() {
        let rm = RiskManager::new(RiskConfig::default());
        // No drawdown: full amount
        let full = rm.drawdown_adjust(100.0, 0.0);
        assert_eq!(full, 100.0);

        // At warning threshold (20%): 50% of amount
        let at_warning = rm.drawdown_adjust(100.0, 0.20);
        assert!((at_warning - 50.0).abs() < 1.0);

        // Halfway between warning and halt: ~30%
        let mid = rm.drawdown_adjust(100.0, 0.30);
        assert!(mid < at_warning);
        assert!(mid > 10.0);
    }

    #[test]
    fn test_record_approval() {
        let mut rm = RiskManager::new(RiskConfig::default());
        let bet = make_sized_bet(MarketCategory::Weather, 50.0);
        rm.record_approval(&bet, 50.0);

        assert_eq!(rm.total_exposure, 50.0);
        assert_eq!(rm.position_count, 1);
        assert_eq!(rm.cycle_bets, 1);
        assert_eq!(*rm.category_exposure.get(&MarketCategory::Weather).unwrap(), 50.0);
    }

    #[test]
    fn test_reset_cycle() {
        let mut rm = RiskManager::new(RiskConfig::default());
        rm.cycle_bets = 5;
        rm.reset_cycle();
        assert_eq!(rm.cycle_bets, 0);
    }

    #[test]
    fn test_drawdown_from_peak() {
        let rm = RiskManager::new(RiskConfig::default());
        let state = make_agent_state(800.0, 1000.0);
        let dd = rm.drawdown_from_peak(&state);
        assert!((dd - 0.20).abs() < 1e-10);
    }

    #[test]
    fn test_drawdown_from_peak_no_loss() {
        let rm = RiskManager::new(RiskConfig::default());
        let state = make_agent_state(1000.0, 1000.0);
        let dd = rm.drawdown_from_peak(&state);
        assert_eq!(dd, 0.0);
    }

    #[test]
    fn test_risk_config_default() {
        let config = RiskConfig::default();
        assert_eq!(config.max_exposure_pct, 0.60);
        assert_eq!(config.max_category_exposure_pct, 0.25);
        assert_eq!(config.max_positions, 20);
        assert_eq!(config.max_bets_per_cycle, 5);
    }
}
