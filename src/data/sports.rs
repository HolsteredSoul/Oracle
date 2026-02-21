//! Sports data provider.
//!
//! Uses the free API-Football (via API-Sports) for fixtures, standings,
//! and team info. Falls back to a keyword-based summary when no API key
//! is configured.
//!
//! API: `https://v3.football.api-sports.io/` (and similar for other sports)
//! Auth: `x-apisports-key` header. Free tier: 100 req/day.
//!
//! For MVP, we extract team/league names from the question and provide
//! a structured summary. Full API integration will come when needed.

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use reqwest::Client;
use serde::Deserialize;
use tracing::debug;

use super::DataProvider;
use crate::types::{DataContext, Market, MarketCategory};

// ---------------------------------------------------------------------------
// Known leagues/sports for keyword extraction
// ---------------------------------------------------------------------------

struct SportKeyword {
    keywords: &'static [&'static str],
    sport: &'static str,
    league: &'static str,
}

const SPORT_KEYWORDS: &[SportKeyword] = &[
    SportKeyword { keywords: &["nba", "basketball"], sport: "basketball", league: "NBA" },
    SportKeyword { keywords: &["nfl", "super bowl", "football"], sport: "american_football", league: "NFL" },
    SportKeyword { keywords: &["mlb", "baseball", "world series"], sport: "baseball", league: "MLB" },
    SportKeyword { keywords: &["nhl", "hockey", "stanley cup"], sport: "hockey", league: "NHL" },
    SportKeyword { keywords: &["premier league", "epl", "soccer", "champions league"], sport: "football", league: "EPL/UCL" },
    SportKeyword { keywords: &["wimbledon", "tennis", "us open", "australian open", "french open"], sport: "tennis", league: "ATP/WTA" },
    SportKeyword { keywords: &["olympics", "olympic"], sport: "multi", league: "Olympics" },
    SportKeyword { keywords: &["f1 ", "formula 1", "formula one", "grand prix"], sport: "motorsport", league: "F1" },
    SportKeyword { keywords: &["ufc", "mma"], sport: "mma", league: "UFC" },
    SportKeyword { keywords: &["cricket", "ashes", "ipl"], sport: "cricket", league: "Cricket" },
    SportKeyword { keywords: &["afl", "aussie rules"], sport: "australian_football", league: "AFL" },
    SportKeyword { keywords: &["nrl", "rugby league"], sport: "rugby_league", league: "NRL" },
];

// ---------------------------------------------------------------------------
// API response types (API-Sports fixtures)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ApiSportsResponse {
    #[serde(default)]
    results: u32,
    #[serde(default)]
    response: Vec<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

pub struct SportsProvider {
    http: Client,
    api_key: Option<String>,
}

impl SportsProvider {
    pub fn new(api_key: Option<String>) -> Result<Self> {
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .user_agent("ORACLE/0.1.0")
            .build()
            .context("Failed to build sports HTTP client")?;
        Ok(Self { http, api_key })
    }

    /// Extract sport/league from market question.
    fn extract_sport(question: &str) -> Option<&'static SportKeyword> {
        let q = question.to_lowercase();
        SPORT_KEYWORDS.iter().find(|sk| {
            sk.keywords.iter().any(|kw| q.contains(kw))
        })
    }

    /// Extract team names from question using common patterns.
    fn extract_teams(question: &str) -> Vec<String> {
        // Common pattern: "Will the [Team] win..."
        let q = question.to_lowercase();
        let mut teams = Vec::new();

        // Look for "the [Proper Noun]" patterns in original question
        let words: Vec<&str> = question.split_whitespace().collect();
        let mut i = 0;
        while i < words.len() {
            if words[i].eq_ignore_ascii_case("the") && i + 1 < words.len() {
                // Collect capitalized words after "the"
                let mut team = Vec::new();
                for j in (i + 1)..words.len() {
                    let w = words[j].trim_matches(|c: char| !c.is_alphanumeric());
                    if w.is_empty() { break; }
                    if w.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
                        && !["Will", "Win", "The", "In", "And", "Or", "For", "Be"].contains(&w)
                    {
                        team.push(w.to_string());
                    } else {
                        break;
                    }
                }
                if !team.is_empty() {
                    teams.push(team.join(" "));
                }
            }
            i += 1;
        }

        // Also check for "vs" or "versus" patterns
        if let Some(vs_pos) = q.find(" vs ").or_else(|| q.find(" versus ")) {
            // There's a versus pattern — teams are on either side
            debug!(question = %question, "Found vs/versus pattern in sports question");
        }

        teams.truncate(4); // Limit
        teams
    }

    /// Build a keyword-based summary when no API key is available.
    fn keyword_summary(market: &Market) -> String {
        let sport = Self::extract_sport(&market.question);
        let teams = Self::extract_teams(&market.question);

        let mut parts = Vec::new();
        parts.push("Sports context (keyword-extracted):".to_string());

        if let Some(sk) = sport {
            parts.push(format!("Sport: {} ({})", sk.sport, sk.league));
        }

        if !teams.is_empty() {
            parts.push(format!("Teams mentioned: {}", teams.join(", ")));
        }

        if let Some(prob) = market.cross_refs.manifold_prob {
            parts.push(format!("Manifold market probability: {:.1}%", prob * 100.0));
        }
        if let Some(prob) = market.cross_refs.metaculus_prob {
            parts.push(format!(
                "Metaculus community forecast: {:.1}% ({} forecasters)",
                prob * 100.0,
                market.cross_refs.metaculus_forecasters.unwrap_or(0)
            ));
        }

        parts.push("Note: Full sports API data not available. Use general sports knowledge and cross-reference signals.".to_string());

        parts.join("\n")
    }
}

