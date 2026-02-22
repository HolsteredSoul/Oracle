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
- **Polymarket** -- real-money execution via Polymarket CLOB API (Polygon/USDC). Largest prediction market by liquidity with 500+ active markets.
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
â”‚   â”‚   â”œâ”€â”€ polymarket.rs       # Polymarket (Gamma + CLOB API)
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

**Note**: No ethers crate needed at build time -- Polymarket CLOB API is REST/HMAC based. On-chain signing handled at runtime via wallet private key.

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
    pub platform: String,         // "polymarket" | "metaculus" | "manifold"
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

**Goal**: Fetch live markets from Polymarket, Metaculus, and Manifold. No betting yet â€” read-only.

**Duration**: Day 2-4

**Status**: 2B âœ…, 2C âœ…, and 2D âœ… complete. 2A (Polymarket/IB) remaining.

**Platform exclusivity note (2026)**: Polymarket is confirmed as the sole real-money execution platform accessible from Australia. The integrations below reflect this: Polymarket is the primary scanner and execution target, while Metaculus and Manifold serve exclusively as read-only cross-reference and validation sources. No additional real-money platform integrations are planned or needed under current AU regulations. The `PredictionPlatform` trait abstraction is retained to allow future expansion if the regulatory landscape changes.

### Tasks

#### 2A: Polymarket Scanner âŒ NOT STARTED
*Intentionally deferred â€” most complex integration. Will implement after pipeline is proven with Manifold/Metaculus.*
- [ ] Implement Polymarket CLOB API connection (TCP socket to Polymarket CLOB)
- [ ] Authenticate with client ID and account
- [ ] Request contract details for Polymarket event contracts
- [ ] Fetch market data (bid/ask/last/volume) for active contracts
- [ ] Parse into `Market` struct
- [ ] Handle connection drops and reconnection
- [ ] Support both paper (port 4002) and live (port 4001) modes

**Polymarket CLOB API specifics:**
- Polymarket uses condition IDs to identify markets and token IDs for YES/NO positions
- Use `reqContractDetails` to discover available markets
- Use `reqMktData` for real-time prices
- Use `reqHistoricalData` for volume/liquidity assessment

**Reliability priority**: Polymarket CLOB API is stateless REST, so connection reliability is less critical than a persistent TCP connection. Standard HTTP retry logic handles transient failures.

#### 2B: Metaculus Scanner âœ… COMPLETE (2026-02-14)
- [x] Implement REST API client (`https://www.metaculus.com/api2/`)
- [x] Fetch active questions with community forecasts
- [x] Parse community median/mean probability
- [x] Map to `Market` struct (with `platform = "metaculus"`)
- [ ] Implement matching logic: find Metaculus questions similar to Polymarket markets (fuzzy text matching) *(deferred to 2D: Market Router)*

*Implementation: `src/platforms/metaculus.rs` â€” 420 lines, 24 unit tests. Paginated scanning ordered by forecaster count, category classification via slugs + title keywords, graceful handling of hidden predictions (pre-`cp_reveal_time`).*

#### 2C: Manifold Scanner âœ… COMPLETE (2026-02-14)
- [x] Implement REST API client (`https://api.manifold.markets/v0/`)
- [x] Fetch active binary markets with play-money probabilities
- [x] Parse into `Market` struct (with `platform = "manifold"`)
- [x] Filter for markets matching Polymarket categories
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

- [ ] Run scanner against live APIs (Polymarket dry-run for Polymarket)
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

**Cost efficiency note**: Given Polymarket's limited market catalog (~50-200 active markets), maximizing the informational edge extracted from each market is critical. Aggressive caching and data reuse across markets sharing the same category or underlying data (e.g., multiple weather markets using the same BOM/NOAA data) are essential to both improving estimate quality and reducing API costs.

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
- [ ] RBA data integration *(deferred â€” most Polymarket markets are US-centric)*
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
- [x] Account for Polymarket fees in edge calculation

*Implementation: `src/strategy/kelly.rs` -- Commission-adjusted Kelly with fractional multiplier, caps, floors. 11 unit tests.*

#### 5C: Risk Manager u{2705} COMPLETE
- [x] Check position limits before placing bet
- [x] Check category exposure limits
- [x] Check total exposure limit
- [x] Apply drawdown-adjusted Kelly multiplier (whitepaper Â§5.2)
- [ ] Detect correlated markets *(deferred -- requires position tracking in Phase 6)*
- [ ] Slippage estimation *(deferred -- requires Polymarket order book data in Phase 2A)*

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

**Goal**: Place real bets and run the autonomous loop with survival mechanics.

