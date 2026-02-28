//! News and sentiment data provider.
//!
//! Provides news context for Politics, Culture, and Other market categories.
//! Uses keyword extraction from market questions to build context summaries.
//! Full NewsAPI integration available when API key is configured.
//!
//! API: `https://newsapi.org/v2/everything`
//! Auth: API key via `apiKey` query param. Free tier: 100 req/day.

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use reqwest::Client;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::Deserialize;
use tracing::debug;

use super::DataProvider;
use crate::types::{DataContext, Market, MarketCategory};

// ---------------------------------------------------------------------------
// Topic classification
// ---------------------------------------------------------------------------

struct NewsTopic {
    keywords: &'static [&'static str],
    label: &'static str,
    search_terms: &'static str,
}

const NEWS_TOPICS: &[NewsTopic] = &[
    NewsTopic {
        keywords: &["trump", "biden", "president", "white house", "congress", "senate"],
        label: "US Politics",
        search_terms: "US politics president",
    },
    NewsTopic {
        keywords: &["election", "vote", "ballot", "polling"],
        label: "Elections",
        search_terms: "election polling",
    },
    NewsTopic {
        keywords: &["war", "conflict", "ukraine", "russia", "nato", "military"],
        label: "Geopolitics / Conflict",
        search_terms: "geopolitics conflict",
    },
    NewsTopic {
        keywords: &["china", "taiwan", "xi jinping", "beijing"],
        label: "China / Asia",
        search_terms: "China geopolitics",
    },
    NewsTopic {
        keywords: &["ai ", "artificial intelligence", "openai", "google", "tech"],
        label: "AI / Technology",
        search_terms: "artificial intelligence technology",
    },
    NewsTopic {
        keywords: &["climate", "carbon", "emissions", "global warming", "paris agreement"],
        label: "Climate",
        search_terms: "climate change policy",
    },
    NewsTopic {
        keywords: &["pandemic", "covid", "virus", "who ", "health"],
        label: "Public Health",
        search_terms: "public health pandemic",
    },
    NewsTopic {
        keywords: &["oscar", "grammy", "emmy", "movie", "film", "album", "celebrity"],
        label: "Entertainment",
        search_terms: "entertainment awards",
    },
    NewsTopic {
        keywords: &["space", "nasa", "spacex", "mars", "moon", "launch"],
        label: "Space",
        search_terms: "space exploration NASA",
    },
];

// ---------------------------------------------------------------------------
// NewsAPI response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct NewsApiResponse {
    #[serde(default)]
    status: String,
    #[serde(default, rename = "totalResults")]
    total_results: u32,
    #[serde(default)]
    articles: Vec<NewsArticle>,
}

