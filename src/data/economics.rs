//! Economics data provider.
//!
//! Fetches macro indicators from the FRED API (Federal Reserve Economic Data)
//! for US economic markets. FRED is the primary source since most ForecastEx
//! economics markets are US-centric.
//!
//! API: `https://api.stlouisfed.org/fred/series/observations`
//! Auth: API key via `api_key` query param. Free registration.
//! Rate limit: 120 req/min.
//!
//! Key series: CPIAUCSL (CPI), UNRATE (unemployment), GDP, FEDFUNDS,
//! T10YIE (breakeven inflation), DFF (effective fed funds rate).

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use reqwest::Client;
use serde::Deserialize;
use tracing::debug;

use super::DataProvider;
use crate::types::{DataContext, Market, MarketCategory};

// ---------------------------------------------------------------------------
// FRED series mapping
// ---------------------------------------------------------------------------

struct EconKeyword {
    keywords: &'static [&'static str],
    series_ids: &'static [&'static str],
    description: &'static str,
}

const ECON_KEYWORDS: &[EconKeyword] = &[
    EconKeyword {
        keywords: &["cpi", "inflation", "consumer price"],
        series_ids: &["CPIAUCSL", "T10YIE", "MICH"],
        description: "CPI / Inflation",
    },
    EconKeyword {
        keywords: &["unemployment", "jobs", "employment", "payroll", "nonfarm"],
        series_ids: &["UNRATE", "PAYEMS", "ICSA"],
        description: "Employment / Unemployment",
    },
    EconKeyword {
        keywords: &["gdp", "gross domestic", "economic growth"],
        series_ids: &["GDP", "GDPC1"],
        description: "GDP / Economic Growth",
    },
    EconKeyword {
        keywords: &["fed ", "federal reserve", "interest rate", "rate cut", "rate hike", "fomc"],
        series_ids: &["DFF", "FEDFUNDS", "T10Y2Y"],
        description: "Federal Reserve / Interest Rates",
    },
    EconKeyword {
        keywords: &["recession", "yield curve", "inversion"],
        series_ids: &["T10Y2Y", "SAHM", "RECPROUSM156N"],
        description: "Recession Indicators",
    },
    EconKeyword {
        keywords: &["s&p", "sp500", "stock market", "dow jones", "nasdaq"],
        series_ids: &["SP500", "VIXCLS"],
        description: "Stock Market",
    },
    EconKeyword {
        keywords: &["bitcoin", "crypto", "cryptocurrency"],
        series_ids: &["CBBTCUSD"],
        description: "Cryptocurrency",
    },
    EconKeyword {
        keywords: &["housing", "home price", "mortgage"],
        series_ids: &["CSUSHPINSA", "MORTGAGE30US"],
        description: "Housing Market",
    },
    EconKeyword {
        keywords: &["tariff", "trade war", "trade deficit", "import", "export"],
        series_ids: &["BOPGSTB"],
        description: "Trade / Tariffs",
    },
];

// ---------------------------------------------------------------------------
// FRED API response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct FredResponse {
    #[serde(default)]
    observations: Vec<FredObservation>,
}

#[derive(Debug, Deserialize)]
struct FredObservation {
    date: String,
    value: String,
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

pub struct EconomicsProvider {
    http: Client,
    fred_api_key: Option<String>,
}

impl EconomicsProvider {
    pub fn new(fred_api_key: Option<String>) -> Result<Self> {
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .user_agent("ORACLE/0.1.0")
            .build()
            .context("Failed to build economics HTTP client")?;
        Ok(Self { http, fred_api_key })
    }

    /// Match market question to relevant FRED series.
    fn match_series(question: &str) -> Vec<&'static EconKeyword> {
        let q = question.to_lowercase();
        ECON_KEYWORDS.iter()
            .filter(|ek| ek.keywords.iter().any(|kw| q.contains(kw)))
            .collect()
    }

