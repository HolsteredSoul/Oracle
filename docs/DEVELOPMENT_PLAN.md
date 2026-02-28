# ORACLE: Iterative Development Plan

## Version 2.0 — Build Roadmap

---

## Overview

This document defines the phased development plan for ORACLE. Each phase produces a working, testable artifact. Phases are designed so that earlier phases are prerequisites for later ones, and the agent becomes progressively more capable with each iteration.

**Total estimated effort**: 10–12 weeks for full live trading capability.

**Language**: Rust (stable toolchain, 2021 edition)
**Async runtime**: Tokio
**Key crates**: reqwest, serde, axum, sqlx, chrono, tracing, betfair-rs (planned)

**Target platforms**:
- **Betfair Exchange** — real-money execution (primary live target). Deep liquidity across sports, politics, and current affairs.
- **Manifold Markets** — all testing, paper trading, and backtesting (zero cost, unlimited markets).
- **Metaculus** — crowd forecast cross-reference (read-only).
- **IBKR ForecastTrader** — optional secondary real-money module (event contracts treated like options with YES/NO strikes).

**LLM stack**:
- **OpenRouter** — single API key for all models.
- **Primary**: `anthropic/claude-sonnet-4` (best probabilistic reasoning and calibration).
- **Fallback**: `x-ai/grok-4.1-fast` (cheap/fast, automatic failover).

**Core tech stack (minimal & focused)**:
- Rust + Tokio + sqlx (SQLite)
- betfair-rs (for Betfair REST + streaming, planned)
- OpenRouter client + reqwest (Manifold & enrichment)
- rust_decimal, tracing, serde, chrono
- Optional: tungstenite for extra streaming if needed

---

## Completed Phases (v1.x)

### Phase 0: Project Scaffolding — COMPLETE

**Completed 2026-02-14**

- [x] Initialize Cargo workspace
- [x] Define module structure
- [x] Implement config loading from TOML (`config.toml`)
- [x] Set up structured logging with `tracing` + `tracing-subscriber`
- [x] Implement graceful shutdown (Ctrl+C handler)
- [x] Create `.env` template for secrets
- [x] Create `Dockerfile` for deployment
- [x] Write `README.md` with setup instructions

### Phase 1: Core Types and Platform Trait — COMPLETE

**Completed 2026-02-14** (76 unit tests)

- [x] Define `Market`, `Side`, `TradeReceipt`, `Position`, `LiquidityInfo`, `MarketCategory` types
- [x] Define `PredictionPlatform` trait
- [x] Define `DataProvider` trait
- [x] Define `LlmEstimator` trait
- [x] Implement `Display` / `Debug` for all types
- [x] Write unit tests for type serialization/deserialization

### Phase 2: Platform Integrations (Scanning) — PARTIAL

**Status**: Metaculus (2B), Manifold (2C), and Market Router (2D) complete. Betfair and IBKR adapters planned for Phase 3 (new roadmap).

#### 2B: Metaculus Scanner — COMPLETE (2026-02-14)
- [x] REST API client (`https://www.metaculus.com/api2/`)
- [x] Paginated scanning ordered by forecaster count
- [x] Category classification via slugs + title keywords
- [x] 24 unit tests

#### 2C: Manifold Scanner — COMPLETE (2026-02-14)
- [x] Full `PredictionPlatform` trait impl including bet placement, balance checking, liquidity checking
- [x] Multi-sort scanning with deduplication
- [x] 17 unit tests

#### 2D: Market Router — COMPLETE (2026-02-17)
- [x] Concurrent platform fetching
- [x] Word-overlap fuzzy matching (Jaccard + containment)
- [x] Priority scoring (cross-refs, liquidity, centrality)
- [x] 20 unit tests

### Phase 3: Data Enrichment Pipeline — COMPLETE

**Completed 2026-02-21** (40 unit tests)

- [x] Weather: Open-Meteo (free, global)
- [x] Sports: Keyword-based extraction + API-Sports integration
- [x] Economics: FRED API (9 keyword groups, 20+ series)
- [x] News: NewsAPI with sentiment scoring
- [x] Enrichment Orchestrator: Category routing, TTL cache, cost tracking

### Phase 4: LLM Integration — COMPLETE

**Completed 2026-02-21 / 2026-02-28** (30 unit tests)

#### 4A: Anthropic Claude — COMPLETE
- [x] Full Anthropic Messages API client with calibration prompts
- [x] Response parsing, cost tracking, echo detection
- [x] Exponential backoff retry on rate limits
- [x] 18 unit tests

#### 4B: OpenAI GPT-4 — COMPLETE
- [x] OpenAI Chat Completions client as alternative
- [x] Reuses Anthropic parsing utilities
- [x] 3 unit tests

