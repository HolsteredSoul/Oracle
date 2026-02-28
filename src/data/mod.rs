//! Data enrichment providers.
//!
//! Defines the `DataProvider` trait and provides category-specific
//! implementations for fetching real-time context data.

pub mod weather;
pub mod sports;
pub mod economics;
pub mod news;

use anyhow::Result;
use async_trait::async_trait;
use rust_decimal::Decimal;

use crate::types::{DataContext, Market, MarketCategory};

/// Abstraction over external data sources.
///
/// Each provider covers a specific market category and fetches
/// real-time context to enrich LLM probability estimates.
#[async_trait]
pub trait DataProvider: Send + Sync {
    /// The market category this provider covers.
    fn category(&self) -> MarketCategory;

    /// Fetch relevant context data for a market question.
    async fn fetch_context(&self, market: &Market) -> Result<DataContext>;

    /// Cost per API call in USD (for survival accounting).
    fn cost_per_call(&self) -> Decimal;
}
