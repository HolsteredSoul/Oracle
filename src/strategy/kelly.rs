//! Kelly criterion position sizing.
//!
//! Computes optimal bet sizes using fractional Kelly with configurable
//! multiplier, caps, and commission-adjusted edge calculations.

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
    pub multiplier: f64,
    /// Maximum bet as a fraction of bankroll.
    pub max_bet_pct: f64,
    /// Minimum bet size in dollars (below this, don't bother).
    pub min_bet_size: f64,
    /// Estimated round-trip commission per trade (IB ForecastEx).
    pub commission_per_trade: f64,
}

impl Default for KellyConfig {
    fn default() -> Self {
        Self {
            multiplier: 0.25,     // Quarter-Kelly: conservative
            max_bet_pct: 0.06,    // Max 6% of bankroll per trade
            min_bet_size: 1.0,    // $1 minimum
            commission_per_trade: 0.50, // IB estimated round-trip
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
    pub kelly_fraction: f64,    // Raw Kelly fraction
    pub bet_fraction: f64,      // After multiplier + caps
    pub bet_amount: f64,        // Dollar amount
    pub expected_value: f64,    // Edge * bet_amount
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
    pub fn size_bet(&self, edge: &Edge, bankroll: f64) -> Option<SizedBet> {
        if bankroll <= 0.0 {
            return None;
        }

        // Determine the effective probabilities
        let (win_prob, market_price) = match edge.side {
            Side::Yes => (edge.estimate.probability, edge.market.current_price_yes),
            Side::No => (1.0 - edge.estimate.probability, edge.market.current_price_no),
        };

        // Commission-adjusted market price
        let effective_price = market_price + self.config.commission_per_trade / bankroll;
        let effective_price = effective_price.min(0.99); // can't exceed 1.0

        // Net odds: what you win per dollar risked
        // Buy YES at price p, win (1-p) if YES, lose p if NO
        let payout_ratio = (1.0 - effective_price) / effective_price;

        if payout_ratio <= 0.0 {
            return None;
        }

        let lose_prob = 1.0 - win_prob;

        // Raw Kelly fraction
        let kelly = (payout_ratio * win_prob - lose_prob) / payout_ratio;

        // Negative Kelly means no bet (edge doesn't justify the odds)
        if kelly <= 0.0 {
            debug!(
                market_id = %edge.market.id,
                kelly,
                "Negative Kelly — no bet"
            );
            return None;
        }

        // Apply fractional Kelly (conservative sizing)
        let fractional = kelly * self.config.multiplier;

        // Cap at maximum
        let capped = fractional.min(self.config.max_bet_pct);

        // Convert to dollar amount
        let bet_amount = (capped * bankroll).max(0.0);

        // Floor check
        if bet_amount < self.config.min_bet_size {
            debug!(
                market_id = %edge.market.id,
                bet_amount,
                min = self.config.min_bet_size,
                "Bet below minimum size"
            );
            return None;
        }

        let expected_value = edge.edge * bet_amount;

        debug!(
            market_id = %edge.market.id,
            raw_kelly = format!("{:.2}%", kelly * 100.0),
            fractional = format!("{:.2}%", capped * 100.0),
            bet_amount = format!("${:.2}", bet_amount),
            ev = format!("${:.4}", expected_value),
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

    fn make_edge(market_price: f64, fair_value: f64, confidence: f64) -> Edge {
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
                current_price_no: 1.0 - market_price,
                volume_24h: 100.0,
                liquidity: 500.0,
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
                cost: 0.01,
            },
            side,
            edge: edge_val,
            signed_edge: fair_value - market_price,
        }
    }

    #[test]
    fn test_basic_yes_bet() {
        let calc = KellyCalculator::new(KellyConfig {
            commission_per_trade: 0.0,
            ..Default::default()
        });
        // Market at 40%, we think 60%. Big edge.
        let edge = make_edge(0.40, 0.60, 0.8);
        let sized = calc.size_bet(&edge, 1000.0);
        assert!(sized.is_some());
        let s = sized.unwrap();
        assert!(s.bet_amount > 0.0);
        assert!(s.bet_amount <= 60.0); // max 6% of $1000
        assert!(s.kelly_fraction > 0.0);
        assert!(matches!(s.edge.side, Side::Yes));
    }

    #[test]
    fn test_basic_no_bet() {
        let calc = KellyCalculator::new(KellyConfig {
            commission_per_trade: 0.0,
            ..Default::default()
        });
        // Market at 70%, we think 50%. YES is overpriced → bet NO.
        let edge = make_edge(0.70, 0.50, 0.8);
        let sized = calc.size_bet(&edge, 1000.0);
        assert!(sized.is_some());
        assert!(matches!(sized.unwrap().edge.side, Side::No));
    }

    #[test]
    fn test_no_edge_no_bet() {
        let calc = KellyCalculator::new(KellyConfig {
            commission_per_trade: 0.0,
            ..Default::default()
        });
        // Market at 50%, we also think 50%. No edge.
        let edge = make_edge(0.50, 0.50, 0.8);
        let sized = calc.size_bet(&edge, 1000.0);
        assert!(sized.is_none());
    }

    #[test]
    fn test_zero_bankroll() {
        let calc = KellyCalculator::new(KellyConfig::default());
        let edge = make_edge(0.40, 0.60, 0.8);
        assert!(calc.size_bet(&edge, 0.0).is_none());
    }

    #[test]
    fn test_negative_bankroll() {
        let calc = KellyCalculator::new(KellyConfig::default());
        let edge = make_edge(0.40, 0.60, 0.8);
        assert!(calc.size_bet(&edge, -100.0).is_none());
    }

    #[test]
    fn test_bet_capped_at_max_pct() {
        let calc = KellyCalculator::new(KellyConfig {
            multiplier: 1.0,  // Full Kelly (very aggressive)
            max_bet_pct: 0.06,
            commission_per_trade: 0.0,
            ..Default::default()
        });
        // Huge edge — Kelly would want a lot, but capped at 6%
        let edge = make_edge(0.20, 0.80, 0.9);
        let sized = calc.size_bet(&edge, 1000.0).unwrap();
        assert!((sized.bet_amount - 60.0).abs() < 0.01); // 6% of $1000
        assert_eq!(sized.bet_fraction, 0.06);
    }

    #[test]
    fn test_bet_below_minimum() {
        let calc = KellyCalculator::new(KellyConfig {
            min_bet_size: 5.0,
            commission_per_trade: 0.0,
            ..Default::default()
        });
        // Small bankroll + small edge → bet below $5 minimum
        let edge = make_edge(0.45, 0.55, 0.8);
        let sized = calc.size_bet(&edge, 10.0);
        assert!(sized.is_none());
    }

    #[test]
    fn test_commission_reduces_bet() {
        let calc_no_comm = KellyCalculator::new(KellyConfig {
            commission_per_trade: 0.0,
            ..Default::default()
        });
        let calc_with_comm = KellyCalculator::new(KellyConfig {
            commission_per_trade: 2.0,
            ..Default::default()
        });

        let edge = make_edge(0.40, 0.60, 0.8);
        let no_comm = calc_no_comm.size_bet(&edge, 100.0);
        let with_comm = calc_with_comm.size_bet(&edge, 100.0);

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
            multiplier: 0.25,
            max_bet_pct: 0.50, // high cap so we see the multiplier difference
            commission_per_trade: 0.0,
            ..Default::default()
        });
        let half = KellyCalculator::new(KellyConfig {
            multiplier: 0.50,
            max_bet_pct: 0.50,
            commission_per_trade: 0.0,
            ..Default::default()
        });

        let edge = make_edge(0.40, 0.60, 0.8);
        let q = quarter.size_bet(&edge, 1000.0).unwrap();
        let h = half.size_bet(&edge, 1000.0).unwrap();

        assert!(q.bet_amount < h.bet_amount, "quarter {} should be less than half {}", q.bet_amount, h.bet_amount);
    }

    #[test]
    fn test_expected_value_positive() {
        let calc = KellyCalculator::new(KellyConfig {
            commission_per_trade: 0.0,
            ..Default::default()
        });
        let edge = make_edge(0.40, 0.60, 0.8);
        let sized = calc.size_bet(&edge, 1000.0).unwrap();
        assert!(sized.expected_value > 0.0);
    }

    #[test]
    fn test_kelly_config_default() {
        let config = KellyConfig::default();
        assert_eq!(config.multiplier, 0.25);
        assert_eq!(config.max_bet_pct, 0.06);
        assert_eq!(config.min_bet_size, 1.0);
    }
}
