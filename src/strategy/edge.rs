//! Mispricing detection.
//!
//! Compares LLM fair-value estimates to market prices and identifies
//! actionable edges exceeding category-specific thresholds.

use rust_decimal::Decimal;
use rust_decimal::prelude::*;
use rust_decimal_macros::dec;
use tracing::debug;

use crate::types::{Estimate, Market, MarketCategory, Side};

// ---------------------------------------------------------------------------
// Configuration (defaults — overridden by config.toml at runtime)
// ---------------------------------------------------------------------------

/// Default mispricing thresholds per category.
/// Markets must exceed these to be considered actionable.
/// More uncertain categories require larger edges.
pub struct EdgeConfig {
    pub weather_threshold: Decimal,
    pub sports_threshold: Decimal,
    pub economics_threshold: Decimal,
    pub politics_threshold: Decimal,
    pub culture_threshold: Decimal,
    pub other_threshold: Decimal,
    /// Minimum absolute edge to consider (noise floor).
    pub min_edge: Decimal,
}

impl Default for EdgeConfig {
    fn default() -> Self {
        Self {
            weather_threshold: dec!(0.06),
            sports_threshold: dec!(0.08),
            economics_threshold: dec!(0.10),
            politics_threshold: dec!(0.12),
            culture_threshold: dec!(0.10),
            other_threshold: dec!(0.10),
            min_edge: dec!(0.03),
        }
    }
}

impl EdgeConfig {
    /// Get the threshold for a given category.
    pub fn threshold_for(&self, category: &MarketCategory) -> Decimal {
        match category {
            MarketCategory::Weather => self.weather_threshold,
            MarketCategory::Sports => self.sports_threshold,
            MarketCategory::Economics => self.economics_threshold,
            MarketCategory::Politics => self.politics_threshold,
            MarketCategory::Culture => self.culture_threshold,
            MarketCategory::Other => self.other_threshold,
        }
    }
}

// ---------------------------------------------------------------------------
// Edge detection
// ---------------------------------------------------------------------------

/// Detected edge (mispricing) in a market.
#[derive(Debug, Clone)]
pub struct Edge {
    pub market: Market,
    pub estimate: Estimate,
    pub side: Side,
    pub edge: Decimal,      // absolute edge (always positive)
    pub signed_edge: Decimal, // positive = YES underpriced, negative = NO underpriced
}

/// Detect mispricings by comparing LLM estimates to market prices.
pub struct EdgeDetector {
    config: EdgeConfig,
}

impl EdgeDetector {
    pub fn new(config: EdgeConfig) -> Self {
        Self { config }
    }

    /// Access the edge configuration.
    pub fn config(&self) -> &EdgeConfig {
        &self.config
    }

    /// Find all markets with actionable edges.
    pub fn find_edges(&self, estimates: &[(Market, Estimate)]) -> Vec<Edge> {
        let mut edges = Vec::new();

        for (market, estimate) in estimates {
            if let Some(edge) = self.detect_edge(market, estimate) {
                edges.push(edge);
            }
        }

        // Sort by absolute edge descending (best opportunities first)
        edges.sort_by(|a, b| b.edge.cmp(&a.edge));

        edges
    }

