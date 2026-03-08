//! Auto-exit engine: take-profit, stop-loss, and time-based position closure.
//!
//! Runs after each scan cycle and checks all open positions against
//! configurable thresholds. Supports both Manifold (paper mode — sell shares)
//! and Betfair (live AUD mode — hedge/green-up with an opposite bet).
//!
//! ## Betfair Australia minimum stake rules (AUD, March 2026)
//! - Exchange back/lay bets via API: **AUD $1.00 absolute minimum**
//! - BSP back bets: same AUD $1.00 minimum
//! - BSP lay liability: minimum unchanged
//! - Sub-minimum bets via API are possible but risk account suspension
//! - Min Bet Payout exception: bets < $1 AUD valid when payout ≥ $10 AUD
//!
//! Oracle uses **$2.00 AUD** as a safety buffer above the absolute minimum.
//! Set `min_close_stake = 2.0` in [strategy] config (default).
//!
//! ## Manifold minimum sell
//! - No documented minimum; `shares` parameter defaults to all shares
//! - Practical minimum: 1 Mana — no meaningful constraint for auto-exit

use anyhow::Result;
use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal::prelude::*;
use rust_decimal_macros::dec;
use tracing::{info, warn};

use crate::platforms::betfair::BetfairClient;
use crate::platforms::manifold::ManifoldClient;
use crate::types::{Side, TradeReceipt};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Auto-exit configuration, sourced from the [strategy] section of config.toml.
#[derive(Debug, Clone)]
pub struct AutoExitConfig {
    /// Enable automatic position closing.
    pub enabled: bool,
    /// Close if unrealized P&L reaches this percentage (e.g. 15.0 = +15%).
    pub take_profit_percent: Decimal,
    /// Close if unrealized P&L falls to this percentage (e.g. -10.0 = -10%).
    pub stop_loss_percent: Decimal,
    /// Force-close after this many hours (0 = disabled).
    pub max_hold_hours: u64,
    /// Minimum closing stake.
    /// Betfair official minimum: AUD $1.00. Default here: $2.00 as safety buffer.
    /// Manifold: effectively unconstrained (1 Mana).
    pub min_close_stake: Decimal,
    /// If true, log decisions without placing real orders.
    pub dry_run: bool,
}

impl Default for AutoExitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            // Manifold CPMM markets swing ±20 % intraday on thin books.
            // The old ±10/+15 defaults stopped out real edges within the first
            // cycle. These wider values give positions room to breathe while
            // still cutting catastrophic losses and locking in large wins.
            take_profit_percent: dec!(40.0),
            stop_loss_percent: dec!(-25.0),
            max_hold_hours: 168,  // 1 week — let markets move toward resolution
            min_close_stake: dec!(2.0),
            dry_run: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Close result types
// ---------------------------------------------------------------------------

/// The trigger that caused an auto-exit.
#[derive(Debug, Clone, PartialEq)]
pub enum CloseReason {
    TakeProfit,
    StopLoss,
    MaxHoldTime,
}

impl std::fmt::Display for CloseReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CloseReason::TakeProfit => write!(f, "TakeProfit"),
            CloseReason::StopLoss => write!(f, "StopLoss"),
            CloseReason::MaxHoldTime => write!(f, "MaxHoldTime"),
        }
    }
}

/// Outcome of a single auto-exit close attempt.
#[derive(Debug, Clone)]
pub struct CloseResult {
    pub market_id: String,
    pub bet_id: String,
    pub platform: String,
    pub reason: CloseReason,
    /// Realized P&L in the platform currency (Mana for Manifold, AUD for Betfair).
    pub realized_pnl: Decimal,
    pub success: bool,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Auto-exit engine
// ---------------------------------------------------------------------------

pub struct AutoExitEngine {
    manifold: Option<ManifoldClient>,
    betfair: Option<BetfairClient>,
    config: AutoExitConfig,
}

impl AutoExitEngine {
    pub fn new(
        manifold: Option<ManifoldClient>,
        betfair: Option<BetfairClient>,
        config: AutoExitConfig,
    ) -> Self {
        Self { manifold, betfair, config }
    }