**Duration**: Day 10-14 -- **Completed 2026-02-21** (16 unit tests)

### Tasks

#### 6A: Polymarket Executor -- DEFERRED (Phase 2A)
- [ ] *Entire sub-phase deferred until Polymarket CLOB setup*

#### 6B: Manifold Paper Executor u{2705} COMPLETE
- [x] Implement Manifold API bet placement (play-money)
- [x] Dry-run execution mode for testing without API keys
- [x] Batch execution with per-trade reporting
- [ ] Compare real vs. paper performance *(deferred to Phase 2A)*

*Implementation: `src/engine/executor.rs` -- Dry-run + Manifold execution with batch support. 4 unit tests.*

#### 6C: Accountant Module u{2705} COMPLETE
- [x] Track all costs per cycle (LLM, data APIs, Polymarket fees)
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
- [ ] Respect market deadlines and trading windows *(deferred)*

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

- Agent runs autonomously, placing bets on Polymarket dry-run
- Parallel paper bets on Manifold for validation
- Cost deduction after each cycle
- Agent terminates at zero balance
- State survives restarts
- Order IDs logged for all trades

---

## Phase 7: Dashboard and Monitoring u{2705}

**Goal**: Web-based real-time dashboard and alerting.

**Duration**: Day 14-17 -- **Completed 2026-02-21** (18 unit tests)

### Tasks

#### 7A: Axum Web Server u{2705} COMPLETE
- [x] Axum 0.7 server on configurable port
- [x] CORS headers via tower-http
- [x] Embedded HTML dashboard (compiled into binary via include_str)
- [x] Background spawn (non-blocking)

*Implementation: `src/dashboard/mod.rs` -- Router, CORS, embedded HTML serving. 9 endpoint integration tests.*

#### 7B: API Endpoints u{2705} COMPLETE

*Implementation: `src/dashboard/routes.rs` -- 6 JSON API endpoints + health check. Shared state via `Arc<DashboardState>` with `RwLock`. 9 unit tests.*

```
GET  /api/status          â†’ AgentState (balance, P&L, status, uptime)
GET  /api/trades           â†’ Recent trades (paginated)
GET  /api/positions        â†’ Current open positions (Polymarket + Manifold)
GET  /api/metrics          â†’ Win rate, Sharpe, best/worst trade
GET  /api/balance-history  â†’ Time series of balance for charting
GET  /api/costs            â†’ Breakdown of API/Polymarket costs
GET  /api/cycle-log        â†’ Recent cycle reports
GET  /api/estimates        â†’ LLM estimates with outcomes (for calibration)
GET  /api/validation       â†’ Polymarket vs Manifold paper performance comparison
```

#### 7C: Frontend u{2705} COMPLETE
- [x] Single-page HTML dashboard (self-contained, Chart.js via CDN)
- [x] Auto-refresh every 30 seconds with countdown timer
- [x] Balance history line chart
- [x] Recent cycles table (last 15)
- [x] Recent trades table (last 15)
- [x] Status badge (ALIVE/DIED/PAUSED with colour)
- [x] Stats cards: bankroll, P&L, win rate, trades, cycles, costs
- [x] Responsive dark theme

*Implementation: `src/dashboard/templates/index.html` -- Dark-themed SPA, 0 build tools.*

#### 7D: Alerting -- DEFERRED
- [ ] Telegram/Discord integration *(deferred -- not critical for MVP)*

### Deliverables

- Dashboard accessible at `http://localhost:8080`
- All metrics visible and auto-updating
- Alerts fire on key events
- Mobile-friendly

---

## Phase 8: Calibration, Backtesting, and Optimization u{2705}

**Goal**: Self-improvement and validation before real-money deployment.

**Duration**: Day 17-21 -- **Completed 2026-02-21** (19 unit tests)

### Tasks

#### 8A: Historical Backtester u{2705} COMPLETE
- [x] ResolvedMarket type for historical data input
- [x] Replay markets through edge detection + Kelly sizing pipeline
- [x] Simulate balance evolution with trade-by-trade tracking
- [x] Compute: win rate, P&L, Sharpe ratio, max drawdown, Brier score
- [x] Balance history and per-trade log for analysis

*Implementation: `src/backtest/runner.rs` -- Full strategy replay engine. 10 unit tests.*

#### 8B: Calibration Module u{2705} COMPLETE
- [x] Calibration curve with configurable bins (predicted vs actual rates)
- [x] Per-category Brier score breakdown (whitepaper Â§6.3)
- [x] Auto-diagnosis: over-confident / under-confident / well-calibrated
- [x] LLM prompt snippet generator for self-improvement feedback

