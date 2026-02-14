//! Mock platform for integration testing.
//!
//! Provides a deterministic `PredictionPlatform` implementation that
//! returns known markets, accepts bets, and tracks positions — all
//! in-memory with no external dependencies.

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::{Duration, Utc};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

use oracle::platforms::PredictionPlatform;
use oracle::types::*;

/// A mock prediction platform for deterministic testing.
///
/// All state is in-memory. Markets, positions, and balance are
/// fully controllable from test code.
pub struct MockPlatform {
    name: String,
    executable: bool,
    markets: Vec<Market>,
    balance: Arc<Mutex<f64>>,
    positions: Arc<Mutex<Vec<Position>>>,
    receipts: Arc<Mutex<Vec<TradeReceipt>>>,
    /// If set, all operations will return this error.
    force_error: Arc<Mutex<Option<String>>>,
}

impl MockPlatform {
    /// Create a new mock platform with default markets and balance.
    pub fn new(name: &str, executable: bool, initial_balance: f64) -> Self {
        Self {
            name: name.to_string(),
            executable,
            markets: Self::default_markets(),
            balance: Arc::new(Mutex::new(initial_balance)),
            positions: Arc::new(Mutex::new(Vec::new())),
            receipts: Arc::new(Mutex::new(Vec::new())),
            force_error: Arc::new(Mutex::new(None)),
        }
    }

    /// Create a mock with custom markets.
    pub fn with_markets(name: &str, executable: bool, balance: f64, markets: Vec<Market>) -> Self {
        Self {
            name: name.to_string(),
            executable,
            markets,
            balance: Arc::new(Mutex::new(balance)),
            positions: Arc::new(Mutex::new(Vec::new())),
            receipts: Arc::new(Mutex::new(Vec::new())),
            force_error: Arc::new(Mutex::new(None)),
        }
    }

    /// Force all subsequent operations to return an error.
    pub fn set_error(&self, msg: &str) {
        *self.force_error.lock().unwrap() = Some(msg.to_string());
    }

    /// Clear any forced error.
    pub fn clear_error(&self) {
        *self.force_error.lock().unwrap() = None;
    }

    /// Get all trade receipts recorded so far.
    pub fn get_receipts(&self) -> Vec<TradeReceipt> {
        self.receipts.lock().unwrap().clone()
    }

    /// A default set of markets spanning multiple categories,
    /// with known prices for deterministic edge detection testing.
    fn default_markets() -> Vec<Market> {
        let base_deadline = Utc::now() + Duration::days(14);

        vec![
            Market {
                id: "MOCK-WX-001".to_string(),
                platform: "mock".to_string(),
                question: "Will Sydney max temperature exceed 35°C on Feb 20?".to_string(),
                description: "Resolves YES if BOM reports >= 35°C".to_string(),
                category: MarketCategory::Weather,
                current_price_yes: 0.30,
                current_price_no: 0.70,
                volume_24h: 8000.0,
                liquidity: 15000.0,
                deadline: base_deadline,
                resolution_criteria: "BOM Observatory Hill station".to_string(),
                url: "https://mock.example.com/MOCK-WX-001".to_string(),
                cross_refs: CrossReferences {
                    metaculus_prob: Some(0.35),
                    metaculus_forecasters: Some(120),
                    manifold_prob: Some(0.32),
                    forecastex_price: Some(0.30),
                },
            },
            Market {
                id: "MOCK-SP-001".to_string(),
                platform: "mock".to_string(),
                question: "Will Team A win the Grand Final?".to_string(),
                description: "Resolves YES if Team A wins".to_string(),
                category: MarketCategory::Sports,
                current_price_yes: 0.55,
                current_price_no: 0.45,
                volume_24h: 25000.0,
                liquidity: 50000.0,
                deadline: base_deadline + Duration::days(7),
                resolution_criteria: "Official match result".to_string(),
                url: "https://mock.example.com/MOCK-SP-001".to_string(),
                cross_refs: CrossReferences {
                    metaculus_prob: None,
                    metaculus_forecasters: None,
                    manifold_prob: Some(0.58),
                    forecastex_price: Some(0.55),
                },
            },
            Market {
                id: "MOCK-EC-001".to_string(),
                platform: "mock".to_string(),
                question: "Will the RBA cut the cash rate in March 2026?".to_string(),
                description: "Resolves YES if RBA lowers the target cash rate".to_string(),
                category: MarketCategory::Economics,
                current_price_yes: 0.40,
                current_price_no: 0.60,
                volume_24h: 12000.0,
                liquidity: 30000.0,
                deadline: base_deadline + Duration::days(30),
                resolution_criteria: "RBA official announcement".to_string(),
                url: "https://mock.example.com/MOCK-EC-001".to_string(),
                cross_refs: CrossReferences {
                    metaculus_prob: Some(0.45),
                    metaculus_forecasters: Some(250),
                    manifold_prob: Some(0.42),
                    forecastex_price: Some(0.40),
                },
            },
            Market {
                id: "MOCK-PO-001".to_string(),
                platform: "mock".to_string(),
                question: "Will the PM announce an election before April 2026?".to_string(),
                description: "Resolves YES if an election date is formally announced".to_string(),
                category: MarketCategory::Politics,
                current_price_yes: 0.25,
                current_price_no: 0.75,
                volume_24h: 3000.0,
                liquidity: 8000.0,
                deadline: base_deadline + Duration::days(60),
                resolution_criteria: "Official government announcement".to_string(),
                url: "https://mock.example.com/MOCK-PO-001".to_string(),
                cross_refs: CrossReferences {
                    metaculus_prob: Some(0.30),
                    metaculus_forecasters: Some(85),
                    manifold_prob: Some(0.28),
                    forecastex_price: Some(0.25),
                },
            },
            // A low-liquidity market that should be filtered out
            Market {
                id: "MOCK-OT-001".to_string(),
                platform: "mock".to_string(),
                question: "Will a specific cultural event happen?".to_string(),
                description: "Low liquidity test market".to_string(),
                category: MarketCategory::Culture,
                current_price_yes: 0.50,
                current_price_no: 0.50,
                volume_24h: 100.0,
                liquidity: 200.0,
                deadline: base_deadline,
                resolution_criteria: "Official source".to_string(),
                url: "https://mock.example.com/MOCK-OT-001".to_string(),
                cross_refs: CrossReferences::default(),
            },
        ]
    }
}

