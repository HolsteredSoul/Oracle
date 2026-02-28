//! Weather data provider.
//!
//! Uses the free Open-Meteo API (no key required) for global weather
//! forecasts and current conditions. Extracts location from market
//! question text using keyword heuristics.
//!
//! API: `https://api.open-meteo.com/v1/forecast`
//! Auth: None required.
//! Rate limit: Generous (free tier).

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use reqwest::Client;
use rust_decimal::Decimal;
use serde::Deserialize;
use tracing::{debug, warn};

use super::DataProvider;
use crate::types::{DataContext, Market, MarketCategory};

// ---------------------------------------------------------------------------
// Known locations for keyword extraction
// ---------------------------------------------------------------------------

/// A city/region we can extract from market question text.
struct KnownLocation {
    keywords: &'static [&'static str],
    lat: f64,
    lon: f64,
    name: &'static str,
}

const LOCATIONS: &[KnownLocation] = &[
    KnownLocation { keywords: &["sydney", "nsw", "new south wales"], lat: -33.87, lon: 151.21, name: "Sydney, AU" },
    KnownLocation { keywords: &["melbourne", "victoria", "vic"], lat: -37.81, lon: 144.96, name: "Melbourne, AU" },
    KnownLocation { keywords: &["brisbane", "queensland", "qld"], lat: -27.47, lon: 153.03, name: "Brisbane, AU" },
    KnownLocation { keywords: &["perth", "western australia"], lat: -31.95, lon: 115.86, name: "Perth, AU" },
    KnownLocation { keywords: &["australia", "australian"], lat: -25.27, lon: 133.78, name: "Central Australia" },
    KnownLocation { keywords: &["new york", "nyc", "manhattan"], lat: 40.71, lon: -74.01, name: "New York, US" },
    KnownLocation { keywords: &["los angeles", "la ", "california", "socal"], lat: 34.05, lon: -118.24, name: "Los Angeles, US" },
    KnownLocation { keywords: &["chicago"], lat: 41.88, lon: -87.63, name: "Chicago, US" },
    KnownLocation { keywords: &["miami", "florida", "fl "], lat: 25.76, lon: -80.19, name: "Miami, US" },
    KnownLocation { keywords: &["houston", "texas", "tx "], lat: 29.76, lon: -95.37, name: "Houston, US" },
    KnownLocation { keywords: &["london", "uk ", "britain", "england"], lat: 51.51, lon: -0.13, name: "London, UK" },
    KnownLocation { keywords: &["tokyo", "japan"], lat: 35.68, lon: 139.69, name: "Tokyo, JP" },
    KnownLocation { keywords: &["washington", "dc ", "d.c."], lat: 38.91, lon: -77.04, name: "Washington DC, US" },
    // Fallback US-centric (most ForecastEx markets are US-focused)
    KnownLocation { keywords: &["united states", "us ", "u.s.", "american", "national"], lat: 39.83, lon: -98.58, name: "Central US" },
];

// ---------------------------------------------------------------------------
// Open-Meteo response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, serde::Serialize)]
struct OpenMeteoResponse {
    #[serde(default)]
    current: Option<OpenMeteoCurrent>,
    #[serde(default)]
    daily: Option<OpenMeteoDaily>,
}

#[derive(Debug, Deserialize, serde::Serialize)]
struct OpenMeteoCurrent {
    #[serde(default)]
    temperature_2m: Option<f64>,
    #[serde(default)]
    relative_humidity_2m: Option<f64>,
    #[serde(default)]
    precipitation: Option<f64>,
    #[serde(default)]
    wind_speed_10m: Option<f64>,
    #[serde(default)]
    weather_code: Option<i32>,
}

#[derive(Debug, Deserialize, serde::Serialize)]
struct OpenMeteoDaily {
    #[serde(default)]
    time: Vec<String>,
    #[serde(default)]
    temperature_2m_max: Vec<f64>,
    #[serde(default)]
    temperature_2m_min: Vec<f64>,
    #[serde(default)]
    precipitation_sum: Vec<f64>,
    #[serde(default)]
    precipitation_probability_max: Vec<f64>,
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

pub struct WeatherProvider {
    http: Client,
}

impl WeatherProvider {
    pub fn new() -> Result<Self> {
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .user_agent("ORACLE/0.1.0")
            .build()
            .context("Failed to build weather HTTP client")?;
        Ok(Self { http })
    }

    /// Extract the best-matching location from a market question.
    fn extract_location(question: &str) -> Option<&'static KnownLocation> {
        let q = question.to_lowercase();
        LOCATIONS.iter().find(|loc| {
            loc.keywords.iter().any(|kw| q.contains(kw))
        })
    }

