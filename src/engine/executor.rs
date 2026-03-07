//! Trade executor.
//!
//! Places bets via platform clients and tracks execution results.
//! IB ForecastEx executor is deferred (Phase 2A). Currently supports
//! Manifold paper-trading for strategy validation.

use anyhow::{Context, Result};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal_macros::dec;
use tracing::{debug, info, warn};

use crate::platforms::betfair::BetfairClient;
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
    pub total_committed: Decimal,
    pub total_commission: Decimal,
}

#[derive(Debug, Clone)]
pub struct ExecutedTrade {
    pub market_id: String,
    pub platform: String,
    pub side: Side,
    pub amount: Decimal,
    pub receipt: TradeReceipt,
    /// Edge percentage at time of bet (for dashboard display).
    pub edge_pct: f64,
    /// Estimate confidence at time of bet (for dashboard display).
    pub confidence: f64,
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
    betfair: Option<BetfairClient>,
    // forecastex: Option<ForecastExClient>,  // Phase 2A
    dry_run: bool,
}

impl Executor {
    pub fn new(manifold: Option<ManifoldClient>, dry_run: bool) -> Self {
        Self {
            manifold,
            betfair: None,
            dry_run,
        }
    }

    pub fn with_betfair(
        manifold: Option<ManifoldClient>,
        betfair: Option<BetfairClient>,
        dry_run: bool,
    ) -> Self {
        Self {
            manifold,
            betfair,
            dry_run,
        }
    }

    /// Fetch the live Mana account snapshot from the Manifold API (best-effort).
    ///
    /// Returns `None` when no Manifold client is configured, the API key is absent,
    /// or the request fails. Used to reconcile `state.mana_bankroll` (liquid balance)
    /// and `state.total_mana_pnl` (resolved profit) with Manifold ground-truth.
    pub async fn get_mana_info(&self) -> Option<crate::platforms::manifold::ManifoldUserInfo> {
        self.manifold.as_ref()?.get_user_info().await
    }

    /// Check which open Manifold bets have resolved and return outcomes.
    ///
    /// Returns an empty vec when no Manifold client is configured or
    /// `open_bets` is empty.
    pub async fn check_manifold_resolutions(
        &self,
        open_bets: &[crate::types::TradeReceipt],
    ) -> Vec<crate::platforms::manifold::ManifoldResolution> {
        if open_bets.is_empty() {
            return Vec::new();
        }
        match &self.manifold {
            Some(client) => client.check_resolutions(open_bets).await,
            None => Vec::new(),
        }
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
            total_committed: Decimal::ZERO,
            total_commission: Decimal::ZERO,
        };

        if bets.is_empty() {
            return Ok(report);
        }

        info!(count = bets.len(), dry_run = self.dry_run, "Executing batch");