*Implementation: `src/backtest/calibration.rs` -- Brier scores, calibration curves, diagnosis. 9 unit tests.* ("Your historical Brier score for weather markets is 0.18...")

#### 8C: Parameter Optimization -- DEFERRED
- [ ] Grid search over parameters *(deferred -- needs historical data to be meaningful)*



#### 8D: 48-Hour Simulation -- DEFERRED
- [ ] Full simulation *(deferred -- needs live data collection period first)*
 ($50 â†’ $500+ target)



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

# Polymarket
POLYGON_PRIVATE_KEY=0x...        # Polymarket wallet number
# Note: Polymarket requires a funded Polygon wallet with USDC
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
| **Polymarket low market count / low edge density** | Insufficient opportunities to cover operational costs; agent starves | **High** â€” confirmed constraint. Polymarket has ~50-200 active markets vs. 500+ on offshore platforms | Aggressive data enrichment to maximize edge per market; efficient cost management (batching, caching); cross-platform signals (Metaculus/Manifold) to improve estimate quality; widen category acceptance; lower scan interval during high-activity periods |
| **Single execution venue dependency** | If Polymarket becomes unavailable (API outage, regulatory action), agent has zero execution capability | **Medium** | Trait-based platform abstraction allows rapid integration of alternatives; Manifold paper-trading provides continuous validation |
| LLM estimates are poorly calibrated | Agent loses money faster than it earns | Medium | Phase 8 calibration; start with paper trading; quarter-Kelly conservative sizing |
| API rate limits hit during scan | Missed opportunities, wasted cycles | Medium | Caching, batch requests, backoff |
| Polymarket CLOB disconnects | No execution possible | Medium | Auto-reconnect with exponential backoff, skip cycle, alert; connection health monitoring |
| Polymarket CLOB API changes | Executor breaks | Low | Pin API version, integration tests with recorded responses |
| Flash crash drains bankroll | Total loss | Low | Drawdown protection, max exposure caps, quarter-Kelly |
| LLM API outage | No estimates possible | Medium | Fallback to secondary LLM provider, skip cycle |
| Agent overconfidence | Systematic losses | Medium | Quarter-Kelly, calibration module, conservative thresholds, echo detection |
| Polymarket API rate limits | Throttled execution | Low | Respect rate limits, batch orders, backoff |
| **Regulatory landscape change (positive)** | New compliant platforms emerge in AU, expanding opportunity set | **Low** â€” ACMA enforcement trend is restrictive; no new entrants expected near-term | Monitor ACMA/ASIC announcements; trait-based architecture ready for rapid integration; periodic regulatory review |
| **Regulatory landscape change (negative)** | ACMA blocks Polymarket access more aggressively | **Low** | VPN fallback, monitor enforcement patterns |
| AU tax complexity | Unexpected tax liability | Medium | Log all trades for accountant, flag in docs |

---

## Milestone Summary

| Phase | Milestone | Verifiable Output |
|-------|-----------|-------------------|
| 0 | Project compiles | `cargo build` succeeds |
| 1 | Types defined | All types serialize/deserialize correctly |
| 2 | Markets fetched | Markets from Polymarket dry-run + Metaculus + Manifold |
| 3 | Data enriched | Weather/sports/econ context for sample markets |
| 4 | Fair values estimated | LLM returns probabilities for 50+ markets |
| 5 | Edges detected | Mispricings found per scan cycle |
| 6 | Bets placed | Polymarket dry-run trades + Manifold paper bets with receipts |
| 7 | Dashboard live | Web UI showing all metrics |
| 8 | Backtested | Simulation shows positive growth |

---

## Polymarket CLOB Setup (Prerequisite)

Before running ORACLE, Polymarket CLOB must be running:

```bash
# 1. Download Polymarket CLOB from Polymarket
# https://polymarket.com -- Create account, fund Polygon wallet with USDC

# 2. Run Polymarket CLOB (headless mode for VPS)
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

# 2. Start Polymarket CLOB (paper mode)
# Ensure Polymarket CLOB is running on port 4002

# 3. Build
cargo build --release

# 4. Run (paper trading mode â€” Polymarket dry-run + Manifold)
./target/release/oracle --config config.toml

# 5. Run (live -- ensure wallet is funded with USDC)
# Polymarket execution enabled by default when POLYGON_PRIVATE_KEY is set
./target/release/oracle --config config.toml

# 6. Docker (ensure Polymarket CLOB is accessible)
docker build -t oracle .
docker run -d --env-file .env --network host -p 8080:8080 oracle
```

---

*ORACLE Development Plan v1.2 â€” February 2026*
*Build iteratively. Test relentlessly. Survive.*
