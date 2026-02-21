//! Anthropic Claude LLM integration.
//!
//! Implements the `LlmEstimator` trait using the Anthropic Messages API.
//! Handles prompt construction, response parsing, cost tracking,
//! rate limiting with exponential backoff, and batch estimation.

use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use super::LlmEstimator;
use crate::types::{DataContext, Estimate, Market};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const DEFAULT_MODEL: &str = "claude-sonnet-4-20250514";
const DEFAULT_MAX_TOKENS: u32 = 1024;

/// Maximum retries on rate limit / server errors.
const MAX_RETRIES: u32 = 3;

/// Base delay for exponential backoff (ms).
const BASE_BACKOFF_MS: u64 = 1000;

/// Approximate cost per 1K input tokens (Sonnet).
const INPUT_COST_PER_1K: f64 = 0.003;
/// Approximate cost per 1K output tokens (Sonnet).
const OUTPUT_COST_PER_1K: f64 = 0.015;

// ---------------------------------------------------------------------------
// API types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct MessagesRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Message {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct MessagesResponse {
    #[serde(default)]
    content: Vec<ContentBlock>,
    #[serde(default)]
    usage: Option<Usage>,
    #[serde(default)]
    stop_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    content_type: String,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Usage {
    #[serde(default)]
    input_tokens: u32,
    #[serde(default)]
    output_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    #[serde(default)]
    error: Option<ErrorBody>,
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    #[serde(default)]
    message: String,
    #[serde(rename = "type")]
    #[serde(default)]
    error_type: String,
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

pub struct AnthropicClient {
    http: Client,
    api_key: String,
    model: String,
    max_tokens: u32,
    total_cost: std::sync::atomic::AtomicU64, // stored as cost * 1_000_000
    total_calls: std::sync::atomic::AtomicU64,
}

impl AnthropicClient {
    pub fn new(api_key: String, model: Option<String>, max_tokens: Option<u32>) -> Result<Self> {
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .context("Failed to build Anthropic HTTP client")?;

        Ok(Self {
            http,
            api_key,
            model: model.unwrap_or_else(|| DEFAULT_MODEL.to_string()),
            max_tokens: max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
            total_cost: std::sync::atomic::AtomicU64::new(0),
            total_calls: std::sync::atomic::AtomicU64::new(0),
        })
    }

    /// Send a messages request with retry + backoff.
    async fn call_api(&self, system: &str, user_message: &str) -> Result<(String, u32, f64)> {
        let request = MessagesRequest {
            model: self.model.clone(),
            max_tokens: self.max_tokens,
            messages: vec![Message {
                role: "user".to_string(),
                content: user_message.to_string(),
            }],
            system: Some(system.to_string()),
        };

        let mut last_error = None;

        for attempt in 0..=MAX_RETRIES {
            if attempt > 0 {
                let delay = BASE_BACKOFF_MS * 2u64.pow(attempt - 1);
                debug!(attempt, delay_ms = delay, "Retrying Anthropic API call");
                tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
            }

            let resp = self.http
                .post(ANTHROPIC_API_URL)
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", ANTHROPIC_VERSION)
                .header("content-type", "application/json")
                .json(&request)
                .send()
                .await;

            match resp {
                Ok(response) => {
                    let status = response.status();

                    if status.is_success() {
                        let body: MessagesResponse = response.json().await
                            .context("Failed to parse Anthropic response")?;

                        let text = body.content.iter()
                            .filter_map(|b| b.text.as_deref())
                            .collect::<Vec<_>>()
                            .join("");

                        let usage = body.usage.unwrap_or(Usage {
                            input_tokens: 0,
                            output_tokens: 0,
                        });

                        let total_tokens = usage.input_tokens + usage.output_tokens;
                        let cost = (usage.input_tokens as f64 / 1000.0) * INPUT_COST_PER_1K
                            + (usage.output_tokens as f64 / 1000.0) * OUTPUT_COST_PER_1K;

                        // Track cumulative cost
                        let cost_micro = (cost * 1_000_000.0) as u64;
                        self.total_cost.fetch_add(cost_micro, std::sync::atomic::Ordering::Relaxed);
                        self.total_calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                        return Ok((text, total_tokens, cost));
                    }

                    // Retryable errors: 429 (rate limit), 500+, 529 (overloaded)
                    if status.as_u16() == 429 || status.as_u16() >= 500 {
                        let error_text = response.text().await.unwrap_or_default();
                        warn!(status = %status, attempt, error = %error_text, "Retryable Anthropic API error");
                        last_error = Some(format!("HTTP {status}: {error_text}"));
                        continue;
                    }

                    // Non-retryable error
                    let error_text = response.text().await.unwrap_or_default();
                    anyhow::bail!("Anthropic API error {status}: {error_text}");
                }
                Err(e) => {
                    warn!(attempt, error = %e, "Anthropic request failed");
                    last_error = Some(format!("Request error: {e}"));
                    continue;
                }
            }
        }

        anyhow::bail!(
            "Anthropic API failed after {} retries: {}",
            MAX_RETRIES,
            last_error.unwrap_or_default()
        )
    }

    /// Build the system prompt for probability estimation.
    pub fn system_prompt() -> &'static str {
        "You are a calibrated probability estimator for prediction markets. \
         Your job is to estimate the probability of stated outcomes as accurately \
         as possible, using all available data and reasoning.\n\n\
         CRITICAL RULES:\n\
         1. Think step-by-step about key factors before giving your estimate.\n\
         2. Account for base rates and reference classes.\n\
         3. Consider what information markets may have already priced in.\n\
         4. Identify what edge, if any, the real-time data provides over the market price.\n\
         5. Be genuinely calibrated: when you say 70%, events should happen ~70% of the time.\n\
         6. Avoid anchoring too heavily to the current market price.\n\
         7. Your final answer MUST be on the very last line in exactly this format:\n\
            PROBABILITY: 0.XX\n\
            CONFIDENCE: 0.XX\n\
         8. Probability must be between 0.01 and 0.99 (never 0 or 1).\n\
         9. Confidence reflects how certain you are in your estimate (0.1=very uncertain, 0.9=very confident)."
    }

    /// Build the user prompt for a single market.
    pub fn build_single_prompt(market: &Market, context: &DataContext) -> String {
        let mut prompt = String::with_capacity(2000);

        prompt.push_str(&format!("MARKET: \"{}\"\n", market.question));

        if !market.resolution_criteria.is_empty() {
            prompt.push_str(&format!("RESOLUTION: \"{}\"\n", market.resolution_criteria));
        }

        prompt.push_str(&format!("DEADLINE: {}\n", market.deadline.format("%Y-%m-%d %H:%M UTC")));
        prompt.push_str(&format!("CURRENT MARKET PRICE (YES): {:.1}%\n", market.current_price_yes * 100.0));
        prompt.push_str(&format!("PLATFORM: {}\n", market.platform));

        prompt.push_str("\nREAL-TIME DATA:\n");
        prompt.push_str(&context.summary);

        prompt.push_str("\n\nCROSS-REFERENCE SIGNALS:\n");
        if let Some(p) = context.metaculus_forecast {
            let n = context.metaculus_forecasters.unwrap_or(0);
            prompt.push_str(&format!("- Metaculus community forecast: {:.1}% ({n} forecasters)\n", p * 100.0));
        }
        if let Some(p) = context.manifold_price {
            prompt.push_str(&format!("- Manifold play-money price: {:.1}%\n", p * 100.0));
        }

        prompt.push_str("\nPlease reason step-by-step, then output your final estimate.\n");

        prompt
    }

    /// Build a batch prompt for multiple markets in the same category.
    pub fn build_batch_prompt(markets: &[(Market, DataContext)]) -> String {
        let mut prompt = String::with_capacity(4000);

        prompt.push_str(&format!(
            "Please estimate probabilities for the following {} markets.\n\
             For EACH market, reason briefly then output:\n\
             MARKET_ID: [id] | PROBABILITY: 0.XX | CONFIDENCE: 0.XX\n\n",
            markets.len()
        ));

        for (i, (market, context)) in markets.iter().enumerate() {
            prompt.push_str(&format!("--- MARKET {} (ID: {}) ---\n", i + 1, market.id));
            prompt.push_str(&format!("QUESTION: \"{}\"\n", market.question));
            prompt.push_str(&format!("DEADLINE: {}\n", market.deadline.format("%Y-%m-%d")));
            prompt.push_str(&format!("CURRENT PRICE: {:.1}%\n", market.current_price_yes * 100.0));
            prompt.push_str(&format!("DATA: {}\n", context.summary));

            if let Some(p) = context.metaculus_forecast {
                prompt.push_str(&format!("METACULUS: {:.1}%\n", p * 100.0));
            }
            if let Some(p) = context.manifold_price {
                prompt.push_str(&format!("MANIFOLD: {:.1}%\n", p * 100.0));
            }
            prompt.push('\n');
        }

        prompt
    }

    /// Parse probability and confidence from LLM response text.
    pub fn parse_estimate(text: &str) -> Result<(f64, f64, String)> {
        let lines: Vec<&str> = text.lines().collect();

        let mut probability: Option<f64> = None;
        let mut confidence: Option<f64> = None;

        // Scan from the end (most likely to find final answer there)
        for line in lines.iter().rev() {
            let line_upper = line.to_uppercase();

            if probability.is_none() {
                if let Some(p) = Self::extract_float_after(&line_upper, "PROBABILITY:") {
                    probability = Some(p);
                }
            }
            if confidence.is_none() {
                if let Some(c) = Self::extract_float_after(&line_upper, "CONFIDENCE:") {
                    confidence = Some(c);
                }
            }

            if probability.is_some() && confidence.is_some() {
                break;
            }
        }

        // Fallback: try to find any float on the last few lines
        if probability.is_none() {
            for line in lines.iter().rev().take(5) {
                if let Some(p) = Self::extract_any_float(line) {
                    if (0.0..=1.0).contains(&p) {
                        probability = Some(p);
                        break;
                    }
                }
            }
        }

        let prob = probability.ok_or_else(|| {
            anyhow::anyhow!("Could not parse probability from LLM response")
        })?;

        // Validate probability bounds
        let prob = prob.clamp(0.01, 0.99);
        let conf = confidence.unwrap_or(0.5).clamp(0.1, 0.99);

        // Extract reasoning (everything before the final PROBABILITY line)
        let reasoning = Self::extract_reasoning(text);

        Ok((prob, conf, reasoning))
    }

    /// Parse batch response into individual estimates.
    pub fn parse_batch_response(text: &str, expected_ids: &[&str]) -> Vec<Option<(f64, f64)>> {
        let mut results: Vec<Option<(f64, f64)>> = vec![None; expected_ids.len()];

        for line in text.lines() {
            let line_upper = line.to_uppercase();

            // Look for: MARKET_ID: xxx | PROBABILITY: 0.XX | CONFIDENCE: 0.XX
            if line_upper.contains("MARKET_ID:") && line_upper.contains("PROBABILITY:") {
                // Extract market ID
                if let Some(id_str) = Self::extract_string_after(&line_upper, "MARKET_ID:") {
                    let id_clean = id_str.trim().trim_matches('|').trim();

                    // Find which index this ID matches
                    if let Some(idx) = expected_ids.iter().position(|eid| {
                        id_clean.eq_ignore_ascii_case(eid)
                    }) {
                        let prob = Self::extract_float_after(&line_upper, "PROBABILITY:");
                        let conf = Self::extract_float_after(&line_upper, "CONFIDENCE:");

                        if let Some(p) = prob {
                            results[idx] = Some((
                                p.clamp(0.01, 0.99),
                                conf.unwrap_or(0.5).clamp(0.1, 0.99),
                            ));
                        }
                    }
                }
            }
        }

        results
    }

    /// Extract a float value after a label like "PROBABILITY:".
    fn extract_float_after(text: &str, label: &str) -> Option<f64> {
        let pos = text.find(label)?;
        let after = &text[pos + label.len()..];
        Self::extract_any_float(after)
    }

    /// Extract a string value after a label, up to the next pipe or end.
    fn extract_string_after(text: &str, label: &str) -> Option<String> {
        let pos = text.find(label)?;
        let after = &text[pos + label.len()..];
        let end = after.find('|').unwrap_or(after.len());
        Some(after[..end].trim().to_string())
    }

    /// Extract the first float-like value from text.
    fn extract_any_float(text: &str) -> Option<f64> {
        // Match patterns like 0.75, .75, 0.7, 75% (convert to 0.75)
        let mut chars = text.chars().peekable();
        while let Some(&c) = chars.peek() {
            if c.is_ascii_digit() || c == '.' {
                let mut num_str = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_ascii_digit() || c == '.' {
                        num_str.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }
                if let Ok(val) = num_str.parse::<f64>() {
                    // Check if followed by %
                    let is_percent = chars.peek() == Some(&'%');
                    if is_percent && val > 1.0 && val <= 100.0 {
                        return Some(val / 100.0);
                    }
                    if (0.0..=1.0).contains(&val) {
                        return Some(val);
                    }
                    // Maybe it's a percentage without % sign
                    if val > 1.0 && val <= 100.0 {
                        // Only treat as percentage if no decimal point suggests it's 0-1
                        if !num_str.contains('.') || val > 1.0 {
                            return Some(val / 100.0);
                        }
                    }
                }
            } else {
                chars.next();
            }
        }
        None
    }

    /// Extract the reasoning portion from the response (before PROBABILITY line).
    fn extract_reasoning(text: &str) -> String {
        let lines: Vec<&str> = text.lines().collect();
        let mut end_idx = lines.len();

        for (i, line) in lines.iter().enumerate().rev() {
            if line.to_uppercase().contains("PROBABILITY:") {
                end_idx = i;
                break;
            }
        }

        // Also strip CONFIDENCE line if right before PROBABILITY
        if end_idx > 0 && lines.get(end_idx.saturating_sub(1))
            .map(|l| l.to_uppercase().contains("CONFIDENCE:"))
            .unwrap_or(false)
        {
            end_idx = end_idx.saturating_sub(1);
        }

        let reasoning: String = lines[..end_idx].join("\n");
        // Truncate very long reasoning
        if reasoning.len() > 2000 {
            format!("{}...[truncated]", &reasoning[..2000])
        } else {
            reasoning
        }
    }
}