        for bet in bets {
            let platform = bet.edge.market.platform.as_str();
            let edge_pct = (bet.edge.edge * dec!(100)).to_f64().unwrap_or(0.0);
            let confidence = bet.edge.estimate.confidence.to_f64().unwrap_or(0.0);

            // Manifold paper execution: always attempt regardless of dry_run (play money).
            if platform == "manifold" {
                if let Some(ref manifold) = self.manifold {
                    match self.execute_on_manifold(manifold, bet).await {
                        Ok(receipt) => {
                            report.executed.push(ExecutedTrade {
                                market_id: bet.edge.market.id.clone(),
                                platform: "manifold".to_string(),
                                side: bet.edge.side.clone(),
                                amount: bet.bet_amount,
                                receipt,
                                edge_pct,
                                confidence,
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
                } else {
                    // No Manifold client available — log as dry-run
                    info!(
                        market_id = %bet.edge.market.id,
                        side = ?bet.edge.side,
                        amount = format!("{:.0} Mana", bet.bet_amount),
                        edge = format!("{:.1}%", bet.edge.edge * dec!(100)),
                        "[DRY RUN] No Manifold client — would place paper bet"
                    );
                    report.executed.push(ExecutedTrade {
                        market_id: bet.edge.market.id.clone(),
                        platform: "dry-run".to_string(),
                        side: bet.edge.side.clone(),
                        amount: bet.bet_amount,
                        receipt: TradeReceipt::dry_run(&bet.edge.market.id, bet.bet_amount, "Mana"),
                        edge_pct,
                        confidence,
                    });
                    report.total_committed += bet.bet_amount;
                }
                continue;
            }

            // For all other platforms, respect the dry_run flag.
            if self.dry_run {
                info!(
                    market_id = %bet.edge.market.id,
                    side = ?bet.edge.side,
                    amount = format!("${:.2}", bet.bet_amount),
                    edge = format!("{:.1}%", bet.edge.edge * dec!(100)),
                    kelly = format!("{:.2}%", bet.kelly_fraction * dec!(100)),
                    "[DRY RUN] Would place bet"
                );
                report.executed.push(ExecutedTrade {
                    market_id: bet.edge.market.id.clone(),
                    platform: "dry-run".to_string(),
                    side: bet.edge.side.clone(),
                    amount: bet.bet_amount,
                    receipt: TradeReceipt::dry_run(&bet.edge.market.id, bet.bet_amount, "AUD"),
                    edge_pct,
                    confidence,
                });
                report.total_committed += bet.bet_amount;
                continue;
            }

            // Betfair real-money execution
            if let Some(ref betfair) = self.betfair {
                if platform == "betfair" {
                    match self.execute_on_betfair(betfair, bet).await {
                        Ok(receipt) => {
                            report.total_commission += receipt.fees;
                            report.executed.push(ExecutedTrade {
                                market_id: bet.edge.market.id.clone(),
                                platform: "betfair".to_string(),
                                side: bet.edge.side.clone(),
                                amount: bet.bet_amount,
                                receipt,
                                edge_pct,
                                confidence,
                            });
                            report.total_committed += bet.bet_amount;
                        }
                        Err(e) => {
                            warn!(
                                market_id = %bet.edge.market.id,
                                error = %e,
                                "Betfair execution failed"
                            );
                            report.failed.push(FailedTrade {
                                market_id: bet.edge.market.id.clone(),
                                platform: "betfair".to_string(),
                                reason: e.to_string(),
                            });
                        }
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

    async fn execute_on_betfair(
        &self,
        client: &BetfairClient,
        bet: &SizedBet,
    ) -> Result<TradeReceipt> {
        if bet.edge.market.platform != "betfair" {
            anyhow::bail!("Market {} is not a Betfair market", bet.edge.market.id);
        }

        client
            .place_bet(&bet.edge.market.id, bet.edge.side.clone(), bet.bet_amount)
            .await
            .context("Betfair bet placement failed")
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
    /// `currency` should be "AUD" for real-money platforms or "Mana" for Manifold.
    pub fn dry_run(market_id: &str, amount: Decimal, currency: &str) -> Self {
        Self {
            order_id: format!("dry-run-{}", uuid::Uuid::new_v4()),
            market_id: market_id.to_string(),
            platform: "dry-run".to_string(),
            side: Side::Yes,
            amount,
            fill_price: Decimal::ZERO,
            fees: Decimal::ZERO,
            timestamp: chrono::Utc::now(),
            currency: currency.to_string(),
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

    fn make_sized_bet(market_id: &str, amount: Decimal) -> SizedBet {
        SizedBet {
            edge: Edge {
                market: Market {
                    id: market_id.to_string(),
                    platform: "manifold".to_string(),
                    question: "Test?".into(),
                    description: String::new(),
                    category: MarketCategory::Weather,
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

    #[tokio::test]
    async fn test_dry_run_execution() {
        let executor = Executor::new(None, true);
        let bets = vec![make_sized_bet("m1", dec!(50)), make_sized_bet("m2", dec!(30))];
        let report = executor.execute_batch(&bets).await.unwrap();

        assert_eq!(report.executed.len(), 2);
        assert_eq!(report.failed.len(), 0);
        assert_eq!(report.total_committed, dec!(80));
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
        let receipt = TradeReceipt::dry_run("test-market", dec!(100), "AUD");
        assert!(receipt.order_id.starts_with("dry-run-"));
        assert_eq!(receipt.amount, dec!(100));
        assert_eq!(receipt.fees, Decimal::ZERO);
        assert_eq!(receipt.currency, "AUD");
    }

    #[tokio::test]
    async fn test_no_manifold_logs_dry_run() {
        // No Manifold client, not global dry-run — Manifold markets still get
        // a dry-run receipt logged (so the accountant can track them).
        let executor = Executor::new(None, false);
        let bets = vec![make_sized_bet("m1", dec!(50))];
        let report = executor.execute_batch(&bets).await.unwrap();
        assert_eq!(report.executed.len(), 1);
        assert_eq!(report.executed[0].platform, "dry-run");
        assert_eq!(report.failed.len(), 0);
    }
}