#[async_trait]
impl DataProvider for SportsProvider {
    fn category(&self) -> MarketCategory {
        MarketCategory::Sports
    }

    async fn fetch_context(&self, market: &Market) -> Result<DataContext> {
        // For MVP: keyword extraction summary. Full API integration later.
        // API-Sports free tier is only 100 req/day, too limited for scanning.
        let summary = Self::keyword_summary(market);

        Ok(DataContext {
            category: MarketCategory::Sports,
            raw_data: serde_json::Value::Null,
            summary,
            freshness: Utc::now(),
            source: "keyword-extraction".to_string(),
            cost: 0.0,
            metaculus_forecast: market.cross_refs.metaculus_prob,
            metaculus_forecasters: market.cross_refs.metaculus_forecasters,
            manifold_price: market.cross_refs.manifold_prob,
        })
    }

    fn cost_per_call(&self) -> f64 {
        0.0 // Keyword extraction is free; API calls would be ~$0 (free tier)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_sport_nba() {
        let sk = SportsProvider::extract_sport("Will the Thunder win the NBA finals?");
        assert!(sk.is_some());
        assert_eq!(sk.unwrap().league, "NBA");
    }

    #[test]
    fn test_extract_sport_olympics() {
        let sk = SportsProvider::extract_sport("Will any athlete win a gold medal at the Olympics?");
        assert!(sk.is_some());
        assert_eq!(sk.unwrap().league, "Olympics");
    }

    #[test]
    fn test_extract_sport_cricket() {
        let sk = SportsProvider::extract_sport("Will Australia win the Ashes in 2026?");
        assert!(sk.is_some());
        assert_eq!(sk.unwrap().league, "Cricket");
    }

    #[test]
    fn test_extract_sport_none() {
        let sk = SportsProvider::extract_sport("Will AGI be developed before 2030?");
        assert!(sk.is_none());
    }

    #[test]
    fn test_extract_teams() {
        let teams = SportsProvider::extract_teams("Will the Oklahoma City Thunder win the NBA Finals?");
        assert!(!teams.is_empty());
        assert!(teams[0].contains("Oklahoma") || teams[0].contains("Thunder"));
    }

    #[test]
    fn test_keyword_summary_includes_sport() {
        let market = Market {
            id: "test".into(), platform: "manifold".into(),
            question: "Will the Lakers win the NBA championship?".into(),
            description: String::new(), category: MarketCategory::Sports,
            current_price_yes: 0.3, current_price_no: 0.7,
            volume_24h: 100.0, liquidity: 500.0,
            deadline: Utc::now() + chrono::Duration::days(30),
            resolution_criteria: String::new(),
            url: "https://example.com".into(),
            cross_refs: crate::types::CrossReferences::default(),
        };
        let summary = SportsProvider::keyword_summary(&market);
        assert!(summary.contains("NBA"));
        assert!(summary.contains("basketball"));
    }

    #[test]
    fn test_provider_category() {
        let p = SportsProvider::new(None).unwrap();
        assert_eq!(p.category(), MarketCategory::Sports);
        assert_eq!(p.cost_per_call(), 0.0);
    }
}