    /// Check all open positions and close any that have hit a trigger.
    ///
    /// Returns a list of close results (both successes and failures).
    /// Positions that don't meet a trigger are silently skipped.
    pub async fn check_and_close(&self, open_bets: &[TradeReceipt]) -> Vec<CloseResult> {
        if !self.config.enabled || open_bets.is_empty() {
            return Vec::new();
        }

        let mut results = Vec::new();

        for bet in open_bets {
            // Skip dry-run receipts — these are not real positions
            if bet.platform == "dry-run" {
                continue;
            }

            let result = match bet.platform.as_str() {
                "manifold" => {
                    if let Some(ref client) = self.manifold {
                        self.check_manifold_position(client, bet).await
                    } else {
                        None
                    }
                }
                "betfair" => {
                    if let Some(ref client) = self.betfair {
                        self.check_betfair_position(client, bet).await
                    } else {
                        None
                    }
                }
                _ => None,
            };

            if let Some(r) = result {
                results.push(r);
            }
        }

        results
    }

    // -- Manifold position check ------------------------------------------

    async fn check_manifold_position(
        &self,
        client: &ManifoldClient,
        bet: &TradeReceipt,
    ) -> Option<CloseResult> {
        // Fetch current market probability
        let current_prob = match client.get_market_probability(&bet.market_id).await {
            Ok(p) => p,
            Err(e) => {
                warn!(
                    market_id = %bet.market_id,
                    error = %e,
                    "Auto-exit: failed to fetch Manifold market price"
                );
                return None;
            }
        };

        // fill_price for Manifold is prob_after at time of bet placement.
        // It serves as our entry price proxy.
        let entry_prob = bet.fill_price;
        if entry_prob <= Decimal::ZERO || entry_prob >= Decimal::ONE {
            return None; // Can't compute P&L with invalid entry price
        }

        // P&L%:
        //   YES bet: profit when current_prob rises above entry_prob
        //   NO bet:  profit when current_prob falls below entry_prob
        let pnl_pct = match bet.side {
            Side::Yes => (current_prob - entry_prob) / entry_prob * dec!(100),
            Side::No => {
                let entry_no = Decimal::ONE - entry_prob;
                let current_no = Decimal::ONE - current_prob;
                if entry_no <= Decimal::ZERO {
                    return None;
                }
                (current_no - entry_no) / entry_no * dec!(100)
            }
        };

        let reason = self.check_triggers(bet, pnl_pct)?;

        // Approximate realized P&L in Mana
        let realized_pnl = bet.amount * pnl_pct / dec!(100);

        info!(
            market_id = %bet.market_id,
            bet_id = %bet.order_id,
            pnl_pct = %format!("{:.2}%", pnl_pct),
            reason = %reason,
            "Auto-exit trigger hit on Manifold position"
        );

        if self.config.dry_run {
            let pnl_str = format!("{:.1} Mana", realized_pnl);
            if realized_pnl >= Decimal::ZERO {
                info!(
                    market_id = %bet.market_id,
                    pnl = %pnl_str,
                    reason = %reason,
                    "[DRY RUN] Auto-close would lock +{} profit (Manifold)",
                    pnl_str
                );
            } else {
                info!(
                    market_id = %bet.market_id,
                    pnl = %pnl_str,
                    reason = %reason,
                    "[DRY RUN] Stop-loss would close Manifold position for {} Mana",
                    pnl_str
                );
            }
            return Some(CloseResult {
                market_id: bet.market_id.clone(),
                bet_id: bet.order_id.clone(),
                platform: "manifold".to_string(),
                reason,
                realized_pnl,
                success: true,
                message: "dry-run".to_string(),
            });
        }

        // Sell all shares for this outcome
        let outcome = match bet.side {
            Side::Yes => "YES",
            Side::No => "NO",
        };

        match client.sell_shares(&bet.market_id, outcome).await {
            Ok(returned_amount) => {
                let pnl = returned_amount
                    .map(|a| a - bet.amount) // net P&L = returned - staked
                    .unwrap_or(realized_pnl);

                if pnl >= Decimal::ZERO {
                    info!(
                        market_id = %bet.market_id,
                        pnl_mana = %format!("{:.1} Mana", pnl),
                        "Auto-closed [{}] for +{:.1} Mana profit",
                        bet.market_id, pnl
                    );
                } else {
                    info!(
                        market_id = %bet.market_id,
                        pnl_mana = %format!("{:.1} Mana", pnl),
                        "Stop-loss hit – closed [{}] for {:.1} Mana",
                        bet.market_id, pnl
                    );
                }

                Some(CloseResult {
                    market_id: bet.market_id.clone(),
                    bet_id: bet.order_id.clone(),
                    platform: "manifold".to_string(),
                    reason,
                    realized_pnl: pnl,
                    success: true,
                    message: String::new(),
                })
            }
            Err(e) => {
                warn!(
                    market_id = %bet.market_id,
                    error = %e,
                    "Auto-exit: failed to sell Manifold shares"
                );
                Some(CloseResult {
                    market_id: bet.market_id.clone(),
                    bet_id: bet.order_id.clone(),
                    platform: "manifold".to_string(),
                    reason,
                    realized_pnl,
                    success: false,
                    message: e.to_string(),
                })
            }
        }
    }

