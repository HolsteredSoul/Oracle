//! Calibration module.
//!
//! Measures how well the LLM's probability estimates match reality.
//! Computes calibration curves, Brier scores per category, and
//! generates adjustment recommendations.

use std::collections::HashMap;

use crate::types::MarketCategory;

// ---------------------------------------------------------------------------
// Calibration data
// ---------------------------------------------------------------------------

/// A single prediction–outcome pair for calibration tracking.
#[derive(Debug, Clone)]
pub struct CalibrationPoint {
    pub market_id: String,
    pub category: MarketCategory,
    pub estimated_probability: f64,
    pub resolved_yes: bool,
}

/// Calibration analysis results.
#[derive(Debug, Clone)]
pub struct CalibrationReport {
    pub total_predictions: usize,
    pub overall_brier: f64,
    /// Brier score per category.
    pub category_brier: HashMap<String, f64>,
    /// Calibration buckets: for each 10% bin, the predicted vs actual rate.
    pub calibration_curve: Vec<CalibrationBucket>,
    /// Whether the model is over-confident, under-confident, or well-calibrated.
    pub diagnosis: CalibrationDiagnosis,
}

/// A bucket in the calibration curve (e.g., all predictions between 0.60-0.70).
#[derive(Debug, Clone)]
pub struct CalibrationBucket {
    pub bin_start: f64,
    pub bin_end: f64,
    pub mean_predicted: f64,
    pub actual_rate: f64,
    pub count: usize,
    /// Absolute deviation: |mean_predicted - actual_rate|
    pub deviation: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CalibrationDiagnosis {
    WellCalibrated,
    OverConfident,    // Predicted probabilities too extreme
    UnderConfident,   // Predicted probabilities too central
    InsufficientData, // Not enough predictions to diagnose
}

// ---------------------------------------------------------------------------
// Calibrator
// ---------------------------------------------------------------------------

pub struct Calibrator {
    points: Vec<CalibrationPoint>,
    /// Number of bins for the calibration curve.
    num_bins: usize,
}

impl Calibrator {
    pub fn new() -> Self {
        Self {
            points: Vec::new(),
            num_bins: 10,
        }
    }

    /// Add a resolved prediction.
    pub fn add_point(&mut self, point: CalibrationPoint) {
        self.points.push(point);
    }

    /// Add multiple resolved predictions.
    pub fn add_points(&mut self, points: Vec<CalibrationPoint>) {
        self.points.extend(points);
    }

    /// Number of tracked predictions.
    pub fn count(&self) -> usize {
        self.points.len()
    }

    /// Generate a full calibration report.
    pub fn report(&self) -> CalibrationReport {
        if self.points.is_empty() {
            return CalibrationReport {
                total_predictions: 0,
                overall_brier: 0.0,
                category_brier: HashMap::new(),
                calibration_curve: Vec::new(),
                diagnosis: CalibrationDiagnosis::InsufficientData,
            };
        }

        let overall_brier = self.compute_brier(&self.points);
        let category_brier = self.compute_category_brier();
        let calibration_curve = self.compute_calibration_curve();
        let diagnosis = self.diagnose(&calibration_curve);

        CalibrationReport {
            total_predictions: self.points.len(),
            overall_brier,
            category_brier,
            calibration_curve,
            diagnosis,
        }
    }

    /// Compute Brier score for a set of predictions.
    /// Brier = (1/N) * Σ(predicted - outcome)²
    /// Lower is better. 0.0 = perfect, 0.25 = random at 50/50.
    fn compute_brier(&self, points: &[CalibrationPoint]) -> f64 {
        if points.is_empty() {
            return 0.0;
        }
        let sum: f64 = points.iter().map(|p| {
            let outcome = if p.resolved_yes { 1.0 } else { 0.0 };
            (p.estimated_probability - outcome).powi(2)
        }).sum();
        sum / points.len() as f64
    }

    /// Compute Brier score broken down by category.
    fn compute_category_brier(&self) -> HashMap<String, f64> {
        let mut by_category: HashMap<String, Vec<&CalibrationPoint>> = HashMap::new();
        for p in &self.points {
            let key = format!("{:?}", p.category);
            by_category.entry(key).or_default().push(p);
        }

        by_category.into_iter().map(|(cat, points)| {
            let brier = if points.is_empty() {
                0.0
            } else {
                let sum: f64 = points.iter().map(|p| {
                    let outcome = if p.resolved_yes { 1.0 } else { 0.0 };
                    (p.estimated_probability - outcome).powi(2)
                }).sum();
                sum / points.len() as f64
            };
            (cat, brier)
        }).collect()
    }

