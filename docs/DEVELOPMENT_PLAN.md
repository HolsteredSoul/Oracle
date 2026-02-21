# ORACLE: Iterative Development Plan

## Version 1.2 â€” Build Roadmap

---

## Overview

This document defines the phased development plan for ORACLE. Each phase produces a working, testable artifact. Phases are designed so that earlier phases are prerequisites for later ones, and the agent becomes progressively more capable with each iteration.

**Total estimated effort**: 6-8 phases, ~2-4 weeks for a working MVP.

**Language**: Rust (stable toolchain, 2021 edition)
**Async runtime**: Tokio
**Key crates**: reqwest, serde, axum, sqlx, chrono, plotters, tracing

**Target platforms** (AU-compliant, confirmed February 2026):
- **IB ForecastEx** â€” real-money execution via Interactive Brokers TWS API. As of February 2026, this remains the sole fully legal, real-money prediction market platform accessible to Australian residents. No additional real-money execution platforms are planned unless the regulatory landscape changes.
- **Metaculus** â€” crowd forecast cross-reference (read-only)
- **Manifold** â€” play-money validation and sentiment signal

---

## Phase 0: Project Scaffolding âœ…

**Goal**: Compilable Rust project with modular structure, config loading, and logging.

**Duration**: Day 1 â€” **Completed 2026-02-14**

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
â”œâ”€â”€ Cargo.toml
â”œâ”€â”€ config.toml
â”œâ”€â”€ .env.example
â”œâ”€â”€ Dockerfile
â”œâ”€â”€ README.md
â”œâ”€â”€ src/
â”‚   â”œâ”€â”€ main.rs                 # Entry point, async main loop
â”‚   â”œâ”€â”€ config.rs               # TOML config + env var resolution
â”‚   â”œâ”€â”€ types.rs                # Shared types (Market, Side, Trade, etc.)
â”‚   â”œâ”€â”€ platforms/
â”‚   â”‚   â”œâ”€â”€ mod.rs              # PredictionPlatform trait
â”‚   â”‚   â”œâ”€â”€ forecastex.rs       # IB ForecastEx implementation (TWS API)
â”‚   â”‚   â”œâ”€â”€ metaculus.rs        # Metaculus read-only implementation
â”‚   â”‚   â””â”€â”€ manifold.rs         # Manifold play-money implementation
â”‚   â”œâ”€â”€ data/
â”‚   â”‚   â”œâ”€â”€ mod.rs              # DataProvider trait
â”‚   â”‚   â”œâ”€â”€ weather.rs          # BOM / OpenWeatherMap / NOAA
â”‚   â”‚   â”œâ”€â”€ sports.rs           # API-Sports
â”‚   â”‚   â”œâ”€â”€ economics.rs        # FRED / RBA / ABS
â”‚   â”‚   â””â”€â”€ news.rs             # NewsAPI / RSS
â”‚   â”œâ”€â”€ llm/
â”‚   â”‚   â”œâ”€â”€ mod.rs              # LLM trait + prompt builder
â”‚   â”‚   â”œâ”€â”€ anthropic.rs        # Claude integration
â”‚   â”‚   â”œâ”€â”€ openai.rs           # GPT-4 integration
â”‚   â”‚   â””â”€â”€ grok.rs             # Grok integration
â”‚   â”œâ”€â”€ strategy/
â”‚   â”‚   â”œâ”€â”€ mod.rs              # Strategy orchestrator
â”‚   â”‚   â”œâ”€â”€ edge.rs             # Mispricing detection
â”‚   â”‚   â”œâ”€â”€ kelly.rs            # Kelly criterion sizing
â”‚   â”‚   â””â”€â”€ risk.rs             # Risk manager (limits, drawdown, correlation)
â”‚   â”œâ”€â”€ engine/
â”‚   â”‚   â”œâ”€â”€ mod.rs              # Main scan-estimate-bet loop
â”‚   â”‚   â”œâ”€â”€ scanner.rs          # Multi-platform market scanner
â”‚   â”‚   â”œâ”€â”€ enricher.rs         # Data enrichment pipeline
â”‚   â”‚   â”œâ”€â”€ executor.rs         # Trade execution with retries
â”‚   â”‚   â””â”€â”€ accountant.rs       # Cost tracking + survival check
â”‚   â”œâ”€â”€ storage/
â”‚   â”‚   â”œâ”€â”€ mod.rs              # SQLite persistence
â”‚   â”‚   â””â”€â”€ schema.sql          # Database schema
â”‚   â””â”€â”€ dashboard/
â”‚       â”œâ”€â”€ mod.rs              # Axum web server
â”‚       â”œâ”€â”€ routes.rs           # API endpoints
â”‚       â””â”€â”€ templates/          # HTML templates (or JSON API only)
â””â”€â”€ tests/
    â”œâ”€â”€ integration/
    â”‚   â”œâ”€â”€ mock_platform.rs    # Mock platform for testing
    â”‚   â””â”€â”€ simulation.rs       # 48-hour simulation harness
    â””â”€â”€ unit/
        â”œâ”€â”€ kelly_tests.rs
        â””â”€â”€ edge_tests.rs
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

