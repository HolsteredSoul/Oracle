//! OpenRouter LLM integration.
//!
//! Routes all LLM calls through OpenRouter's unified API, giving access to
//! multiple model providers with a single API key. Uses the OpenAI-compatible
//! chat completions format.
//!
//! Primary model: Claude 4 Sonnet (best probabilistic reasoning & calibration).
//! Fallback model: Grok-4.1-fast (cheap & fast, used when primary fails).

use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use super::LlmEstimator;
use crate::llm::anthropic::AnthropicClient; // Reuse prompt templates + parsing
use crate::types::{DataContext, Estimate, Market};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

const OPENROUTER_API_URL: &str = "https://openrouter.ai/api/v1/chat/completions";

/// Default primary model: Claude 4 Sonnet via OpenRouter.
const DEFAULT_PRIMARY_MODEL: &str = "anthropic/claude-sonnet-4";

/// Default fallback model: Grok-4.1-fast via OpenRouter (cheap/fast).
const DEFAULT_FALLBACK_MODEL: &str = "x-ai/grok-4.1-fast";

const DEFAULT_MAX_TOKENS: u32 = 1024;

/// Maximum retries on rate limit / server errors per model attempt.
const MAX_RETRIES: u32 = 3;

/// Base delay for exponential backoff (ms).
const BASE_BACKOFF_MS: u64 = 1000;

// ---------------------------------------------------------------------------
// Cost tables (approximate per-1K-token pricing via OpenRouter)
// ---------------------------------------------------------------------------

/// Returns (input_cost_per_1k, output_cost_per_1k) for known models.
fn model_costs(model: &str) -> (f64, f64) {
    match model {
        // Claude 4 Sonnet
        m if m.contains("claude") && m.contains("sonnet") => (0.003, 0.015),
        // Claude 4 Opus
        m if m.contains("claude") && m.contains("opus") => (0.015, 0.075),
        // Claude 4 Haiku
        m if m.contains("claude") && m.contains("haiku") => (0.0008, 0.004),
        // Grok models
        m if m.contains("grok") => (0.003, 0.015),
        // GPT-4o
        m if m.contains("gpt-4o") => (0.005, 0.015),
        // Conservative default
        _ => (0.005, 0.015),
    }
}

// ---------------------------------------------------------------------------
// API types (OpenAI-compatible)
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
    #[serde(default)]
    model: Option<String>,
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

pub struct OpenRouterClient {
    http: Client,
    api_key: String,
    primary_model: String,
    fallback_model: Option<String>,
    max_tokens: u32,
    total_cost: std::sync::atomic::AtomicU64, // stored as cost * 1_000_000
    total_calls: std::sync::atomic::AtomicU64,
}

impl OpenRouterClient {
    /// Create a new OpenRouter client.
    ///
    /// - `api_key`: OpenRouter API key.
    /// - `primary_model`: Primary model ID (e.g. "anthropic/claude-sonnet-4").
    /// - `fallback_model`: Optional fallback model for when primary fails.
    /// - `max_tokens`: Max output tokens per request.
    pub fn new(
        api_key: String,
        primary_model: Option<String>,
        fallback_model: Option<String>,
        max_tokens: Option<u32>,
    ) -> Result<Self> {
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .context("Failed to build OpenRouter HTTP client")?;

        Ok(Self {
            http,
            api_key,
            primary_model: primary_model.unwrap_or_else(|| DEFAULT_PRIMARY_MODEL.to_string()),
            fallback_model: Some(
                fallback_model.unwrap_or_else(|| DEFAULT_FALLBACK_MODEL.to_string()),
            ),
            max_tokens: max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
            total_cost: std::sync::atomic::AtomicU64::new(0),
            total_calls: std::sync::atomic::AtomicU64::new(0),
        })
    }