    /// Fetch recent observations for a FRED series.
    async fn fetch_fred_series(
        &self,
        series_id: &str,
        api_key: &str,
    ) -> Result<Vec<FredObservation>> {
        let url = format!(
            "https://api.stlouisfed.org/fred/series/observations?\
             series_id={series_id}&api_key={api_key}\
             &file_type=json&sort_order=desc&limit=12"
        );

        let resp = self.http.get(&url).send().await
            .context(format!("FRED request failed for {series_id}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            anyhow::bail!("FRED API error for {series_id}: {status}");
        }

        let data: FredResponse = resp.json().await
            .context(format!("Failed to parse FRED response for {series_id}"))?;

        Ok(data.observations)
    }

    /// Build summary from FRED data.
    fn build_summary(
        matched: &[&EconKeyword],
        observations: &[(String, Vec<FredObservation>)],
        market: &Market,
    ) -> String {
        let mut parts = Vec::new();
        parts.push("Economics context (FRED data):".to_string());

        for (series_id, obs) in observations {
            if obs.is_empty() {
                continue;
            }
            // Find which keyword group this series belongs to
            let desc = matched.iter()
                .find(|ek| ek.series_ids.contains(&series_id.as_str()))
                .map(|ek| ek.description)
                .unwrap_or("Economic indicator");

            parts.push(format!("\n{desc} ({series_id}):"));

            // Show last few data points
            let show = obs.len().min(6);
            for o in &obs[..show] {
                if o.value != "." {
                    parts.push(format!("  {}: {}", o.date, o.value));
                }
            }
        }

        // Cross-references
        if let Some(prob) = market.cross_refs.manifold_prob {
            parts.push(format!("\nManifold market probability: {:.1}%", prob * 100.0));
        }
        if let Some(prob) = market.cross_refs.metaculus_prob {
            parts.push(format!(
                "Metaculus community forecast: {:.1}% ({} forecasters)",
                prob * 100.0,
                market.cross_refs.metaculus_forecasters.unwrap_or(0)
            ));
        }

        parts.join("\n")
    }

    /// Build a keyword-only summary when no FRED key is available.
    fn keyword_only_summary(matched: &[&EconKeyword], market: &Market) -> String {
        let mut parts = Vec::new();
        parts.push("Economics context (no FRED API key, keyword-only):".to_string());

        for ek in matched {
            parts.push(format!(
                "Relevant indicator: {} (FRED series: {})",
                ek.description,
                ek.series_ids.join(", ")
            ));
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

        parts.push("Note: Configure FRED_API_KEY for full economic data.".to_string());
        parts.join("\n")
    }
}

#[async_trait]
impl DataProvider for EconomicsProvider {
    fn category(&self) -> MarketCategory {
        MarketCategory::Economics
    }

    async fn fetch_context(&self, market: &Market) -> Result<DataContext> {
        let matched = Self::match_series(&market.question);

        if matched.is_empty() {
            debug!(question = %market.question, "No economic indicators matched");
            return Ok(DataContext::empty(MarketCategory::Economics));
        }

        let (summary, raw_data, cost) = match &self.fred_api_key {
            Some(key) => {
                // Fetch data for matched series (limit to first 3 to control costs)
                let mut observations = Vec::new();
                let series_to_fetch: Vec<&str> = matched.iter()
                    .flat_map(|ek| ek.series_ids.iter().copied())
                    .collect::<Vec<_>>();
                let series_to_fetch: Vec<&str> = series_to_fetch.into_iter()
                    .collect::<std::collections::HashSet<_>>()
                    .into_iter()
                    .take(4)
                    .collect();

                for series_id in &series_to_fetch {
                    match self.fetch_fred_series(series_id, key).await {
                        Ok(obs) => observations.push((series_id.to_string(), obs)),
                        Err(e) => debug!(series = series_id, error = %e, "FRED fetch failed"),
                    }
                }

                let raw = serde_json::to_value(&observations
                    .iter()
                    .map(|(id, obs)| {
                        (id.clone(), obs.iter().map(|o| format!("{}={}", o.date, o.value)).collect::<Vec<_>>())
                    })
                    .collect::<Vec<_>>())
                    .unwrap_or_default();

                let summary = Self::build_summary(&matched, &observations, market);
                (summary, raw, 0.0) // FRED is free
            }
            None => {
                let summary = Self::keyword_only_summary(&matched, market);
                (summary, serde_json::Value::Null, 0.0)
            }
        };

        Ok(DataContext {
            category: MarketCategory::Economics,
            raw_data,
            summary,
            freshness: Utc::now(),
            source: if self.fred_api_key.is_some() { "fred".to_string() } else { "keyword-extraction".to_string() },
            cost,
            metaculus_forecast: market.cross_refs.metaculus_prob,
            metaculus_forecasters: market.cross_refs.metaculus_forecasters,
            manifold_price: market.cross_refs.manifold_prob,
        })
    }

    fn cost_per_call(&self) -> f64 {
        0.0 // FRED is free
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_match_series_cpi() {
        let matched = EconomicsProvider::match_series("Will US CPI exceed 3% in Q2 2026?");
        assert!(!matched.is_empty());
        assert!(matched[0].series_ids.contains(&"CPIAUCSL"));
    }

    #[test]
    fn test_match_series_fed() {
        let matched = EconomicsProvider::match_series("Will the Fed cut interest rates in 2026?");
        assert!(!matched.is_empty());
        assert!(matched[0].description.contains("Federal Reserve"));
    }

    #[test]
    fn test_match_series_multiple() {
        let matched = EconomicsProvider::match_series(
            "Will inflation and unemployment both rise causing a recession?"
        );
        assert!(matched.len() >= 2, "Should match inflation + unemployment + recession");
    }

    #[test]
    fn test_match_series_none() {
        let matched = EconomicsProvider::match_series("Will it rain tomorrow?");
        assert!(matched.is_empty());
    }

    #[test]
    fn test_keyword_only_summary() {
        let matched = EconomicsProvider::match_series("Will US CPI exceed 3%?");
        let market = Market {
            id: "test".into(), platform: "manifold".into(),
            question: "Will US CPI exceed 3%?".into(),
            description: String::new(), category: MarketCategory::Economics,
            current_price_yes: 0.4, current_price_no: 0.6,
            volume_24h: 100.0, liquidity: 200.0,
            deadline: Utc::now() + chrono::Duration::days(30),
            resolution_criteria: String::new(),
            url: "https://example.com".into(),
            cross_refs: crate::types::CrossReferences::default(),
        };
        let summary = EconomicsProvider::keyword_only_summary(&matched, &market);
        assert!(summary.contains("CPIAUCSL"));
        assert!(summary.contains("FRED_API_KEY"));
    }

    #[test]
    fn test_provider_category() {
        let p = EconomicsProvider::new(None).unwrap();
        assert_eq!(p.category(), MarketCategory::Economics);
        assert_eq!(p.cost_per_call(), 0.0);
    }
}
