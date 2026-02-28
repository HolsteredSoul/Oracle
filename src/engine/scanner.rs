//! Multi-platform market scanner and router.
//!
//! Aggregates markets from all enabled platforms (Manifold, Metaculus,
//! and eventually ForecastEx), matches cross-platform events via fuzzy
//! text similarity, attaches cross-references, and filters/sorts the
//! unified market list for downstream processing.
//!
//! This is the "2D: Market Router" from the development plan.

use anyhow::{Context, Result};
use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal::prelude::*;
use rust_decimal_macros::dec;
use tracing::{debug, info, warn};

use crate::platforms::manifold::ManifoldClient;
use crate::platforms::metaculus::MetaculusClient;
use crate::platforms::polymarket::PolymarketClient;
use crate::platforms::PredictionPlatform;
use crate::types::{CrossReferences, Market};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Minimum similarity score (0.0–1.0) to consider two questions as
/// referring to the same underlying event.
const MATCH_THRESHOLD: f64 = 0.45;

/// Minimum liquidity (or forecaster count for Metaculus) to keep a market.
const MIN_LIQUIDITY: Decimal = dec!(5.0);

/// Maximum hours until deadline — skip markets closing too far out
/// (reduces noise from long-dated, low-activity markets).
const MAX_HOURS_TO_DEADLINE: f64 = 24.0 * 365.0; // 1 year

/// Minimum hours until deadline — skip markets about to close
/// (not enough time to act on information).
const MIN_HOURS_TO_DEADLINE: f64 = 1.0;

// ---------------------------------------------------------------------------
// Text similarity
// ---------------------------------------------------------------------------

/// Compute a normalised similarity score between two strings.
///
/// Uses a combination of:
/// 1. Word overlap (Jaccard index on normalised tokens)
/// 2. Substring containment bonus
///
/// Returns 0.0 (no similarity) to 1.0 (identical after normalisation).
fn text_similarity(a: &str, b: &str) -> f64 {
    let norm = |s: &str| -> Vec<String> {
        s.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.len() > 2) // drop short words like "a", "in", "to"
            .map(String::from)
            .collect()
    };

    let words_a = norm(a);
    let words_b = norm(b);

    if words_a.is_empty() || words_b.is_empty() {
        return 0.0;
    }

    // Jaccard index: |intersection| / |union|
    let set_a: std::collections::HashSet<&str> =
        words_a.iter().map(|s| s.as_str()).collect();
    let set_b: std::collections::HashSet<&str> =
        words_b.iter().map(|s| s.as_str()).collect();

    let intersection = set_a.intersection(&set_b).count() as f64;
    let union = set_a.union(&set_b).count() as f64;

    let jaccard = if union > 0.0 {
        intersection / union
    } else {
        0.0
    };

    // Containment bonus: if one question is substantially contained in the other
    let containment = if set_a.len() <= set_b.len() {
        intersection / set_a.len() as f64
    } else {
        intersection / set_b.len() as f64
    };

    // Weighted combination: Jaccard for general similarity,
    // containment for asymmetric matches (short vs long questions)
    (0.6 * jaccard + 0.4 * containment).min(1.0)
}

// ---------------------------------------------------------------------------
// Market Router
// ---------------------------------------------------------------------------

/// Unified market scanner that aggregates and cross-references markets
/// from all enabled platforms.
pub struct MarketRouter {
    manifold: Option<ManifoldClient>,
    metaculus: Option<MetaculusClient>,
    polymarket: Option<PolymarketClient>,
}

impl MarketRouter {
    /// Create a new router with the specified platform clients.
    ///
    /// Pass `None` for any platform that is disabled in config.
    pub fn new(
        manifold: Option<ManifoldClient>,
        metaculus: Option<MetaculusClient>,
    ) -> Self {
        Self {
            manifold,
            metaculus,
            polymarket: None,
        }
    }

    /// Create a router with Polymarket as primary execution venue.
    pub fn with_polymarket(
        polymarket: PolymarketClient,
        metaculus: Option<MetaculusClient>,
        manifold: Option<ManifoldClient>,
    ) -> Self {
        Self {
            manifold,
            metaculus,
            polymarket: Some(polymarket),
        }
    }

