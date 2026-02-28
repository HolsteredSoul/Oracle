# ORACLE: Autonomous Prediction Market AI Agent

## Whitepaper v2.0

---

## Abstract

ORACLE (Optimized Risk-Adjusted Cross-platform Leveraged Engine) is an autonomous AI agent built in Rust that operates across prediction market and forecasting platforms — Betfair Exchange for real-money execution, Manifold Markets for paper-trading validation and backtesting, Metaculus for crowd-sourced probability cross-references, and optionally IBKR ForecastTrader for secondary event-contract execution — to detect mispricings, estimate fair-value probabilities via LLM reasoning, and size positions using Kelly-criterion methodology. The agent tracks its operational costs (LLM inference, commissions, API calls) and halts when its bankroll is depleted. The architecture draws on research into systematic edge detection and disciplined risk management in prediction markets.

This whitepaper defines the agent's theory, architecture, risk framework, and operational model. The accompanying **Development Plan** provides the iterative build roadmap.

**LLM layer**: All LLM calls route through OpenRouter with a single API key. Claude 4 Sonnet is the primary model for best probabilistic reasoning and calibration; Grok-4.1-fast is the cheap/fast fallback with automatic failover.

**Execution venues**: Betfair Exchange is the primary live target. Manifold Markets handles all testing, paper trading, and backtesting at zero cost. IBKR ForecastTrader/Event Contracts is an optional secondary module.

---

## 1. Problem Statement

Prediction markets are informationally efficient — but not perfectly so. Mispricings arise from:

- **Temporal lag**: Markets react slowly to breaking news, data releases (e.g., weather forecasts, injury reports), and financial signals.
- **Cognitive bias**: Human participants systematically over/underweight tail risks, round probabilities, and anchor to stale prices.
- **Fragmentation**: Different platforms and forecasting communities price the same underlying event differently. Betfair (deep liquidity, real-money) diverges from Metaculus (crowd wisdom, no monetary skin-in-game) which diverges from Manifold (play-money, retail sentiment).
- **Liquidity asymmetry**: Thin markets offer outsized edges but require careful sizing.

An autonomous agent that continuously scans, estimates, cross-references, and bets can systematically harvest these edges faster and more consistently than manual traders.

---

## 2. Core Thesis

### 2.1 Fair-Value Estimation via LLM

The agent uses an LLM routed through OpenRouter to estimate the "true" probability of each market outcome. The primary model is Claude 4 Sonnet (best probabilistic reasoning and calibration), with automatic fallback to Grok-4.1-fast (cheap/fast) when the primary is unavailable.

The LLM receives:

1. **Market description** — the question, resolution criteria, deadline.
2. **Real-time data** — domain-specific signals fetched from external APIs (weather, sports, economics, news).
3. **Cross-platform reference** — Metaculus community forecasts and Manifold play-money prices as Bayesian anchors.
4. **Historical calibration** — the agent's own track record for self-correction over time.

**Sample LLM prompt:**

```
You are a calibrated probability estimator. Based on the following information,
estimate the probability (0.00 to 1.00) of the stated outcome occurring.

MARKET: "{market_question}"
RESOLUTION: "{resolution_criteria}"
DEADLINE: "{deadline}"

REAL-TIME DATA:
{data_payload}

CROSS-REFERENCE:
- Metaculus community forecast: {metaculus_prob} (N={metaculus_forecasters} forecasters)
- Betfair current price: {betfair_price}
- Manifold play-money price: {manifold_price}

INSTRUCTIONS:
- Reason step-by-step about key factors
- Account for base rates and reference classes
- Consider information that markets may have already priced in
- Identify what edge, if any, the real-time data provides
- Output your final estimate as a single float on the last line

PROBABILITY:
```

Every call returns a typed `Estimate` struct containing `probability`, `confidence` (0–100), `reasoning`, `sources`, and `uncertainty_flags`. When confidence is below 65, the agent triggers automatic deeper tool calls (search, news scrape) for agentic escalation.

### 2.2 Mispricing Detection

A mispricing exists when:

```
|LLM_estimate - market_price| > threshold
```

Default threshold: **8%** (0.08). Configurable per category:

| Category | Default Threshold | Rationale |
|----------|------------------|-----------|
| Weather  | 6%  | Weather data is high-signal, fast-decaying |
| Sports   | 8%  | Injury reports create clear but priced-in edges |
| Economics | 10% | Macro data is noisy, requires wider margin |
| Politics | 12% | Low-frequency, hard to estimate, high noise |

### 2.3 Kelly Criterion Position Sizing

