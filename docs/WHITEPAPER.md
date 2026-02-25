# ORACLE: Autonomous Prediction Market AI Agent

## Whitepaper v1.2

---

## Abstract

ORACLE (Optimized Risk-Adjusted Cross-platform Leveraged Engine) is an autonomous AI agent built in Rust that operates across prediction market and forecasting platforms — Polymarket for execution via CLOB orders, Metaculus for crowd-sourced probability cross-references, and Manifold Markets for paper-trading validation — to detect mispricings, estimate fair-value probabilities via LLM reasoning, and size positions using Kelly-criterion methodology. The agent tracks its operational costs (LLM inference, commissions, API calls) and halts when its bankroll is depleted. The architecture draws on research into systematic edge detection and disciplined risk management in prediction markets.

This whitepaper defines the agent's theory, architecture, risk framework, and operational model. The accompanying **Development Plan** provides the iterative build roadmap.

**Execution venue**: Polymarket is the primary execution venue, offering liquidity across politics, economics, crypto, sports, culture, and more. Trading is conducted via the Polymarket CLOB (Central Limit Order Book) API on Polygon, settling in USDC. Metaculus and Manifold serve as cross-reference signals for crowd forecasts and play-money sentiment.

---

## 1. Problem Statement

Prediction markets are informationally efficient — but not perfectly so. Mispricings arise from:

- **Temporal lag**: Markets react slowly to breaking news, data releases (e.g., weather forecasts, injury reports), and financial signals.
- **Cognitive bias**: Human participants systematically over/underweight tail risks, round probabilities, and anchor to stale prices.
- **Fragmentation**: Different platforms and forecasting communities price the same underlying event differently. Polymarket (deep liquidity, real-money) diverges from Metaculus (crowd wisdom, no monetary skin-in-game) which diverges from Manifold (play-money, retail sentiment).
- **Liquidity asymmetry**: Thin markets offer outsized edges but require careful sizing.

An autonomous agent that continuously scans, estimates, cross-references, and bets can systematically harvest these edges faster and more consistently than manual traders.

---

## 2. Core Thesis

### 2.1 Fair-Value Estimation via LLM

The agent uses an LLM (Claude, GPT-4, or Grok — configurable) to estimate the "true" probability of each market outcome. The LLM receives:

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
- Polymarket current price: {polymarket_price}
- Manifold play-money price: {manifold_price}

INSTRUCTIONS:
- Reason step-by-step about key factors
- Account for base rates and reference classes
- Consider information that markets may have already priced in
- Identify what edge, if any, the real-time data provides
- Output your final estimate as a single float on the last line