**Note**: No `ethers` crate needed â€” we're not interacting with blockchain. IB TWS API is TCP socket-based, handled via `ibapi` crate or raw TCP with custom protocol implementation.

### Deliverables

- `cargo build` succeeds
- `cargo run` starts, loads config, prints banner, and enters idle loop
- Structured JSON logs to stdout

---

## Phase 1: Core Types and Platform Trait âœ…

**Goal**: Define the shared data model and platform abstraction so all subsequent modules have a stable interface.

**Duration**: Day 1-2 â€” **Completed 2026-02-14** (76 unit tests)

### Tasks

- [x] Define `Market` struct (id, question, platform, category, current_price, volume, deadline, etc.)
- [x] Define `Side` enum (Yes, No)
- [x] Define `TradeReceipt` struct (order_id, amount, price, fees, timestamp)
- [x] Define `Position` struct (market_id, side, size, entry_price, current_value)
- [x] Define `LiquidityInfo` struct (bid_depth, ask_depth, volume_24h)
- [x] Define `MarketCategory` enum (Weather, Sports, Economics, Politics, Culture, Other)
- [x] Define `PredictionPlatform` trait (see whitepaper Â§3.3)
- [x] Define `DataProvider` trait (see whitepaper Â§4.2)
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

## Phase 2: Platform Integrations (Scanning) â€” IN PROGRESS

**Goal**: Fetch live markets from ForecastEx, Metaculus, and Manifold. No betting yet â€” read-only.

**Duration**: Day 2-4

**Status**: 2B âœ…, 2C âœ…, and 2D âœ… complete. 2A (ForecastEx/IB) remaining.

**Platform exclusivity note (2026)**: IB ForecastEx is confirmed as the sole real-money execution platform accessible from Australia. The integrations below reflect this: ForecastEx is the primary scanner and execution target, while Metaculus and Manifold serve exclusively as read-only cross-reference and validation sources. No additional real-money platform integrations are planned or needed under current AU regulations. The `PredictionPlatform` trait abstraction is retained to allow future expansion if the regulatory landscape changes.

### Tasks

#### 2A: IB ForecastEx Scanner âŒ NOT STARTED
*Intentionally deferred â€” most complex integration. Will implement after pipeline is proven with Manifold/Metaculus.*
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

#### 2B: Metaculus Scanner âœ… COMPLETE (2026-02-14)
- [x] Implement REST API client (`https://www.metaculus.com/api2/`)
- [x] Fetch active questions with community forecasts
- [x] Parse community median/mean probability
- [x] Map to `Market` struct (with `platform = "metaculus"`)
- [ ] Implement matching logic: find Metaculus questions similar to ForecastEx markets (fuzzy text matching) *(deferred to 2D: Market Router)*