When a mispricing is detected, the bet size follows a **fractional Kelly** approach:

```
edge = LLM_estimate - market_price    (for YES bets)
       market_price - LLM_estimate    (for NO bets)

odds = (1 / market_price) - 1         (decimal odds from market price)

kelly_fraction = edge / odds

bet_size = kelly_fraction * bankroll * kelly_multiplier
```

Where `kelly_multiplier` defaults to **0.25** (quarter-Kelly) for conservative growth, and bet size is capped at **1.5% per position** with **max 10–12% total exposure**.

**Why quarter-Kelly?** Full Kelly is theoretically optimal for geometric growth but assumes perfect edge estimation. Since LLM estimates have uncertainty, quarter-Kelly sacrifices ~50% of growth rate for ~75% reduction in variance — critical for survival.

### 2.4 Operational Cost Model

After each 10-minute cycle, the agent deducts operational costs:

| Cost Component | Estimated per Cycle | Per Day (144 cycles) |
|----------------|--------------------|--------------------|
| LLM inference via OpenRouter | $0.10 - $0.35 | $14.40 - $50.40 |
| Data API calls (weather, sports, economics) | $0.01 - $0.05 | $1.44 - $7.20 |
| Betfair commissions (per trade) | ~2-5% of profit | Variable |
| **Total estimated** | **$0.12 - $0.47** | **$16.00 - $58.00** |

If `balance <= 0` after cost deduction, the agent logs its final state and halts. This creates a natural feedback loop: the agent must generate returns exceeding its operational costs to continue running.

**Cost optimization strategies:**
- Batch LLM calls (multiple markets per prompt)
- Cache data across markets sharing the same category
- Skip markets with insufficient liquidity (no edge worth the inference cost)
- Progressive scanning: quick filter → deep analysis only for candidates
- Automatic fallback to cheaper model (Grok) when primary is unavailable

---

## 3. Multi-Platform Architecture

### 3.1 Platform Comparison

| Feature | Betfair Exchange | Manifold | Metaculus | IBKR ForecastTrader |
|---------|-----------------|----------|-----------|---------------------|
| **Type** | Betting exchange | Play-money prediction | Crowd forecasting | Event contracts |
| **Settlement** | GBP/AUD/EUR | Mana (play currency) | Reputation points | USD |
| **Fees** | 2-5% of profit | Free | Free | ~$0.50-$1.00/trade |
| **Liquidity** | Deep, sports-focused | Variable | N/A (no trading) | Thin |
| **API** | REST + streaming | REST API | REST API | TWS/Web API |
| **Edge opportunity** | Primary execution | Backtesting + paper trading | Reference only | Secondary execution |
| **Agent role** | Scan + Bet | Test + Validate + Backtest | Cross-reference | Optional bet |

### 3.2 Cross-Platform Signal Aggregation

The agent triangulates fair value using signals from all platforms:

1. **Betfair prices** — real money at stake, deep liquidity. Primary execution target.
2. **Metaculus community forecasts** — large forecaster base with tracked calibration. Strong Bayesian anchor, especially for science/tech/geopolitics.
3. **Manifold play-money prices** — fast-moving, high-volume sentiment indicator. Useful for breaking events where crowds react faster than formal forecasters.

When the same event appears across platforms, disagreement signals opportunity:
- Betfair YES = 0.40, Metaculus median = 0.55 → potential buy on Betfair.
- Metaculus and Manifold agree at 0.60, Betfair at 0.45 → strong buy signal.

### 3.3 Platform Abstraction (Trait-Based)

```rust
#[async_trait]
pub trait PredictionPlatform: Send + Sync {
    /// Fetch active markets from this platform
    async fn fetch_markets(&self) -> Result<Vec<Market>>;

    /// Place a bet on a specific market (no-op for read-only platforms)
    async fn place_bet(&self, market_id: &str, side: Side, amount: f64) -> Result<TradeReceipt>;

    /// Get current positions and P&L
    async fn get_positions(&self) -> Result<Vec<Position>>;

    /// Check available balance on this platform
    async fn get_balance(&self) -> Result<f64>;

    /// Platform-specific liquidity check
    async fn check_liquidity(&self, market_id: &str) -> Result<LiquidityInfo>;

    /// Whether this platform supports real-money execution
    fn is_executable(&self) -> bool;

    /// Platform name for logging
    fn name(&self) -> &str;
}
```

### 3.4 Betfair Exchange Integration

The Betfair integration is the most critical component for real-money execution:

- **Connection**: REST API for market data + order placement, streaming API for real-time odds.
- **Crate**: `betfair-rs` provides typed Rust bindings for Betfair's API.
- **Authentication**: App key + session token (username/password login or SSL certificate).
- **Market access**: Thousands of active markets across sports, politics, current affairs.
- **Order types**: Back (buy YES) and Lay (sell YES / buy NO) at specified odds.
- **Position tracking**: Via listCurrentOrders and listClearedOrders endpoints.
- **Streaming**: Real-time odds updates via Betfair's streaming API for fast reaction.

### 3.5 IBKR ForecastTrader Integration (Optional)

- **Connection**: TWS API or Web API.
- **Market access**: Event contracts (YES/NO binary options on outcomes).
- **Structure**: Treated like options with YES/NO strikes — uses existing options infrastructure.
- **Paper port**: 4002. Live port: 4001.
- **Status**: Optional secondary module, added after Betfair is stable.

---

## 4. Data Pipeline

### 4.1 Real-Time Data Sources

| Category | Source | Signal Type | Refresh Rate |
|----------|--------|-------------|-------------|
| Weather | Open-Meteo (free) | Forecasts, alerts, actuals | 30 min |
| Sports | API-Sports | Injuries, lineups, odds | 15 min |
| Economics | FRED | CPI, rates, employment | Daily |
| News | NewsAPI | Breaking events, sentiment | 10 min |

### 4.2 Data Provider Abstraction

```rust
#[async_trait]
pub trait DataProvider: Send + Sync {
    /// Category this provider covers
    fn category(&self) -> MarketCategory;

    /// Fetch relevant data for a market question
    async fn fetch_context(&self, market: &Market) -> Result<DataContext>;

    /// Cost per API call (for cost accounting)
    fn cost_per_call(&self) -> f64;
}
```

### 4.3 Data Flow

```
┌──────────────┐    ┌──────────────┐    ┌──────────────┐
│   Betfair    │    │   Metaculus   │    │   Manifold   │
│   Markets    │    │   Forecasts   │    │  Play-money  │
└──────┬───────┘    └──────┬───────┘    └──────┬───────┘
       │                   │                   │
       └───────────┬───────┘                   │
                   ▼                           │
         ┌─────────────────┐                   │
         │  Market Router   │◄─────────────────┘
         │  (dedup, merge,  │    cross-reference + sentiment
         │   match events)  │
         └───────┬─────────┘
                 │
                 ▼
         ┌─────────────────┐
         │  Data Enricher   │◄──── Weather / Sports / Econ APIs
         │  (per category)  │
         └───────┬─────────┘
                 │
                 ▼
         ┌─────────────────┐
         │  LLM Estimator   │◄──── OpenRouter → Claude / Grok
         │  (fair value)    │      (auto-fallback)
         └───────┬─────────┘
                 │
                 ▼
         ┌─────────────────┐
         │  Edge Detector   │
         │  (threshold +    │
         │   Kelly sizing)  │
         └───────┬─────────┘
                 │
                 ▼
         ┌─────────────────┐
         │  Trade Executor   │──── Betfair (real money)
         │  (platform-aware) │──── Manifold (paper validation)
         └───────┬─────────┘     IBKR (optional secondary)
                 │
                 ▼
         ┌─────────────────┐
         │  Accountant      │
         │  (P&L, costs,    │
         │   cost tracking) │
         └─────────────────┘
```

---

## 5. Risk Management Framework

### 5.1 Position Limits

| Parameter | Default | Configurable |
|-----------|---------|-------------|
| Max single bet | 1.5% of bankroll | Yes |
| Max exposure per category | 25% of bankroll | Yes |
| Max total exposure | 10-12% of bankroll | Yes |
| Minimum liquidity | Platform-dependent | Yes |
| Kelly multiplier | 0.25 (quarter-Kelly) | Yes |
| Max bets per cycle | 5 | Yes |

### 5.2 Drawdown Protection

The agent adapts its risk profile based on bankroll trajectory:

| Bankroll vs. Starting | Behavior | Kelly Multiplier |
|----------------------|----------|-----------------|
| > 200% | Aggressive growth | 0.35 |
| 100% - 200% | Normal | 0.25 |
| 50% - 100% | Conservative | 0.15 |
| 25% - 50% | Survival mode | 0.10 |
| < 25% | Ultra-conservative | 0.05 |

Auto-pause on excessive drawdown (configurable threshold, default 40% from peak).

### 5.3 Correlation Management

Markets are often correlated (e.g., "Will CPI exceed 3%?" and "Will the Fed cut rates?"). The agent:

- Groups related markets by keyword/category overlap.
- Limits aggregate exposure to correlated markets.
- Avoids double-counting the same edge across correlated bets.

### 5.4 Slippage Model

For Betfair Exchange (back/lay order book):
```
effective_price = market_price + slippage_estimate(amount, order_book_depth)
```

Bets are only placed if the edge exceeds `threshold + estimated_slippage + commission`.

---

## 6. LLM Calibration and Self-Improvement

### 6.1 Track Record Database

Every estimate is logged:

```sql
CREATE TABLE estimates (
    id INTEGER PRIMARY KEY,
    market_id TEXT,
    platform TEXT,
    question TEXT,
    llm_estimate REAL,
    market_price REAL,
    metaculus_forecast REAL,
    manifold_price REAL,
    actual_outcome INTEGER,  -- 0 or 1, filled on resolution
    timestamp TEXT,
    data_context TEXT,       -- JSON blob of data used
    llm_model TEXT,          -- e.g. "anthropic/claude-sonnet-4" or "x-ai/grok-4.1-fast"
    llm_provider TEXT        -- "openrouter"
);
```

### 6.2 Calibration Metrics

After sufficient history (50+ resolved markets), the agent computes:

- **Brier score**: Mean squared error of probability estimates.
- **Calibration curve**: Plot predicted vs. actual frequencies in probability bins.
- **Category-specific accuracy**: Which domains yield the best edges.
- **Model comparison**: Per-model Brier scores (Claude vs Grok) to validate primary model choice.
- **Overconfidence detection**: If estimates cluster at extremes (0.1 or 0.9) but actuals are closer to 0.3/0.7, the agent adjusts thresholds.

### 6.3 Adaptive Thresholds

```rust
// After 100+ resolved estimates in a category:
let brier = compute_brier_score(category_estimates);
if brier > 0.25 {
    // Poor calibration — widen threshold to require larger edges
    category_threshold *= 1.5;
} else if brier < 0.15 {
    // Good calibration — can trust tighter edges
    category_threshold *= 0.8;
}
```

---

## 7. Dashboard and Monitoring

### 7.1 Web Dashboard (axum)

Real-time dashboard accessible at `http://localhost:8080` showing:

**Header Panel:**
- Agent status: RUNNING or STOPPED
- Current bankroll
- Uptime and cycle count

**Performance Panel:**
- Total P&L (absolute and percentage)
- Win rate (% of resolved bets that were profitable)
- Sharpe ratio (annualized return / volatility of returns)
- Best and worst trades

**Activity Panel:**
- Balance history chart (using Chart.js)
- Recent trades table (time, market, side, size, price, P&L)
- Markets scanned per cycle
- Current open positions

**Cost Panel:**
- Cumulative LLM/data API costs (per-model breakdown)
- Betfair/IBKR commissions paid
- Daily burn rate estimate
- Estimated cycles remaining at current burn rate
- Cost per profitable trade

**Risk Panel:**
- Current exposure by category
- Drawdown from peak
- Kelly multiplier in effect

### 7.2 Alerts

- Telegram webhook on: trade execution, balance milestones, low-balance warnings, agent shutdown, model fallback events.

---

## 8. Deployment Model

### 8.1 Recommended Infrastructure

| Option | Cost/Month | Best For |
|--------|-----------|----------|
| DigitalOcean Droplet (1GB) | $6 | Production, simple |
| AWS EC2 t3.micro | $8 | Scalable, monitoring |
| Google Cloud Run | ~$5 (pay-per-use) | Serverless cycles |
| Raspberry Pi 5 | $0 (hardware owned) | Dev/testing |
| Local (tmux/systemd) | $0 | Development |

Docker + VPS-ready from day one. No persistent connection required for Betfair REST API.

### 8.2 Docker Deployment

```dockerfile
FROM rust:1.77-slim AS builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/oracle /usr/local/bin/
COPY config.toml /etc/oracle/
CMD ["oracle", "--config", "/etc/oracle/config.toml"]
```

### 8.3 Configuration