#[async_trait]
impl PredictionPlatform for MockPlatform {
    async fn fetch_markets(&self) -> Result<Vec<Market>> {
        if let Some(err) = self.force_error.lock().unwrap().as_ref() {
            return Err(anyhow!("{}", err));
        }
        Ok(self.markets.clone())
    }

    async fn place_bet(
        &self,
        market_id: &str,
        side: Side,
        amount: f64,
    ) -> Result<TradeReceipt> {
        if let Some(err) = self.force_error.lock().unwrap().as_ref() {
            return Err(anyhow!("{}", err));
        }

        if !self.executable {
            return Err(anyhow!("Platform '{}' is read-only", self.name));
        }

        // Find the market
        let market = self
            .markets
            .iter()
            .find(|m| m.id == market_id)
            .ok_or_else(|| anyhow!("Market not found: {market_id}"))?;

        let fill_price = match side {
            Side::Yes => market.current_price_yes,
            Side::No => market.current_price_no,
        };

        // Check balance
        let mut balance = self.balance.lock().unwrap();
        let fees = 0.25; // Mock fixed fee
        let total_cost = amount + fees;
        if *balance < total_cost {
            return Err(anyhow!(
                "Insufficient balance: need ${total_cost:.2}, have ${:.2}",
                *balance
            ));
        }
        *balance -= total_cost;

        let receipt = TradeReceipt {
            order_id: format!("MOCK-{}", Uuid::new_v4()),
            market_id: market_id.to_string(),
            platform: self.name.clone(),
            side,
            amount,
            fill_price,
            fees,
            timestamp: Utc::now(),
        };

        // Track position
        let mut positions = self.positions.lock().unwrap();
        positions.push(Position {
            market_id: market_id.to_string(),
            platform: self.name.clone(),
            side,
            size: amount / fill_price,
            entry_price: fill_price,
            current_value: amount,
        });

        self.receipts.lock().unwrap().push(receipt.clone());

        Ok(receipt)
    }

    async fn get_positions(&self) -> Result<Vec<Position>> {
        if let Some(err) = self.force_error.lock().unwrap().as_ref() {
            return Err(anyhow!("{}", err));
        }
        Ok(self.positions.lock().unwrap().clone())
    }

    async fn get_balance(&self) -> Result<f64> {
        if let Some(err) = self.force_error.lock().unwrap().as_ref() {
            return Err(anyhow!("{}", err));
        }
        Ok(*self.balance.lock().unwrap())
    }

    async fn check_liquidity(&self, market_id: &str) -> Result<LiquidityInfo> {
        if let Some(err) = self.force_error.lock().unwrap().as_ref() {
            return Err(anyhow!("{}", err));
        }
        let market = self
            .markets
            .iter()
            .find(|m| m.id == market_id)
            .ok_or_else(|| anyhow!("Market not found: {market_id}"))?;

        Ok(LiquidityInfo {
            bid_depth: market.liquidity * 0.5,
            ask_depth: market.liquidity * 0.5,
            volume_24h: market.volume_24h,
        })
    }

    fn is_executable(&self) -> bool {
        self.executable
    }

