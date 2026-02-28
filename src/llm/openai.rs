//! OpenAI GPT-4 LLM integration.
//!
//! Implements the `LlmEstimator` trait as an alternative provider.
//! Uses the same prompt templates as Anthropic but targets the
//! OpenAI Chat Completions API.

use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use super::LlmEstimator;
use crate::llm::anthropic::AnthropicClient; // Reuse parsing utilities
use crate::types::{d, DataContext, Estimate, Market};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

const OPENAI_API_URL: &str = "https://api.openai.com/v1/chat/completions";
const DEFAULT_MODEL: &str = "gpt-4o";
const DEFAULT_MAX_TOKENS: u32 = 1024;

const MAX_RETRIES: u32 = 3;
const BASE_BACKOFF_MS: u64 = 1000;

/// Approximate cost per 1K input tokens (GPT-4o).
const INPUT_COST_PER_1K: f64 = 0.005;
/// Approximate cost per 1K output tokens (GPT-4o).
const OUTPUT_COST_PER_1K: f64 = 0.015;

// ---------------------------------------------------------------------------
// API types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<ChatMessage>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    #[serde(default)]
    choices: Vec<Choice>,
    #[serde(default)]
    usage: Option<ChatUsage>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    #[serde(default)]
    message: Option<ChatMessage>,
}

#[derive(Debug, Deserialize)]
struct ChatUsage {
    #[serde(default)]
    prompt_tokens: u32,
    #[serde(default)]
    completion_tokens: u32,
    #[serde(default)]
    total_tokens: u32,
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

pub struct OpenAiClient {
    http: Client,
    api_key: String,
    model: String,
    max_tokens: u32,
    total_cost: std::sync::atomic::AtomicU64,
    total_calls: std::sync::atomic::AtomicU64,
}

impl OpenAiClient {
    pub fn new(api_key: String, model: Option<String>, max_tokens: Option<u32>) -> Result<Self> {
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .context("Failed to build OpenAI HTTP client")?;

        Ok(Self {
            http,
            api_key,
            model: model.unwrap_or_else(|| DEFAULT_MODEL.to_string()),
            max_tokens: max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
            total_cost: std::sync::atomic::AtomicU64::new(0),
            total_calls: std::sync::atomic::AtomicU64::new(0),
        })
    }

    async fn call_api(&self, system: &str, user_message: &str) -> Result<(String, u32, f64)> {
        let request = ChatRequest {
            model: self.model.clone(),
            max_tokens: self.max_tokens,
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: system.to_string(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: user_message.to_string(),
                },
            ],
        };

        let mut last_error = None;

        for attempt in 0..=MAX_RETRIES {
            if attempt > 0 {
                let delay = BASE_BACKOFF_MS * 2u64.pow(attempt - 1);
                tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
            }

            let resp = self.http
                .post(OPENAI_API_URL)
                .header("Authorization", format!("Bearer {}", self.api_key))
                .header("Content-Type", "application/json")
                .json(&request)
                .send()
                .await;

            match resp {
                Ok(response) => {
                    let status = response.status();

                    if status.is_success() {
                        let body: ChatResponse = response.json().await
                            .context("Failed to parse OpenAI response")?;

                        let text = body.choices.first()
                            .and_then(|c| c.message.as_ref())
                            .map(|m| m.content.clone())
                            .unwrap_or_default();

                        let usage = body.usage.unwrap_or(ChatUsage {
                            prompt_tokens: 0,
                            completion_tokens: 0,
                            total_tokens: 0,
                        });

                        let cost = (usage.prompt_tokens as f64 / 1000.0) * INPUT_COST_PER_1K
                            + (usage.completion_tokens as f64 / 1000.0) * OUTPUT_COST_PER_1K;

                        let cost_micro = (cost * 1_000_000.0) as u64;
                        self.total_cost.fetch_add(cost_micro, std::sync::atomic::Ordering::Relaxed);
                        self.total_calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                        return Ok((text, usage.total_tokens, cost));
                    }

                    if status.as_u16() == 429 || status.as_u16() >= 500 {
                        let error_text = response.text().await.unwrap_or_default();
                        warn!(status = %status, attempt, "Retryable OpenAI error");
                        last_error = Some(format!("HTTP {status}: {error_text}"));
                        continue;
                    }

                    let error_text = response.text().await.unwrap_or_default();
                    anyhow::bail!("OpenAI API error {status}: {error_text}");
                }
                Err(e) => {
                    last_error = Some(format!("Request error: {e}"));
                    continue;
                }
            }
        }