// ---------------------------------------------------------------------------
// LlmEstimator implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl LlmEstimator for AnthropicClient {
    async fn estimate_probability(
        &self,
        market: &Market,
        context: &DataContext,
    ) -> Result<Estimate> {
        let system = Self::system_prompt();
        let user_msg = Self::build_single_prompt(market, context);

        debug!(
            market_id = %market.id,
            model = %self.model,
            "Requesting single probability estimate"
        );

        let (response_text, tokens, cost) = self.call_api(system, &user_msg).await
            .context("Anthropic API call failed")?;

        let (probability, confidence, reasoning) = Self::parse_estimate(&response_text)
            .context("Failed to parse estimate from LLM response")?;

        // Echo detection: warn if estimate is suspiciously close to market price
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
            "Estimate complete"
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

        // For small batches, just do individual calls
        if markets.len() <= 2 {
            let mut results = Vec::with_capacity(markets.len());
            for (market, context) in markets {
                results.push(self.estimate_probability(market, context).await?);
            }
            return Ok(results);
        }

        info!(count = markets.len(), "Starting batch estimation");

        let system = Self::system_prompt();
        let user_msg = Self::build_batch_prompt(markets);

        let (response_text, tokens, cost) = self.call_api(system, &user_msg).await
            .context("Batch estimation API call failed")?;

        let expected_ids: Vec<&str> = markets.iter().map(|(m, _)| m.id.as_str()).collect();
        let parsed = Self::parse_batch_response(&response_text, &expected_ids);

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
                    // Batch parse failed for this market — fall back to individual
                    debug!(market_id = %market.id, "Batch parse failed, falling back to individual call");
                    fallback_count += 1;
                    match self.estimate_probability(market, context).await {
                        Ok(est) => results.push(est),
                        Err(e) => {
                            warn!(market_id = %market.id, error = %e, "Individual fallback also failed");
                            // Return a default low-confidence estimate
                            results.push(Estimate {
                                probability: market.current_price_yes, // echo market price
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
            info!(fallback_count, total = markets.len(), "Batch estimation complete with fallbacks");
        } else {
            info!(total = markets.len(), "Batch estimation complete");
        }

        Ok(results)
    }

    fn cost_per_call(&self) -> f64 {
        // Approximate cost for a typical single estimation
        // ~500 input tokens + ~300 output tokens
        (500.0 / 1000.0) * INPUT_COST_PER_1K + (300.0 / 1000.0) * OUTPUT_COST_PER_1K
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}

impl AnthropicClient {
    /// Total cumulative cost across all calls.
    pub fn cumulative_cost(&self) -> f64 {
        self.total_cost.load(std::sync::atomic::Ordering::Relaxed) as f64 / 1_000_000.0
    }

    /// Total number of API calls made.
    pub fn total_calls(&self) -> u64 {
        self.total_calls.load(std::sync::atomic::Ordering::Relaxed)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Prompt construction tests ---------------------------------------

    #[test]
    fn test_system_prompt_not_empty() {
        let sp = AnthropicClient::system_prompt();
        assert!(!sp.is_empty());
        assert!(sp.contains("calibrated"));
        assert!(sp.contains("PROBABILITY"));
        assert!(sp.contains("CONFIDENCE"));
    }

    #[test]
    fn test_build_single_prompt() {
        let market = Market {
            id: "test1".into(),
            platform: "manifold".into(),
            question: "Will it rain in Sydney tomorrow?".into(),
            description: String::new(),
            category: crate::types::MarketCategory::Weather,
            current_price_yes: 0.65,
            current_price_no: 0.35,
            volume_24h: 500.0,
            liquidity: 1000.0,
            deadline: chrono::Utc::now() + chrono::Duration::days(1),
            resolution_criteria: "Resolves YES if BOM records >0.2mm".into(),
            url: "https://example.com".into(),
            cross_refs: crate::types::CrossReferences {
                metaculus_prob: Some(0.70),
                metaculus_forecasters: Some(50),
                manifold_prob: Some(0.65),
                forecastex_price: None,
            },
        };

        let context = DataContext {
            category: crate::types::MarketCategory::Weather,
            raw_data: serde_json::Value::Null,
            summary: "Sydney: 25°C, 60% humidity, 80% rain chance".into(),
            freshness: chrono::Utc::now(),
            source: "open-meteo".into(),
            cost: 0.0,
            metaculus_forecast: Some(0.70),
            metaculus_forecasters: Some(50),
            manifold_price: Some(0.65),
        };

        let prompt = AnthropicClient::build_single_prompt(&market, &context);
        assert!(prompt.contains("rain in Sydney"));
        assert!(prompt.contains("65.0%")); // current price
        assert!(prompt.contains("BOM records")); // resolution criteria
        assert!(prompt.contains("Metaculus"));
        assert!(prompt.contains("70.0%")); // metaculus forecast
        assert!(prompt.contains("Manifold"));
        assert!(prompt.contains("80% rain chance")); // data summary
    }

    #[test]
    fn test_build_batch_prompt() {
        let m1 = (
            Market {
                id: "m1".into(), platform: "manifold".into(),
                question: "Q1?".into(), description: String::new(),
                category: crate::types::MarketCategory::Weather,
                current_price_yes: 0.5, current_price_no: 0.5,
                volume_24h: 0.0, liquidity: 0.0,
                deadline: chrono::Utc::now() + chrono::Duration::days(7),
                resolution_criteria: String::new(), url: String::new(),
                cross_refs: Default::default(),
            },
            DataContext::empty(crate::types::MarketCategory::Weather),
        );

        let batch = vec![m1];
        let prompt = AnthropicClient::build_batch_prompt(&batch);
        assert!(prompt.contains("MARKET 1"));
        assert!(prompt.contains("MARKET_ID:"));
        assert!(prompt.contains("Q1?"));
    }

    // -- Parse tests -----------------------------------------------------

    #[test]
    fn test_parse_estimate_standard() {
        let text = "Some reasoning here.\nFactors include X and Y.\n\nPROBABILITY: 0.72\nCONFIDENCE: 0.85";
        let (prob, conf, reasoning) = AnthropicClient::parse_estimate(text).unwrap();
        assert!((prob - 0.72).abs() < 1e-10);
        assert!((conf - 0.85).abs() < 1e-10);
        assert!(reasoning.contains("reasoning"));
    }

    #[test]
    fn test_parse_estimate_no_confidence() {
        let text = "Analysis.\n\nPROBABILITY: 0.55";
        let (prob, conf, _) = AnthropicClient::parse_estimate(text).unwrap();
        assert!((prob - 0.55).abs() < 1e-10);
        assert!((conf - 0.5).abs() < 1e-10); // default
    }

    #[test]
    fn test_parse_estimate_clamped() {
        let text = "PROBABILITY: 0.001\nCONFIDENCE: 0.999";
        let (prob, conf, _) = AnthropicClient::parse_estimate(text).unwrap();
        assert!((prob - 0.01).abs() < 1e-10); // clamped to 0.01
        assert!((conf - 0.99).abs() < 1e-10); // clamped to 0.99
    }

    #[test]
    fn test_parse_estimate_percentage() {
        let text = "I think it's about 72%.\n\nPROBABILITY: 72%";
        let (prob, _, _) = AnthropicClient::parse_estimate(text).unwrap();
        assert!((prob - 0.72).abs() < 1e-10);
    }

    #[test]
    fn test_parse_estimate_fallback_float() {
        // No PROBABILITY: label, but last line has a float
        let text = "After analysis, my estimate is:\n0.68";
        let (prob, _, _) = AnthropicClient::parse_estimate(text).unwrap();
        assert!((prob - 0.68).abs() < 1e-10);
    }

    #[test]
    fn test_parse_estimate_no_float_fails() {
        let text = "I cannot estimate this market.";
        assert!(AnthropicClient::parse_estimate(text).is_err());
    }

    #[test]
    fn test_parse_batch_response() {
        let text = "MARKET_ID: abc | PROBABILITY: 0.72 | CONFIDENCE: 0.80\n\
                    MARKET_ID: def | PROBABILITY: 0.45 | CONFIDENCE: 0.60";
        let results = AnthropicClient::parse_batch_response(text, &["abc", "def"]);
        assert_eq!(results.len(), 2);
        let (p1, c1) = results[0].unwrap();
        assert!((p1 - 0.72).abs() < 1e-10);
        assert!((c1 - 0.80).abs() < 1e-10);
        let (p2, c2) = results[1].unwrap();
        assert!((p2 - 0.45).abs() < 1e-10);
        assert!((c2 - 0.60).abs() < 1e-10);
    }

    #[test]
    fn test_parse_batch_response_partial() {
        let text = "MARKET_ID: abc | PROBABILITY: 0.72 | CONFIDENCE: 0.80\n\
                    Some other text without proper format";
        let results = AnthropicClient::parse_batch_response(text, &["abc", "def"]);
        assert!(results[0].is_some());
        assert!(results[1].is_none()); // def not found
    }

    // -- Float extraction tests ------------------------------------------

    #[test]
    fn test_extract_float_after() {
        assert_eq!(AnthropicClient::extract_float_after("PROBABILITY: 0.75", "PROBABILITY:"), Some(0.75));
        assert_eq!(AnthropicClient::extract_float_after("no match", "PROBABILITY:"), None);
    }

    #[test]
    fn test_extract_any_float() {
        assert_eq!(AnthropicClient::extract_any_float("the answer is 0.72"), Some(0.72));
        assert_eq!(AnthropicClient::extract_any_float("about 65%"), Some(0.65));
        assert_eq!(AnthropicClient::extract_any_float("no numbers"), None);
    }

    #[test]
    fn test_extract_any_float_edge_cases() {
        assert_eq!(AnthropicClient::extract_any_float("0.5"), Some(0.5));
        assert_eq!(AnthropicClient::extract_any_float("0.01"), Some(0.01));
        assert_eq!(AnthropicClient::extract_any_float("0.99"), Some(0.99));
    }

    // -- Reasoning extraction tests --------------------------------------

    #[test]
    fn test_extract_reasoning() {
        let text = "Step 1: Look at data.\nStep 2: Consider factors.\n\nPROBABILITY: 0.72";
        let reasoning = AnthropicClient::extract_reasoning(text);
        assert!(reasoning.contains("Step 1"));
        assert!(reasoning.contains("Step 2"));
        assert!(!reasoning.contains("PROBABILITY"));
    }

    // -- Client construction tests ---------------------------------------

    #[test]
    fn test_client_construction() {
        let client = AnthropicClient::new(
            "test-key".to_string(),
            None,
            None,
        ).unwrap();
        assert_eq!(client.model_name(), DEFAULT_MODEL);
        assert_eq!(client.cumulative_cost(), 0.0);
        assert_eq!(client.total_calls(), 0);
    }

    #[test]
    fn test_client_custom_model() {
        let client = AnthropicClient::new(
            "test-key".to_string(),
            Some("claude-opus-4-6".to_string()),
            Some(2048),
        ).unwrap();
        assert_eq!(client.model_name(), "claude-opus-4-6");
    }

    #[test]
    fn test_cost_per_call_positive() {
        let client = AnthropicClient::new("key".into(), None, None).unwrap();
        assert!(client.cost_per_call() > 0.0);
    }
}