*Implementation: `src/platforms/metaculus.rs` â€” 420 lines, 24 unit tests. Paginated scanning ordered by forecaster count, category classification via slugs + title keywords, graceful handling of hidden predictions (pre-`cp_reveal_time`).*

#### 2C: Manifold Scanner âœ… COMPLETE (2026-02-14)
- [x] Implement REST API client (`https://api.manifold.markets/v0/`)
- [x] Fetch active binary markets with play-money probabilities
- [x] Parse into `Market` struct (with `platform = "manifold"`)
- [x] Filter for markets matching ForecastEx categories
- [x] Track Mana prices as sentiment signals

*Implementation: `src/platforms/manifold.rs` â€” full `PredictionPlatform` trait impl including bet placement, balance checking, liquidity checking. Multi-sort scanning with deduplication. 17 unit tests.*

#### 2D: Market Router âœ… COMPLETE (2026-02-17)
- [x] Aggregate markets from all enabled platforms
- [x] Match cross-platform markets (same underlying event) via fuzzy text matching
- [x] Attach Metaculus forecasts and Manifold prices as `CrossReferences`
- [x] Sort by category, volume, deadline
- [x] Filter out markets below liquidity thresholds

*Implementation: `src/engine/scanner.rs` â€” MarketRouter with concurrent platform fetching, word-overlap fuzzy matching (Jaccard + containment), category pre-filtering, priority scoring (cross-refs, liquidity, centrality). 20 unit tests.*

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

## Phase 3: Data Enrichment Pipeline âœ…

**Goal**: For each candidate market, fetch domain-specific real-time data to inform LLM estimates.

**Duration**: Day 4-6 â€” **Completed 2026-02-21** (40 unit tests across providers + enricher)

**Cost efficiency note**: Given ForecastEx's limited market catalog (~50-200 active markets), maximizing the informational edge extracted from each market is critical. Aggressive caching and data reuse across markets sharing the same category or underlying data (e.g., multiple weather markets using the same BOM/NOAA data) are essential to both improving estimate quality and reducing API costs.

### Tasks

#### 3A: Weather Data Provider âœ… COMPLETE
- [x] Open-Meteo API integration (free, no key required â€” global current + 7-day forecast)
- [x] Parse into `DataContext` struct
- [x] Keyword extraction from market questions to determine relevant location/metric (14 known locations: AU cities, US cities, London, Tokyo)

*Implementation: `src/data/weather.rs` â€” Open-Meteo (free) instead of BOM/OWM/NOAA. Covers global weather with zero API cost. 7 unit tests.*

#### 3B: Sports Data Provider âœ… COMPLETE
- [x] Keyword-based sport/league extraction (12 sports: NBA, NFL, MLB, NHL, EPL, Tennis, F1, UFC, Cricket, AFL, NRL, Olympics)
- [x] Team name extraction from question text
- [x] Parse into `DataContext` with cross-reference signals
- [ ] Full API-Sports integration *(deferred â€” free tier only 100 req/day, insufficient for scanning)*

*Implementation: `src/data/sports.rs` â€” keyword extraction MVP. Full API integration deferred until needed. 7 unit tests.*

#### 3C: Economics Data Provider âœ… COMPLETE
- [x] FRED API integration (CPI, unemployment, GDP, Fed funds, yield curve, S&P 500, housing, crypto, trade â€” 9 keyword groups mapping to 20+ FRED series)
- [x] Keyword-only fallback when no API key configured
- [x] Parse macro indicators into `DataContext`
- [ ] RBA data integration *(deferred â€” most ForecastEx markets are US-centric)*
- [ ] ABS data integration *(deferred)*

*Implementation: `src/data/economics.rs` â€” FRED primary source. Free API. 6 unit tests.*