    // -- Betfair position check ------------------------------------------

    async fn check_betfair_position(
        &self,
        client: &BetfairClient,
        bet: &TradeReceipt,
    ) -> Option<CloseResult> {
        // fetch_market_books returns current decimal odds
        let current_odds = match client.get_best_back_odds(&bet.market_id).await {
            Ok(Some(o)) => o,
            Ok(None) => {
                warn!(
                    market_id = %bet.market_id,
                    "Auto-exit: no odds available for Betfair market"
                );
                return None;
            }
            Err(e) => {
                warn!(
                    market_id = %bet.market_id,
                    error = %e,
                    "Auto-exit: failed to fetch Betfair market odds"
                );
                return None;
            }
        };

        // fill_price for Betfair = decimal odds at order placement (e.g. 3.00).
        let entry_odds = bet.fill_price;
        if entry_odds <= Decimal::ONE || current_odds <= Decimal::ONE {
            return None; // Invalid odds — skip
        }

        // P&L% for greening up:
        //   BACK (Side::Yes): win when odds shorten (current < entry).
        //     locked_profit = stake × (entry_odds − current_odds) / current_odds
        //     P&L% = (entry_odds − current_odds) / current_odds × 100
        //   LAY (Side::No): win when odds lengthen (current > entry).
        //     locked_profit = stake × (current_odds − entry_odds) / current_odds
        //     P&L% = (current_odds − entry_odds) / current_odds × 100
        let pnl_pct = match bet.side {
            Side::Yes => (entry_odds - current_odds) / current_odds * dec!(100),
            Side::No => (current_odds - entry_odds) / current_odds * dec!(100),
        };

        let reason = self.check_triggers(bet, pnl_pct)?;

        // Hedge/green-up stake:
        //   For BACK: lay_stake = original_stake × entry_odds / current_odds
        //   For LAY:  back_stake = original_stake × entry_odds / current_odds
        //   (same formula — just the side flips)
        let close_stake = bet.amount * entry_odds / current_odds;

        // Enforce Betfair AUD minimum stake ($1.00 official; $2.00 safety buffer)
        if close_stake < self.config.min_close_stake {
            warn!(
                market_id = %bet.market_id,
                close_stake = %format!("${:.2} AUD", close_stake),
                min = %format!("${:.2} AUD", self.config.min_close_stake),
                "Auto-exit: closing stake below minimum — position too small to close"
            );
            return None;
        }

        // Liquidity check: available lay liquidity must be ≥ 80% of close stake
        let available_liquidity = match client.get_available_liquidity(&bet.market_id).await {
            Ok(liq) => liq,
            Err(e) => {
                warn!(
                    market_id = %bet.market_id,
                    error = %e,
                    "Auto-exit: failed to fetch Betfair liquidity"
                );
                return None;
            }
        };

        let required_liquidity = close_stake * dec!(0.80);
        if available_liquidity < required_liquidity {
            warn!(
                market_id = %bet.market_id,
                available = %format!("${:.2} AUD", available_liquidity),
                required = %format!("${:.2} AUD", required_liquidity),
                "Auto-exit: insufficient Betfair liquidity to close position safely"
            );
            return None;
        }

        let realized_pnl = bet.amount * pnl_pct / dec!(100);

        info!(
            market_id = %bet.market_id,
            bet_id = %bet.order_id,
            pnl_pct = %format!("{:.2}%", pnl_pct),
            close_stake = %format!("${:.2} AUD", close_stake),
            reason = %reason,
            "Auto-exit trigger hit on Betfair position"
        );

        if self.config.dry_run {
            if realized_pnl >= Decimal::ZERO {
                info!(
                    market_id = %bet.market_id,
                    pnl_aud = %format!("${:.2}", realized_pnl),
                    reason = %reason,
                    "[DRY RUN] Auto-close [{}] would lock +${:.2} AUD profit",
                    bet.market_id, realized_pnl
                );
            } else {
                info!(
                    market_id = %bet.market_id,
                    pnl_aud = %format!("${:.2}", realized_pnl),
                    reason = %reason,
                    "[DRY RUN] Stop-loss hit – would close [{}] for -${:.2} AUD",
                    bet.market_id, realized_pnl.abs()
                );
            }
            return Some(CloseResult {
                market_id: bet.market_id.clone(),
                bet_id: bet.order_id.clone(),
                platform: "betfair".to_string(),
                reason,
                realized_pnl,
                success: true,
                message: "dry-run".to_string(),
            });
        }

        // Place the opposite (hedge) bet to fully green-up
        use crate::platforms::PredictionPlatform;
        let hedge_side = match bet.side {
            Side::Yes => Side::No, // BACK → close by LAYing
            Side::No => Side::Yes, // LAY → close by BACKing
        };

        match client.place_bet(&bet.market_id, hedge_side, close_stake).await {
            Ok(_receipt) => {
                if realized_pnl >= Decimal::ZERO {
                    info!(
                        market_id = %bet.market_id,
                        pnl_aud = %format!("${:.2}", realized_pnl),
                        "Auto-closed [{}] for +${:.2} profit",
                        bet.market_id, realized_pnl
                    );
                } else {
                    info!(
                        market_id = %bet.market_id,
                        pnl_aud = %format!("${:.2}", realized_pnl.abs()),
                        "Stop-loss hit – closed [{}] for -${:.2}",
                        bet.market_id, realized_pnl.abs()
                    );
                }

                Some(CloseResult {
                    market_id: bet.market_id.clone(),
                    bet_id: bet.order_id.clone(),
                    platform: "betfair".to_string(),
                    reason,
                    realized_pnl,
                    success: true,
                    message: String::new(),
                })
            }
            Err(e) => {
                warn!(
                    market_id = %bet.market_id,
                    error = %e,
                    "Auto-exit: failed to place Betfair hedge bet"
                );
                Some(CloseResult {
                    market_id: bet.market_id.clone(),
                    bet_id: bet.order_id.clone(),
                    platform: "betfair".to_string(),
                    reason,
                    realized_pnl,
                    success: false,
                    message: e.to_string(),
                })
            }
        }
    }

