//! Trade executor.
//!
//! Places bets via platform clients and tracks execution results.
//! IB ForecastEx executor is deferred (Phase 2A). Currently supports
//! Manifold paper-trading for strategy validation.

use anyhow::{Context, Result};
use tracing::{debug, info, warn};

use crate::platforms::manifold::ManifoldClient;
use crate::platforms::PredictionPlatform;
use crate::strategy::kelly::SizedBet;
use crate::types::{Side, TradeReceipt};

// ---------------------------------------------------------------------------
// Execution result
// ---------------------------------------------------------------------------

/// Result of executing a batch of bets.
#[derive(Debug, Clone)]
pub struct ExecutionReport {
    pub executed: Vec<ExecutedTrade>,
    pub failed: Vec<FailedTrade>,
    pub total_committed: f64,
    pub total_commission: f64,
}

#[derive(Debug, Clone)]
pub struct ExecutedTrade {
    pub market_id: String,
    pub platform: String,
    pub side: Side,
    pub amount: f64,
    pub receipt: TradeReceipt,
}

#[derive(Debug, Clone)]
pub struct FailedTrade {
    pub market_id: String,
    pub platform: String,
    pub reason: String,
}

// ---------------------------------------------------------------------------
// Executor
// ---------------------------------------------------------------------------

pub struct Executor {
    manifold: Option<ManifoldClient>,
    // forecastex: Option<ForecastExClient>,  // Phase 2A
    dry_run: bool,
}

impl Executor {
    pub fn new(manifold: Option<ManifoldClient>, dry_run: bool) -> Self {
        Self { manifold, dry_run }
    }

    /// Execute a batch of sized bets.
    ///
    /// In dry-run mode, logs but doesn't place real bets.
    /// With Manifold enabled, places play-money bets for validation.
    /// IB ForecastEx execution comes in Phase 2A.
    pub async fn execute_batch(&self, bets: &[SizedBet]) -> Result<ExecutionReport> {
        let mut report = ExecutionReport {
            executed: Vec::new(),
            failed: Vec::new(),
            total_committed: 0.0,
            total_commission: 0.0,
        };

        if bets.is_empty() {
            return Ok(report);
        }

        info!(count = bets.len(), dry_run = self.dry_run, "Executing batch");

        for bet in bets {
            if self.dry_run {
                info!(
                    market_id = %bet.edge.market.id,
                    side = ?bet.edge.side,
                    amount = format!("${:.2}", bet.bet_amount),
                    edge = format!("{:.1}%", bet.edge.edge * 100.0),
                    kelly = format!("{:.2}%", bet.kelly_fraction * 100.0),
                    "[DRY RUN] Would place bet"
                );
                report.executed.push(ExecutedTrade {
                    market_id: bet.edge.market.id.clone(),
                    platform: "dry-run".to_string(),
                    side: bet.edge.side.clone(),
                    amount: bet.bet_amount,
                    receipt: TradeReceipt::dry_run(&bet.edge.market.id, bet.bet_amount),
                });
                report.total_committed += bet.bet_amount;
                continue;
            }

            // Try Manifold paper execution
            if let Some(ref manifold) = self.manifold {
                match self.execute_on_manifold(manifold, bet).await {
                    Ok(receipt) => {
                        report.executed.push(ExecutedTrade {
                            market_id: bet.edge.market.id.clone(),
                            platform: "manifold".to_string(),
                            side: bet.edge.side.clone(),
                            amount: bet.bet_amount,
                            receipt,
                        });
                        report.total_committed += bet.bet_amount;
                    }
                    Err(e) => {
                        warn!(
                            market_id = %bet.edge.market.id,
                            error = %e,
                            "Manifold execution failed"
                        );
                        report.failed.push(FailedTrade {
                            market_id: bet.edge.market.id.clone(),
                            platform: "manifold".to_string(),
                            reason: e.to_string(),
                        });
                    }
                }
            }

            // TODO (Phase 2A): Execute on IB ForecastEx
        }

        info!(
            executed = report.executed.len(),
            failed = report.failed.len(),
            committed = format!("${:.2}", report.total_committed),
            "Batch execution complete"
        );

        Ok(report)
    }

    async fn execute_on_manifold(
        &self,
        client: &ManifoldClient,
        bet: &SizedBet,
    ) -> Result<TradeReceipt> {
        // Only execute on Manifold markets
        if bet.edge.market.platform != "manifold" {
            anyhow::bail!("Market {} is not a Manifold market", bet.edge.market.id);
        }

        client
            .place_bet(&bet.edge.market.id, bet.edge.side.clone(), bet.bet_amount)
            .await
            .context("Manifold bet placement failed")
    }
}

// ---------------------------------------------------------------------------
// TradeReceipt helpers
// ---------------------------------------------------------------------------

impl TradeReceipt {
    /// Create a dry-run receipt (no real execution).
    pub fn dry_run(market_id: &str, amount: f64) -> Self {
        Self {
            order_id: format!("dry-run-{}", uuid::Uuid::new_v4()),
            market_id: market_id.to_string(),
            platform: "dry-run".to_string(),
            side: Side::Yes,
            amount,
            fill_price: 0.0,
            fees: 0.0,
            timestamp: chrono::Utc::now(),
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
    use crate::types::*;
    use chrono::{Duration, Utc};

    fn make_sized_bet(market_id: &str, amount: f64) -> SizedBet {
        SizedBet {
            edge: Edge {
                market: Market {
                    id: market_id.to_string(),
                    platform: "manifold".to_string(),
                    question: "Test?".into(),
                    description: String::new(),
                    category: MarketCategory::Weather,
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

    #[tokio::test]
    async fn test_dry_run_execution() {
        let executor = Executor::new(None, true);
        let bets = vec![make_sized_bet("m1", 50.0), make_sized_bet("m2", 30.0)];
        let report = executor.execute_batch(&bets).await.unwrap();

        assert_eq!(report.executed.len(), 2);
        assert_eq!(report.failed.len(), 0);
        assert!((report.total_committed - 80.0).abs() < 1e-10);
        assert_eq!(report.executed[0].platform, "dry-run");
    }

    #[tokio::test]
    async fn test_empty_batch() {
        let executor = Executor::new(None, false);
        let report = executor.execute_batch(&[]).await.unwrap();
        assert_eq!(report.executed.len(), 0);
        assert_eq!(report.failed.len(), 0);
    }

    #[test]
    fn test_dry_run_receipt() {
        let receipt = TradeReceipt::dry_run("test-market", 100.0);
        assert!(receipt.order_id.starts_with("dry-run-"));
        assert_eq!(receipt.amount, 100.0);
        assert_eq!(receipt.fees, 0.0);
    }

    #[tokio::test]
    async fn test_no_manifold_no_execution() {
        let executor = Executor::new(None, false); // not dry-run, but no manifold client
        let bets = vec![make_sized_bet("m1", 50.0)];
        let report = executor.execute_batch(&bets).await.unwrap();
        // No platforms available, nothing executed
        assert_eq!(report.executed.len(), 0);
        assert_eq!(report.failed.len(), 0);
    }
}