PROBABILITY:
```

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

Where `kelly_multiplier` defaults to **0.25** (quarter-Kelly) for conservative growth, and bet size is capped at **6% of bankroll** maximum.

**Why quarter-Kelly?** Full Kelly is theoretically optimal for geometric growth but assumes perfect edge estimation. Since LLM estimates have uncertainty, quarter-Kelly sacrifices ~50% of growth rate for ~75% reduction in variance — critical for survival.

### 2.4 Operational Cost Model

After each 10-minute cycle, the agent deducts operational costs:

| Cost Component | Estimated per Cycle | Per Day (144 cycles) |
|----------------|--------------------|--------------------|
| LLM inference (300 markets × ~500 tokens) | $0.10 - $0.35 | $14.40 - $50.40 |
| Data API calls (weather, sports, economics) | $0.01 - $0.05 | $1.44 - $7.20 |
| Polymarket fees (per trade) | ~2% of proceeds | Variable |
| Polygon gas fees | ~$0.01 per transaction | Negligible |
| **Total estimated** | **$0.12 - $0.47** | **$16.00 - $68.00** |

If `balance <= 0` after cost deduction, the agent logs its final state and halts. This creates a natural feedback loop: the agent must generate returns exceeding its operational costs to continue running.

**Cost optimization strategies:**
- Batch LLM calls (multiple markets per prompt)
- Cache data across markets sharing the same category
- Skip markets with insufficient liquidity (no edge worth the inference cost)
- Progressive scanning: quick filter → deep analysis only for candidates

---

## 3. Multi-Platform Architecture

### 3.1 Platform Comparison

| Feature | Polymarket | Metaculus | Manifold |
|---------|--------------|-----------|----------|
| **Type** | Prediction exchange | Crowd forecasting | Play-money prediction |
| **Settlement** | USDC (on-chain) | Reputation points | Mana (play currency) |
| **Fees** | ~$0.25-$1.00/trade | Free | Free |
| **Liquidity** | Moderate, event-driven | N/A (no trading) | Variable, play-money |
| **API** | CLOB REST + Gamma REST | REST API | REST API |
| **Edge opportunity** | Primary execution | Reference only | Validation + signal |
| **Agent role** | Scan + Bet | Cross-reference | Paper trade + validate |

#### 3.1.1 Platform Selection Rationale

Polymarket is the largest prediction market by liquidity and market breadth, operating on Polygon with USDC settlement. Alternative platforms were evaluated:

- **Kalshi**: US-regulated (CFTC), narrower market selection and lower liquidity than Polymarket.
- **Betfair**: Betting exchange structured as back/lay odds, not binary contracts. Narrow category depth, incompatible market structure for binary event contract strategies.
- **ForecastEx (IB)**: Thin markets, low liquidity. Available as optional fallback via Phase 2A.

Polymarket is the primary execution venue due to its superior liquidity, market breadth, and 24/7 availability. The trait-based architecture allows adding additional platforms as needed.

### 3.2 Cross-Platform Signal Aggregation

The agent triangulates fair value using signals from all three platforms:

1. **Polymarket prices** - real money at stake, deep liquidity. Primary execution target.
2. **Metaculus community forecasts** — large forecaster base with tracked calibration. Strong Bayesian anchor, especially for science/tech/geopolitics.
3. **Manifold play-money prices** — fast-moving, high-volume sentiment indicator. Useful for breaking events where crowds react faster than formal forecasters.

When the same event appears across platforms, disagreement signals opportunity:
- Polymarket YES = 0.40, Metaculus median = 0.55 -> potential buy on Polymarket.
- Metaculus and Manifold agree at 0.60, Polymarket at 0.45 -> strong buy signal.

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

### 3.4 Polymarket Integration

The Polymarket integration is the most critical component for real-money execution:

- **Connection**: Gamma REST API (market data, no auth) + CLOB REST API (trading, HMAC-SHA256 auth).
- **Account**: Polygon wallet with USDC. Connect via MetaMask or derive API keys from private key.
- **Market access**: 500+ active markets across politics, economics, crypto, sports, culture, weather.
- **Order types**: Limit orders preferred (better fill prices), market orders for urgent executions.
- **Position tracking**: Via CLOB API positions endpoint and on-chain token balances.
- **Interest**: USDC on Polygon. Conditional tokens (ERC-1155) for YES/NO positions.
- **Order types**: Limit (GTC, GTD) and market (FOK, FAK) orders via CLOB. Batch support up to 15 orders per request.

---

## 4. Data Pipeline

### 4.1 Real-Time Data Sources

| Category | Source | Signal Type | Refresh Rate |
|----------|--------|-------------|-------------|
| Weather | OpenWeatherMap, NOAA | Forecasts, alerts, actuals | 30 min |
| Sports | API-Sports, ESPN | Injuries, lineups, odds | 15 min |
| Economics | FRED, World Bank | CPI, rates, employment | Daily |
| News | NewsAPI, RSS feeds | Breaking events, sentiment | 10 min |
| Financial | Yahoo Finance, CoinGecko | Prices, yields, volatility | 5 min |

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
│  Polymarket│    │   Metaculus   │    │   Manifold   │
│   Markets     │    │   Forecasts   │    │  Play-money  │
└──────┬────────┘    └──────┬────────┘    └──────┬────────┘
       │                    │                    │
       └────────────┬───────┘                    │
                    ▼                            │
          ┌─────────────────┐                    │
          │  Market Router   │◄───────────────────┘
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
          │  LLM Estimator   │◄──── Claude / GPT-4 / Grok
          │  (fair value)    │
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
          │  Trade Executor   │──── Polymarket (real money)
          │  (platform-aware) │──── Manifold (paper validation)
          └───────┬─────────┘
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
| Max single bet | 6% of bankroll | Yes |
| Max exposure per market | 10% of bankroll | Yes |
| Max exposure per category | 30% of bankroll | Yes |
| Max total exposure | 60% of bankroll | Yes |
| Minimum liquidity (Polymarket) | 50 contracts / 24h | Yes |
| Kelly multiplier | 0.25 (quarter-Kelly) | Yes |

### 5.2 Drawdown Protection

The agent adapts its risk profile based on bankroll trajectory:

| Bankroll vs. Starting | Behavior | Kelly Multiplier |
|----------------------|----------|-----------------|
| > 200% | Aggressive growth | 0.35 |
| 100% - 200% | Normal | 0.25 |
| 50% - 100% | Conservative | 0.15 |
| 25% - 50% | Survival mode | 0.10 |
| < 25% | Ultra-conservative | 0.05 |

### 5.3 Correlation Management

Markets are often correlated (e.g., "Will CPI exceed 3%?" and "Will the Fed cut rates?"). The agent:

- Groups related markets by keyword/category overlap.
- Limits aggregate exposure to correlated markets.
- Avoids double-counting the same edge across correlated bets.

### 5.4 Slippage Model

For Polymarket (CLOB order book):
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
    llm_model TEXT
);
```

### 6.2 Calibration Metrics

After sufficient history (50+ resolved markets), the agent computes:

- **Brier score**: Mean squared error of probability estimates.
- **Calibration curve**: Plot predicted vs. actual frequencies in probability bins.
- **Category-specific accuracy**: Which domains yield the best edges.
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
- Agent status: `🟢 RUNNING` or `🔴 STOPPED`
- Current bankroll (USD equivalent across platforms)
- Uptime and cycle count

