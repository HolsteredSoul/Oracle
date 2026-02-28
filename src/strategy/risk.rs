//! Risk manager.
//!
//! Enforces position limits, category exposure caps, drawdown-adjusted
//! Kelly multiplier, and aggregate exposure limits. Acts as the final
//! gate before trade execution.

use std::collections::HashMap;

use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use super::kelly::SizedBet;
use crate::types::{AgentState, MarketCategory};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Risk management configuration.
#[derive(Debug, Clone)]
pub struct RiskConfig {
    /// Maximum total exposure as fraction of bankroll.
    pub max_exposure_pct: Decimal,
    /// Maximum exposure per category as fraction of bankroll.
    pub max_category_exposure_pct: Decimal,
    /// Maximum number of open positions.
    pub max_positions: usize,
    /// Maximum bets per single scan cycle.
    pub max_bets_per_cycle: usize,
    /// Drawdown threshold to start reducing bets (fraction from peak).
    pub drawdown_warning_pct: Decimal,
    /// Drawdown threshold to halt all betting (fraction from peak).
    pub drawdown_halt_pct: Decimal,
}

impl Default for RiskConfig {
    fn default() -> Self {
        Self {
            max_exposure_pct: dec!(0.60),           // 60% of bankroll
            max_category_exposure_pct: dec!(0.25),  // 25% per category
            max_positions: 20,
            max_bets_per_cycle: 5,
            drawdown_warning_pct: dec!(0.20),       // 20% from peak
            drawdown_halt_pct: dec!(0.40),          // 40% from peak
        }
    }
}

// ---------------------------------------------------------------------------
// Risk manager
// ---------------------------------------------------------------------------