```toml
[agent]
name = "ORACLE-001"
scan_interval_secs = 600       # 10 minutes
initial_bankroll = 100.0       # AUD
survival_threshold = 0.0       # Halt at $0
currency = "AUD"

[llm]
provider = "openrouter"        # "openrouter" | "anthropic" | "openai"
model = "anthropic/claude-sonnet-4"
fallback_model = "x-ai/grok-4.1-fast"
api_key_env = "OPENROUTER_API_KEY"
max_tokens = 1024
batch_size = 10                # Markets per LLM call

[platforms.betfair]
enabled = true
app_key_env = "BETFAIR_APP_KEY"
username_env = "BETFAIR_USERNAME"
password_env = "BETFAIR_PASSWORD"

[platforms.metaculus]
enabled = true                 # Read-only cross-reference

[platforms.manifold]
enabled = true                 # Paper trading + backtesting + sentiment signal

[platforms.forecastex]
enabled = false                # IBKR optional secondary
ib_host = "127.0.0.1"
ib_port = 4002
ib_client_id = 1
account_id_env = "IB_ACCOUNT_ID"

[risk]
mispricing_threshold = 0.08
kelly_multiplier = 0.25
max_bet_pct = 0.06
max_exposure_pct = 0.60
min_liquidity_contracts = 50

[risk.category_thresholds]
weather = 0.06
sports = 0.08
economics = 0.10
politics = 0.12

[data_sources]
openweathermap_key_env = "OWM_API_KEY"
api_sports_key_env = "API_SPORTS_KEY"
fred_api_key_env = "FRED_API_KEY"
coingecko = { enabled = true }

[dashboard]
enabled = true
port = 8080

[alerts]
telegram_bot_token_env = "TG_BOT_TOKEN"
telegram_chat_id_env = "TG_CHAT_ID"
```

---

## 9. Legal and Regulatory Context

### 9.1 Platform Overview

ORACLE integrates with the following platforms:

- **Betfair Exchange**: World's largest betting exchange. Deep liquidity across sports, politics, current affairs. Back/lay market structure.
- **Manifold Markets**: Play-money only (Mana currency). Used for backtesting, paper trading, and sentiment signals.
- **Metaculus**: No monetary bets; used as a forecasting data source and cross-reference only.
- **IBKR ForecastTrader**: Regulated event contracts. Optional secondary execution venue.

**Summary**: Betfair Exchange is ORACLE's primary execution venue, offering the deepest liquidity and broadest market coverage for its target categories. Manifold provides zero-cost testing and backtesting. Metaculus provides crowd forecast signals.

### 9.2 General

- **LLM costs**: The agent tracks its own inference costs via OpenRouter. If it cannot generate positive returns, it halts — a natural feedback mechanism for strategy viability.
- **Data sources**: All data sources are public APIs. The agent does not use non-public information.
- **Tax implications**: Users should consult a qualified tax professional regarding the tax treatment of prediction market and betting exchange activity in their jurisdiction.
- **Regulatory compliance**: Users are responsible for ensuring compliance with applicable laws and regulations in their jurisdiction before operating the agent.

---

## 10. Expected Performance Envelope

Based on simulation parameters:

| Scenario | Starting | 48h Target | Win Rate | Daily Burn |
|----------|---------|-----------|----------|-----------|
| Aggressive ($50 start) | $50 | $500-$3,000 | 68-72% | ~$20 |
| Conservative ($100 start) | $100 | $200-$800 | 65-70% | ~$25 |
| Minimal ($10 start) | $10 | Halt or $50 | 60-65% | ~$15 |

**Key insight**: The agent needs to find ~2-3 high-edge bets per day to cover costs. Betfair Exchange has thousands of active markets across sports, politics, and current affairs, providing ample opportunity for edge detection. Manifold's historical data (since 2021) enables thorough backtesting before live deployment. Metaculus cross-references further improve estimate quality.

**Betfair market categories available:**
- Sports: Football, horse racing, tennis, cricket, basketball, and dozens more
- Politics: Elections, referendums, leadership contests
- Current Affairs: Economic events, weather, entertainment awards
- Specials: Financial markets, TV shows, awards ceremonies

---

## Appendix A: Glossary

- **Edge**: The difference between estimated fair value and market price.
- **Kelly criterion**: Optimal bet sizing formula that maximizes geometric growth rate.
- **Brier score**: Mean squared error of probabilistic predictions (lower = better; 0 = perfect, 0.25 = chance).
- **OpenRouter**: Unified LLM API that routes to multiple providers (Anthropic, xAI, OpenAI, etc.) with a single API key.
- **Back/Lay**: Betfair's market structure — "Back" = buy (bet for), "Lay" = sell (bet against).
- **Mana**: Manifold Markets' play-money currency.
- **betfair-rs**: Rust crate providing typed bindings for Betfair's REST and streaming APIs.

---

*ORACLE v2.0 — Last updated: February 2026*
*"The market can stay irrational longer than you can stay solvent — unless you're a bot."*
