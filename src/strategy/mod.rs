//! Strategy engine — edge detection, Kelly sizing, and risk management.

pub mod edge;
pub mod kelly;
pub mod risk;

use rust_decimal::Decimal;
use rust_decimal::prelude::*;
use rust_decimal_macros::dec;
use tracing::{debug, info, warn};

use crate::types::{AgentState, BetDecision, Estimate, Market};
use edge::{Edge, EdgeDetector};
use kelly::{KellyCalculator, SizedBet};
use risk::{RejectionReason, RiskManager};

// ---------------------------------------------------------------------------
// Decision log
// ---------------------------------------------------------------------------

/// Record of every decision made (or skipped) during a strategy pass.
/// Kept for analysis and transparency — including opportunities that were
/// passed on and the reason why.
#[derive(Debug, Clone)]
pub enum DecisionRecord {
    /// Bet selected and queued for execution.
    Selected {
        bet: SizedBet,
        /// Final amount after drawdown adjustment.
        adjusted_amount: Decimal,
    },
    /// Edge detected but Kelly sizing returned None (negative or zero Kelly).
    KellyRejected { edge: Edge },
    /// Sized bet blocked by the risk manager.
    RiskRejected {
        bet: SizedBet,
        reason: RejectionReason,
    },
}

// ---------------------------------------------------------------------------
// Orchestrator
// ---------------------------------------------------------------------------

/// Pipelines edge detection -> Kelly sizing -> risk approval -> bet selection.
///
/// Instantiate once per agent; call `reset_cycle` at the start of each scan
/// cycle, then `select_bets` with the LLM estimates for that cycle.
pub struct StrategyOrchestrator {
    edge_detector: EdgeDetector,
    kelly: KellyCalculator,
    risk: RiskManager,
}

impl StrategyOrchestrator {
    pub fn new(edge_detector: EdgeDetector, kelly: KellyCalculator, risk: RiskManager) -> Self {
        Self {
            edge_detector,
            kelly,
            risk,
        }
    }

    /// Reset per-cycle counters (call once at the start of every scan cycle).
    pub fn reset_cycle(&mut self) {
        self.risk.reset_cycle();
    }

    /// Run the full strategy pipeline for a batch of LLM estimates.
    ///
    /// Steps:
    /// 1. Detect actionable edges (above category thresholds).
    /// 2. Kelly-size each edge.
    /// 3. Rank survivors by composite score: `expected_value * confidence`.
    /// 4. Approve in rank order through the risk manager (enforces cycle
    ///    limit, exposure caps, drawdown halt, etc.).
    ///
    /// Returns the approved bets (ready for `Executor::execute_batch`) and a
    /// complete decision log including all rejected opportunities.
    pub fn select_bets(
        &mut self,
        estimates: &[(Market, Estimate)],
        state: &AgentState,
    ) -> (Vec<SizedBet>, Vec<DecisionRecord>) {
        let mut decisions: Vec<DecisionRecord> = Vec::new();

        // Step 1 – edge detection (sorted by edge size descending internally)
        let edges = self.edge_detector.find_edges(estimates);
        info!(
            markets_in = estimates.len(),
            edges_found = edges.len(),
            "Edge detection complete"
        );

        // Step 2 – Kelly sizing
        let mut sized: Vec<SizedBet> = Vec::new();
        for edge in edges {
            match self.kelly.size_bet(&edge, state.bankroll) {
                Some(bet) => sized.push(bet),
                None => {
                    debug!(
                        market_id = %edge.market.id,
                        edge = %format!("{:.1}%", (edge.edge * dec!(100)).to_f64().unwrap_or(0.0)),
                        "Kelly rejected (negative/zero Kelly fraction)"
                    );
                    decisions.push(DecisionRecord::KellyRejected { edge });
                }
            }
        }

        // Step 3 – rank by composite score (expected value * confidence)
        // Higher score -> higher priority for scarce risk budget.
        sized.sort_by(|a, b| {
            let score_a = a.expected_value * a.edge.estimate.confidence;
            let score_b = b.expected_value * b.edge.estimate.confidence;
            score_b.cmp(&score_a)
        });

        // Step 4 – risk approval in rank order
        let mut selected: Vec<SizedBet> = Vec::new();
        for bet in sized {
            match self.risk.approve(&bet, state) {
                Ok(adjusted_amount) => {
                    info!(
                        market_id = %bet.edge.market.id,
                        side = ?bet.edge.side,
                        original = %format!("${:.2}", bet.bet_amount.to_f64().unwrap_or(0.0)),
                        adjusted = %format!("${:.2}", adjusted_amount.to_f64().unwrap_or(0.0)),
                        ev = %format!("${:.4}", bet.expected_value.to_f64().unwrap_or(0.0)),
                        confidence = %format!("{:.0}%", (bet.edge.estimate.confidence * dec!(100)).to_f64().unwrap_or(0.0)),
                        "Bet approved"
                    );
                    self.risk.record_approval(&bet, adjusted_amount);
                    let mut approved = bet.clone();
                    approved.bet_amount = adjusted_amount;
                    decisions.push(DecisionRecord::Selected {
                        bet: approved.clone(),
                        adjusted_amount,
                    });
                    selected.push(approved);
                }
                Err(reason) => {
                    warn!(
                        market_id = %bet.edge.market.id,
                        reason = %reason,
                        "Bet rejected by risk manager"
                    );
                    decisions.push(DecisionRecord::RiskRejected { bet, reason });
                }
            }
        }

        info!(
            selected = selected.len(),
            total_estimates = estimates.len(),
            "Strategy cycle complete"
        );

        (selected, decisions)
    }

