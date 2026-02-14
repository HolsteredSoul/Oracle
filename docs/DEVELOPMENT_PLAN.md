# ORACLE: Iterative Development Plan

## Version 1.2 — Build Roadmap

---

## Overview

This document defines the phased development plan for ORACLE. Each phase produces a working, testable artifact. Phases are designed so that earlier phases are prerequisites for later ones, and the agent becomes progressively more capable with each iteration.

**Total estimated effort**: 6-8 phases, ~2-4 weeks for a working MVP.

**Language**: Rust (stable toolchain, 2021 edition)
**Async runtime**: Tokio
**Key crates**: reqwest, serde, axum, sqlx, chrono, plotters, tracing

**Target platforms** (AU-compliant, confirmed February 2026):
- **IB ForecastEx** — real-money execution via Interactive Brokers TWS API. As of February 2026, this remains the sole fully legal, real-money prediction market platform accessible to Australian residents. No additional real-money execution platforms are planned unless the regulatory landscape changes.
- **Metaculus** — crowd forecast cross-reference (read-only)
- **Manifold** — play-money validation and sentiment signal

---

## Phase 0: Project Scaffolding

**Goal**: Compilable Rust project with modular structure, config loading, and logging.

**Duration**: Day 1

### Tasks

- [x] Initialize Cargo workspace
- [x] Define module structure (see below)
- [x] Implement config loading from TOML (`config.toml`)
- [x] Set up structured logging with `tracing` + `tracing-subscriber`
- [x] Implement graceful shutdown (Ctrl+C handler)
- [x] Create `.env` template for secrets
- [x] Create `Dockerfile` for deployment
- [x] Write `README.md` with setup instructions

### Module Structure

```
oracle/
├── Cargo.toml
├── config.toml
├── .env.example
├── Dockerfile
├── README.md
├── src/
│   ├── main.rs                 # Entry point, async main loop
│   ├── config.rs               # TOML config + env var resolution
│   ├── types.rs                # Shared types (Market, Side, Trade, etc.)
│   ├── platforms/
│   │   ├── mod.rs              # PredictionPlatform trait
│   │   ├── forecastex.rs       # IB ForecastEx implementation (TWS API)
│   │   ├── metaculus.rs        # Metaculus read-only implementation
│   │   └── manifold.rs         # Manifold play-money implementation
│   ├── data/
│   │   ├── mod.rs              # DataProvider trait
│   │   ├── weather.rs          # BOM / OpenWeatherMap / NOAA
│   │   ├── sports.rs           # API-Sports
│   │   ├── economics.rs        # FRED / RBA / ABS
│   │   └── news.rs             # NewsAPI / RSS
│   ├── llm/
│   │   ├── mod.rs              # LLM trait + prompt builder
│   │   ├── anthropic.rs        # Claude integration
│   │   ├── openai.rs           # GPT-4 integration
│   │   └── grok.rs             # Grok integration
│   ├── strategy/
│   │   ├── mod.rs              # Strategy orchestrator
│   │   ├── edge.rs             # Mispricing detection
│   │   ├── kelly.rs            # Kelly criterion sizing
│   │   └── risk.rs             # Risk manager (limits, drawdown, correlation)
│   ├── engine/
│   │   ├── mod.rs              # Main scan-estimate-bet loop
│   │   ├── scanner.rs          # Multi-platform market scanner
│   │   ├── enricher.rs         # Data enrichment pipeline
│   │   ├── executor.rs         # Trade execution with retries
│   │   └── accountant.rs       # Cost tracking + survival check
│   ├── storage/
│   │   ├── mod.rs              # SQLite persistence
│   │   └── schema.sql          # Database schema
│   └── dashboard/
│       ├── mod.rs              # Axum web server
│       ├── routes.rs           # API endpoints
│       └── templates/          # HTML templates (or JSON API only)
└── tests/
    ├── integration/
    │   ├── mock_platform.rs    # Mock platform for testing
    │   └── simulation.rs       # 48-hour simulation harness
    └── unit/
        ├── kelly_tests.rs
        └── edge_tests.rs
```