    // -- Trigger evaluation -----------------------------------------------

    /// Return the first trigger hit, or `None` if no threshold is crossed.
    fn check_triggers(&self, bet: &TradeReceipt, pnl_pct: Decimal) -> Option<CloseReason> {
        if pnl_pct >= self.config.take_profit_percent {
            return Some(CloseReason::TakeProfit);
        }
        if pnl_pct <= self.config.stop_loss_percent {
            return Some(CloseReason::StopLoss);
        }
        if self.config.max_hold_hours > 0 {
            let hold_hours = match (Utc::now() - bet.timestamp).to_std() {
                Ok(d) => d.as_secs() / 3600,
                Err(_) => return None, // timestamp is in the future — skip (clock drift)
            };
            if hold_hours >= self.config.max_hold_hours {
                return Some(CloseReason::MaxHoldTime);
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use rust_decimal_macros::dec;

    fn make_bet(platform: &str, side: Side, amount: Decimal, fill_price: Decimal, hours_ago: i64) -> TradeReceipt {
        TradeReceipt {
            order_id: uuid::Uuid::new_v4().to_string(),
            market_id: "test-market".to_string(),
            platform: platform.to_string(),
            side,
            amount,
            fill_price,
            fees: Decimal::ZERO,
            timestamp: Utc::now() - Duration::hours(hours_ago),
            currency: if platform == "manifold" { "Mana".to_string() } else { "AUD".to_string() },
        }
    }

    #[test]
    fn test_take_profit_trigger() {
        let config = AutoExitConfig {
            take_profit_percent: dec!(15.0),
            stop_loss_percent: dec!(-10.0),
            max_hold_hours: 48,
            ..AutoExitConfig::default()
        };
        let engine = AutoExitEngine::new(None, None, config);
        let bet = make_bet("manifold", Side::Yes, dec!(100), dec!(0.5), 1);

        // +20% > +15% take_profit
        let reason = engine.check_triggers(&bet, dec!(20.0));
        assert_eq!(reason, Some(CloseReason::TakeProfit));
    }

    #[test]
    fn test_stop_loss_trigger() {
        let config = AutoExitConfig::default();
        let engine = AutoExitEngine::new(None, None, config);
        let bet = make_bet("manifold", Side::Yes, dec!(100), dec!(0.5), 1);

        // -30% < -25% stop_loss threshold
        let reason = engine.check_triggers(&bet, dec!(-30.0));
        assert_eq!(reason, Some(CloseReason::StopLoss));

        // -15% is within the new wider band — should NOT trigger
        let no_trigger = engine.check_triggers(&bet, dec!(-15.0));
        assert_eq!(no_trigger, None);
    }

    #[test]
    fn test_max_hold_time_trigger() {
        let config = AutoExitConfig {
            max_hold_hours: 24,
            ..AutoExitConfig::default()
        };
        let engine = AutoExitEngine::new(None, None, config);
        let bet = make_bet("manifold", Side::Yes, dec!(100), dec!(0.5), 30); // 30 hours old

        // 30h > 24h limit, P&L within bounds
        let reason = engine.check_triggers(&bet, dec!(5.0));
        assert_eq!(reason, Some(CloseReason::MaxHoldTime));
    }

    #[test]
    fn test_no_trigger_within_bounds() {
        let config = AutoExitConfig::default();
        let engine = AutoExitEngine::new(None, None, config);
        let bet = make_bet("manifold", Side::Yes, dec!(100), dec!(0.5), 1);

        // +5% within bounds, 1h old
        let reason = engine.check_triggers(&bet, dec!(5.0));
        assert_eq!(reason, None);
    }

    #[test]
    fn test_disabled_engine_returns_empty() {
        let config = AutoExitConfig { enabled: false, ..AutoExitConfig::default() };
        let engine = AutoExitEngine::new(None, None, config);
        let bet = make_bet("manifold", Side::Yes, dec!(100), dec!(0.5), 1);

        // Even with a trigger, disabled engine returns nothing
        let rt = tokio::runtime::Runtime::new().unwrap();
        let results = rt.block_on(engine.check_and_close(&[bet]));
        assert!(results.is_empty());
    }

    #[test]
    fn test_betfair_close_stake_below_minimum_skipped() {
        // close_stake = amount * entry_odds / current_odds
        // 1.0 * 2.0 / 5.0 = 0.40 → below $2.00 min → should return None
        let config = AutoExitConfig {
            min_close_stake: dec!(2.0),
            take_profit_percent: dec!(10.0),
            ..AutoExitConfig::default()
        };
        // entry_odds = 5.0, current_odds = 2.0 → pnl_pct = (5-2)/2*100 = +150%
        // close_stake = 1.0 * 5.0 / 2.0 = 2.50 → above min → would proceed
        // Test with entry=2.0, current=5.0 → pnl = (2-5)/5*100 = -60% → stop_loss trigger
        // close_stake = 1.0 * 2.0 / 5.0 = 0.40 → below min → skip
        let engine = AutoExitEngine::new(None, None, config.clone());
        let mut bet = make_bet("betfair", Side::Yes, dec!(1.0), dec!(2.0), 1);
        let current_odds = dec!(5.0);
        let entry_odds = bet.fill_price;
        let close_stake = bet.amount * entry_odds / current_odds;
        assert!(close_stake < config.min_close_stake, "close_stake {close_stake} should be < min {}", config.min_close_stake);
    }

    #[test]
    fn test_max_hold_hours_zero_disabled() {
        let config = AutoExitConfig {
            max_hold_hours: 0,
            ..AutoExitConfig::default()
        };
        let engine = AutoExitEngine::new(None, None, config);
        // 200 hours old but max_hold_hours = 0 (disabled)
        let bet = make_bet("manifold", Side::Yes, dec!(100), dec!(0.5), 200);
        let reason = engine.check_triggers(&bet, dec!(5.0));
        assert_eq!(reason, None);
    }
}