**Performance Panel:**
- Total P&L (absolute and percentage)
- Win rate (% of resolved bets that were profitable)
- Sharpe ratio (annualized return / volatility of returns)
- Best and worst trades

**Activity Panel:**
- Balance history chart (log-scale, using plotters or Chart.js)
- Recent trades table (time, market, side, size, price, P&L)
- Markets scanned per cycle
- Current open positions

**Cost Panel:**
- Cumulative API/inference costs
- Polymarket fees paid
- Daily burn rate estimate
- Estimated cycles remaining at current burn rate
- Cost per profitable trade

**Risk Panel:**
- Current exposure by category
- Drawdown from peak
- Kelly multiplier in effect
- Correlation-adjusted exposure

### 7.2 Alerts

- Telegram/Discord webhook on: trade execution, balance milestones, low-balance warnings, agent shutdown.

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

**Note**: Polymarket CLOB API is stateless REST, so ORACLE can run anywhere with internet access. No persistent connection required.

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
initial_bankroll = 100.0       # USD
survival_threshold = 0.0       # Halt at $0
currency = "USD"

[llm]
provider = "anthropic"         # "anthropic" | "openai" | "grok"
model = "claude-sonnet-4-20250514"
api_key_env = "ANTHROPIC_API_KEY"
max_tokens = 500
batch_size = 10                # Markets per LLM call

[platforms.polymarket]
enabled = true
ib_host = "127.0.0.1"
# wallet_key_env = POLYGON_PRIVATE_KEY
ib_client_id = 1
account_id_env = "IB_ACCOUNT_ID"

[platforms.metaculus]
enabled = true                 # Read-only cross-reference

[platforms.manifold]
enabled = true                 # Play-money validation + sentiment signal

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
noaa_enabled = true            # NOAA weather data
api_sports_key_env = "API_SPORTS_KEY"
fred_api_key_env = "FRED_API_KEY"
coingecko = { enabled = true } # Free tier

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

- **Polymarket**: Prediction market on Polygon blockchain. Largest by liquidity and market breadth (500+ active markets). Trades settle in USDC via on-chain conditional tokens.
- **ForecastEx (IB)**: Regulated, thin markets. Available as optional fallback (Phase 2A).
- **Metaculus**: No monetary bets; used as a forecasting data source and cross-reference only.
- **Manifold**: Play-money only (Mana currency). Used for validation and sentiment signals.

**Summary**: Polymarket is ORACLE's primary execution venue, offering the deepest liquidity and broadest market coverage. Metaculus and Manifold provide informational signals via crowd forecasts and sentiment data.

### 9.2 General

- **LLM costs**: The agent tracks its own inference costs. If it cannot generate positive returns, it halts — a natural feedback mechanism for strategy viability.
- **Data sources**: All data sources are public APIs. The agent does not use non-public information.
- **Tax implications**: Users should consult a qualified tax professional regarding the tax treatment of prediction market activity in their jurisdiction.
- **Regulatory compliance**: Users are responsible for ensuring compliance with applicable laws and regulations in their jurisdiction before operating the agent.

---

## 10. Expected Performance Envelope

Based on simulation parameters:

| Scenario | Starting | 48h Target | Win Rate | Daily Burn |
|----------|---------|-----------|----------|-----------|
| Aggressive ($50 start) | $50 | $500-$3,000 | 68-72% | ~$20 |
| Conservative ($100 start) | $100 | $200-$800 | 65-70% | ~$25 |
| Minimal ($10 start) | $10 | Halt or $50 | 60-65% | ~$15 |

**Key insight**: The agent needs to find ~2-3 high-edge bets per day to cover costs. Polymarket has 500+ active markets with deep liquidity, providing ample opportunity for edge detection. Metaculus cross-references further improve estimate quality.

**Market breadth advantage**: Polymarket's 500+ active markets across diverse categories provide a much larger opportunity set than any regulated alternative. Combined with cross-platform signal aggregation (Metaculus, Manifold), this gives ORACLE strong coverage for systematic edge detection.

**Polymarket market categories available:**
- Economics: CPI, central bank rates, GDP, unemployment, inflation
- Weather: Hurricane strength, temperature records, tornado counts
- Culture: Billboard 100, Oscars, Grammys, Emmy awards
- Sports: NFL, NBA, soccer, tennis, MMA, cricket, and more

---

## Appendix A: Glossary

- **Edge**: The difference between estimated fair value and market price.
- **Kelly criterion**: Optimal bet sizing formula that maximizes geometric growth rate.
- **Brier score**: Mean squared error of probabilistic predictions (lower = better; 0 = perfect, 0.25 = chance).
- **Polymarket**: Prediction market on Polygon, offering binary outcome markets that pay $1 USDC if correct.
- **Mana**: Manifold Markets' play-money currency.
- **CLOB**: Central Limit Order Book (Polymarket's trading engine).
- **USDC**: USD-pegged stablecoin used for Polymarket settlement on Polygon.

---

*ORACLE v1.2 — Last updated: February 2026*
*"The market can stay irrational longer than you can stay solvent — unless you're a bot."*