    fn name(&self) -> &str {
        &self.name
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_fetch_markets() {
        let platform = MockPlatform::new("test-exchange", true, 100.0);
        let markets = platform.fetch_markets().await.unwrap();
        assert_eq!(markets.len(), 5);
        assert!(markets.iter().any(|m| m.category == MarketCategory::Weather));
        assert!(markets.iter().any(|m| m.category == MarketCategory::Sports));
        assert!(markets.iter().any(|m| m.category == MarketCategory::Economics));
        assert!(markets.iter().any(|m| m.category == MarketCategory::Politics));
    }

    #[tokio::test]
    async fn test_mock_place_bet_success() {
        let platform = MockPlatform::new("test-exchange", true, 100.0);
        let receipt = platform
            .place_bet("MOCK-WX-001", Side::Yes, 5.0)
            .await
            .unwrap();

        assert_eq!(receipt.market_id, "MOCK-WX-001");
        assert_eq!(receipt.side, Side::Yes);
        assert_eq!(receipt.amount, 5.0);
        assert!((receipt.fill_price - 0.30).abs() < 1e-10);
        assert!((receipt.fees - 0.25).abs() < 1e-10);

        // Balance should be reduced
        let balance = platform.get_balance().await.unwrap();
        assert!((balance - 94.75).abs() < 1e-10); // 100 - 5.0 - 0.25

        // Position should exist
        let positions = platform.get_positions().await.unwrap();
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].side, Side::Yes);
    }

    #[tokio::test]
    async fn test_mock_place_bet_insufficient_balance() {
        let platform = MockPlatform::new("test-exchange", true, 3.0);
        let result = platform.place_bet("MOCK-WX-001", Side::Yes, 5.0).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Insufficient"));
    }

    #[tokio::test]
    async fn test_mock_place_bet_read_only() {
        let platform = MockPlatform::new("metaculus-mock", false, 0.0);
        let result = platform.place_bet("MOCK-WX-001", Side::Yes, 5.0).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("read-only"));
    }

    #[tokio::test]
    async fn test_mock_place_bet_market_not_found() {
        let platform = MockPlatform::new("test-exchange", true, 100.0);
        let result = platform
            .place_bet("NONEXISTENT", Side::Yes, 5.0)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_mock_check_liquidity() {
        let platform = MockPlatform::new("test-exchange", true, 100.0);
        let liq = platform.check_liquidity("MOCK-SP-001").await.unwrap();
        assert!(liq.total_depth() > 0.0);
        assert_eq!(liq.volume_24h, 25000.0);
    }

    #[tokio::test]
    async fn test_mock_forced_error() {
        let platform = MockPlatform::new("test-exchange", true, 100.0);
        platform.set_error("simulated IB disconnect");

        assert!(platform.fetch_markets().await.is_err());
        assert!(platform.get_balance().await.is_err());
        assert!(platform.place_bet("MOCK-WX-001", Side::Yes, 1.0).await.is_err());

        platform.clear_error();
        assert!(platform.fetch_markets().await.is_ok());
    }

    #[tokio::test]
    async fn test_mock_multiple_bets_track_receipts() {
        let platform = MockPlatform::new("test-exchange", true, 100.0);

        platform.place_bet("MOCK-WX-001", Side::Yes, 5.0).await.unwrap();
        platform.place_bet("MOCK-SP-001", Side::No, 3.0).await.unwrap();
        platform.place_bet("MOCK-EC-001", Side::Yes, 2.0).await.unwrap();

        let receipts = platform.get_receipts();
        assert_eq!(receipts.len(), 3);

        let positions = platform.get_positions().await.unwrap();
        assert_eq!(positions.len(), 3);

        // Balance: 100 - (5+0.25) - (3+0.25) - (2+0.25) = 89.25
        let balance = platform.get_balance().await.unwrap();
        assert!((balance - 89.25).abs() < 1e-10);
    }

    #[tokio::test]
    async fn test_mock_is_executable() {
        let exec = MockPlatform::new("exchange", true, 100.0);
        let read_only = MockPlatform::new("metaculus", false, 0.0);
        assert!(exec.is_executable());
        assert!(!read_only.is_executable());
    }

    #[tokio::test]
    async fn test_mock_custom_markets() {
        let custom = vec![Market {
            id: "CUSTOM-001".to_string(),
            platform: "custom".to_string(),
            question: "Custom test market".to_string(),
            description: "".to_string(),
            category: MarketCategory::Other,
            current_price_yes: 0.50,
            current_price_no: 0.50,
            volume_24h: 1000.0,
            liquidity: 5000.0,
            deadline: Utc::now() + Duration::days(7),
            resolution_criteria: "".to_string(),
            url: "".to_string(),
            cross_refs: CrossReferences::default(),
        }];

        let platform = MockPlatform::with_markets("custom", true, 50.0, custom);
        let markets = platform.fetch_markets().await.unwrap();
        assert_eq!(markets.len(), 1);
        assert_eq!(markets[0].id, "CUSTOM-001");
    }
}