#### 4C: OpenRouter (unified endpoint) — COMPLETE (2026-02-28)
- [x] Single API key routes to all models via OpenRouter
- [x] Primary model: `anthropic/claude-sonnet-4`
- [x] Fallback model: `x-ai/grok-4.1-fast` with automatic failover
- [x] Per-model cost tracking with dynamic cost tables
- [x] OpenRouter-specific headers (HTTP-Referer, X-Title)
- [x] Config support: `provider = "openrouter"` with `fallback_model` field
- [x] Wired into main.rs with provider dispatch
- [x] 9 unit tests

#### 4D: Batch Estimation — COMPLETE
- [x] Multi-market batch prompts with per-market fallback
- [x] Probability clamping [0.01, 0.99]

### Phase 5: Strategy Engine — COMPLETE

**Completed 2026-02-21** (32 unit tests)

- [x] Edge detection with category-specific thresholds
- [x] Kelly criterion sizing (fractional, commission-adjusted)
- [x] Risk management (exposure limits, category caps, drawdown halt)
- [x] Strategy orchestrator with decision logging

### Phase 6: Execution & Survival Loop — COMPLETE

**Completed 2026-02-21** (16 unit tests)

- [x] Dry-run execution mode
- [x] Manifold paper executor
- [x] Accountant module (cost tracking, P&L, survival checks)
- [x] Main async loop with state persistence and graceful shutdown

### Phase 7: Dashboard & Monitoring — COMPLETE

**Completed 2026-02-21** (18 unit tests)

- [x] Axum 0.7 web server with CORS
- [x] 6 JSON API endpoints + health check
- [x] Single-page HTML dashboard with auto-refresh and Chart.js

### Phase 8: Calibration & Backtesting — COMPLETE

**Completed 2026-02-21** (19 unit tests)

- [x] Historical backtester (strategy replay engine)
- [x] Calibration module (Brier scores, per-category breakdown, auto-diagnosis)

---

## New Roadmap (v2.0 — Betfair Migration)

### Phase 0 (New): Foundations & Credentials (3–5 days)

**Goal**: Set up all API credentials, add dependencies, implement base adapters, verify connectivity.

- [x] OpenRouter key + integration with Claude 4 Sonnet + Grok-4.1-fast fallback
- [x] Config updated: `provider = "openrouter"`, `fallback_model` support
- [ ] Set up Betfair API app key + session token flow
- [ ] Create Manifold account + API key (for writes)
- [ ] Add dependency: `betfair-rs` (Betfair REST + streaming)
- [ ] Extend `PredictionPlatform` trait with `scan_markets()`, `get_odds()`, `place_order()`, `get_balance()`
- [ ] Docker + env config that lets you flip between manifold / betfair / ibkr with one flag
- **Milestone**: "Hello world" that lists 10 live markets from both Manifold and Betfair.

### Phase 1 (New): Backtester & Simulation Engine (Weeks 2–3)

**Goal**: Prove positive expectancy on historical data before risking real money.

- [ ] Download Manifold historical data dumps (markets + bets + resolutions since 2021)
- [ ] Build full backtester that replays every resolved market through LLM + strategy logic
- [ ] Implement fractional Kelly (start at 0.25x), portfolio correlation, max 1.5% per position, max 10–12% total exposure, auto-pause on drawdown
- [ ] Add SQLite tables for every simulated trade, resolved outcome, and calibration score
- [ ] Output: Weekly P&L report + Brier score calibration chart
- [ ] Target: Prove positive expectancy on 300+ historical markets before moving forward
- **Milestone**: Backtester runs end-to-end and prints "+EV confirmed".

### Phase 2 (New): LLM Brain with Strict Output (Weeks 4–5)

**Goal**: Structured LLM output, agentic escalation, calibration validation.

- [ ] Every call returns typed `Estimate` struct (`prob_yes`, `confidence 0-100`, `reasoning`, `sources`, `uncertainty_flags`)
- [ ] Agentic escalation: confidence < 65 triggers automatic deeper tool calls (search, news scrape)
- [ ] Run the real LLM against 400–500 historical markets and measure accuracy
- [ ] Default model: `anthropic/claude-sonnet-4`. One-line fallback to `x-ai/grok-4.1-fast`
- **Milestone**: Calibration table in SQLite showing edge is real.

### Phase 3 (New): Data Enrichment + Platform Adapters (Week 6)

**Goal**: Full platform adapters for Betfair and Manifold, with enrichment pipeline.

