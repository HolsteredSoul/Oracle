//! Platform integrations.
//!
//! Defines the `PredictionPlatform` trait and provides implementations for:
//! - ForecastEx (IB TWS API) — real-money execution (sole AU-compliant venue)
//! - Metaculus — read-only crowd forecast cross-reference
//! - Manifold — play-money validation and sentiment signal

pub mod forecastex;
pub mod metaculus;
pub mod manifold;
pub mod polymarket;

use anyhow::Result;
use async_trait::async_trait;
use rust_decimal::Decimal;

use crate::types::{LiquidityInfo, Market, Position, Side, TradeReceipt};

/// Abstraction over prediction market platforms.
///
/// Implementors provide market scanning, bet placement, and position tracking.
/// Read-only platforms (Metaculus) return errors or no-ops for write operations.
#[async_trait]
pub trait PredictionPlatform: Send + Sync {
    /// Fetch all active markets from this platform.
    async fn fetch_markets(&self) -> Result<Vec<Market>>;

    /// Place a bet on a specific market.
    /// Returns an error for read-only platforms.
    async fn place_bet(
        &self,
        market_id: &str,
        side: Side,
        amount: Decimal,
    ) -> Result<TradeReceipt>;

    /// Get current open positions on this platform.
    async fn get_positions(&self) -> Result<Vec<Position>>;

    /// Get available balance on this platform.
    async fn get_balance(&self) -> Result<Decimal>;

    /// Check liquidity for a specific market.
    async fn check_liquidity(&self, market_id: &str) -> Result<LiquidityInfo>;

    /// Whether this platform supports real-money execution.
    fn is_executable(&self) -> bool;

    /// Platform name for logging and identification.
    fn name(&self) -> &str;
}