    /// Send a chat completion request to OpenRouter for a specific model,
    /// with retry + exponential backoff.
    async fn call_model(
        &self,
        model: &str,
        system: &str,
        user_message: &str,
    ) -> Result<(String, u32, f64)> {
        let request = ChatRequest {
            model: model.to_string(),
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
                debug!(attempt, delay_ms = delay, model, "Retrying OpenRouter API call");
                tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
            }

            let resp = self
                .http
                .post(OPENROUTER_API_URL)
                .header("Authorization", format!("Bearer {}", self.api_key))
                .header("Content-Type", "application/json")
                .header("HTTP-Referer", "https://github.com/HolsteredSoul/Oracle")
                .header("X-Title", "ORACLE Prediction Agent")
                .json(&request)
                .send()
                .await;

            match resp {
                Ok(response) => {
                    let status = response.status();

                    if status.is_success() {
                        let body: ChatResponse = response
                            .json()
                            .await
                            .context("Failed to parse OpenRouter response")?;

                        let text = body
                            .choices
                            .first()
                            .and_then(|c| c.message.as_ref())
                            .map(|m| m.content.clone())
                            .unwrap_or_default();

                        let usage = body.usage.unwrap_or(ChatUsage {
                            prompt_tokens: 0,
                            completion_tokens: 0,
                            total_tokens: 0,
                        });

                        // Use the actual model returned (may differ from requested)
                        let actual_model = body.model.as_deref().unwrap_or(model);
                        let (input_cost, output_cost) = model_costs(actual_model);

                        let cost = (usage.prompt_tokens as f64 / 1000.0) * input_cost
                            + (usage.completion_tokens as f64 / 1000.0) * output_cost;

                        // Track cumulative cost
                        let cost_micro = (cost * 1_000_000.0) as u64;
                        self.total_cost
                            .fetch_add(cost_micro, std::sync::atomic::Ordering::Relaxed);
                        self.total_calls
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                        return Ok((text, usage.total_tokens, cost));
                    }

                    // Retryable errors: 429 (rate limit), 500+, 502 (upstream), 503
                    if status.as_u16() == 429 || status.as_u16() >= 500 {
                        let error_text = response.text().await.unwrap_or_default();
                        warn!(
                            status = %status,
                            attempt,
                            model,
                            error = %error_text,
                            "Retryable OpenRouter error"
                        );
                        last_error = Some(format!("HTTP {status}: {error_text}"));
                        continue;
                    }

                    // Non-retryable error
                    let error_text = response.text().await.unwrap_or_default();
                    anyhow::bail!("OpenRouter API error {status} (model={model}): {error_text}");
                }
                Err(e) => {
                    warn!(attempt, model, error = %e, "OpenRouter request failed");
                    last_error = Some(format!("Request error: {e}"));
                    continue;
                }
            }
        }

        anyhow::bail!(
            "OpenRouter API failed after {} retries (model={}): {}",
            MAX_RETRIES,
            model,
            last_error.unwrap_or_default()
        )
    }

    /// Call the primary model, falling back to the secondary if configured
    /// and the primary fails.
    async fn call_api(&self, system: &str, user_message: &str) -> Result<(String, u32, f64)> {
        match self.call_model(&self.primary_model, system, user_message).await {
            Ok(result) => Ok(result),
            Err(primary_err) => {
                if let Some(ref fallback) = self.fallback_model {
                    warn!(
                        primary = %self.primary_model,
                        fallback = %fallback,
                        error = %primary_err,
                        "Primary model failed, falling back"
                    );
                    self.call_model(fallback, system, user_message)
                        .await
                        .with_context(|| {
                            format!(
                                "Both primary ({}) and fallback ({}) models failed. Primary error: {}",
                                self.primary_model, fallback, primary_err
                            )
                        })
                } else {
                    Err(primary_err)
                }
            }
        }
    }

    /// Total cumulative cost across all calls.
    pub fn cumulative_cost(&self) -> f64 {
        self.total_cost
            .load(std::sync::atomic::Ordering::Relaxed) as f64
            / 1_000_000.0
    }

    /// Total number of API calls made.
    pub fn total_calls(&self) -> u64 {
        self.total_calls
            .load(std::sync::atomic::Ordering::Relaxed)
    }
}

