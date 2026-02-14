//! LLM integration for fair-value probability estimation.
//!
//! Defines the `LlmEstimator` trait and provides implementations for
//! Claude (Anthropic), GPT-4 (OpenAI), and Grok.

pub mod anthropic;
pub mod openai;
pub mod grok;

use anyhow::Result;
use async_trait::async_trait;

use crate::types::{DataContext, Estimate, Market};

/// Abstraction over LLM probability estimators.
///
/// Implementors send enriched market data to an LLM and parse
/// a calibrated probability estimate from the response.
#[async_trait]
pub trait LlmEstimator: Send + Sync {
    /// Estimate the probability of a single market outcome.
    async fn estimate_probability(
        &self,
        market: &Market,
        context: &DataContext,
    ) -> Result<Estimate>;

    /// Batch-estimate probabilities for multiple markets.
    /// More efficient than individual calls (fewer API round-trips).
    async fn batch_estimate(
        &self,
        markets: &[(Market, DataContext)],
    ) -> Result<Vec<Estimate>>;

    /// Cost per individual API call in USD.
    fn cost_per_call(&self) -> f64;

    /// Model identifier string.
    fn model_name(&self) -> &str;
}