- [ ] Async parallel enrichment (news, weather, sports APIs) with caching
- [ ] Implement full `ManifoldPlatform` (reqwest to `https://api.manifold.markets`)
- [ ] Implement `BetfairPlatform` using betfair-rs (market catalogue, odds streaming, order placement)
- [ ] Basic skeleton for IBKR (TWS/Web API contract discovery — event contracts as options with YES/NO strikes)
- **Milestone**: Switch platforms with one config line and the engine still runs.

### Phase 4 (New): Paper Trading Mode (Weeks 7–8)

**Goal**: 24/7 autonomous paper trading loop on Manifold.

- [ ] Full autonomous cycle: scan → enrich → LLM → decide → "execute" (paper)
- [ ] Telegram alerts on every interesting market and simulated trade
- [ ] Live dashboard (Axum + HTMX) showing positions and P&L
- [ ] Run minimum 3–4 weeks with detailed decision logs
- **Milestone**: Positive paper P&L + alert quality and log readability confirmed.

### Phase 5 (New): Live Trading Ramp-up (Week 9+)

**Goal**: Real-money execution on Betfair with safety controls.

- [ ] Flip config to Betfair real mode (start tiny)
- [ ] Daily manual review for first 10–14 days
- [ ] Automated weekly performance + recalibration report
- [ ] Kill-switch and dry-run flag always active
- [ ] Once stable, add IBKR module for extra event-contract markets
- [ ] Ongoing: Add new market categories as capabilities expand

---

## Module Structure

```
oracle/
├── Cargo.toml
├── config.toml
├── .env.example
├── Dockerfile
├── README.md
├── docs/
│   ├── WHITEPAPER.md
│   ├── DEVELOPMENT_PLAN.md
│   └── QUICKSTART.md
├── src/
│   ├── main.rs                 # Entry point, async main loop
│   ├── config.rs               # TOML config + env var resolution
│   ├── types.rs                # Shared types (Market, Side, Trade, etc.)
│   ├── platforms/
│   │   ├── mod.rs              # PredictionPlatform trait
│   │   ├── manifold.rs         # Manifold (paper trading + sentiment)
│   │   ├── metaculus.rs        # Metaculus (read-only cross-reference)
│   │   ├── forecastex.rs       # IBKR ForecastTrader (optional secondary)
│   │   └── polymarket.rs       # Polymarket (stub)
│   ├── data/
│   │   ├── mod.rs              # DataProvider trait
│   │   ├── weather.rs          # Open-Meteo (free, global)
│   │   ├── sports.rs           # API-Sports
│   │   ├── economics.rs        # FRED
│   │   └── news.rs             # NewsAPI
│   ├── llm/
│   │   ├── mod.rs              # LlmEstimator trait
│   │   ├── openrouter.rs       # OpenRouter (primary — unified endpoint)
│   │   ├── anthropic.rs        # Anthropic direct (fallback)
│   │   ├── openai.rs           # OpenAI direct (fallback)
│   │   └── grok.rs             # Grok (stub)
│   ├── strategy/
│   │   ├── mod.rs              # Strategy orchestrator
│   │   ├── edge.rs             # Mispricing detection
│   │   ├── kelly.rs            # Kelly criterion sizing
│   │   └── risk.rs             # Risk manager
│   ├── engine/
│   │   ├── mod.rs              # Main scan-estimate-bet loop
│   │   ├── scanner.rs          # Multi-platform market scanner
│   │   ├── enricher.rs         # Data enrichment pipeline
│   │   ├── executor.rs         # Trade execution with retries
│   │   └── accountant.rs       # Cost tracking + survival check
│   ├── storage/
│   │   └── mod.rs              # JSON state persistence
│   ├── dashboard/
│   │   ├── mod.rs              # Axum web server
│   │   └── routes.rs           # API endpoints
│   └── backtest/
│       ├── mod.rs
│       ├── runner.rs           # Strategy replay engine
│       └── calibration.rs      # Brier scores + calibration
└── tests/
    ├── integration/
    │   ├── mock_platform.rs
    │   └── simulation.rs
    └── unit/
        ├── kelly_tests.rs
        └── edge_tests.rs
```

---

## Environment Variables

```bash
# .env.example

# LLM (one key for all models)
OPENROUTER_API_KEY=sk-or-...

# Betfair Exchange (primary execution)
BETFAIR_APP_KEY=...
BETFAIR_USERNAME=...
BETFAIR_PASSWORD=...

# IBKR (optional secondary)
IB_ACCOUNT_ID=...

# Manifold (paper trading writes)
MANIFOLD_API_KEY=...

# Data Sources
OWM_API_KEY=...               # OpenWeatherMap (optional)
API_SPORTS_KEY=...
FRED_API_KEY=...              # Federal Reserve Economic Data
NEWS_API_KEY=...

# Alerting
TG_BOT_TOKEN=...              # Telegram bot
TG_CHAT_ID=...                # Telegram chat ID

# Database
DATABASE_URL=sqlite://oracle.db
```