    /// Check a single market for a mispricing.
    fn detect_edge(&self, market: &Market, estimate: &Estimate) -> Option<Edge> {
        let threshold = self.config.threshold_for(&market.category);
        let market_price = market.current_price_yes;
        let fair_value = estimate.probability;

        // Signed edge: positive means YES is underpriced, negative means overpriced
        let signed_edge = fair_value - market_price;
        let abs_edge = signed_edge.abs();

        // Below noise floor — not actionable
        if abs_edge < self.config.min_edge {
            return None;
        }

        // Below category threshold — not confident enough
        if abs_edge < threshold {
            debug!(
                market_id = %market.id,
                edge = %format!("{:.1}%", (abs_edge * dec!(100)).to_f64().unwrap_or(0.0)),
                threshold = %format!("{:.1}%", (threshold * dec!(100)).to_f64().unwrap_or(0.0)),
                "Edge below category threshold"
            );
            return None;
        }

        // Low confidence estimates need extra-large edges
        if estimate.confidence < dec!(0.3) && abs_edge < threshold * dec!(2) {
            debug!(
                market_id = %market.id,
                confidence = %estimate.confidence,
                "Low confidence, requiring double threshold"
            );
            return None;
        }

        let side = if signed_edge > Decimal::ZERO { Side::Yes } else { Side::No };

        debug!(
            market_id = %market.id,
            side = ?side,
            edge = %format!("{:.1}%", (abs_edge * dec!(100)).to_f64().unwrap_or(0.0)),
            fair_value = %format!("{:.1}%", (fair_value * dec!(100)).to_f64().unwrap_or(0.0)),
            market_price = %format!("{:.1}%", (market_price * dec!(100)).to_f64().unwrap_or(0.0)),
            confidence = %format!("{:.0}%", (estimate.confidence * dec!(100)).to_f64().unwrap_or(0.0)),
            "Edge detected"
        );

        Some(Edge {
            market: market.clone(),
            estimate: estimate.clone(),
            side,
            edge: abs_edge,
            signed_edge,
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    fn make_market(id: &str, category: MarketCategory, price_yes: Decimal) -> Market {
        Market {
            id: id.to_string(),
            platform: "manifold".to_string(),
            question: format!("Test market {id}"),
            description: String::new(),
            category,
            current_price_yes: price_yes,
            current_price_no: Decimal::ONE - price_yes,
            volume_24h: dec!(100),
            liquidity: dec!(500),
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

    #[test]
    fn test_detect_yes_edge() {
        let detector = EdgeDetector::new(EdgeConfig::default());
        let market = make_market("m1", MarketCategory::Weather, dec!(0.40));
        let estimate = make_estimate(dec!(0.55), dec!(0.8)); // 15% edge, above 6% threshold

        let edge = detector.detect_edge(&market, &estimate);
        assert!(edge.is_some());
        let e = edge.unwrap();
        assert!(matches!(e.side, Side::Yes));
        assert_eq!(e.edge, dec!(0.15));
        assert!(e.signed_edge > Decimal::ZERO);
    }

    #[test]
    fn test_detect_no_edge() {
        let detector = EdgeDetector::new(EdgeConfig::default());
        let market = make_market("m1", MarketCategory::Weather, dec!(0.70));
        let estimate = make_estimate(dec!(0.50), dec!(0.8)); // -20% edge

        let edge = detector.detect_edge(&market, &estimate);
        assert!(edge.is_some());
        let e = edge.unwrap();
        assert!(matches!(e.side, Side::No));
        assert_eq!(e.edge, dec!(0.20));
        assert!(e.signed_edge < Decimal::ZERO);
    }

    #[test]
    fn test_no_edge_below_threshold() {
        let detector = EdgeDetector::new(EdgeConfig::default());
        let market = make_market("m1", MarketCategory::Politics, dec!(0.50));
        let estimate = make_estimate(dec!(0.55), dec!(0.8)); // 5% edge, below 12% politics threshold

        let edge = detector.detect_edge(&market, &estimate);
        assert!(edge.is_none());
    }

    #[test]
    fn test_no_edge_below_noise_floor() {
        let detector = EdgeDetector::new(EdgeConfig::default());
        let market = make_market("m1", MarketCategory::Weather, dec!(0.50));
        let estimate = make_estimate(dec!(0.52), dec!(0.9)); // 2% edge, below 3% noise floor

        let edge = detector.detect_edge(&market, &estimate);
        assert!(edge.is_none());
    }

    #[test]
    fn test_low_confidence_needs_double_threshold() {
        let detector = EdgeDetector::new(EdgeConfig::default());
        let market = make_market("m1", MarketCategory::Weather, dec!(0.40));
        // 10% edge but only 0.2 confidence — needs 12% (double 6% threshold)
        let estimate = make_estimate(dec!(0.50), dec!(0.2));
        let edge = detector.detect_edge(&market, &estimate);
        assert!(edge.is_none());

        // 15% edge with 0.2 confidence — now exceeds double threshold
        let estimate2 = make_estimate(dec!(0.55), dec!(0.2));
        let edge2 = detector.detect_edge(&market, &estimate2);
        assert!(edge2.is_some());
    }

    #[test]
    fn test_find_edges_sorts_by_edge() {
        let detector = EdgeDetector::new(EdgeConfig::default());
        let estimates = vec![
            (make_market("small", MarketCategory::Weather, dec!(0.40)), make_estimate(dec!(0.50), dec!(0.8))),  // 10%
            (make_market("big", MarketCategory::Weather, dec!(0.40)), make_estimate(dec!(0.70), dec!(0.8))),    // 30%
            (make_market("medium", MarketCategory::Weather, dec!(0.40)), make_estimate(dec!(0.60), dec!(0.8))), // 20%
        ];

        let edges = detector.find_edges(&estimates);
        assert_eq!(edges.len(), 3);
        assert_eq!(edges[0].market.id, "big");
        assert_eq!(edges[1].market.id, "medium");
        assert_eq!(edges[2].market.id, "small");
    }

    #[test]
    fn test_find_edges_filters_no_edge() {
        let detector = EdgeDetector::new(EdgeConfig::default());
        let estimates = vec![
            (make_market("good", MarketCategory::Weather, dec!(0.40)), make_estimate(dec!(0.60), dec!(0.8))),   // 20% edge
            (make_market("bad", MarketCategory::Weather, dec!(0.50)), make_estimate(dec!(0.51), dec!(0.8))),    // 1% edge
        ];

        let edges = detector.find_edges(&estimates);
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].market.id, "good");
    }

    #[test]
    fn test_category_specific_thresholds() {
        let config = EdgeConfig::default();
        assert!(config.threshold_for(&MarketCategory::Weather) < config.threshold_for(&MarketCategory::Politics));
        assert!(config.threshold_for(&MarketCategory::Sports) < config.threshold_for(&MarketCategory::Politics));
    }

    #[test]
    fn test_edge_config_default() {
        let config = EdgeConfig::default();
        assert_eq!(config.weather_threshold, dec!(0.06));
        assert_eq!(config.politics_threshold, dec!(0.12));
        assert_eq!(config.min_edge, dec!(0.03));
    }
}