    /// Build a human-readable summary from the API response.
    fn summarise(location: &str, resp: &OpenMeteoResponse) -> String {
        let mut parts = Vec::new();
        parts.push(format!("Weather data for {location}:"));

        if let Some(cur) = &resp.current {
            let mut current_parts = Vec::new();
            if let Some(t) = cur.temperature_2m {
                current_parts.push(format!("{t:.1}°C"));
            }
            if let Some(h) = cur.relative_humidity_2m {
                current_parts.push(format!("{h:.0}% humidity"));
            }
            if let Some(p) = cur.precipitation {
                current_parts.push(format!("{p:.1}mm precip"));
            }
            if let Some(w) = cur.wind_speed_10m {
                current_parts.push(format!("{w:.1}km/h wind"));
            }
            if !current_parts.is_empty() {
                parts.push(format!("Current: {}", current_parts.join(", ")));
            }
        }

        if let Some(daily) = &resp.daily {
            let days = daily.time.len().min(7);
            if days > 0 {
                parts.push(format!("{days}-day forecast:"));
                for i in 0..days {
                    let hi = daily.temperature_2m_max.get(i).copied().unwrap_or(0.0);
                    let lo = daily.temperature_2m_min.get(i).copied().unwrap_or(0.0);
                    let rain = daily.precipitation_sum.get(i).copied().unwrap_or(0.0);
                    let prob = daily.precipitation_probability_max.get(i).copied().unwrap_or(0.0);
                    let date = daily.time.get(i).map(|s| s.as_str()).unwrap_or("?");
                    parts.push(format!(
                        "  {date}: {lo:.0}–{hi:.0}°C, {rain:.1}mm rain ({prob:.0}% chance)"
                    ));
                }
            }
        }

        parts.join("\n")
    }
}

#[async_trait]
impl DataProvider for WeatherProvider {
    fn category(&self) -> MarketCategory {
        MarketCategory::Weather
    }

    async fn fetch_context(&self, market: &Market) -> Result<DataContext> {
        let location = Self::extract_location(&market.question);

        let (lat, lon, name) = match location {
            Some(loc) => (loc.lat, loc.lon, loc.name),
            None => {
                debug!(question = %market.question, "No location found in weather market, using Central US fallback");
                (39.83, -98.58, "Central US (fallback)")
            }
        };

        let url = format!(
            "https://api.open-meteo.com/v1/forecast?\
             latitude={lat}&longitude={lon}\
             &current=temperature_2m,relative_humidity_2m,precipitation,wind_speed_10m,weather_code\
             &daily=temperature_2m_max,temperature_2m_min,precipitation_sum,precipitation_probability_max\
             &forecast_days=7&timezone=auto"
        );

        let resp = self.http.get(&url).send().await
            .context("Open-Meteo request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            anyhow::bail!("Open-Meteo API error: {status}");
        }

        let data: OpenMeteoResponse = resp.json().await
            .context("Failed to parse Open-Meteo response")?;

        let summary = Self::summarise(name, &data);
        let raw = serde_json::to_value(&data).unwrap_or_default();

        Ok(DataContext {
            category: MarketCategory::Weather,
            raw_data: raw,
            summary,
            freshness: Utc::now(),
            source: format!("open-meteo ({name})"),
            cost: Decimal::ZERO, // Free API
            metaculus_forecast: market.cross_refs.metaculus_prob,
            metaculus_forecasters: market.cross_refs.metaculus_forecasters,
            manifold_price: market.cross_refs.manifold_prob,
        })
    }

    fn cost_per_call(&self) -> Decimal {
        Decimal::ZERO // Open-Meteo is free
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_location_sydney() {
        let loc = WeatherProvider::extract_location("Will Sydney get more than 100mm rainfall?");
        assert!(loc.is_some());
        assert_eq!(loc.unwrap().name, "Sydney, AU");
    }

    #[test]
    fn test_extract_location_california() {
        let loc = WeatherProvider::extract_location("California heat wave above 45C in 2026?");
        assert!(loc.is_some());
        assert!(loc.unwrap().name.contains("Los Angeles"));
    }

    #[test]
    fn test_extract_location_us_fallback() {
        let loc = WeatherProvider::extract_location("Will there be a major hurricane in the US this year?");
        assert!(loc.is_some());
        // "us " matches the US fallback
    }

    #[test]
    fn test_extract_location_none() {
        let loc = WeatherProvider::extract_location("Will it rain on Mars?");
        assert!(loc.is_none());
    }

    #[test]
    fn test_summarise_with_current() {
        let resp = OpenMeteoResponse {
            current: Some(OpenMeteoCurrent {
                temperature_2m: Some(25.3),
                relative_humidity_2m: Some(60.0),
                precipitation: Some(0.0),
                wind_speed_10m: Some(12.5),
                weather_code: Some(0),
            }),
            daily: None,
        };
        let summary = WeatherProvider::summarise("Sydney, AU", &resp);
        assert!(summary.contains("25.3°C"));
        assert!(summary.contains("60% humidity"));
        assert!(summary.contains("Sydney"));
    }

    #[test]
    fn test_summarise_with_daily() {
        let resp = OpenMeteoResponse {
            current: None,
            daily: Some(OpenMeteoDaily {
                time: vec!["2026-02-17".to_string(), "2026-02-18".to_string()],
                temperature_2m_max: vec![30.0, 28.0],
                temperature_2m_min: vec![20.0, 18.0],
                precipitation_sum: vec![5.0, 0.0],
                precipitation_probability_max: vec![80.0, 10.0],
            }),
        };
        let summary = WeatherProvider::summarise("Test", &resp);
        assert!(summary.contains("2-day forecast"));
        assert!(summary.contains("80% chance"));
    }

    #[test]
    fn test_provider_category() {
        let p = WeatherProvider::new().unwrap();
        assert_eq!(p.category(), MarketCategory::Weather);
        assert_eq!(p.cost_per_call(), Decimal::ZERO);
    }
}