---

## Testing Strategy

### Unit Tests (Every Phase)

| Module | Tests |
|--------|-------|
| `types.rs` | Serialization roundtrips, display formatting |
| `kelly.rs` | Edge cases: 0% edge, 100% edge, negative edge, large bankroll, tiny bankroll |
| `edge.rs` | Threshold detection, category-specific thresholds, boundary conditions |
| `risk.rs` | Position limits, drawdown multiplier, correlation blocking |
| `openrouter.rs` | Client construction, model costs, fallback logic |

### Integration Tests

| Test | Description |
|------|-------------|
| `mock_platform.rs` | Full cycle with deterministic mock platform returning known markets |
| `simulation.rs` | 48-hour simulation with historical or generated data |

### Property-Based Tests

- Kelly bet size is always <= max_bet_pct × bankroll
- Kelly bet size is always >= 0
- Risk manager never approves a bet that would exceed exposure limits
- Agent always terminates when balance reaches 0

---

## Definition of Done (Per Phase)

Each phase is complete when:

1. All tasks are checked off
2. `cargo build` succeeds with no warnings
3. `cargo test` passes all unit and integration tests
4. Key functionality is demonstrated via a CLI command or test
5. State is persisted correctly (if applicable)
6. Costs are tracked accurately (if applicable)
7. Documentation is updated

---

## Milestone Summary

| Phase | Milestone | Verifiable Output |
|-------|-----------|-------------------|
| 0 (v1) | Project compiles | `cargo build` succeeds |
| 1 (v1) | Types defined | All types serialize/deserialize correctly |
| 2 (v1) | Markets fetched | Manifold + Metaculus markets scanned |
| 3 (v1) | Data enriched | Weather/sports/econ context for sample markets |
| 4 (v1) | Fair values estimated | LLM returns probabilities via OpenRouter |
| 5 (v1) | Edges detected | Mispricings found per scan cycle |
| 6 (v1) | Bets placed | Dry-run trades + Manifold paper bets |
| 7 (v1) | Dashboard live | Web UI showing all metrics |
| 8 (v1) | Backtested | Calibration module + replay engine |
| 0 (v2) | Credentials ready | OpenRouter + Betfair + Manifold API keys configured |
| 1 (v2) | Backtester proven | +EV confirmed on 300+ historical markets |
| 2 (v2) | LLM calibrated | Calibration table showing real edge |
| 3 (v2) | Adapters ready | Betfair + Manifold platform switching works |
| 4 (v2) | Paper trading stable | 3-4 weeks positive paper P&L |
| 5 (v2) | Live trading | Real-money Betfair execution + IBKR secondary |

---

## Risk Register

| Risk | Impact | Likelihood | Mitigation |
|------|--------|------------|-----------|
| LLM estimates poorly calibrated | Agent loses money faster than it earns | Medium | Phase 8 calibration; start with paper trading; quarter-Kelly conservative sizing |
| Betfair API rate limits hit during scan | Missed opportunities, wasted cycles | Medium | Caching, batch requests, backoff; betfair-rs handles streaming efficiently |
| Betfair API changes or outage | No execution possible | Low | Trait-based abstraction; IBKR as secondary venue; Manifold for continuous validation |
| Single execution venue dependency | If Betfair unavailable, zero execution | Medium | IBKR ForecastTrader as secondary module; Manifold paper-trading continuous |
| LLM API outage (OpenRouter) | No estimates possible | Medium | Automatic fallback from Claude to Grok; skip cycle on total failure |
| Flash crash drains bankroll | Total loss | Low | Drawdown protection, max exposure caps, quarter-Kelly |
| Agent overconfidence | Systematic losses | Medium | Quarter-Kelly, calibration module, conservative thresholds, echo detection |
| High operational costs vs edge | Agent starves | Medium | OpenRouter cost tracking; cheap fallback model; batching + caching |

---

## Quick Start (After All Phases)

```bash
# 1. Clone and build
git clone https://github.com/HolsteredSoul/Oracle.git
cd Oracle
cp .env.example .env
# Fill in API keys in .env

# 2. Build
cargo build --release

# 3. Run (paper trading mode — Manifold + dry-run)
./target/release/oracle --config config.toml

# 4. Run (live — ensure Betfair account is funded)
# Set BETFAIR_APP_KEY, BETFAIR_USERNAME, BETFAIR_PASSWORD in .env
./target/release/oracle --config config.toml

# 5. Docker
docker build -t oracle .
docker run -d --env-file .env --network host -p 8080:8080 oracle
```

---

*ORACLE Development Plan v2.0 — February 2026*
*Build iteratively. Test relentlessly. Survive.*
