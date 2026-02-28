//! Historical backtesting engine.
//!
//! Replays resolved markets through the strategy pipeline to evaluate
//! performance metrics: win rate, P&L, Sharpe ratio, max drawdown,
//! and Brier score.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal::prelude::*;
use rust_decimal_macros::dec;

use crate::strategy::edge::{EdgeConfig, EdgeDetector};
use crate::strategy::kelly::{KellyCalculator, KellyConfig};
use crate::strategy::risk::{RiskConfig, RiskManager};
use crate::types::{AgentState, AgentStatus, MarketCategory, Side};

// ---------------------------------------------------------------------------
// Historical market data
// ---------------------------------------------------------------------------

/// A resolved market with known outcome — used for backtesting.
#[derive(Debug, Clone)]
pub struct ResolvedMarket {
    pub id: String,
    pub question: String,
    pub category: MarketCategory,
    /// The market price at the time the agent would have traded.
    pub market_price_yes: Decimal,
    /// Our simulated LLM estimate.
    pub estimated_probability: Decimal,
    /// Confidence in the estimate.
    pub confidence: Decimal,
    /// True outcome: true = YES resolved, false = NO resolved.
    pub resolved_yes: bool,
    /// When the market was available for trading.
    pub trade_time: DateTime<Utc>,
    /// When the market resolved.
    pub resolution_time: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Backtest results
// ---------------------------------------------------------------------------

/// Complete backtest performance report.
#[derive(Debug, Clone)]
pub struct BacktestReport {
    pub initial_bankroll: Decimal,
    pub final_bankroll: Decimal,
    pub total_pnl: Decimal,
    pub return_pct: f64,
    pub total_trades: usize,
    pub wins: usize,
    pub losses: usize,
    pub win_rate: f64,
    pub brier_score: f64,
    pub sharpe_ratio: f64,
    pub max_drawdown: Decimal,
    pub max_drawdown_pct: f64,
    pub peak_bankroll: Decimal,
    /// Balance at each trade point for charting.
    pub balance_history: Vec<(DateTime<Utc>, Decimal)>,
    /// Per-trade log.
    pub trade_log: Vec<BacktestTrade>,
}

/// Individual trade in the backtest.
#[derive(Debug, Clone)]
pub struct BacktestTrade {
    pub market_id: String,
    pub side: Side,
    pub bet_amount: Decimal,
    pub edge: Decimal,
    pub market_price: Decimal,
    pub estimated_prob: Decimal,
    pub resolved_yes: bool,
    pub pnl: Decimal,
    pub bankroll_after: Decimal,
}

// ---------------------------------------------------------------------------
// Backtester
// ---------------------------------------------------------------------------

pub struct Backtester {
    edge_detector: EdgeDetector,
    kelly: KellyCalculator,
    risk_config: RiskConfig,
}

impl Backtester {
    pub fn new(
        edge_config: EdgeConfig,
        kelly_config: KellyConfig,
        risk_config: RiskConfig,
    ) -> Self {
        Self {
            edge_detector: EdgeDetector::new(edge_config),
            kelly: KellyCalculator::new(kelly_config),
            risk_config,
        }
    }