    /// Convert a slice of approved bets to `BetDecision`s for logging or
    /// persistence.  Data sources are not tracked at the strategy layer so
    /// that field is left empty; callers may populate it if desired.
    pub fn to_bet_decisions(bets: &[SizedBet]) -> Vec<BetDecision> {
        bets.iter()
            .map(|b| BetDecision {
                market: b.edge.market.clone(),
                side: b.edge.side,
                fair_value: b.edge.estimate.probability,
                edge: b.edge.edge,
                kelly_fraction: b.kelly_fraction,
                bet_amount: b.bet_amount,
                confidence: b.edge.estimate.confidence,
                rationale: b.edge.estimate.reasoning.clone(),
                data_sources_used: Vec::new(),
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strategy::edge::EdgeConfig;
    use crate::strategy::kelly::KellyConfig;
    use crate::strategy::risk::RiskConfig;
    use crate::types::{AgentState, AgentStatus, Estimate, Market, MarketCategory};
    use chrono::{Duration, Utc};

    // ---- helpers -----------------------------------------------------------

    fn make_market(id: &str, category: MarketCategory, price_yes: Decimal) -> Market {
        Market {
            id: id.to_string(),
            platform: "manifold".to_string(),
            question: format!("Test market {id}"),
            description: String::new(),
            category,
            current_price_yes: price_yes,
            current_price_no: Decimal::ONE - price_yes,
            volume_24h: dec!(1000),
            liquidity: dec!(5000),
            deadline: Utc::now() + Duration::days(30),
            resolution_criteria: String::new(),
            url: String::new(),
            cross_refs: Default::default(),
        }
    }

    fn make_estimate(probability: Decimal, confidence: Decimal) -> Estimate {
        Estimate {
            probability,
            confidence,
            reasoning: "test reasoning".to_string(),
            tokens_used: 100,
            cost: dec!(0.01),
        }
    }

    fn make_state(bankroll: Decimal) -> AgentState {
        AgentState {
            bankroll,
            total_pnl: Decimal::ZERO,
            cycle_count: 0,
            trades_placed: 0,
            trades_won: 0,
            trades_lost: 0,
            total_api_costs: Decimal::ZERO,
            total_ib_commissions: Decimal::ZERO,
            start_time: Utc::now(),
            peak_bankroll: bankroll,
            status: AgentStatus::Alive,
        }
    }

    fn make_orchestrator() -> StrategyOrchestrator {
        StrategyOrchestrator::new(
            EdgeDetector::new(EdgeConfig::default()),
            KellyCalculator::new(KellyConfig {
                commission_per_trade: Decimal::ZERO,
                ..KellyConfig::default()
            }),
            RiskManager::new(RiskConfig::default()),
        )
    }

    // ---- tests -------------------------------------------------------------

    #[test]
    fn test_no_estimates_returns_empty() {
        let mut orc = make_orchestrator();
        let state = make_state(dec!(1000));
        let (bets, decisions) = orc.select_bets(&[], &state);
        assert!(bets.is_empty());
        assert!(decisions.is_empty());
    }

    #[test]
    fn test_below_threshold_produces_no_bet() {
        let mut orc = make_orchestrator();
        let state = make_state(dec!(1000));
        // 4% edge, below the 6% Weather threshold -> no edge detected at all
        let estimates = vec![(
            make_market("m1", MarketCategory::Weather, dec!(0.50)),
            make_estimate(dec!(0.54), dec!(0.9)),
        )];
        let (bets, decisions) = orc.select_bets(&estimates, &state);
        assert!(bets.is_empty());
        // No decisions logged because edge was filtered before the decision log
        assert!(decisions.is_empty());
    }

    #[test]
    fn test_strong_edge_produces_bet() {
        let mut orc = make_orchestrator();
        let state = make_state(dec!(1000));
        // 20% edge (market at 40%, estimate at 60%) -> well above thresholds
        let estimates = vec![(
            make_market("m1", MarketCategory::Weather, dec!(0.40)),
            make_estimate(dec!(0.60), dec!(0.8)),
        )];
        let (bets, decisions) = orc.select_bets(&estimates, &state);
        assert_eq!(bets.len(), 1);
        assert!(bets[0].bet_amount > Decimal::ZERO);
        assert!(matches!(decisions[0], DecisionRecord::Selected { .. }));
    }

    #[test]
    fn test_bets_ranked_by_ev_times_confidence() {
        let mut orc = make_orchestrator();
        // Large bankroll so exposure limits don't interfere
        let state = make_state(dec!(10_000));

        let estimates = vec![
            // Small edge -> lower composite score
            (
                make_market("low_score", MarketCategory::Weather, dec!(0.40)),
                make_estimate(dec!(0.47), dec!(0.9)), // 7% edge
            ),
            // Large edge, high confidence -> highest composite score
            (
                make_market("high_score", MarketCategory::Weather, dec!(0.40)),
                make_estimate(dec!(0.70), dec!(0.9)), // 30% edge
            ),
            // Medium edge, moderate confidence
            (
                make_market("mid_score", MarketCategory::Weather, dec!(0.40)),
                make_estimate(dec!(0.58), dec!(0.7)), // 18% edge, lower confidence
            ),
        ];

        let (bets, _) = orc.select_bets(&estimates, &state);
        assert!(!bets.is_empty());
        assert_eq!(bets[0].edge.market.id, "high_score");
    }

    #[test]
    fn test_kelly_rejection_logged() {
        // Floor so high that no bet survives Kelly sizing
        let mut orc = StrategyOrchestrator::new(
            EdgeDetector::new(EdgeConfig::default()),
            KellyCalculator::new(KellyConfig {
                commission_per_trade: Decimal::ZERO,
                min_bet_size: dec!(1_000_000),
                ..KellyConfig::default()
            }),
            RiskManager::new(RiskConfig::default()),
        );
        let state = make_state(dec!(1000));
        let estimates = vec![(
            make_market("m1", MarketCategory::Weather, dec!(0.40)),
            make_estimate(dec!(0.60), dec!(0.8)),
        )];
        let (bets, decisions) = orc.select_bets(&estimates, &state);
        assert!(bets.is_empty());
        assert!(decisions
            .iter()
            .any(|d| matches!(d, DecisionRecord::KellyRejected { .. })));
    }

    #[test]
    fn test_risk_rejection_logged_when_cycle_limit_hit() {
        let mut orc = make_orchestrator();
        let state = make_state(dec!(1000));

        // 6 identical bets: risk manager allows max 5 per cycle
        let estimates: Vec<_> = (0..6)
            .map(|i| {
                (
                    make_market(&format!("m{i}"), MarketCategory::Weather, dec!(0.40)),
                    make_estimate(dec!(0.60), dec!(0.8)),
                )
            })
            .collect();

        let (bets, decisions) = orc.select_bets(&estimates, &state);
        assert!(bets.len() <= 5);
        assert!(decisions
            .iter()
            .any(|d| matches!(d, DecisionRecord::RiskRejected { .. })));
    }

    #[test]
    fn test_to_bet_decisions_conversion() {
        let mut orc = make_orchestrator();
        let state = make_state(dec!(1000));
        let estimates = vec![(
            make_market("m1", MarketCategory::Weather, dec!(0.40)),
            make_estimate(dec!(0.60), dec!(0.8)),
        )];
        let (bets, _) = orc.select_bets(&estimates, &state);
        let decisions = StrategyOrchestrator::to_bet_decisions(&bets);
        assert_eq!(decisions.len(), bets.len());
        if let Some(d) = decisions.first() {
            assert_eq!(d.market.id, "m1");
            assert_eq!(d.fair_value, dec!(0.60));
            assert!(d.bet_amount > Decimal::ZERO);
        }
    }

    #[test]
    fn test_reset_cycle_allows_new_bets() {
        let mut orc = make_orchestrator();
        let state = make_state(dec!(10_000));

        // Spread across different categories to avoid the per-category exposure
        // cap (25%) before reaching the cycle limit (5). 20% edge is above all
        // category thresholds (highest is Politics at 12%).
        let categories = [
            MarketCategory::Weather,
            MarketCategory::Sports,
            MarketCategory::Economics,
            MarketCategory::Politics,
            MarketCategory::Culture,
        ];
        let estimates: Vec<_> = (0..5)
            .map(|i| {
                (
                    make_market(&format!("m{i}"), categories[i], dec!(0.40)),
                    make_estimate(dec!(0.60), dec!(0.8)),
                )
            })
            .collect();

        // Fill the cycle limit (5 bets)
        let (bets_first, _) = orc.select_bets(&estimates, &state);
        assert_eq!(bets_first.len(), 5);

        // After reset, a new cycle can approve bets again
        orc.reset_cycle();
        let estimates2 = vec![(
            make_market("new", MarketCategory::Weather, dec!(0.40)),
            make_estimate(dec!(0.60), dec!(0.8)),
        )];
        let (bets_second, _) = orc.select_bets(&estimates2, &state);
        assert_eq!(bets_second.len(), 1);
    }

    #[test]
    fn test_drawdown_halt_blocks_all_bets() {
        let mut orc = make_orchestrator();
        // 45% drawdown exceeds the 40% halt threshold
        let mut state = make_state(dec!(550));
        state.peak_bankroll = dec!(1000);
        let estimates = vec![(
            make_market("m1", MarketCategory::Weather, dec!(0.40)),
            make_estimate(dec!(0.60), dec!(0.8)),
        )];
        let (bets, decisions) = orc.select_bets(&estimates, &state);
        assert!(bets.is_empty());
        assert!(decisions
            .iter()
            .any(|d| matches!(d, DecisionRecord::RiskRejected { .. })));
    }
}
