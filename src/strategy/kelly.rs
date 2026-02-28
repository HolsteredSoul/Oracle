//! Kelly criterion position sizing.
//!
//! Computes optimal bet sizes using fractional Kelly with configurable
//! multiplier, caps, and commission-adjusted edge calculations.

use rust_decimal::Decimal;
use rust_decimal::prelude::*;
use rust_decimal_macros::dec;
use tracing::debug;

use super::edge::Edge;
use crate::types::Side;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Kelly sizing configuration.
#[derive(Debug, Clone)]
pub struct KellyConfig {
    /// Fractional Kelly multiplier (0.25 = quarter-Kelly). Lower = more conservative.
    pub multiplier: Decimal,
    /// Maximum bet as a fraction of bankroll.
    pub max_bet_pct: Decimal,
    /// Minimum bet size in dollars (below this, don't bother).
    pub min_bet_size: Decimal,
    /// Estimated round-trip commission per trade (IB ForecastEx).
    pub commission_per_trade: Decimal,
}

impl Default for KellyConfig {
    fn default() -> Self {
        Self {
            multiplier: dec!(0.25),     // Quarter-Kelly: conservative
            max_bet_pct: dec!(0.06),    // Max 6% of bankroll per trade
            min_bet_size: dec!(1.0),    // $1 minimum
            commission_per_trade: dec!(0.50), // IB estimated round-trip
        }
    }
}

// ---------------------------------------------------------------------------
// Kelly calculator
// ---------------------------------------------------------------------------

/// Sized bet recommendation.
#[derive(Debug, Clone)]
pub struct SizedBet {
    pub edge: Edge,
    pub kelly_fraction: Decimal,    // Raw Kelly fraction
    pub bet_fraction: Decimal,      // After multiplier + caps
    pub bet_amount: Decimal,        // Dollar amount
    pub expected_value: Decimal,    // Edge * bet_amount
}

pub struct KellyCalculator {
    config: KellyConfig,
}

impl KellyCalculator {
    pub fn new(config: KellyConfig) -> Self {
        Self { config }
    }

    /// Access the Kelly configuration.
    pub fn config(&self) -> &KellyConfig {
        &self.config
    }