#[derive(Debug, Deserialize)]
struct NewsArticle {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    source: Option<NewsSource>,
    #[serde(default, rename = "publishedAt")]
    published_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NewsSource {
    #[serde(default)]
    name: Option<String>,
}

// ---------------------------------------------------------------------------
// Sentiment scoring
// ---------------------------------------------------------------------------

const POSITIVE_WORDS: &[&str] = &[
    "win", "success", "gain", "rise", "surge", "approve", "agree", "pass",
    "breakthrough", "progress", "strong", "boost", "improve", "record",
    "optimistic", "confident", "support", "growth",
];

const NEGATIVE_WORDS: &[&str] = &[
    "lose", "fail", "drop", "fall", "crash", "reject", "oppose", "block",
    "crisis", "collapse", "weak", "decline", "worst", "threat", "risk",
    "pessimistic", "concern", "fear", "scandal",
];

/// Simple keyword-based sentiment score: -1.0 (very negative) to +1.0 (very positive).
fn sentiment_score(text: &str) -> f64 {
    let lower = text.to_lowercase();
    let words: Vec<&str> = lower.split_whitespace().collect();
    let total = words.len() as f64;
    if total == 0.0 {
        return 0.0;
    }

    let pos = words.iter().filter(|w| {
        POSITIVE_WORDS.iter().any(|pw| w.contains(pw))
    }).count() as f64;

    let neg = words.iter().filter(|w| {
        NEGATIVE_WORDS.iter().any(|nw| w.contains(nw))
    }).count() as f64;

    let denom = pos + neg;
    if denom == 0.0 {
        return 0.0;
    }

    (pos - neg) / denom
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

pub struct NewsProvider {
    http: Client,
    api_key: Option<String>,
}

impl NewsProvider {
    pub fn new(api_key: Option<String>) -> Result<Self> {
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .user_agent("ORACLE/0.1.0")
            .build()
            .context("Failed to build news HTTP client")?;
        Ok(Self { http, api_key })
    }

    /// Match market question to news topics.
    fn match_topics(question: &str) -> Vec<&'static NewsTopic> {
        let q = question.to_lowercase();
        NEWS_TOPICS.iter()
            .filter(|t| t.keywords.iter().any(|kw| q.contains(kw)))
            .collect()
    }

    /// Extract key search terms from the question directly.
    fn extract_search_query(question: &str) -> String {
        // Take significant words from the question
        let stop_words = [
            "will", "the", "be", "in", "a", "an", "is", "it", "of", "to",
            "for", "and", "or", "by", "at", "on", "this", "that", "before",
            "after", "than", "more", "less", "above", "below", "between",
        ];
        let words: Vec<&str> = question
            .split(|c: char| !c.is_alphanumeric() && c != '\'')
            .filter(|w| w.len() > 2 && !stop_words.contains(&w.to_lowercase().as_str()))
            .take(5)
            .collect();
        words.join(" ")
    }

    /// Build summary from NewsAPI articles.
    fn build_news_summary(
        topics: &[&NewsTopic],
        articles: &[NewsArticle],
        market: &Market,
    ) -> String {
        let mut parts = Vec::new();
        parts.push("News context:".to_string());

        if !topics.is_empty() {
            let labels: Vec<&str> = topics.iter().map(|t| t.label).collect();
            parts.push(format!("Topics: {}", labels.join(", ")));
        }

        if !articles.is_empty() {
            parts.push(format!("\nRecent headlines ({} articles):", articles.len()));
            for (i, article) in articles.iter().take(5).enumerate() {
                let title = article.title.as_deref().unwrap_or("(no title)");
                let source = article.source.as_ref()
                    .and_then(|s| s.name.as_deref())
                    .unwrap_or("unknown");
                let date = article.published_at.as_deref().unwrap_or("");
                parts.push(format!("  {}. [{}] {} ({})", i + 1, source, title, date));
            }

            // Aggregate sentiment from headlines
            let headlines: String = articles.iter()
                .filter_map(|a| a.title.as_deref())
                .collect::<Vec<_>>()
                .join(" ");
            let sent = sentiment_score(&headlines);
            let sentiment_label = if sent > 0.3 { "positive" }
                else if sent < -0.3 { "negative" }
                else { "neutral/mixed" };
            parts.push(format!("\nHeadline sentiment: {} ({:.2})", sentiment_label, sent));
        }

        // Cross-references
        if let Some(prob) = market.cross_refs.manifold_prob {
            parts.push(format!("Manifold probability: {:.1}%", prob * dec!(100)));
        }
        if let Some(prob) = market.cross_refs.metaculus_prob {
            parts.push(format!(
                "Metaculus forecast: {:.1}% ({} forecasters)",
                prob * dec!(100),
                market.cross_refs.metaculus_forecasters.unwrap_or(0)
            ));
        }

        parts.join("\n")
    }

    /// Build keyword-only summary when no API key.
    fn keyword_only_summary(topics: &[&NewsTopic], market: &Market) -> String {
        let mut parts = Vec::new();
        parts.push("News context (no NewsAPI key, keyword-only):".to_string());

        if !topics.is_empty() {
            let labels: Vec<&str> = topics.iter().map(|t| t.label).collect();
            parts.push(format!("Topics: {}", labels.join(", ")));
            parts.push(format!("Suggested search: {}", topics[0].search_terms));
        }

        let sentiment = sentiment_score(&market.question);
        if sentiment.abs() > 0.1 {
            parts.push(format!(
                "Question framing sentiment: {:.2} ({})",
                sentiment,
                if sentiment > 0.0 { "leans positive" } else { "leans negative" }
            ));
        }

        if let Some(prob) = market.cross_refs.manifold_prob {
            parts.push(format!("Manifold probability: {:.1}%", prob * dec!(100)));
        }
        if let Some(prob) = market.cross_refs.metaculus_prob {
            parts.push(format!(
                "Metaculus forecast: {:.1}% ({} forecasters)",
                prob * dec!(100),
                market.cross_refs.metaculus_forecasters.unwrap_or(0)
            ));
        }

        parts.push("Note: Configure NEWS_API_KEY for full news data.".to_string());
        parts.join("\n")
    }
}

#[async_trait]
impl DataProvider for NewsProvider {
    fn category(&self) -> MarketCategory {
        // News covers Politics, Culture, and Other
        MarketCategory::Politics
    }