#### 3D: News/Sentiment Provider âœ… COMPLETE
- [x] NewsAPI integration for breaking news (with keyword-only fallback)
- [x] Basic sentiment scoring (positive/negative keyword count, 18+19 word lists)
- [x] Topic classification (9 topics: US Politics, Elections, Geopolitics, China, AI/Tech, Climate, Health, Entertainment, Space)
- [x] Search query extraction from market questions

*Implementation: `src/data/news.rs` â€” Covers Politics, Culture, and Other categories. 11 unit tests.*

#### 3E: Enrichment Orchestrator âœ… COMPLETE
- [x] Route market to appropriate data providers based on `MarketCategory`
- [x] Cache responses (TTL-based: 60min weather, 30min econ/sports, 15min news)
- [x] Track and accumulate data API costs
- [x] Cross-market data sharing via keyword-based cache keys (same topic = shared fetch)
- [x] Graceful degradation (failed enrichment falls back to empty context)
- [x] Cache hit rate monitoring

*Implementation: `src/engine/enricher.rs` â€” Category-routed orchestrator with in-memory TTL cache. 9 unit tests.*

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

## Phase 4: LLM Integration and Fair-Value Estimation u{2705}

**Goal**: Send enriched market data to the LLM and extract probability estimates.

**Duration**: Day 6-8 -- **Completed 2026-02-21** (21 unit tests)

### Tasks

#### 4A: LLM Trait and Anthropic Implementation u{2705} COMPLETE
- [x] Define `LlmEstimator` trait with `estimate_probability` method
- [x] Implement Anthropic Claude client (reqwest to `https://api.anthropic.com/v1/messages`)
- [x] Build prompt template with calibration rules, step-by-step reasoning, cross-reference signals
- [x] Parse float from LLM response (label extraction + fallback + percentage conversion)
- [x] Handle API errors, rate limits, and retries (exponential backoff, 3 retries)
- [x] Track token usage and compute cost per call (atomic counters)
- [x] Echo detection: warn when estimate is near market price

*Implementation: `src/llm/anthropic.rs` -- Full Anthropic Messages API client. 18 unit tests.*

#### 4B: OpenAI GPT-4 Implementation u{2705} COMPLETE
- [x] Implement OpenAI Chat Completions client as alternative
- [x] Same prompt template, reuses Anthropic parsing utilities

*Implementation: `src/llm/openai.rs` -- GPT-4o default. 3 unit tests.*

#### 4C: Batch Estimation u{2705} COMPLETE
- [x] Batch prompt builder for multiple markets
- [x] Parse `MARKET_ID: xxx | PROBABILITY: 0.XX | CONFIDENCE: 0.XX` format
- [x] Fall back to individual calls if batch parsing fails
- [x] Small batches (2 or fewer markets) use individual calls directly

#### 4D: Estimate Validation u{2705} COMPLETE
- [x] Reject estimates outside [0.01, 0.99] (clamped automatically)
- [x] Echo detection: warn if estimate near market price (0.02 threshold)
- [ ] Log all estimates to SQLite *(deferred to Phase 6 -- requires storage layer)*

### Deliverables

- LLM returns probability estimates for test markets
- Estimates are logged to database
- Cost tracking per estimate is working
- Batch mode reduces per-market cost by ~60%
- `cargo run -- --estimate-only` shows fair values vs. market prices
---

## Phase 5: Strategy Engine (Edge Detection + Kelly Sizing) u{2705}

**Goal**: The core brain â€” detect mispricings and size bets.

**Duration**: Day 8-10 -- **Completed 2026-02-21** (32 unit tests)

### Tasks

#### 5A: Edge Detector u{2705} COMPLETE
- [x] Compare LLM estimate to market price
- [x] Apply category-specific thresholds (whitepaper Â§2.2)
- [x] Determine bet side (YES if estimate > price + threshold, NO if estimate < price - threshold)
- [x] Filter out edges below minimum (noise reduction)
- [x] Low-confidence estimates require double threshold