/// Reason a bet was rejected by the risk manager.
#[derive(Debug, Clone)]
pub enum RejectionReason {
    ExposureLimitExceeded { current: Decimal, limit: Decimal },
    CategoryLimitExceeded { category: MarketCategory, current: Decimal, limit: Decimal },
    MaxPositionsReached { current: usize, limit: usize },
    MaxBetsPerCycleReached { current: usize, limit: usize },
    DrawdownHalt { drawdown_pct: Decimal },
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
    category_exposure: HashMap<MarketCategory, Decimal>,
    /// Total current exposure.
    total_exposure: Decimal,
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
            total_exposure: Decimal::ZERO,
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
        total_exposure: Decimal,
        category_exposure: HashMap<MarketCategory, Decimal>,
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
    ) -> Result<Decimal, RejectionReason> {
        let bankroll = state.bankroll;

        // 1. Drawdown check
        let drawdown = self.drawdown_from_peak(state);
        if drawdown >= self.config.drawdown_halt_pct {
            return Err(RejectionReason::DrawdownHalt {
                drawdown_pct: drawdown * dec!(100),
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
                current: (new_total / bankroll) * dec!(100),
                limit: self.config.max_exposure_pct * dec!(100),
            });
        }

        // 5. Category exposure check
        let category = &bet.edge.market.category;
        let current_cat = self.category_exposure.get(category).copied().unwrap_or(Decimal::ZERO);
        let new_cat = current_cat + bet.bet_amount;
        let max_cat = bankroll * self.config.max_category_exposure_pct;
        if new_cat > max_cat {
            return Err(RejectionReason::CategoryLimitExceeded {
                category: category.clone(),
                current: (new_cat / bankroll) * dec!(100),
                limit: self.config.max_category_exposure_pct * dec!(100),
            });
        }

        // 6. Drawdown-adjusted sizing
        let adjusted_amount = self.drawdown_adjust(bet.bet_amount, drawdown);

        Ok(adjusted_amount)
    }

    /// Record that a bet was approved (updates internal counters).
    pub fn record_approval(&mut self, bet: &SizedBet, amount: Decimal) {
        self.total_exposure += amount;
        let cat = &bet.edge.market.category;
        *self.category_exposure.entry(cat.clone()).or_insert(Decimal::ZERO) += amount;
        self.position_count += 1;
        self.cycle_bets += 1;
    }

    /// Compute drawdown from peak as a fraction (0.0 = at peak, 0.5 = 50% below).
    fn drawdown_from_peak(&self, state: &AgentState) -> Decimal {
        if state.peak_bankroll <= Decimal::ZERO {
            return Decimal::ZERO;
        }
        let dd = Decimal::ONE - (state.bankroll / state.peak_bankroll);
        dd.max(Decimal::ZERO)
    }

    /// Reduce bet size proportionally to drawdown severity.
    ///
    /// At no drawdown -> full bet. At drawdown_warning -> 50% bet.
    /// Linear interpolation between.
    fn drawdown_adjust(&self, amount: Decimal, drawdown: Decimal) -> Decimal {
        if drawdown <= Decimal::ZERO {
            return amount;
        }

        let warning = self.config.drawdown_warning_pct;
        if drawdown >= warning {
            // Scale from 50% at warning to 10% at halt
            let halt = self.config.drawdown_halt_pct;
            let ratio = ((halt - drawdown) / (halt - warning)).max(Decimal::ZERO).min(Decimal::ONE);
            amount * (dec!(0.1) + dec!(0.4) * ratio)
        } else {
            // Scale from 100% at 0 to 50% at warning
            let ratio = Decimal::ONE - (drawdown / warning) * dec!(0.5);
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

    fn make_agent_state(bankroll: Decimal, peak: Decimal) -> AgentState {
        AgentState {
            bankroll,
            total_pnl: Decimal::ZERO,
            cycle_count: 10,
            trades_placed: 5,
            trades_won: 3,
            trades_lost: 2,
            total_api_costs: Decimal::ONE,
            total_ib_commissions: dec!(0.5),
            start_time: Utc::now() - Duration::days(7),
            peak_bankroll: peak,
            status: AgentStatus::Alive,
        }
    }

    fn make_sized_bet(category: MarketCategory, amount: Decimal) -> SizedBet {
        SizedBet {
            edge: Edge {
                market: Market {
                    id: "test".into(),
                    platform: "manifold".into(),
                    question: "Test?".into(),
                    description: String::new(),
                    category,
                    current_price_yes: dec!(0.50),
                    current_price_no: dec!(0.50),
                    volume_24h: dec!(100),
                    liquidity: dec!(500),
                    deadline: Utc::now() + Duration::days(30),
                    resolution_criteria: String::new(),
                    url: String::new(),
                    cross_refs: Default::default(),
                },
                estimate: Estimate {
                    probability: dec!(0.65),
                    confidence: dec!(0.8),
                    reasoning: String::new(),
                    tokens_used: 100,
                    cost: dec!(0.01),
                },
                side: Side::Yes,
                edge: dec!(0.15),
                signed_edge: dec!(0.15),
            },
            kelly_fraction: dec!(0.10),
            bet_fraction: dec!(0.05),
            bet_amount: amount,
            expected_value: amount * dec!(0.15),
        }
    }

    #[test]
    fn test_approve_basic() {
        let rm = RiskManager::new(RiskConfig::default());
        let state = make_agent_state(dec!(1000), dec!(1000));
        let bet = make_sized_bet(MarketCategory::Weather, dec!(50));
        let result = rm.approve(&bet, &state);
        assert!(result.is_ok());
        assert!(result.unwrap() > Decimal::ZERO);
    }

    #[test]
    fn test_reject_exposure_limit() {
        let mut rm = RiskManager::new(RiskConfig::default());
        rm.total_exposure = dec!(550); // Already at 55% of $1000
        let state = make_agent_state(dec!(1000), dec!(1000));
        let bet = make_sized_bet(MarketCategory::Weather, dec!(60)); // Would push to 61%
        let result = rm.approve(&bet, &state);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RejectionReason::ExposureLimitExceeded { .. }));
    }

    #[test]
    fn test_reject_category_limit() {
        let mut rm = RiskManager::new(RiskConfig::default());
        rm.category_exposure.insert(MarketCategory::Weather, dec!(240)); // Already at 24%
        let state = make_agent_state(dec!(1000), dec!(1000));
        let bet = make_sized_bet(MarketCategory::Weather, dec!(20)); // Would push to 26%
        let result = rm.approve(&bet, &state);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RejectionReason::CategoryLimitExceeded { .. }));
    }

    #[test]
    fn test_reject_max_positions() {
        let mut rm = RiskManager::new(RiskConfig::default());
        rm.position_count = 20;
        let state = make_agent_state(dec!(1000), dec!(1000));
        let bet = make_sized_bet(MarketCategory::Weather, dec!(50));
        let result = rm.approve(&bet, &state);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RejectionReason::MaxPositionsReached { .. }));
    }

    #[test]
    fn test_reject_max_bets_per_cycle() {
        let mut rm = RiskManager::new(RiskConfig::default());
        rm.cycle_bets = 5;
        let state = make_agent_state(dec!(1000), dec!(1000));
        let bet = make_sized_bet(MarketCategory::Weather, dec!(50));
        let result = rm.approve(&bet, &state);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RejectionReason::MaxBetsPerCycleReached { .. }));
    }

    #[test]
    fn test_reject_drawdown_halt() {
        let rm = RiskManager::new(RiskConfig::default());
        let state = make_agent_state(dec!(550), dec!(1000)); // 45% drawdown, above 40% halt
        let bet = make_sized_bet(MarketCategory::Weather, dec!(10));
        let result = rm.approve(&bet, &state);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RejectionReason::DrawdownHalt { .. }));
    }

    #[test]
    fn test_drawdown_reduces_bet() {
        let rm = RiskManager::new(RiskConfig::default());
        // No drawdown: full amount
        let full = rm.drawdown_adjust(dec!(100), Decimal::ZERO);
        assert_eq!(full, dec!(100));

        // At warning threshold (20%): 50% of amount
        let at_warning = rm.drawdown_adjust(dec!(100), dec!(0.20));
        assert!((at_warning - dec!(50)).abs() < Decimal::ONE);

        // Halfway between warning and halt: ~30%
        let mid = rm.drawdown_adjust(dec!(100), dec!(0.30));
        assert!(mid < at_warning);
        assert!(mid > dec!(10));
    }

    #[test]
    fn test_record_approval() {
        let mut rm = RiskManager::new(RiskConfig::default());
        let bet = make_sized_bet(MarketCategory::Weather, dec!(50));
        rm.record_approval(&bet, dec!(50));

        assert_eq!(rm.total_exposure, dec!(50));
        assert_eq!(rm.position_count, 1);
        assert_eq!(rm.cycle_bets, 1);
        assert_eq!(*rm.category_exposure.get(&MarketCategory::Weather).unwrap(), dec!(50));
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
        let state = make_agent_state(dec!(800), dec!(1000));
        let dd = rm.drawdown_from_peak(&state);
        assert_eq!(dd, dec!(0.2));
    }

    #[test]
    fn test_drawdown_from_peak_no_loss() {
        let rm = RiskManager::new(RiskConfig::default());
        let state = make_agent_state(dec!(1000), dec!(1000));
        let dd = rm.drawdown_from_peak(&state);
        assert_eq!(dd, Decimal::ZERO);
    }

    #[test]
    fn test_risk_config_default() {
        let config = RiskConfig::default();
        assert_eq!(config.max_exposure_pct, dec!(0.60));
        assert_eq!(config.max_category_exposure_pct, dec!(0.25));
        assert_eq!(config.max_positions, 20);
        assert_eq!(config.max_bets_per_cycle, 5);
    }
}