    /// Scan all enabled platforms, cross-reference markets, and return
    /// a filtered, sorted list of actionable markets.
    ///
    /// This is the main entry point called by the engine's scan cycle.
    pub async fn scan_all(&self) -> Result<Vec<Market>> {
        info!("Starting multi-platform market scan...");

        // 1. Fetch from all platforms concurrently
        let (manifold_markets, metaculus_markets, polymarket_markets) = tokio::join!(
            self.fetch_manifold(),
            self.fetch_metaculus(),
            self.fetch_polymarket(),
        );

        let mut manifold_markets = manifold_markets.unwrap_or_else(|e| {
            warn!(error = %e, "Manifold scan failed, continuing without");
            Vec::new()
        });

        let metaculus_markets = metaculus_markets.unwrap_or_else(|e| {
            warn!(error = %e, "Metaculus scan failed, continuing without");
            Vec::new()
        });

        let polymarket_markets = polymarket_markets.unwrap_or_else(|e| {
            warn!(error = %e, "Polymarket scan failed, continuing without");
            Vec::new()
        });

        info!(
            manifold = manifold_markets.len(),
            metaculus = metaculus_markets.len(),
            polymarket = polymarket_markets.len(),
            "Raw markets fetched"
        );

        // 2. Cross-reference: attach Metaculus forecasts to matching Manifold markets
        Self::cross_reference(&mut manifold_markets, &metaculus_markets);

        // 3. Merge all markets into a single list
        //    Polymarket markets are primary (real-money execution venue).
        //    Manifold markets are secondary (play-money validation).
        //    Metaculus-only markets are informational signals.
        let mut all_markets = polymarket_markets;
        all_markets.extend(manifold_markets);

        // Add Metaculus markets that didn't match any Manifold market
        // (useful for discovering questions we might want to track)
        for mc in &metaculus_markets {
            let already_referenced = all_markets.iter().any(|m| {
                m.cross_refs.metaculus_prob.is_some()
                    && text_similarity(&m.question, &mc.question) >= MATCH_THRESHOLD
            });
            if !already_referenced {
                all_markets.push(mc.clone());
            }
        }

        // 4. Filter
        let before_filter = all_markets.len();
        let all_markets = self.filter_markets(all_markets);
        debug!(
            before = before_filter,
            after = all_markets.len(),
            "Markets filtered"
        );

        // 5. Sort by cross-reference richness, then by liquidity
        let mut all_markets = all_markets;
        all_markets.sort_by(|a, b| {
            let score_a = Self::priority_score(a);
            let score_b = Self::priority_score(b);
            score_b
                .partial_cmp(&score_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        info!(
            total = all_markets.len(),
            "Market scan complete"
        );

        Ok(all_markets)
    }

    // -- Platform fetch helpers ------------------------------------------

    async fn fetch_manifold(&self) -> Result<Vec<Market>> {
        match &self.manifold {
            Some(client) => client.fetch_markets().await,
            None => Ok(Vec::new()),
        }
    }

    async fn fetch_metaculus(&self) -> Result<Vec<Market>> {
        match &self.metaculus {
            Some(client) => client.fetch_markets().await,
            None => Ok(Vec::new()),
        }
    }

    async fn fetch_polymarket(&self) -> Result<Vec<Market>> {
        match &self.polymarket {
            Some(client) => client.fetch_markets().await,
            None => Ok(Vec::new()),
        }
    }

    // -- Cross-referencing -----------------------------------------------

    /// For each Manifold market, find the best-matching Metaculus question
    /// and attach its community forecast as a cross-reference.
    fn cross_reference(manifold: &mut [Market], metaculus: &[Market]) {
        if metaculus.is_empty() {
            return;
        }

        let mut match_count = 0u32;

        for mf_market in manifold.iter_mut() {
            let mut best_score = 0.0f64;
            let mut best_match: Option<&Market> = None;

            for mc_market in metaculus {
                // Quick category pre-filter: only match within same category
                // or if either is "Other"
                if mf_market.category != mc_market.category
                    && mf_market.category != crate::types::MarketCategory::Other
                    && mc_market.category != crate::types::MarketCategory::Other
                {
                    continue;
                }

                let score = text_similarity(&mf_market.question, &mc_market.question);
                if score > best_score {
                    best_score = score;
                    best_match = Some(mc_market);
                }
            }

            if best_score >= MATCH_THRESHOLD {
                if let Some(mc) = best_match {
                    mf_market.cross_refs.metaculus_prob = mc.cross_refs.metaculus_prob;
                    mf_market.cross_refs.metaculus_forecasters =
                        mc.cross_refs.metaculus_forecasters;

                    debug!(
                        manifold_q = %mf_market.question,
                        metaculus_q = %mc.question,
                        score = best_score,
                        metaculus_prob = ?mc.cross_refs.metaculus_prob,
                        "Cross-platform match found"
                    );

                    match_count += 1;
                }
            }
        }

        info!(
            matches = match_count,
            manifold_total = manifold.len(),
            metaculus_total = metaculus.len(),
            "Cross-referencing complete"
        );
    }

    // -- Filtering -------------------------------------------------------

    /// Filter out markets that are too illiquid, too far/close to deadline,
    /// or already resolved.
    fn filter_markets(&self, markets: Vec<Market>) -> Vec<Market> {
        let now = Utc::now();

        markets
            .into_iter()
            .filter(|m| {
                // Liquidity check
                if m.liquidity < MIN_LIQUIDITY {
                    return false;
                }

                // Deadline checks
                let hours_remaining =
                    (m.deadline - now).num_minutes() as f64 / 60.0;

                if hours_remaining < MIN_HOURS_TO_DEADLINE {
                    return false;
                }
                if hours_remaining > MAX_HOURS_TO_DEADLINE {
                    return false;
                }

                // Price sanity: skip markets at extreme probabilities
                // (very little edge to be found at 1% or 99%)
                if m.current_price_yes < dec!(0.02) || m.current_price_yes > dec!(0.98) {
                    return false;
                }

                true
            })
            .collect()
    }

    // -- Sorting / Prioritisation ----------------------------------------

    /// Compute a priority score for sorting. Higher = more interesting.
    ///
    /// Factors:
    /// - Has cross-references (Metaculus + Manifold = more data points)
    /// - Higher liquidity
    /// - More bettors / forecasters
    /// - Probability away from extremes (more room for edge)
    fn priority_score(market: &Market) -> f64 {
        let mut score = 0.0;

        // Cross-reference bonus: markets with Metaculus data are richer
        if market.cross_refs.metaculus_prob.is_some() {
            score += 50.0;

            // Extra bonus for many forecasters (more reliable signal)
            if let Some(f) = market.cross_refs.metaculus_forecasters {
                score += (f as f64).min(100.0) * 0.5;
            }
        }

        // Manifold bonus
        if market.cross_refs.manifold_prob.is_some() {
            score += 20.0;
        }

        // Liquidity score (log scale to avoid mega-liquid markets dominating)
        let liq_f64 = market.liquidity.to_f64().unwrap_or(0.0);
        score += (liq_f64 + 1.0).ln() * 5.0;

        // Volume bonus
        let vol_f64 = market.volume_24h.to_f64().unwrap_or(0.0);
        score += (vol_f64 + 1.0).ln() * 3.0;

        // Probability centrality: markets near 50% have the most room for edge
        let price_f64 = market.current_price_yes.to_f64().unwrap_or(0.5);
        let centrality = 1.0 - (2.0 * (price_f64 - 0.5)).abs();
        score += centrality * 10.0;

        score
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{d, MarketCategory};
    use chrono::Duration;

    // -- Text similarity tests -------------------------------------------

    #[test]
    fn test_similarity_identical() {
        let s = text_similarity(
            "Will Trump win the 2028 election?",
            "Will Trump win the 2028 election?",
        );
        assert!((s - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_similarity_very_similar() {
        let s = text_similarity(
            "Will Trump win the 2028 presidential election?",
            "Will Donald Trump win the US 2028 presidential election?",
        );
        assert!(s > 0.5, "Score {s} should be > 0.5");
    }

    #[test]
    fn test_similarity_same_topic_different_wording() {
        let s = text_similarity(
            "Will US CPI exceed 3% in Q2 2026?",
            "US inflation CPI above 3 percent Q2 2026",
        );
        assert!(s > 0.3, "Score {s} should be > 0.3");
    }

    #[test]
    fn test_similarity_unrelated() {
        let s = text_similarity(
            "Will Trump win the 2028 election?",
            "Will California experience an earthquake before 2030?",
        );
        assert!(s < 0.2, "Score {s} should be < 0.2");
    }

    #[test]
    fn test_similarity_empty() {
        assert_eq!(text_similarity("", "something"), 0.0);
        assert_eq!(text_similarity("hello", ""), 0.0);
        assert_eq!(text_similarity("", ""), 0.0);
    }

    #[test]
    fn test_similarity_short_words_ignored() {
        // "a" and "in" should be dropped, not counting as matches
        let s = text_similarity("a in to", "a in to");
        assert_eq!(s, 0.0); // all words <= 2 chars, filtered out
    }

    #[test]
    fn test_similarity_case_insensitive() {
        let s = text_similarity("TRUMP ELECTION", "trump election");
        assert!((s - 1.0).abs() < 1e-10);
    }

    // -- Helper to create test markets -----------------------------------

    fn make_market(
        id: &str,
        platform: &str,
        question: &str,
        category: MarketCategory,
        prob: f64,
        liquidity: f64,
        hours_to_deadline: f64,
    ) -> Market {
        Market {
            id: id.to_string(),
            platform: platform.to_string(),
            question: question.to_string(),
            description: String::new(),
            category,
            current_price_yes: d(prob),
            current_price_no: d(1.0 - prob),
            volume_24h: dec!(100),
            liquidity: d(liquidity),
            deadline: Utc::now() + Duration::hours(hours_to_deadline as i64),
            resolution_criteria: String::new(),
            url: format!("https://example.com/{id}"),
            cross_refs: CrossReferences::default(),
        }
    }

    fn make_metaculus_market(
        id: &str,
        question: &str,
        category: MarketCategory,
        prob: f64,
        forecasters: u32,
    ) -> Market {
        let mut m = make_market(id, "metaculus", question, category, prob, forecasters as f64, 720.0);
        m.cross_refs.metaculus_prob = Some(d(prob));
        m.cross_refs.metaculus_forecasters = Some(forecasters);
        m
    }

    // -- Cross-referencing tests -----------------------------------------

    #[test]
    fn test_cross_reference_exact_match() {
        let mut manifold = vec![make_market(
            "mf1",
            "manifold",
            "Will Trump finish his second term?",
            MarketCategory::Politics,
            0.75,
            500.0,
            720.0,
        )];
        let metaculus = vec![make_metaculus_market(
            "mc1",
            "Will Trump finish his second term?",
            MarketCategory::Politics,
            0.68,
            150,
        )];

        MarketRouter::cross_reference(&mut manifold, &metaculus);

        assert_eq!(manifold[0].cross_refs.metaculus_prob, Some(d(0.68)));
        assert_eq!(manifold[0].cross_refs.metaculus_forecasters, Some(150));
    }

    #[test]
    fn test_cross_reference_fuzzy_match() {
        let mut manifold = vec![make_market(
            "mf1",
            "manifold",
            "Will Trump finish his second presidential term in office?",
            MarketCategory::Politics,
            0.75,
            200.0,
            720.0,
        )];
        let metaculus = vec![make_metaculus_market(
            "mc1",
            "Will Trump finish his second term as president?",
            MarketCategory::Politics,
            0.68,
            80,
        )];

        MarketRouter::cross_reference(&mut manifold, &metaculus);

        assert!(manifold[0].cross_refs.metaculus_prob.is_some(),
            "Should have matched on similar Trump/second/term wording");
    }

    #[test]
    fn test_cross_reference_no_match_different_topics() {
        let mut manifold = vec![make_market(
            "mf1",
            "manifold",
            "Will Trump finish his second term?",
            MarketCategory::Politics,
            0.75,
            500.0,
            720.0,
        )];
        let metaculus = vec![make_metaculus_market(
            "mc1",
            "Will California experience a major earthquake?",
            MarketCategory::Weather,
            0.30,
            200,
        )];

        MarketRouter::cross_reference(&mut manifold, &metaculus);

        assert!(manifold[0].cross_refs.metaculus_prob.is_none());
    }

    #[test]
    fn test_cross_reference_category_prefilter() {
        let mut manifold = vec![make_market(
            "mf1",
            "manifold",
            "Will the Thunder win the NBA finals?",
            MarketCategory::Sports,
            0.40,
            300.0,
            720.0,
        )];
        // Same question text but wrong category — shouldn't match
        let metaculus = vec![make_metaculus_market(
            "mc1",
            "Will the Thunder win the NBA finals?",
            MarketCategory::Economics, // deliberately wrong
            0.35,
            50,
        )];

        MarketRouter::cross_reference(&mut manifold, &metaculus);

        // Should NOT match due to category mismatch
        assert!(manifold[0].cross_refs.metaculus_prob.is_none());
    }

    #[test]
    fn test_cross_reference_empty_metaculus() {
        let mut manifold = vec![make_market(
            "mf1", "manifold", "Test?",
            MarketCategory::Other, 0.5, 100.0, 720.0,
        )];
        MarketRouter::cross_reference(&mut manifold, &[]);
        assert!(manifold[0].cross_refs.metaculus_prob.is_none());
    }

    // -- Filter tests ----------------------------------------------------

    #[test]
    fn test_filter_removes_low_liquidity() {
        let router = MarketRouter::new(None, None);
        let markets = vec![
            make_market("ok", "manifold", "Good market", MarketCategory::Politics, 0.5, 100.0, 720.0),
            make_market("bad", "manifold", "Low liq", MarketCategory::Politics, 0.5, 1.0, 720.0),
        ];
        let filtered = router.filter_markets(markets);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, "ok");
    }

    #[test]
    fn test_filter_removes_extreme_probability() {
        let router = MarketRouter::new(None, None);
        let markets = vec![
            make_market("ok", "manifold", "Normal", MarketCategory::Politics, 0.5, 100.0, 720.0),
            make_market("hi", "manifold", "Too high", MarketCategory::Politics, 0.99, 100.0, 720.0),
            make_market("lo", "manifold", "Too low", MarketCategory::Politics, 0.01, 100.0, 720.0),
        ];
        let filtered = router.filter_markets(markets);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, "ok");
    }

    #[test]
    fn test_filter_removes_closing_too_soon() {
        let router = MarketRouter::new(None, None);
        let markets = vec![
            make_market("ok", "manifold", "Normal", MarketCategory::Politics, 0.5, 100.0, 720.0),
            make_market("soon", "manifold", "Closing soon", MarketCategory::Politics, 0.5, 100.0, 0.5),
        ];
        let filtered = router.filter_markets(markets);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, "ok");
    }

    // -- Priority scoring tests ------------------------------------------

    #[test]
    fn test_priority_cross_referenced_higher() {
        let mut plain = make_market("a", "manifold", "Q?", MarketCategory::Politics, 0.5, 100.0, 720.0);
        let mut xref = make_market("b", "manifold", "Q?", MarketCategory::Politics, 0.5, 100.0, 720.0);
        xref.cross_refs.metaculus_prob = Some(d(0.55));
        xref.cross_refs.metaculus_forecasters = Some(100);

        assert!(
            MarketRouter::priority_score(&xref) > MarketRouter::priority_score(&plain),
            "Cross-referenced market should score higher"
        );
    }

    #[test]
    fn test_priority_central_probability_higher() {
        let central = make_market("a", "manifold", "Q?", MarketCategory::Politics, 0.50, 100.0, 720.0);
        let extreme = make_market("b", "manifold", "Q?", MarketCategory::Politics, 0.90, 100.0, 720.0);

        assert!(
            MarketRouter::priority_score(&central) > MarketRouter::priority_score(&extreme),
            "Market at 50% should score higher than at 90%"
        );
    }

    #[test]
    fn test_priority_higher_liquidity_higher() {
        let low = make_market("a", "manifold", "Q?", MarketCategory::Politics, 0.5, 10.0, 720.0);
        let high = make_market("b", "manifold", "Q?", MarketCategory::Politics, 0.5, 1000.0, 720.0);

        assert!(
            MarketRouter::priority_score(&high) > MarketRouter::priority_score(&low),
            "Higher liquidity should score higher"
        );
    }

    // -- Router construction ---------------------------------------------

    #[test]
    fn test_router_new_no_platforms() {
        let router = MarketRouter::new(None, None);
        assert!(router.manifold.is_none());
        assert!(router.metaculus.is_none());
    }

    #[test]
    fn test_router_new_with_platforms() {
        let manifold = ManifoldClient::new(None).unwrap();
        let metaculus = MetaculusClient::new().unwrap();
        let router = MarketRouter::new(Some(manifold), Some(metaculus));
        assert!(router.manifold.is_some());
        assert!(router.metaculus.is_some());
    }
}