### Cargo.toml (Initial)

```toml
[package]
name = "oracle"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.12", features = ["json"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
chrono = { version = "0.4", features = ["serde"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite"] }
axum = "0.7"
tower-http = { version = "0.5", features = ["cors"] }
dotenv = "0.15"
anyhow = "1"
thiserror = "1"
async-trait = "0.1"
uuid = { version = "1", features = ["v4"] }

[dev-dependencies]
tokio-test = "0.4"
mockall = "0.13"
```

**Note**: No `ethers` crate needed — we're not interacting with blockchain. IB TWS API is TCP socket-based, handled via `ibapi` crate or raw TCP with custom protocol implementation.

### Deliverables

- `cargo build` succeeds
- `cargo run` starts, loads config, prints banner, and enters idle loop
- Structured JSON logs to stdout

---

## Phase 1: Core Types and Platform Trait

**Goal**: Define the shared data model and platform abstraction so all subsequent modules have a stable interface.

**Duration**: Day 1-2

### Tasks

- [x] Define `Market` struct (id, question, platform, category, current_price, volume, deadline, etc.)
- [x] Define `Side` enum (Yes, No)
- [x] Define `TradeReceipt` struct (order_id, amount, price, fees, timestamp)
- [x] Define `Position` struct (market_id, side, size, entry_price, current_value)
- [x] Define `LiquidityInfo` struct (bid_depth, ask_depth, volume_24h)
- [x] Define `MarketCategory` enum (Weather, Sports, Economics, Politics, Culture, Other)
- [x] Define `PredictionPlatform` trait (see whitepaper §3.3)
- [x] Define `DataProvider` trait (see whitepaper §4.2)
- [x] Define `LlmEstimator` trait
- [x] Implement `Display` / `Debug` for all types
- [x] Write unit tests for type serialization/deserialization