        anyhow::bail!("OpenAI API failed after {MAX_RETRIES} retries: {}", last_error.unwrap_or_default())
    }

    pub fn cumulative_cost(&self) -> f64 {
        self.total_cost.load(std::sync::atomic::Ordering::Relaxed) as f64 / 1_000_000.0
    }

    pub fn total_calls(&self) -> u64 {
        self.total_calls.load(std::sync::atomic::Ordering::Relaxed)
    }
}

#[async_trait]
impl LlmEstimator for OpenAiClient {
    async fn estimate_probability(
        &self,
        market: &Market,
        context: &DataContext,
    ) -> Result<Estimate> {
        let system = AnthropicClient::system_prompt();
        let user_msg = AnthropicClient::build_single_prompt(market, context);

        debug!(market_id = %market.id, model = %self.model, "OpenAI single estimate");

        let (response_text, tokens, cost) = self.call_api(system, &user_msg).await?;
        let (prob_f64, conf_f64, reasoning) = AnthropicClient::parse_estimate(&response_text)?;

        Ok(Estimate {
            probability: d(prob_f64),
            confidence: d(conf_f64),
            reasoning,
            tokens_used: tokens,
            cost: d(cost),
        })
    }

    async fn batch_estimate(
        &self,
        markets: &[(Market, DataContext)],
    ) -> Result<Vec<Estimate>> {
        // Same batch logic as Anthropic — reuse prompt + parsing
        if markets.is_empty() {
            return Ok(Vec::new());
        }

        if markets.len() <= 2 {
            let mut results = Vec::with_capacity(markets.len());
            for (market, context) in markets {
                results.push(self.estimate_probability(market, context).await?);
            }
            return Ok(results);
        }

        let system = AnthropicClient::system_prompt();
        let user_msg = AnthropicClient::build_batch_prompt(markets);

        let (response_text, tokens, cost) = self.call_api(system, &user_msg).await?;

        let expected_ids: Vec<&str> = markets.iter().map(|(m, _)| m.id.as_str()).collect();
        let parsed = AnthropicClient::parse_batch_response(&response_text, &expected_ids);

        let cost_per = cost / markets.len() as f64;
        let tokens_per = tokens / markets.len() as u32;

        let mut results = Vec::with_capacity(markets.len());

        for (i, (market, context)) in markets.iter().enumerate() {
            match parsed.get(i).and_then(|p| p.as_ref()) {
                Some((prob, conf)) => {
                    results.push(Estimate {
                        probability: d(*prob),
                        confidence: d(*conf),
                        reasoning: format!("(batch estimate for {})", market.id),
                        tokens_used: tokens_per,
                        cost: d(cost_per),
                    });
                }
                None => {
                    match self.estimate_probability(market, context).await {
                        Ok(est) => results.push(est),
                        Err(e) => {
                            results.push(Estimate {
                                probability: market.current_price_yes,
                                confidence: dec!(0.1),
                                reasoning: format!("Estimation failed: {e}"),
                                tokens_used: 0,
                                cost: Decimal::ZERO,
                            });
                        }
                    }
                }
            }
        }

        Ok(results)
    }

    fn cost_per_call(&self) -> Decimal {
        d((500.0 / 1000.0) * INPUT_COST_PER_1K + (300.0 / 1000.0) * OUTPUT_COST_PER_1K)
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_construction() {
        let client = OpenAiClient::new("test-key".into(), None, None).unwrap();
        assert_eq!(client.model_name(), DEFAULT_MODEL);
        assert_eq!(client.cumulative_cost(), 0.0);
    }

    #[test]
    fn test_client_custom_model() {
        let client = OpenAiClient::new("key".into(), Some("gpt-4-turbo".into()), Some(2048)).unwrap();
        assert_eq!(client.model_name(), "gpt-4-turbo");
    }

    #[test]
    fn test_cost_per_call_positive() {
        let client = OpenAiClient::new("key".into(), None, None).unwrap();
        assert!(client.cost_per_call() > Decimal::ZERO);
    }
}