*Implementation: `src/strategy/edge.rs` -- Category-specific thresholds, noise floor, confidence scaling. 10 unit tests.*

#### 5B: Kelly Calculator u{2705} COMPLETE
- [x] Implement Kelly fraction formula (whitepaper Â§2.3)
- [x] Apply fractional Kelly multiplier (default 0.25)
- [x] Cap at max_bet_pct (default 6%)
- [x] Floor at minimum bet size (IB minimum order: 1 contract)
- [x] Account for IB commissions in edge calculation

*Implementation: `src/strategy/kelly.rs` -- Commission-adjusted Kelly with fractional multiplier, caps, floors. 11 unit tests.*

#### 5C: Risk Manager u{2705} COMPLETE
- [x] Check position limits before placing bet
- [x] Check category exposure limits
- [x] Check total exposure limit
- [x] Apply drawdown-adjusted Kelly multiplier (whitepaper Â§5.2)
- [ ] Detect correlated markets *(deferred -- requires position tracking in Phase 6)*
- [ ] Slippage estimation *(deferred -- requires IB market data in Phase 2A)*

*Implementation: `src/strategy/risk.rs` -- Exposure limits, category caps, drawdown-adjusted sizing, cycle limits. 11 unit tests.*

#### 5D: Strategy Orchestrator -- PARTIAL
- [ ] Full pipeline: markets â†’ filter â†’ enrich â†’ estimate â†’ detect edge â†’ size â†’ risk check â†’ execute
- [ ] Rank opportunities by expected value (edge Ã— size Ã— confidence)
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

## Phase 6: Trade Execution and Survival Loop u{2705}

**Goal**: Place real bets via IB and run the autonomous loop with survival mechanics.

**Duration**: Day 10-14 -- **Completed 2026-02-21** (16 unit tests)

### Tasks

#### 6A: IB ForecastEx Executor -- DEFERRED (Phase 2A)
- [ ] *Entire sub-phase deferred until IB Gateway setup*

#### 6B: Manifold Paper Executor u{2705} COMPLETE
- [x] Implement Manifold API bet placement (play-money)
- [x] Dry-run execution mode for testing without API keys
- [x] Batch execution with per-trade reporting
- [ ] Compare real vs. paper performance *(deferred to Phase 2A)*

*Implementation: `src/engine/executor.rs` -- Dry-run + Manifold execution with batch support. 4 unit tests.*

#### 6C: Accountant Module u{2705} COMPLETE
- [x] Track all costs per cycle (LLM, data APIs, IB commissions)
- [x] Track bankroll, peak, and P&L
- [x] Compute running P&L
- [x] Check survival condition after each cycle
- [x] If balance <= 0: log final state, set status to Died

*Implementation: `src/engine/accountant.rs` -- Cycle reconciliation with cost tracking. 7 unit tests.*

#### 6D: Main Loop u{2705} COMPLETE
- [x] Configurable interval with `tokio::time::interval`
- [x] Error recovery: if a cycle fails, log error and continue
- [x] State persistence: save `AgentState` to JSON after each cycle
- [x] Resume from last state on restart
- [x] Graceful shutdown via Ctrl+C signal handling
- [ ] Respect IB market hours *(deferred to Phase 2A)*

*Implementation: `src/main.rs` -- Full async main loop + `src/storage/mod.rs` -- JSON state persistence. 5 storage tests.*

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
GET  /api/status          â†’ AgentState (balance, P&L, status, uptime)
GET  /api/trades           â†’ Recent trades (paginated)
GET  /api/positions        â†’ Current open positions (IB + Manifold)
GET  /api/metrics          â†’ Win rate, Sharpe, best/worst trade
GET  /api/balance-history  â†’ Time series of balance for charting
GET  /api/costs            â†’ Breakdown of API/IB costs
GET  /api/cycle-log        â†’ Recent cycle reports
GET  /api/estimates        â†’ LLM estimates with outcomes (for calibration)
GET  /api/validation       â†’ IB vs Manifold paper performance comparison
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
- [ ] Auto-adjust thresholds per category (whitepaper Â§6.3)
- [ ] Feed calibration data back into LLM prompts ("Your historical Brier score for weather markets is 0.18...")