    /// Compute the calibration curve — bin predictions and compare to actual rates.
    fn compute_calibration_curve(&self) -> Vec<CalibrationBucket> {
        let bin_width = 1.0 / self.num_bins as f64;
        let mut buckets = Vec::with_capacity(self.num_bins);

        for i in 0..self.num_bins {
            let bin_start = i as f64 * bin_width;
            let bin_end = bin_start + bin_width;

            let in_bin: Vec<&CalibrationPoint> = self.points.iter()
                .filter(|p| {
                    p.estimated_probability >= bin_start
                        && (p.estimated_probability < bin_end || (i == self.num_bins - 1 && p.estimated_probability <= bin_end))
                })
                .collect();

            if in_bin.is_empty() {
                buckets.push(CalibrationBucket {
                    bin_start,
                    bin_end,
                    mean_predicted: (bin_start + bin_end) / 2.0,
                    actual_rate: 0.0,
                    count: 0,
                    deviation: 0.0,
                });
                continue;
            }

            let count = in_bin.len();
            let mean_predicted = in_bin.iter()
                .map(|p| p.estimated_probability)
                .sum::<f64>() / count as f64;
            let actual_rate = in_bin.iter()
                .filter(|p| p.resolved_yes)
                .count() as f64 / count as f64;
            let deviation = (mean_predicted - actual_rate).abs();

            buckets.push(CalibrationBucket {
                bin_start,
                bin_end,
                mean_predicted,
                actual_rate,
                count,
                deviation,
            });
        }

        buckets
    }

    /// Diagnose overall calibration quality.
    fn diagnose(&self, curve: &[CalibrationBucket]) -> CalibrationDiagnosis {
        let populated: Vec<&CalibrationBucket> = curve.iter()
            .filter(|b| b.count >= 3)
            .collect();

        if populated.len() < 3 || self.points.len() < 20 {
            return CalibrationDiagnosis::InsufficientData;
        }

        // Check if extreme bins (0-0.2 and 0.8-1.0) show overconfidence
        let mut overconfident_signals = 0;
        let mut underconfident_signals = 0;

        for bucket in &populated {
            if bucket.deviation < 0.05 {
                continue; // Well-calibrated bucket
            }

            let mid = (bucket.bin_start + bucket.bin_end) / 2.0;

            if mid < 0.3 {
                // Low-probability bin
                if bucket.actual_rate > bucket.mean_predicted {
                    overconfident_signals += 1; // Predicted too low (overconfident in NO)
                } else {
                    underconfident_signals += 1;
                }
            } else if mid > 0.7 {
                // High-probability bin
                if bucket.actual_rate < bucket.mean_predicted {
                    overconfident_signals += 1; // Predicted too high (overconfident in YES)
                } else {
                    underconfident_signals += 1;
                }
            }
        }

        if overconfident_signals > underconfident_signals + 1 {
            CalibrationDiagnosis::OverConfident
        } else if underconfident_signals > overconfident_signals + 1 {
            CalibrationDiagnosis::UnderConfident
        } else {
            CalibrationDiagnosis::WellCalibrated
        }
    }