    async fn fetch_context(&self, market: &Market) -> Result<DataContext> {
        let topics = Self::match_topics(&market.question);

        let (summary, raw_data) = match &self.api_key {
            Some(key) => {
                let query = if !topics.is_empty() {
                    topics[0].search_terms.to_string()
                } else {
                    Self::extract_search_query(&market.question)
                };

                let url = format!(
                    "https://newsapi.org/v2/everything?\
                     q={}&sortBy=publishedAt&pageSize=10&language=en&apiKey={}",
                    urlencoding::encode(&query),
                    key
                );

                match self.http.get(&url).send().await {
                    Ok(resp) if resp.status().is_success() => {
                        match resp.json::<NewsApiResponse>().await {
                            Ok(data) => {
                                let raw = serde_json::to_value(&data.articles.iter().map(|a| {
                                    a.title.as_deref().unwrap_or("")
                                }).collect::<Vec<_>>()).unwrap_or_default();
                                let summary = Self::build_news_summary(&topics, &data.articles, market);
                                (summary, raw)
                            }
                            Err(e) => {
                                debug!(error = %e, "Failed to parse NewsAPI response");
                                (Self::keyword_only_summary(&topics, market), serde_json::Value::Null)
                            }
                        }
                    }
                    Ok(resp) => {
                        debug!(status = %resp.status(), "NewsAPI returned error");
                        (Self::keyword_only_summary(&topics, market), serde_json::Value::Null)
                    }
                    Err(e) => {
                        debug!(error = %e, "NewsAPI request failed");
                        (Self::keyword_only_summary(&topics, market), serde_json::Value::Null)
                    }
                }
            }
            None => {
                (Self::keyword_only_summary(&topics, market), serde_json::Value::Null)
            }
        };

        let effective_category = market.category.clone();

        Ok(DataContext {
            category: effective_category,
            raw_data,
            summary,
            freshness: Utc::now(),
            source: if self.api_key.is_some() { "newsapi".to_string() } else { "keyword-extraction".to_string() },
            cost: Decimal::ZERO,
            metaculus_forecast: market.cross_refs.metaculus_prob,
            metaculus_forecasters: market.cross_refs.metaculus_forecasters,
            manifold_price: market.cross_refs.manifold_prob,
        })
    }

    fn cost_per_call(&self) -> Decimal {
        Decimal::ZERO // NewsAPI free tier
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_match_topics_trump() {
        let topics = NewsProvider::match_topics("Will Trump win the 2028 election?");
        assert!(!topics.is_empty());
        assert!(topics.iter().any(|t| t.label == "US Politics"));
    }

    #[test]
    fn test_match_topics_ai() {
        let topics = NewsProvider::match_topics("Will OpenAI release GPT-5 before July 2026?");
        assert!(!topics.is_empty());
        assert!(topics.iter().any(|t| t.label == "AI / Technology"));
    }

    #[test]
    fn test_match_topics_multiple() {
        let topics = NewsProvider::match_topics("Will the election results lead to conflict?");
        assert!(topics.len() >= 2);
    }

    #[test]
    fn test_match_topics_none() {
        let topics = NewsProvider::match_topics("Will it rain on my birthday?");
        assert!(topics.is_empty());
    }

    #[test]
    fn test_sentiment_positive() {
        let s = sentiment_score("Team achieves breakthrough success with record gains");
        assert!(s > 0.0, "Score {s} should be positive");
    }

    #[test]
    fn test_sentiment_negative() {
        let s = sentiment_score("Markets crash in worst decline amid growing fear of crisis");
        assert!(s < 0.0, "Score {s} should be negative");
    }

    #[test]
    fn test_sentiment_neutral() {
        let s = sentiment_score("The committee met to discuss the upcoming schedule");
        assert!((s - 0.0).abs() < 1e-10, "Score {s} should be neutral");
    }

    #[test]
    fn test_sentiment_empty() {
        assert_eq!(sentiment_score(""), 0.0);
    }

    #[test]
    fn test_extract_search_query() {
        let q = NewsProvider::extract_search_query("Will Trump win the 2028 presidential election?");
        assert!(q.contains("Trump"));
        assert!(q.contains("presidential") || q.contains("election"));
        // Should NOT contain stop words
        assert!(!q.to_lowercase().split_whitespace().any(|w| w == "will" || w == "the"));
    }

    #[test]
    fn test_keyword_only_summary_with_topics() {
        use crate::types::d;
        let topics = NewsProvider::match_topics("Will Trump finish his second term?");
        let market = Market {
            id: "test".into(), platform: "manifold".into(),
            question: "Will Trump finish his second term?".into(),
            description: String::new(), category: MarketCategory::Politics,
            current_price_yes: d(0.75), current_price_no: d(0.25),
            volume_24h: d(100.0), liquidity: d(500.0),
            deadline: Utc::now() + chrono::Duration::days(30),
            resolution_criteria: String::new(),
            url: "https://example.com".into(),
            cross_refs: crate::types::CrossReferences::default(),
        };
        let summary = NewsProvider::keyword_only_summary(&topics, &market);
        assert!(summary.contains("US Politics"));
        assert!(summary.contains("NEWS_API_KEY"));
    }

    #[test]
    fn test_provider_category() {
        let p = NewsProvider::new(None).unwrap();
        assert_eq!(p.category(), MarketCategory::Politics);
        assert_eq!(p.cost_per_call(), Decimal::ZERO);
    }
}