#### 8C: Parameter Optimization
- [ ] Grid search over: Kelly multiplier, threshold, batch size
- [ ] Evaluate on historical data
- [ ] Select parameters that maximize Sharpe ratio (not just P&L)

#### 8D: 48-Hour Simulation
- [ ] Simulate 48 hours with historical data
- [ ] Verify growth trajectory ($50 â†’ $500+ target)
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

- Kelly bet size is always <= max_bet_pct Ã— bankroll
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
| **ForecastEx low market count / low edge density** | Insufficient opportunities to cover operational costs; agent starves | **High** â€” confirmed constraint. ForecastEx has ~50-200 active markets vs. 500+ on offshore platforms | Aggressive data enrichment to maximize edge per market; efficient cost management (batching, caching); cross-platform signals (Metaculus/Manifold) to improve estimate quality; widen category acceptance; lower scan interval during high-activity periods |
| **Single execution venue dependency** | If ForecastEx becomes unavailable (IB outage, product discontinuation, regulatory change), agent has zero execution capability | **Medium** â€” IB is a major, stable institution, but single-point-of-failure risk is inherent | Trait-based platform abstraction allows rapid integration of new platforms if they emerge; monitor IB product announcements; Manifold paper-trading provides continuous strategy validation even during IB outages; alert immediately on ForecastEx unavailability |
| LLM estimates are poorly calibrated | Agent loses money faster than it earns | Medium | Phase 8 calibration; start with paper trading; quarter-Kelly conservative sizing |
| API rate limits hit during scan | Missed opportunities, wasted cycles | Medium | Caching, batch requests, backoff |
| IB Gateway disconnects | No execution possible | Medium | Auto-reconnect with exponential backoff, skip cycle, alert; connection health monitoring |
| IB TWS API changes | Executor breaks | Low | Pin API version, integration tests with recorded responses |
| Flash crash drains bankroll | Total loss | Low | Drawdown protection, max exposure caps, quarter-Kelly |
| LLM API outage | No estimates possible | Medium | Fallback to secondary LLM provider, skip cycle |
| Agent overconfidence | Systematic losses | Medium | Quarter-Kelly, calibration module, conservative thresholds, echo detection |
| IB market hours restrictions | Can't trade outside hours | Low | Respect trading windows, queue orders for next session |
| **Regulatory landscape change (positive)** | New compliant platforms emerge in AU, expanding opportunity set | **Low** â€” ACMA enforcement trend is restrictive; no new entrants expected near-term | Monitor ACMA/ASIC announcements; trait-based architecture ready for rapid integration; periodic regulatory review |
| **Regulatory landscape change (negative)** | ACMA restricts ForecastEx or IB changes AU product offering | **Very Low** â€” ForecastEx operates through ASIC-regulated IB entity | Monitor IB product announcements and ACMA enforcement actions; maintain Manifold paper-trading as fallback validation; document all trades for compliance |
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
# Configure: API Settings â†’ Enable ActiveX and Socket Clients
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

# 4. Run (paper trading mode â€” IB paper + Manifold)
./target/release/oracle --config config.toml

# 5. Run (live â€” change IB port to 4001 in config)
# Edit config.toml: ib_port = 4001
./target/release/oracle --config config.toml

# 6. Docker (ensure IB Gateway is accessible)
docker build -t oracle .
docker run -d --env-file .env --network host -p 8080:8080 oracle
```

---

*ORACLE Development Plan v1.2 â€” February 2026*
*Build iteratively. Test relentlessly. Survive.*