    /// Generate a prompt snippet for LLM self-improvement.
    ///
    /// Feed this into future LLM calls so the model can adjust.
    pub fn prompt_snippet(&self) -> String {
        let report = self.report();
        let mut parts = Vec::new();

        parts.push(format!(
            "CALIBRATION DATA ({} resolved predictions):",
            report.total_predictions
        ));
        parts.push(format!("Overall Brier score: {:.3}", report.overall_brier));

        for (cat, brier) in &report.category_brier {
            parts.push(format!("  {cat} Brier: {brier:.3}"));
        }

        match &report.diagnosis {
            CalibrationDiagnosis::OverConfident =>
                parts.push("DIAGNOSIS: You have been OVERCONFIDENT. Pull estimates toward 50%.".into()),
            CalibrationDiagnosis::UnderConfident =>
                parts.push("DIAGNOSIS: You have been UNDERCONFIDENT. Be more decisive.".into()),
            CalibrationDiagnosis::WellCalibrated =>
                parts.push("DIAGNOSIS: Your calibration is good. Maintain current approach.".into()),
            CalibrationDiagnosis::InsufficientData =>
                parts.push("DIAGNOSIS: Not enough data yet for calibration feedback.".into()),
        }

        parts.join("\n")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_point(estimated: f64, resolved_yes: bool) -> CalibrationPoint {
        CalibrationPoint {
            market_id: "test".into(),
            category: MarketCategory::Weather,
            estimated_probability: estimated,
            resolved_yes,
        }
    }

    #[test]
    fn test_perfect_calibration() {
        let mut cal = Calibrator::new();
        // Predict 0.9 for things that happen, 0.1 for things that don't
        for _ in 0..10 {
            cal.add_point(make_point(0.90, true));
            cal.add_point(make_point(0.10, false));
        }

        let report = cal.report();
        assert!(report.overall_brier < 0.05, "Brier: {}", report.overall_brier);
    }

    #[test]
    fn test_terrible_calibration() {
        let mut cal = Calibrator::new();
        // Predict 0.9 for things that DON'T happen
        for _ in 0..10 {
            cal.add_point(make_point(0.90, false));
            cal.add_point(make_point(0.10, true));
        }

        let report = cal.report();
        assert!(report.overall_brier > 0.5, "Brier: {}", report.overall_brier);
    }

    #[test]
    fn test_empty_calibrator() {
        let cal = Calibrator::new();
        let report = cal.report();
        assert_eq!(report.total_predictions, 0);
        assert_eq!(report.diagnosis, CalibrationDiagnosis::InsufficientData);
    }

    #[test]
    fn test_insufficient_data() {
        let mut cal = Calibrator::new();
        for _ in 0..5 {
            cal.add_point(make_point(0.70, true));
        }
        let report = cal.report();
        assert_eq!(report.diagnosis, CalibrationDiagnosis::InsufficientData);
    }

    #[test]
    fn test_category_brier() {
        let mut cal = Calibrator::new();
        cal.add_point(CalibrationPoint {
            market_id: "w1".into(),
            category: MarketCategory::Weather,
            estimated_probability: 0.80,
            resolved_yes: true,
        });
        cal.add_point(CalibrationPoint {
            market_id: "s1".into(),
            category: MarketCategory::Sports,
            estimated_probability: 0.80,
            resolved_yes: false,
        });

        let report = cal.report();
        assert!(report.category_brier.contains_key("Weather"));
        assert!(report.category_brier.contains_key("Sports"));
        // Weather: (0.8 - 1.0)² = 0.04
        // Sports:  (0.8 - 0.0)² = 0.64
        assert!(*report.category_brier.get("Weather").unwrap() < 0.1);
        assert!(*report.category_brier.get("Sports").unwrap() > 0.5);
    }

    #[test]
    fn test_calibration_curve_buckets() {
        let mut cal = Calibrator::new();
        for _ in 0..10 {
            cal.add_point(make_point(0.25, true));
            cal.add_point(make_point(0.75, false));
        }
        let report = cal.report();
        assert_eq!(report.calibration_curve.len(), 10);

        // Find the bucket containing 0.25
        let bucket_25 = report.calibration_curve.iter()
            .find(|b| b.bin_start <= 0.25 && b.bin_end > 0.25)
            .unwrap();
        assert_eq!(bucket_25.count, 10);
        assert!((bucket_25.actual_rate - 1.0).abs() < 1e-10); // All resolved YES
    }

    #[test]
    fn test_prompt_snippet() {
        let mut cal = Calibrator::new();
        for _ in 0..5 {
            cal.add_point(make_point(0.80, true));
            cal.add_point(make_point(0.20, false));
        }
        let snippet = cal.prompt_snippet();
        assert!(snippet.contains("CALIBRATION DATA"));
        assert!(snippet.contains("Brier"));
    }

    #[test]
    fn test_brier_score_at_50() {
        let mut cal = Calibrator::new();
        // Always predict 50% — Brier should be 0.25 for balanced outcomes
        for _ in 0..50 {
            cal.add_point(make_point(0.50, true));
            cal.add_point(make_point(0.50, false));
        }
        let report = cal.report();
        assert!((report.overall_brier - 0.25).abs() < 0.01, "Brier: {}", report.overall_brier);
    }

    #[test]
    fn test_add_points_batch() {
        let mut cal = Calibrator::new();
        let points = vec![
            make_point(0.7, true),
            make_point(0.3, false),
            make_point(0.5, true),
        ];
        cal.add_points(points);
        assert_eq!(cal.count(), 3);
    }
}