### Key Type Definitions

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Market {
    pub id: String,
    pub platform: String,         // "forecastex" | "metaculus" | "manifold"
    pub question: String,
    pub description: String,
    pub category: MarketCategory,
    pub current_price_yes: f64,   // 0.0 - 1.0
    pub current_price_no: f64,    // 0.0 - 1.0
    pub volume_24h: f64,          // USD equivalent
    pub liquidity: f64,           // USD equivalent
    pub deadline: DateTime<Utc>,
    pub resolution_criteria: String,
    pub url: String,
    pub cross_refs: CrossReferences,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CrossReferences {
    pub metaculus_prob: Option<f64>,
    pub metaculus_forecasters: Option<u32>,
    pub manifold_prob: Option<f64>,
    pub forecastex_price: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetDecision {
    pub market: Market,
    pub side: Side,
    pub fair_value: f64,          // LLM estimate
    pub edge: f64,                // |fair_value - market_price|
    pub kelly_fraction: f64,      // Raw Kelly
    pub bet_amount: f64,          // After caps and risk limits
    pub confidence: f64,          // LLM self-reported confidence
    pub rationale: String,        // LLM reasoning summary
    pub data_sources_used: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentState {
    pub bankroll: f64,
    pub total_pnl: f64,
    pub cycle_count: u64,
    pub trades_placed: u64,
    pub trades_won: u64,
    pub trades_lost: u64,
    pub total_api_costs: f64,
    pub total_ib_commissions: f64,
    pub start_time: DateTime<Utc>,
    pub peak_bankroll: f64,
    pub status: AgentStatus,      // Alive, Died, Paused
}
```

### Deliverables

- All types compile and serialize to/from JSON
- Trait definitions are stable
- Mock implementations pass basic tests

---

## Phase 2: Platform Integrations (Scanning)

**Goal**: Fetch live markets from ForecastEx, Metaculus, and Manifold. No betting yet — read-only.

**Duration**: Day 2-4

**Platform exclusivity note (2026)**: IB ForecastEx is confirmed as the sole real-money execution platform accessible from Australia. The integrations below reflect this: ForecastEx is the primary scanner and execution target, while Metaculus and Manifold serve exclusively as read-only cross-reference and validation sources. No additional real-money platform integrations are planned or needed under current AU regulations. The `PredictionPlatform` trait abstraction is retained to allow future expansion if the regulatory landscape changes.

### Tasks

#### 2A: IB ForecastEx Scanner
- [ ] Implement IB TWS API connection (TCP socket to IB Gateway)
- [ ] Authenticate with client ID and account
- [ ] Request contract details for ForecastEx event contracts
- [ ] Fetch market data (bid/ask/last/volume) for active contracts
- [ ] Parse into `Market` struct
- [ ] Handle connection drops and reconnection
- [ ] Support both paper (port 4002) and live (port 4001) modes

**IB TWS API specifics:**
- ForecastEx contracts use `secType = "FUT"` or custom IB contract type
- Use `reqContractDetails` to discover available markets
- Use `reqMktData` for real-time prices
- Use `reqHistoricalData` for volume/liquidity assessment

**Reliability priority**: Since ForecastEx is the sole execution venue, the IB connection must be treated as mission-critical infrastructure. Implement robust reconnection logic, connection health monitoring, and clear alerting when the IB link is degraded or lost.

#### 2B: Metaculus Scanner
- [x] Implement REST API client (`https://www.metaculus.com/api2/`)
- [x] Fetch active questions with community forecasts
- [x] Parse community median/mean probability
- [x] Map to `Market` struct (with `platform = "metaculus"`)
- [ ] Implement matching logic: find Metaculus questions similar to ForecastEx markets (fuzzy text matching) *(deferred to 2D: Market Router)*

#### 2C: Manifold Scanner
- [x] Implement REST API client (`https://api.manifold.markets/v0/`)
- [x] Fetch active binary markets with play-money probabilities
- [x] Parse into `Market` struct (with `platform = "manifold"`)
- [x] Filter for markets matching ForecastEx categories
- [x] Track Mana prices as sentiment signals

#### 2D: Market Router
- [ ] Aggregate markets from all enabled platforms
- [ ] Match cross-platform markets (same underlying event) via fuzzy text matching
- [ ] Attach Metaculus forecasts and Manifold prices as `CrossReferences`
- [ ] Sort by category, volume, deadline
- [ ] Filter out markets below liquidity thresholds

### Testing

- [ ] Run scanner against live APIs (IB paper account for ForecastEx)
- [ ] Verify market count, data completeness
- [ ] Log sample output for manual inspection
- [ ] Write integration tests with recorded API responses

### Deliverables

- `cargo run -- --scan-only` fetches and displays markets from all platforms
- Markets are correctly categorized and cross-referenced
- Liquidity data is populated

---

## Phase 3: Data Enrichment Pipeline

**Goal**: For each candidate market, fetch domain-specific real-time data to inform LLM estimates.

**Duration**: Day 4-6

**Cost efficiency note**: Given ForecastEx's limited market catalog (~50-200 active markets), maximizing the informational edge extracted from each market is critical. Aggressive caching and data reuse across markets sharing the same category or underlying data (e.g., multiple weather markets using the same BOM/NOAA data) are essential to both improving estimate quality and reducing API costs.

### Tasks

#### 3A: Weather Data Provider
- [ ] BOM API integration (Australian weather data, forecasts)
- [ ] OpenWeatherMap API integration (global current + 5-day forecast)
- [ ] NOAA API integration (US-specific weather data for US-centric markets)
- [ ] Parse into `DataContext` struct
- [ ] Keyword extraction from market questions to determine relevant location/metric

#### 3B: Sports Data Provider
- [ ] API-Sports integration (fixtures, injuries, standings)
- [ ] Parse team/player data relevant to market questions
- [ ] Map sport + team from market question text

#### 3C: Economics Data Provider
- [ ] FRED API integration (US CPI, employment, GDP, Fed rates)
- [ ] RBA data integration (AU cash rate, inflation)
- [ ] ABS data integration (AU employment, GDP)
- [ ] Parse macro indicators and forecasts relevant to ForecastEx economics markets

#### 3D: News/Sentiment Provider
- [ ] NewsAPI integration for breaking news
- [ ] Basic sentiment scoring (positive/negative keyword count)
- [ ] Rate limiting and caching (news doesn't change per-minute)

#### 3E: Enrichment Orchestrator
- [ ] Route market to appropriate data providers based on `MarketCategory`
- [ ] Aggregate data contexts from multiple providers
- [ ] Cache responses (TTL-based) to reduce API costs
- [ ] Track and accumulate data API costs
- [ ] Implement aggressive cross-market data sharing (e.g., one BOM fetch serves all AU weather markets in the same cycle)

### Data Context Structure

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataContext {
    pub category: MarketCategory,
    pub raw_data: serde_json::Value,    // Full API response
    pub summary: String,                 // Human-readable summary for LLM
    pub freshness: DateTime<Utc>,        // When data was fetched
    pub source: String,                  // "bom", "openweathermap", "fred", etc.
    pub cost: f64,                       // API call cost
    pub metaculus_forecast: Option<f64>, // Cross-reference if available
    pub metaculus_forecasters: Option<u32>,
    pub manifold_price: Option<f64>,    // Play-money sentiment signal
}
```

### Deliverables

- Each data provider returns structured context for test markets
- Caching reduces redundant API calls
- Cost tracking is accurate
- `cargo run -- --enrich-only` shows data context for sample markets

---

## Phase 4: LLM Integration and Fair-Value Estimation

**Goal**: Send enriched market data to the LLM and extract probability estimates.

**Duration**: Day 6-8

### Tasks

#### 4A: LLM Trait and Anthropic Implementation
- [ ] Define `LlmEstimator` trait with `estimate_probability` method
- [ ] Implement Anthropic Claude client (reqwest to `https://api.anthropic.com/v1/messages`)
- [ ] Build prompt template (see whitepaper §2.1)
- [ ] Parse float from LLM response (regex extraction + validation)
- [ ] Handle API errors, rate limits, and retries (exponential backoff)
- [ ] Track token usage and compute cost per call

#### 4B: OpenAI GPT-4 Implementation
- [ ] Implement OpenAI client as alternative
- [ ] Same prompt template, different API format

#### 4C: Batch Estimation
- [ ] Group markets by category for batch prompts
- [ ] "Estimate probabilities for these 10 weather markets" (one LLM call, multiple outputs)
- [ ] Parse multi-market responses
- [ ] Fall back to individual calls if batch parsing fails

#### 4D: Estimate Validation
- [ ] Reject estimates outside [0.01, 0.99] (overconfidence guard)
- [ ] Re-query if estimate is suspiciously close to market price (possible echo)
- [ ] Log all estimates to SQLite for calibration tracking

### LLM Estimator Interface

```rust
#[async_trait]
pub trait LlmEstimator: Send + Sync {
    async fn estimate_probability(
        &self,
        market: &Market,
        context: &DataContext,
    ) -> Result<Estimate>;
    
    async fn batch_estimate(
        &self,
        markets: &[(Market, DataContext)],
    ) -> Result<Vec<Estimate>>;
    
    fn cost_per_call(&self) -> f64;
    fn model_name(&self) -> &str;
}

#[derive(Debug, Clone)]
pub struct Estimate {
    pub probability: f64,
    pub confidence: f64,       // 0-1, self-reported by LLM
    pub reasoning: String,     // Chain-of-thought summary
    pub tokens_used: u32,
    pub cost: f64,
}
```

### Deliverables

- LLM returns probability estimates for test markets
- Estimates are logged to database
- Cost tracking per estimate is working
- Batch mode reduces per-market cost by ~60%
- `cargo run -- --estimate-only` shows fair values vs. market prices

---

## Phase 5: Strategy Engine (Edge Detection + Kelly Sizing)

**Goal**: The core brain — detect mispricings and size bets.

**Duration**: Day 8-10

### Tasks

#### 5A: Edge Detector
- [ ] Compare LLM estimate to market price
- [ ] Apply category-specific thresholds (whitepaper §2.2)
- [ ] Determine bet side (YES if estimate > price + threshold, NO if estimate < price - threshold)
- [ ] Filter out edges below minimum (noise reduction)

#### 5B: Kelly Calculator
- [ ] Implement Kelly fraction formula (whitepaper §2.3)
- [ ] Apply fractional Kelly multiplier (default 0.25)
- [ ] Cap at max_bet_pct (default 6%)
- [ ] Floor at minimum bet size (IB minimum order: 1 contract)
- [ ] Account for IB commissions in edge calculation

#### 5C: Risk Manager
- [ ] Check position limits before placing bet
- [ ] Check category exposure limits
- [ ] Check total exposure limit
- [ ] Apply drawdown-adjusted Kelly multiplier (whitepaper §5.2)
- [ ] Detect correlated markets and limit aggregate exposure
- [ ] Slippage estimation based on order book depth

#### 5D: Strategy Orchestrator
- [ ] Pipeline: markets → filter → enrich → estimate → detect edge → size → risk check → execute
- [ ] Rank opportunities by expected value (edge × size × confidence)
- [ ] Select top N bets per cycle (avoid over-trading)
- [ ] Log all decisions (including passed-on opportunities) for analysis

### Core Strategy Flow (Pseudocode)

```rust
pub async fn run_cycle(&mut self) -> Result<CycleReport> {
    // 1. Scan markets from all platforms
    let markets = self.scanner.fetch_all_markets().await?;
    
    // 2. Quick filter (liquidity, deadline, category)
    let candidates = self.filter_candidates(markets);
    
    // 3. Enrich with real-time data + cross-platform refs
    let enriched = self.enricher.enrich_batch(candidates).await?;
    
    // 4. LLM fair-value estimation
    let estimates = self.llm.batch_estimate(&enriched).await?;
    
    // 5. Detect mispricings
    let edges = self.edge_detector.find_edges(&estimates);
    
    // 6. Size and risk-check bets
    let bets: Vec<BetDecision> = edges.iter()
        .map(|e| self.kelly.size_bet(e, self.state.bankroll))
        .filter(|b| self.risk_manager.approve(b, &self.state))
        .collect();
    
    // 7. Rank and select top opportunities
    let selected = self.rank_and_select(bets, MAX_BETS_PER_CYCLE);
    
    // 8. Execute trades (IB for real money, Manifold for validation)
    let receipts = self.executor.execute_batch(&selected).await?;
    
    // 9. Update state, deduct costs, check survival
    self.accountant.reconcile(&receipts, &cycle_costs)?;
    
    Ok(CycleReport { ... })
}
```

### Deliverables

- Edge detection finds mispricings in test data
- Kelly sizing produces reasonable bet amounts
- Risk manager blocks over-concentration
- Full dry-run cycle works end-to-end (no real money)
- Detailed cycle reports logged

---

## Phase 6: Trade Execution and Survival Loop

**Goal**: Place real bets via IB and run the autonomous loop with survival mechanics.

**Duration**: Day 10-14

### Tasks

#### 6A: IB ForecastEx Executor
- [ ] Implement order placement via TWS API
- [ ] Support limit orders (preferred) and market orders
- [ ] Handle order status callbacks (submitted, filled, cancelled, error)
- [ ] Confirm fills and parse execution price + commissions
- [ ] Implement retry logic for failed/rejected orders
- [ ] Handle IB-specific edge cases (market closed, contract expired, insufficient margin)

#### 6B: Manifold Paper Executor
- [ ] Implement Manifold API bet placement (play-money)
- [ ] Mirror all ForecastEx bets on matching Manifold markets
- [ ] Track parallel P&L for strategy validation
- [ ] Compare real vs. paper performance for calibration

#### 6C: Accountant Module
- [ ] Track all costs per cycle (LLM, data APIs, IB commissions, IB data fees)
- [ ] Track all revenue (resolved bets, IB interest on idle cash)
- [ ] Compute running P&L
- [ ] Check survival condition after each cycle
- [ ] If balance <= 0: log final state, send death alert, terminate

#### 6D: Main Loop
- [ ] Implement 10-minute interval with `tokio::time::interval`
- [ ] Error recovery: if a cycle fails, log error and continue to next cycle
- [ ] State persistence: save `AgentState` to SQLite after each cycle
- [ ] Resume from last state on restart
- [ ] Graceful shutdown: finish current cycle, save state, exit
- [ ] Respect IB market hours (ForecastEx may have trading windows)

### Main Loop Skeleton

```rust
#[tokio::main]
async fn main() -> Result<()> {
    // Init config, logging, DB, platforms, LLM
    let mut agent = Agent::initialize().await?;
    agent.print_banner();
    
    let mut interval = tokio::time::interval(Duration::from_secs(600));
    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);
    
    loop {
        tokio::select! {
            _ = interval.tick() => {
                match agent.run_cycle().await {
                    Ok(report) => {
                        agent.log_cycle_report(&report);
                        if agent.state.status == AgentStatus::Died {
                            agent.log_death();
                            break;
                        }
                    }
                    Err(e) => {
                        tracing::error!("Cycle failed: {e}");
                        // Continue to next cycle
                    }
                }
            }
            _ = &mut shutdown => {
                tracing::info!("Shutting down gracefully...");
                agent.save_state().await?;
                break;
            }
        }
    }
    Ok(())
}
```

### Deliverables

- Agent runs autonomously, placing bets on IB paper account
- Parallel paper bets on Manifold for validation
- Cost deduction after each cycle
- Agent terminates at zero balance
- State survives restarts
- Order IDs logged for all IB trades

---

## Phase 7: Dashboard and Monitoring

**Goal**: Web-based real-time dashboard and alerting.

**Duration**: Day 14-17

### Tasks

#### 7A: Axum Web Server
- [ ] Set up axum server on configurable port
- [ ] CORS headers for local development
- [ ] Static file serving for frontend

#### 7B: API Endpoints

```
GET  /api/status          → AgentState (balance, P&L, status, uptime)
GET  /api/trades           → Recent trades (paginated)
GET  /api/positions        → Current open positions (IB + Manifold)
GET  /api/metrics          → Win rate, Sharpe, best/worst trade
GET  /api/balance-history  → Time series of balance for charting
GET  /api/costs            → Breakdown of API/IB costs
GET  /api/cycle-log        → Recent cycle reports
GET  /api/estimates        → LLM estimates with outcomes (for calibration)
GET  /api/validation       → IB vs Manifold paper performance comparison
```

#### 7C: Frontend
- [ ] Single-page HTML dashboard (self-contained, no build tools)
- [ ] Fetch from API endpoints, auto-refresh every 30 seconds
- [ ] Balance history chart (Chart.js, log scale)
- [ ] Trade table with sorting/filtering
- [ ] Status indicator (ALIVE/DIED with colour)
- [ ] Cost breakdown pie chart
- [ ] IB vs Manifold performance comparison panel
- [ ] Responsive layout for mobile monitoring

#### 7D: Alerting
- [ ] Telegram bot integration (send messages via bot API)
- [ ] Discord webhook integration
- [ ] Alert on: trade placed, milestone hit (2x, 5x, 10x), drawdown warning, death
- [ ] Configurable alert thresholds

### Deliverables

- Dashboard accessible at `http://localhost:8080`
- All metrics visible and auto-updating
- Alerts fire on key events
- Mobile-friendly

---

## Phase 8: Calibration, Backtesting, and Optimization

**Goal**: Self-improvement and validation before real-money deployment.

**Duration**: Day 17-21

### Tasks

#### 8A: Historical Backtester
- [ ] Download historical ForecastEx/Metaculus data (resolved markets)
- [ ] Replay markets through the strategy pipeline
- [ ] Simulate balance evolution over time
- [ ] Compute: win rate, P&L, Sharpe, max drawdown, Brier score

#### 8B: Calibration Module
- [ ] After 50+ resolved estimates, compute calibration curve
- [ ] Auto-adjust thresholds per category (whitepaper §6.3)
- [ ] Feed calibration data back into LLM prompts ("Your historical Brier score for weather markets is 0.18...")

#### 8C: Parameter Optimization
- [ ] Grid search over: Kelly multiplier, threshold, batch size
- [ ] Evaluate on historical data
- [ ] Select parameters that maximize Sharpe ratio (not just P&L)

#### 8D: 48-Hour Simulation
- [ ] Simulate 48 hours with historical data
- [ ] Verify growth trajectory ($50 → $500+ target)
- [ ] Identify failure modes and add safeguards
- [ ] Stress test with adverse scenarios (all bets lose, APIs fail, IB disconnects)

### Deliverables

- Backtester produces performance report
- Calibration adjustments improve Brier score
- Optimized parameters documented
- 48-hour simulation passes survival threshold

---

## Testing Strategy

### Unit Tests (Every Phase)

| Module | Tests |
|--------|-------|
| `kelly.rs` | Edge cases: 0% edge, 100% edge, negative edge, large bankroll, tiny bankroll |
| `edge.rs` | Threshold detection, category-specific thresholds, boundary conditions |
| `risk.rs` | Position limits, drawdown multiplier, correlation blocking |
| `types.rs` | Serialization roundtrips, display formatting |

### Integration Tests

| Test | Description |
|------|-------------|
| `mock_platform.rs` | Full cycle with deterministic mock platform returning known markets |
| `simulation.rs` | 48-hour simulation with historical or generated data |
| `api_recording.rs` | Replay recorded API responses for deterministic testing |

### Property-Based Tests

- Kelly bet size is always <= max_bet_pct × bankroll
- Kelly bet size is always >= 0
- Risk manager never approves a bet that would exceed exposure limits
- Agent always terminates when balance reaches 0

---

## Environment Variables

```bash
# .env.example
# LLM
ANTHROPIC_API_KEY=sk-ant-...
OPENAI_API_KEY=sk-...

# Interactive Brokers
IB_ACCOUNT_ID=U1234567        # IB account number
# Note: IB Gateway/TWS must be running locally or on VPS
# Paper: port 4002, Live: port 4001

# Data Sources
OWM_API_KEY=...               # OpenWeatherMap
API_SPORTS_KEY=...
FRED_API_KEY=...              # Federal Reserve Economic Data
NEWS_API_KEY=...

# Alerting
TG_BOT_TOKEN=...              # Telegram bot
TG_CHAT_ID=...                # Telegram chat ID
DISCORD_WEBHOOK_URL=...       # Discord webhook

# Database
DATABASE_URL=sqlite://oracle.db
```

---

## Definition of Done (Per Phase)

Each phase is complete when:

1. All tasks are checked off
2. `cargo build` succeeds with no warnings
3. `cargo test` passes all unit and integration tests
4. `cargo clippy` produces no warnings
5. Key functionality is demonstrated via a CLI command or test
6. State is persisted correctly (if applicable)
7. Costs are tracked accurately (if applicable)
8. Documentation is updated

---

## Risk Register

| Risk | Impact | Likelihood (2026) | Mitigation |
|------|--------|--------------------|-----------|
| **ForecastEx low market count / low edge density** | Insufficient opportunities to cover operational costs; agent starves | **High** — confirmed constraint. ForecastEx has ~50-200 active markets vs. 500+ on offshore platforms | Aggressive data enrichment to maximize edge per market; efficient cost management (batching, caching); cross-platform signals (Metaculus/Manifold) to improve estimate quality; widen category acceptance; lower scan interval during high-activity periods |
| **Single execution venue dependency** | If ForecastEx becomes unavailable (IB outage, product discontinuation, regulatory change), agent has zero execution capability | **Medium** — IB is a major, stable institution, but single-point-of-failure risk is inherent | Trait-based platform abstraction allows rapid integration of new platforms if they emerge; monitor IB product announcements; Manifold paper-trading provides continuous strategy validation even during IB outages; alert immediately on ForecastEx unavailability |
| LLM estimates are poorly calibrated | Agent loses money faster than it earns | Medium | Phase 8 calibration; start with paper trading; quarter-Kelly conservative sizing |
| API rate limits hit during scan | Missed opportunities, wasted cycles | Medium | Caching, batch requests, backoff |
| IB Gateway disconnects | No execution possible | Medium | Auto-reconnect with exponential backoff, skip cycle, alert; connection health monitoring |
| IB TWS API changes | Executor breaks | Low | Pin API version, integration tests with recorded responses |
| Flash crash drains bankroll | Total loss | Low | Drawdown protection, max exposure caps, quarter-Kelly |
| LLM API outage | No estimates possible | Medium | Fallback to secondary LLM provider, skip cycle |
| Agent overconfidence | Systematic losses | Medium | Quarter-Kelly, calibration module, conservative thresholds, echo detection |
| IB market hours restrictions | Can't trade outside hours | Low | Respect trading windows, queue orders for next session |
| **Regulatory landscape change (positive)** | New compliant platforms emerge in AU, expanding opportunity set | **Low** — ACMA enforcement trend is restrictive; no new entrants expected near-term | Monitor ACMA/ASIC announcements; trait-based architecture ready for rapid integration; periodic regulatory review |
| **Regulatory landscape change (negative)** | ACMA restricts ForecastEx or IB changes AU product offering | **Very Low** — ForecastEx operates through ASIC-regulated IB entity | Monitor IB product announcements and ACMA enforcement actions; maintain Manifold paper-trading as fallback validation; document all trades for compliance |
| AU tax complexity | Unexpected tax liability | Medium | Log all trades for accountant, flag in docs |

---

## Milestone Summary

| Phase | Milestone | Verifiable Output |
|-------|-----------|-------------------|
| 0 | Project compiles | `cargo build` succeeds |
| 1 | Types defined | All types serialize/deserialize correctly |
| 2 | Markets fetched | Markets from IB paper + Metaculus + Manifold |
| 3 | Data enriched | Weather/sports/econ context for sample markets |
| 4 | Fair values estimated | LLM returns probabilities for 50+ markets |
| 5 | Edges detected | Mispricings found per scan cycle |
| 6 | Bets placed | IB paper trades + Manifold paper bets with receipts |
| 7 | Dashboard live | Web UI showing all metrics |
| 8 | Backtested | Simulation shows positive growth |

---

## IB Gateway Setup (Prerequisite)

Before running ORACLE, IB Gateway must be running:

```bash
# 1. Download IB Gateway from Interactive Brokers
# https://www.interactivebrokers.com/en/trading/ibgateway-stable.php

# 2. Run IB Gateway (headless mode for VPS)
# Configure: API Settings → Enable ActiveX and Socket Clients
# Paper trading port: 4002
# Live trading port: 4001
# Trusted IPs: 127.0.0.1

# 3. For VPS deployment, use IBC (IB Controller) for auto-login:
# https://github.com/IbcAlpha/IBC
```

---

## Quick Start (After All Phases)

```bash
# 1. Clone and build
git clone https://github.com/youruser/oracle.git
cd oracle
cp .env.example .env
# Fill in API keys in .env

# 2. Start IB Gateway (paper mode)
# Ensure IB Gateway is running on port 4002

# 3. Build
cargo build --release

# 4. Run (paper trading mode — IB paper + Manifold)
./target/release/oracle --config config.toml

# 5. Run (live — change IB port to 4001 in config)
# Edit config.toml: ib_port = 4001
./target/release/oracle --config config.toml

# 6. Docker (ensure IB Gateway is accessible)
docker build -t oracle .
docker run -d --env-file .env --network host -p 8080:8080 oracle
```

---

*ORACLE Development Plan v1.2 — February 2026*
*Build iteratively. Test relentlessly. Survive.*