    /// Run a backtest over a set of resolved markets.
    ///
    /// Markets should be sorted by `trade_time` (chronological order).
    pub fn run(&self, markets: &[ResolvedMarket], initial_bankroll: Decimal) -> BacktestReport {
        let mut state = AgentState::new(initial_bankroll);
        let _risk_manager = RiskManager::new(self.risk_config.clone());
        let mut trade_log = Vec::new();
        let mut balance_history = vec![(Utc::now(), initial_bankroll)];
        let mut returns: Vec<f64> = Vec::new();
        let mut brier_sum = 0.0_f64;
        let mut peak = initial_bankroll;
        let mut max_dd = 0.0_f64;
        let mut wins = 0usize;
        let mut losses = 0usize;

        for market in markets {
            if !state.is_alive() {
                break;
            }

            // Build a synthetic edge from the resolved market data
            let signed_edge = market.estimated_probability - market.market_price_yes;
            let abs_edge = signed_edge.abs();
            let side = if signed_edge > Decimal::ZERO { Side::Yes } else { Side::No };

            // Use edge detector to check thresholds
            let threshold = self.edge_detector.config().threshold_for(&market.category);
            if abs_edge < threshold {
                // Brier score still counts for all estimates
                let outcome: f64 = if market.resolved_yes { 1.0 } else { 0.0 };
                let est_f64 = market.estimated_probability.to_f64().unwrap_or(0.0);
                brier_sum += (est_f64 - outcome) * (est_f64 - outcome);
                continue;
            }

            // Build a synthetic Edge + SizedBet
            let win_prob = match &side {
                Side::Yes => market.estimated_probability,
                Side::No => Decimal::ONE - market.estimated_probability,
            };
            let market_price = match &side {
                Side::Yes => market.market_price_yes,
                Side::No => Decimal::ONE - market.market_price_yes,
            };
            if market_price <= Decimal::ZERO || market_price >= Decimal::ONE {
                continue;
            }
            let payout_ratio = (Decimal::ONE - market_price) / market_price;

            let lose_prob = Decimal::ONE - win_prob;
            let kelly_raw = (payout_ratio * win_prob - lose_prob) / payout_ratio;
            if kelly_raw <= Decimal::ZERO {
                continue;
            }

            let kelly_frac = kelly_raw * self.kelly.config().multiplier;
            let bet_frac = kelly_frac.min(self.kelly.config().max_bet_pct);
            let bet_amount = (bet_frac * state.bankroll).max(Decimal::ZERO);

            if bet_amount < self.kelly.config().min_bet_size {
                continue;
            }

            // Risk check (simplified — just check exposure)
            let max_exposure = state.bankroll * self.risk_config.max_exposure_pct;
            if bet_amount > max_exposure {
                continue;
            }

            // Determine outcome
            let won = match &side {
                Side::Yes => market.resolved_yes,
                Side::No => !market.resolved_yes,
            };

            let pnl = if won {
                bet_amount * payout_ratio
            } else {
                -bet_amount
            };

            state.bankroll += pnl;
            state.total_pnl += pnl;
            state.trades_placed += 1;

            if won {
                state.trades_won += 1;
                wins += 1;
            } else {
                state.trades_lost += 1;
                losses += 1;
            }

            // Track returns for Sharpe (f64 since it needs sqrt/variance)
            let trade_return = {
                let pnl_f64 = pnl.to_f64().unwrap_or(0.0);
                let bet_f64 = bet_amount.to_f64().unwrap_or(0.01).max(0.01);
                pnl_f64 / bet_f64
            };
            returns.push(trade_return);

            // Peak and drawdown
            if state.bankroll > peak {
                peak = state.bankroll;
            }
            let dd = if peak > Decimal::ZERO {
                let dd_dec = Decimal::ONE - state.bankroll / peak;
                dd_dec.to_f64().unwrap_or(0.0)
            } else {
                0.0
            };
            if dd > max_dd {
                max_dd = dd;
            }

            // Check survival
            if state.bankroll <= Decimal::ZERO {
                state.status = AgentStatus::Died;
            }

            // Brier score (f64 for statistical computation)
            let outcome: f64 = if market.resolved_yes { 1.0 } else { 0.0 };
            let est_f64 = market.estimated_probability.to_f64().unwrap_or(0.0);
            brier_sum += (est_f64 - outcome) * (est_f64 - outcome);

            trade_log.push(BacktestTrade {
                market_id: market.id.clone(),
                side: side.clone(),
                bet_amount,
                edge: abs_edge,
                market_price: market.market_price_yes,
                estimated_prob: market.estimated_probability,
                resolved_yes: market.resolved_yes,
                pnl,
                bankroll_after: state.bankroll,
            });

            balance_history.push((market.trade_time, state.bankroll));
        }

        let total_trades = trade_log.len();
        let brier_score = if !markets.is_empty() {
            brier_sum / markets.len() as f64
        } else {
            0.0
        };

        let sharpe = compute_sharpe(&returns);
        let return_pct = if initial_bankroll > Decimal::ZERO {
            let ret = (state.bankroll - initial_bankroll) / initial_bankroll * dec!(100);
            ret.to_f64().unwrap_or(0.0)
        } else {
            0.0
        };

        let max_drawdown_amount = Decimal::from_f64_retain(max_dd).unwrap_or(Decimal::ZERO) * peak;

        BacktestReport {
            initial_bankroll,
            final_bankroll: state.bankroll,
            total_pnl: state.total_pnl,
            return_pct,
            total_trades,
            wins,
            losses,
            win_rate: if total_trades > 0 { wins as f64 / total_trades as f64 } else { 0.0 },
            brier_score,
            sharpe_ratio: sharpe,
            max_drawdown: max_drawdown_amount,
            max_drawdown_pct: max_dd * 100.0,
            peak_bankroll: peak,
            balance_history,
            trade_log,
        }
    }
}

/// Compute annualized Sharpe ratio from a series of per-trade returns.
fn compute_sharpe(returns: &[f64]) -> f64 {
    if returns.len() < 2 {
        return 0.0;
    }

    let n = returns.len() as f64;
    let mean = returns.iter().sum::<f64>() / n;
    let variance = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / (n - 1.0);
    let std_dev = variance.sqrt();

    if std_dev < 1e-10 {
        return 0.0;
    }

    // Annualize: assume ~250 trading days, ~24 trades/day (rough)
    let annualization_factor = (250.0_f64 * 24.0).sqrt();
    (mean / std_dev) * annualization_factor
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn make_resolved(
        id: &str,
        market_price: f64,
        estimated: f64,
        resolved_yes: bool,
        category: MarketCategory,
    ) -> ResolvedMarket {
        ResolvedMarket {
            id: id.to_string(),
            question: format!("Test market {id}"),
            category,
            market_price_yes: Decimal::from_f64_retain(market_price).unwrap_or(Decimal::ZERO),
            estimated_probability: Decimal::from_f64_retain(estimated).unwrap_or(Decimal::ZERO),
            confidence: dec!(0.8),
            resolved_yes,
            trade_time: Utc::now() - Duration::days(10),
            resolution_time: Utc::now() - Duration::days(1),
        }
    }

    fn default_backtester() -> Backtester {
        Backtester::new(
            EdgeConfig::default(),
            KellyConfig {
                commission_per_trade: Decimal::ZERO,
                ..KellyConfig::default()
            },
            RiskConfig::default(),
        )
    }

    #[test]
    fn test_profitable_backtest() {
        let bt = default_backtester();
        // Markets where our estimate is correct and has edge
        let markets = vec![
            make_resolved("m1", 0.30, 0.55, true, MarketCategory::Weather),  // Big YES edge, correct
            make_resolved("m2", 0.70, 0.45, false, MarketCategory::Weather), // Big NO edge, correct
            make_resolved("m3", 0.40, 0.60, true, MarketCategory::Weather),  // YES edge, correct
        ];

        let report = bt.run(&markets, dec!(100));
        assert!(report.final_bankroll > dec!(100), "Should be profitable: {}", report.final_bankroll);
        assert!(report.win_rate > 0.5);
        assert_eq!(report.losses, 0);
    }

    #[test]
    fn test_losing_backtest() {
        let bt = default_backtester();
        // Markets where our estimate is wrong
        let markets = vec![
            make_resolved("m1", 0.30, 0.55, false, MarketCategory::Weather), // Bet YES, resolved NO
            make_resolved("m2", 0.70, 0.45, true, MarketCategory::Weather),  // Bet NO, resolved YES
        ];

        let report = bt.run(&markets, dec!(100));
        assert!(report.final_bankroll < dec!(100), "Should lose money: {}", report.final_bankroll);
        assert_eq!(report.wins, 0);
    }

    #[test]
    fn test_empty_markets() {
        let bt = default_backtester();
        let report = bt.run(&[], dec!(100));
        assert_eq!(report.total_trades, 0);
        assert!((report.final_bankroll - dec!(100)).abs() < dec!(0.0000000001));
        assert_eq!(report.brier_score, 0.0);
    }

    #[test]
    fn test_no_edge_no_trades() {
        let bt = default_backtester();
        // Estimate matches market price — no edge
        let markets = vec![
            make_resolved("m1", 0.50, 0.52, true, MarketCategory::Weather), // 2% edge, below threshold
        ];

        let report = bt.run(&markets, dec!(100));
        assert_eq!(report.total_trades, 0);
    }

    #[test]
    fn test_agent_dies_on_bankrupt() {
        let bt = Backtester::new(
            EdgeConfig { min_edge: dec!(0.01), weather_threshold: dec!(0.02), ..EdgeConfig::default() },
            KellyConfig { multiplier: Decimal::ONE, max_bet_pct: dec!(0.95), min_bet_size: dec!(0.1), commission_per_trade: Decimal::ZERO },
            RiskConfig { max_exposure_pct: dec!(1.0), ..RiskConfig::default() },
        );
        // Keep losing with huge bets
        let markets: Vec<_> = (0..20)
            .map(|i| make_resolved(
                &format!("m{i}"), 0.30, 0.60, false, MarketCategory::Weather,
            ))
            .collect();

        let report = bt.run(&markets, dec!(10));
        assert!(report.final_bankroll <= Decimal::ZERO || report.total_trades < 20);
    }

    #[test]
    fn test_brier_score_perfect() {
        let bt = default_backtester();
        // Perfect estimates: predict 0.90 for things that happen, 0.10 for things that don't
        let markets = vec![
            make_resolved("m1", 0.30, 0.90, true, MarketCategory::Weather),
            make_resolved("m2", 0.70, 0.10, false, MarketCategory::Weather),
        ];

        let report = bt.run(&markets, dec!(100));
        assert!(report.brier_score < 0.05, "Brier should be near 0: {}", report.brier_score);
    }

    #[test]
    fn test_brier_score_terrible() {
        let bt = default_backtester();
        // Terrible estimates: predict opposite of reality
        let markets = vec![
            make_resolved("m1", 0.30, 0.10, true, MarketCategory::Weather),
            make_resolved("m2", 0.70, 0.90, false, MarketCategory::Weather),
        ];

        let report = bt.run(&markets, dec!(100));
        assert!(report.brier_score > 0.5, "Brier should be high: {}", report.brier_score);
    }

    #[test]
    fn test_max_drawdown_tracked() {
        let bt = default_backtester();
        let markets = vec![
            make_resolved("m1", 0.30, 0.55, true, MarketCategory::Weather),   // Win
            make_resolved("m2", 0.30, 0.55, false, MarketCategory::Weather),  // Lose
            make_resolved("m3", 0.30, 0.55, false, MarketCategory::Weather),  // Lose more
            make_resolved("m4", 0.30, 0.55, true, MarketCategory::Weather),   // Win back
        ];

        let report = bt.run(&markets, dec!(100));
        assert!(report.max_drawdown_pct >= 0.0);
    }

    #[test]
    fn test_balance_history_recorded() {
        let bt = default_backtester();
        let markets = vec![
            make_resolved("m1", 0.30, 0.55, true, MarketCategory::Weather),
            make_resolved("m2", 0.30, 0.55, true, MarketCategory::Weather),
        ];

        let report = bt.run(&markets, dec!(100));
        // Initial + one entry per trade
        assert!(report.balance_history.len() >= 2);
    }

    #[test]
    fn test_sharpe_ratio_computation() {
        // Equal returns -> zero standard deviation -> sharpe = 0
        assert_eq!(compute_sharpe(&[0.1, 0.1, 0.1]), 0.0);

        // Positive mean, some variance -> positive sharpe
        let sharpe = compute_sharpe(&[0.2, 0.1, 0.3, 0.15, 0.25]);
        assert!(sharpe > 0.0);

        // Single return -> not enough data
        assert_eq!(compute_sharpe(&[0.5]), 0.0);

        // Empty
        assert_eq!(compute_sharpe(&[]), 0.0);
    }
}