// ---------------------------------------------------------------------------
// LlmEstimator implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl LlmEstimator for OpenRouterClient {
    async fn estimate_probability(
        &self,
        market: &Market,
        context: &DataContext,
    ) -> Result<Estimate> {
        let system = AnthropicClient::system_prompt();
        let user_msg = AnthropicClient::build_single_prompt(market, context);

        debug!(
            market_id = %market.id,
            model = %self.primary_model,
            "Requesting single probability estimate via OpenRouter"
        );

        let (response_text, tokens, cost) = self
            .call_api(system, &user_msg)
            .await
            .context("OpenRouter API call failed")?;

        let (probability, confidence, reasoning) = AnthropicClient::parse_estimate(&response_text)
            .context("Failed to parse estimate from LLM response")?;

        // Echo detection
        let echo_threshold = 0.02;
        if (probability - market.current_price_yes).abs() < echo_threshold {
            warn!(
                market_id = %market.id,
                estimate = probability,
                market_price = market.current_price_yes,
                "Possible echo: estimate very close to market price"
            );
        }

        info!(
            market_id = %market.id,
            probability = format!("{:.1}%", probability * 100.0),
            confidence = format!("{:.0}%", confidence * 100.0),
            tokens,
            cost = format!("${:.4}", cost),
            "Estimate complete (OpenRouter)"
        );

        Ok(Estimate {
            probability,
            confidence,
            reasoning,
            tokens_used: tokens,
            cost,
        })
    }

    async fn batch_estimate(
        &self,
        markets: &[(Market, DataContext)],
    ) -> Result<Vec<Estimate>> {
        if markets.is_empty() {
            return Ok(Vec::new());
        }

        // For small batches, individual calls are simpler
        if markets.len() <= 2 {
            let mut results = Vec::with_capacity(markets.len());
            for (market, context) in markets {
                results.push(self.estimate_probability(market, context).await?);
            }
            return Ok(results);
        }

        info!(count = markets.len(), "Starting batch estimation via OpenRouter");

        let system = AnthropicClient::system_prompt();
        let user_msg = AnthropicClient::build_batch_prompt(markets);

        let (response_text, tokens, cost) = self
            .call_api(system, &user_msg)
            .await
            .context("Batch estimation API call failed")?;

        let expected_ids: Vec<&str> = markets.iter().map(|(m, _)| m.id.as_str()).collect();
        let parsed = AnthropicClient::parse_batch_response(&response_text, &expected_ids);

        let cost_per_market = cost / markets.len() as f64;
        let tokens_per_market = tokens / markets.len() as u32;

        let mut results = Vec::with_capacity(markets.len());
        let mut fallback_count = 0u32;

        for (i, (market, context)) in markets.iter().enumerate() {
            match parsed.get(i).and_then(|p| p.as_ref()) {
                Some((prob, conf)) => {
                    results.push(Estimate {
                        probability: *prob,
                        confidence: *conf,
                        reasoning: format!("(batch estimate for {})", market.id),
                        tokens_used: tokens_per_market,
                        cost: cost_per_market,
                    });
                }
                None => {
                    debug!(
                        market_id = %market.id,
                        "Batch parse failed, falling back to individual call"
                    );
                    fallback_count += 1;
                    match self.estimate_probability(market, context).await {
                        Ok(est) => results.push(est),
                        Err(e) => {
                            warn!(
                                market_id = %market.id,
                                error = %e,
                                "Individual fallback also failed"
                            );
                            results.push(Estimate {
                                probability: market.current_price_yes,
                                confidence: 0.1,
                                reasoning: format!("Estimation failed: {e}"),
                                tokens_used: 0,
                                cost: 0.0,
                            });
                        }
                    }
                }
            }
        }

        if fallback_count > 0 {
            info!(
                fallback_count,
                total = markets.len(),
                "Batch estimation complete with fallbacks (OpenRouter)"
            );
        } else {
            info!(total = markets.len(), "Batch estimation complete (OpenRouter)");
        }

        Ok(results)
    }

    fn cost_per_call(&self) -> f64 {
        let (input_cost, output_cost) = model_costs(&self.primary_model);
        // Approximate: ~500 input tokens + ~300 output tokens
        (500.0 / 1000.0) * input_cost + (300.0 / 1000.0) * output_cost
    }

    fn model_name(&self) -> &str {
        &self.primary_model
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_construction_defaults() {
        let client = OpenRouterClient::new("test-key".into(), None, None, None).unwrap();
        assert_eq!(client.model_name(), DEFAULT_PRIMARY_MODEL);
        assert_eq!(
            client.fallback_model.as_deref(),
            Some(DEFAULT_FALLBACK_MODEL)
        );
        assert_eq!(client.max_tokens, DEFAULT_MAX_TOKENS);
        assert_eq!(client.cumulative_cost(), 0.0);
        assert_eq!(client.total_calls(), 0);
    }

    #[test]
    fn test_client_custom_models() {
        let client = OpenRouterClient::new(
            "test-key".into(),
            Some("anthropic/claude-opus-4".into()),
            Some("openai/gpt-4o".into()),
            Some(2048),
        )
        .unwrap();
        assert_eq!(client.model_name(), "anthropic/claude-opus-4");
        assert_eq!(client.fallback_model.as_deref(), Some("openai/gpt-4o"));
        assert_eq!(client.max_tokens, 2048);
    }

    #[test]
    fn test_cost_per_call_positive() {
        let client = OpenRouterClient::new("key".into(), None, None, None).unwrap();
        assert!(client.cost_per_call() > 0.0);
    }

    #[test]
    fn test_model_costs_claude_sonnet() {
        let (input, output) = model_costs("anthropic/claude-sonnet-4");
        assert!((input - 0.003).abs() < 1e-10);
        assert!((output - 0.015).abs() < 1e-10);
    }

    #[test]
    fn test_model_costs_grok() {
        let (input, output) = model_costs("x-ai/grok-4.1-fast");
        assert!((input - 0.003).abs() < 1e-10);
        assert!((output - 0.015).abs() < 1e-10);
    }

    #[test]
    fn test_model_costs_unknown_uses_default() {
        let (input, output) = model_costs("some-unknown/model-xyz");
        assert!(input > 0.0);
        assert!(output > 0.0);
    }

    #[test]
    fn test_model_costs_haiku() {
        let (input, output) = model_costs("anthropic/claude-haiku-4");
        assert!((input - 0.0008).abs() < 1e-10);
        assert!((output - 0.004).abs() < 1e-10);
    }

    #[test]
    fn test_model_costs_opus() {
        let (input, output) = model_costs("anthropic/claude-opus-4");
        assert!((input - 0.015).abs() < 1e-10);
        assert!((output - 0.075).abs() < 1e-10);
    }
}