    /// Size a bet for a detected edge using Kelly criterion.
    ///
    /// Kelly formula: f* = (bp - q) / b
    /// where:
    ///   b = net odds (payout ratio)
    ///   p = estimated win probability
    ///   q = 1 - p
    pub fn size_bet(&self, edge: &Edge, bankroll: Decimal) -> Option<SizedBet> {
        if bankroll <= Decimal::ZERO {
            return None;
        }

        // Determine the effective probabilities
        let (win_prob, market_price) = match edge.side {
            Side::Yes => (edge.estimate.probability, edge.market.current_price_yes),
            Side::No => (Decimal::ONE - edge.estimate.probability, edge.market.current_price_no),
        };

        // Commission-adjusted market price
        let effective_price = market_price + self.config.commission_per_trade / bankroll;
        let effective_price = effective_price.min(dec!(0.99)); // can't exceed 1.0

        // Net odds: what you win per dollar risked
        // Buy YES at price p, win (1-p) if YES, lose p if NO
        let payout_ratio = (Decimal::ONE - effective_price) / effective_price;

        if payout_ratio <= Decimal::ZERO {
            return None;
        }

        let lose_prob = Decimal::ONE - win_prob;

        // Raw Kelly fraction
        let kelly = (payout_ratio * win_prob - lose_prob) / payout_ratio;

        // Negative Kelly means no bet (edge doesn't justify the odds)
        if kelly <= Decimal::ZERO {
            debug!(
                market_id = %edge.market.id,
                kelly = %kelly,
                "Negative Kelly — no bet"
            );
            return None;
        }

        // Apply fractional Kelly (conservative sizing)
        let fractional = kelly * self.config.multiplier;

        // Cap at maximum
        let capped = fractional.min(self.config.max_bet_pct);

        // Convert to dollar amount
        let bet_amount = (capped * bankroll).max(Decimal::ZERO);

        // Floor check
        if bet_amount < self.config.min_bet_size {
            debug!(
                market_id = %edge.market.id,
                bet_amount = %bet_amount,
                min = %self.config.min_bet_size,
                "Bet below minimum size"
            );
            return None;
        }

        let expected_value = edge.edge * bet_amount;

        debug!(
            market_id = %edge.market.id,
            raw_kelly = %format!("{:.2}%", (kelly * dec!(100)).to_f64().unwrap_or(0.0)),
            fractional = %format!("{:.2}%", (capped * dec!(100)).to_f64().unwrap_or(0.0)),
            bet_amount = %format!("${:.2}", bet_amount.to_f64().unwrap_or(0.0)),
            ev = %format!("${:.4}", expected_value.to_f64().unwrap_or(0.0)),
            "Bet sized"
        );

        Some(SizedBet {
            edge: edge.clone(),
            kelly_fraction: kelly,
            bet_fraction: capped,
            bet_amount,
            expected_value,
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Estimate, Market, MarketCategory};
    use chrono::{Duration, Utc};

    fn make_edge(market_price: Decimal, fair_value: Decimal, confidence: Decimal) -> Edge {
        let side = if fair_value > market_price { Side::Yes } else { Side::No };
        let edge_val = (fair_value - market_price).abs();
        Edge {
            market: Market {
                id: "test".into(),
                platform: "manifold".into(),
                question: "Test?".into(),
                description: String::new(),
                category: MarketCategory::Weather,
                current_price_yes: market_price,
                current_price_no: Decimal::ONE - market_price,
                volume_24h: dec!(100),
                liquidity: dec!(500),
                deadline: Utc::now() + Duration::days(30),
                resolution_criteria: String::new(),
                url: String::new(),
                cross_refs: Default::default(),
            },
            estimate: Estimate {
                probability: fair_value,
                confidence,
                reasoning: String::new(),
                tokens_used: 100,
                cost: dec!(0.01),
            },
            side,
            edge: edge_val,
            signed_edge: fair_value - market_price,
        }
    }

    #[test]
    fn test_basic_yes_bet() {
        let calc = KellyCalculator::new(KellyConfig {
            commission_per_trade: Decimal::ZERO,
            ..Default::default()
        });
        // Market at 40%, we think 60%. Big edge.
        let edge = make_edge(dec!(0.40), dec!(0.60), dec!(0.8));
        let sized = calc.size_bet(&edge, dec!(1000));
        assert!(sized.is_some());
        let s = sized.unwrap();
        assert!(s.bet_amount > Decimal::ZERO);
        assert!(s.bet_amount <= dec!(60)); // max 6% of $1000
        assert!(s.kelly_fraction > Decimal::ZERO);
        assert!(matches!(s.edge.side, Side::Yes));
    }

    #[test]
    fn test_basic_no_bet() {
        let calc = KellyCalculator::new(KellyConfig {
            commission_per_trade: Decimal::ZERO,
            ..Default::default()
        });
        // Market at 70%, we think 50%. YES is overpriced → bet NO.
        let edge = make_edge(dec!(0.70), dec!(0.50), dec!(0.8));
        let sized = calc.size_bet(&edge, dec!(1000));
        assert!(sized.is_some());
        assert!(matches!(sized.unwrap().edge.side, Side::No));
    }

    #[test]
    fn test_no_edge_no_bet() {
        let calc = KellyCalculator::new(KellyConfig {
            commission_per_trade: Decimal::ZERO,
            ..Default::default()
        });
        // Market at 50%, we also think 50%. No edge.
        let edge = make_edge(dec!(0.50), dec!(0.50), dec!(0.8));
        let sized = calc.size_bet(&edge, dec!(1000));
        assert!(sized.is_none());
    }

    #[test]
    fn test_zero_bankroll() {
        let calc = KellyCalculator::new(KellyConfig::default());
        let edge = make_edge(dec!(0.40), dec!(0.60), dec!(0.8));
        assert!(calc.size_bet(&edge, Decimal::ZERO).is_none());
    }

    #[test]
    fn test_negative_bankroll() {
        let calc = KellyCalculator::new(KellyConfig::default());
        let edge = make_edge(dec!(0.40), dec!(0.60), dec!(0.8));
        assert!(calc.size_bet(&edge, dec!(-100)).is_none());
    }

    #[test]
    fn test_bet_capped_at_max_pct() {
        let calc = KellyCalculator::new(KellyConfig {
            multiplier: Decimal::ONE,  // Full Kelly (very aggressive)
            max_bet_pct: dec!(0.06),
            commission_per_trade: Decimal::ZERO,
            ..Default::default()
        });
        // Huge edge — Kelly would want a lot, but capped at 6%
        let edge = make_edge(dec!(0.20), dec!(0.80), dec!(0.9));
        let sized = calc.size_bet(&edge, dec!(1000)).unwrap();
        assert_eq!(sized.bet_amount, dec!(60)); // 6% of $1000
        assert_eq!(sized.bet_fraction, dec!(0.06));
    }

    #[test]
    fn test_bet_below_minimum() {
        let calc = KellyCalculator::new(KellyConfig {
            min_bet_size: dec!(5.0),
            commission_per_trade: Decimal::ZERO,
            ..Default::default()
        });
        // Small bankroll + small edge → bet below $5 minimum
        let edge = make_edge(dec!(0.45), dec!(0.55), dec!(0.8));
        let sized = calc.size_bet(&edge, dec!(10));
        assert!(sized.is_none());
    }

    #[test]
    fn test_commission_reduces_bet() {
        let calc_no_comm = KellyCalculator::new(KellyConfig {
            commission_per_trade: Decimal::ZERO,
            ..Default::default()
        });
        let calc_with_comm = KellyCalculator::new(KellyConfig {
            commission_per_trade: dec!(2.0),
            ..Default::default()
        });

        let edge = make_edge(dec!(0.40), dec!(0.60), dec!(0.8));
        let no_comm = calc_no_comm.size_bet(&edge, dec!(100));
        let with_comm = calc_with_comm.size_bet(&edge, dec!(100));

        // Commission should reduce bet size (or eliminate it)
        match (no_comm, with_comm) {
            (Some(a), Some(b)) => assert!(a.bet_amount >= b.bet_amount),
            (Some(_), None) => (), // commission killed the bet entirely — fine
            _ => panic!("no-commission should produce a bet"),
        }
    }

    #[test]
    fn test_quarter_kelly_is_conservative() {
        let quarter = KellyCalculator::new(KellyConfig {
            multiplier: dec!(0.25),
            max_bet_pct: dec!(0.50), // high cap so we see the multiplier difference
            commission_per_trade: Decimal::ZERO,
            ..Default::default()
        });
        let half = KellyCalculator::new(KellyConfig {
            multiplier: dec!(0.50),
            max_bet_pct: dec!(0.50),
            commission_per_trade: Decimal::ZERO,
            ..Default::default()
        });

        let edge = make_edge(dec!(0.40), dec!(0.60), dec!(0.8));
        let q = quarter.size_bet(&edge, dec!(1000)).unwrap();
        let h = half.size_bet(&edge, dec!(1000)).unwrap();

        assert!(q.bet_amount < h.bet_amount, "quarter {} should be less than half {}", q.bet_amount, h.bet_amount);
    }

    #[test]
    fn test_expected_value_positive() {
        let calc = KellyCalculator::new(KellyConfig {
            commission_per_trade: Decimal::ZERO,
            ..Default::default()
        });
        let edge = make_edge(dec!(0.40), dec!(0.60), dec!(0.8));
        let sized = calc.size_bet(&edge, dec!(1000)).unwrap();
        assert!(sized.expected_value > Decimal::ZERO);
    }

    #[test]
    fn test_kelly_config_default() {
        let config = KellyConfig::default();
        assert_eq!(config.multiplier, dec!(0.25));
        assert_eq!(config.max_bet_pct, dec!(0.06));
        assert_eq!(config.min_bet_size, dec!(1.0));
    }
}
