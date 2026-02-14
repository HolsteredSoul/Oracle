# ORACLE: Autonomous Prediction Market AI Agent

## Whitepaper v1.2

---

## Abstract

ORACLE (Optimized Risk-Adjusted Cross-platform Leveraged Engine) is a fully autonomous AI agent built in Rust that operates across prediction market and forecasting platforms — Interactive Brokers ForecastEx for real-money execution, Metaculus for crowd-sourced probability cross-references, and Manifold Markets for paper-trading validation — to detect mispricings, estimate fair-value probabilities via LLM reasoning, and place Kelly-criterion-sized bets. The agent self-funds its own operational costs (LLM inference, brokerage commissions, API calls), and terminates ("dies") if its bankroll reaches zero. The architecture is inspired by documented cases of turning $50 into ~$2,980 in 48 hours through systematic edge detection and disciplined risk management.

This whitepaper defines the agent's theory, architecture, risk framework, and operational model. The accompanying **Development Plan** provides the iterative build roadmap.

**Regulatory context**: This agent is designed for operation from Australia. As of February 2026, Interactive Brokers ForecastEx remains the sole viable real-money prediction market platform fully accessible to Australian residents. Polymarket is blocked by ACMA under the Interactive Gambling Act 2001 (enforcement since August 2025), Kalshi remains restricted for AU residents, and no new licensed prediction market platforms have emerged in Australia. The platform stack is chosen for full AU compliance given this confirmed regulatory landscape.

---

## 1. Problem Statement

Prediction markets are informationally efficient — but not perfectly so. Mispricings arise from:

- **Temporal lag**: Markets react slowly to breaking news, data releases (e.g., BOM/NOAA forecasts, injury reports), and financial signals.
- **Cognitive bias**: Human participants systematically over/underweight tail risks, round probabilities, and anchor to stale prices.
- **Fragmentation**: Different platforms and forecasting communities price the same underlying event differently. ForecastEx (institutional, regulated) diverges from Metaculus (crowd wisdom, no monetary skin-in-game) which diverges from Manifold (play-money, retail sentiment).
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
- ForecastEx current price: {forecastex_price}
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
| Weather  | 6%  | BOM/NOAA data is high-signal, fast-decaying |
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

### 2.4 Survival Mechanic

After each 10-minute cycle, the agent deducts operational costs:

| Cost Component | Estimated per Cycle | Per Day (144 cycles) |
|----------------|--------------------|--------------------|
| LLM inference (300 markets × ~500 tokens) | $0.10 - $0.35 | $14.40 - $50.40 |
| Data API calls (weather, sports, economics) | $0.01 - $0.05 | $1.44 - $7.20 |
| IB commissions (per executed trade) | $0.25 - $1.00 per trade | Variable |
| IB market data fees | ~$0.07 (prorated monthly) | ~$10/month |
| **Total estimated** | **$0.12 - $0.47** | **$16.00 - $68.00** |

If `balance <= 0` after cost deduction, the agent logs its final state and terminates. This creates genuine evolutionary pressure: the agent must generate returns exceeding its operational costs to survive.

**Cost optimization strategies:**
- Batch LLM calls (multiple markets per prompt)
- Cache data across markets sharing the same category
- Skip markets with insufficient liquidity (no edge worth the inference cost)
- Progressive scanning: quick filter → deep analysis only for candidates

---

## 3. Multi-Platform Architecture

### 3.1 Platform Comparison

| Feature | IB ForecastEx | Metaculus | Manifold |
|---------|--------------|-----------|----------|
| **Type** | Regulated exchange (IB) | Crowd forecasting | Play-money prediction |
| **Settlement** | USD (brokerage account) | Reputation points | Mana (play currency) |
| **Fees** | ~$0.25-$1.00/trade | Free | Free |
| **Liquidity** | Moderate, event-driven | N/A (no trading) | Variable, play-money |
| **API** | IB TWS API / Client Portal | REST API | REST API |
| **AU Access** | ✅ Full (via IB AU entity) | ✅ Full | ✅ Full (play-money) |
| **Edge opportunity** | Primary (real-money execution) | Reference only | Validation + signal |
| **Agent role** | Scan + Bet | Cross-reference | Paper trade + validate |

#### 3.1.1 Why Not Other Platforms?

As of February 2026, ForecastEx is the only broad-category, real-money event contract platform fully accessible and legally tradeable from Australia. The landscape is constrained by the following:

- **Polymarket**: Blocked in Australia by ACMA since August 2025 under the Interactive Gambling Act 2001. Not accessible without circumvention (which would be illegal).
- **Kalshi**: US-regulated (CFTC) but restricted for Australian residents. No AU entity, no ASIC regulation.
- **Betfair Australia**: Licensed Australian betting exchange (via Sportsbet/Flutter) that offers some novelty and politics markets. However, Betfair is structured as a traditional betting exchange (back/lay odds), not a binary event contract platform. Its event category depth is narrow (primarily sports, with limited politics and entertainment), market structures are incompatible with ORACLE's binary YES/NO contract model, and liquidity on non-sports markets is inconsistent. Betfair does not offer the economics, weather, or culture categories that ForecastEx covers. For these reasons, Betfair is not a suitable execution target for ORACLE, though its odds data could theoretically serve as an additional cross-reference signal for overlapping sports/politics markets in future versions.
- **No new entrants**: As of February 2026, no new ASIC-regulated or ACMA-compliant prediction market platforms have launched in Australia.

This regulatory reality makes ForecastEx not just the preferred choice but effectively the only viable real-money execution platform for ORACLE operating from Australia. The architectural decision to build around ForecastEx as the sole execution target — supplemented by Metaculus and Manifold for cross-referencing and validation — is fully validated by the current environment.

### 3.2 Cross-Platform Signal Aggregation

The agent triangulates fair value using signals from all three platforms:

1. **ForecastEx prices** — real money at stake, reflects institutional/informed opinion. Primary execution target.
2. **Metaculus community forecasts** — large forecaster base with tracked calibration. Strong Bayesian anchor, especially for science/tech/geopolitics.
3. **Manifold play-money prices** — fast-moving, high-volume sentiment indicator. Useful for breaking events where crowds react faster than formal forecasters.

When the same event appears across platforms, disagreement signals opportunity:
- ForecastEx YES = 0.40, Metaculus median = 0.55 → potential buy on ForecastEx.
- Metaculus and Manifold agree at 0.60, ForecastEx at 0.45 → strong buy signal.

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

### 3.4 Interactive Brokers Integration

The IB integration is the most critical component for real-money execution:

- **Connection**: TWS API via `ibapi` Rust crate or Client Portal REST API.
- **Account**: Standard IB Australia account (ASIC-regulated). KYC completed by user.
- **ForecastEx access**: Enabled via IB account settings. Event contracts on economics (CPI, Fed rates, GDP), weather, and culture.
- **Order types**: Limit orders preferred (better fill prices), market orders for urgent executions.
- **Position tracking**: Real-time P&L via IB API position/account callbacks.
- **Interest**: IB pays ~4% on idle cash, partially offsetting operational costs.
- **Exclusivity note (2026)**: ForecastEx is confirmed as the sole compliant real-money execution venue for ORACLE. All real-money trade execution flows through this single integration. Reliability and robustness of the IB connection are therefore mission-critical — see Risk Register in the Development Plan for mitigations.

---

## 4. Data Pipeline

### 4.1 Real-Time Data Sources

| Category | Source | Signal Type | Refresh Rate |
|----------|--------|-------------|-------------|
| Weather | BOM (AU), OpenWeatherMap, NOAA | Forecasts, alerts, actuals | 30 min |
| Sports | API-Sports, ESPN | Injuries, lineups, odds | 15 min |
| Economics | FRED, RBA, ABS | CPI, rates, employment | Daily |
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
    
    /// Cost per API call (for survival accounting)
    fn cost_per_call(&self) -> f64;
}
```

### 4.3 Data Flow

```
┌──────────────┐    ┌──────────────┐    ┌──────────────┐
│  IB/ForecastEx│    │   Metaculus   │    │   Manifold   │
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
          │  Trade Executor   │──── IB/ForecastEx (real money)
          │  (platform-aware) │──── Manifold (paper validation)
          └───────┬─────────┘
                  │
                  ▼
          ┌─────────────────┐
          │  Accountant      │
          │  (P&L, costs,    │
          │   survival)      │
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
| Minimum liquidity (ForecastEx) | 50 contracts / 24h | Yes |
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

For ForecastEx (order book-based):
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
- Agent status: `🟢 ALIVE` or `🔴 DIED`
- Current bankroll (AUD equivalent across platforms)
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
- IB commissions paid
- Daily burn rate estimate
- Cycles until bankruptcy at current rate (if no wins)
- Cost per profitable trade

**Risk Panel:**
- Current exposure by category
- Drawdown from peak
- Kelly multiplier in effect
- Correlation-adjusted exposure

### 7.2 Alerts

- Telegram/Discord webhook on: trade execution, balance milestones, survival warnings, agent death.

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

**Note**: IB TWS API requires a persistent connection to IB Gateway or TWS. This favours always-on VPS/local deployments over serverless. IB Gateway is headless and ideal for VPS.

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
survival_threshold = 0.0       # Die at $0
currency = "AUD"

[llm]
provider = "anthropic"         # "anthropic" | "openai" | "grok"
model = "claude-sonnet-4-20250514"
api_key_env = "ANTHROPIC_API_KEY"
max_tokens = 500
batch_size = 10                # Markets per LLM call

[platforms.forecastex]
enabled = true
ib_host = "127.0.0.1"
ib_port = 4002                 # IB Gateway paper=4002, live=4001
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
bom_enabled = true             # Australian Bureau of Meteorology
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

### 9.1 Australian Compliance (Confirmed February 2026)

The Australian prediction market and event contract landscape is highly constrained by the Interactive Gambling Act 2001, actively enforced by ACMA. ORACLE's platform selection reflects this reality:

- **Interactive Brokers ForecastEx**: ASIC-regulated (AFSL holder via IB Australia Pty Ltd). ForecastEx event contracts are offered through IB's regulated brokerage infrastructure. Fully compliant and accessible for AU residents with no geo-blocks or restrictions. As of February 2026, ForecastEx remains the only broad-category, real-money event contract platform legally available in Australia. It covers economics (CPI, Fed/RBA rates, GDP, unemployment), weather, culture, and select other categories — providing the category depth ORACLE requires.
- **Polymarket**: Blocked by ACMA since August 2025 under the Interactive Gambling Act 2001 as an unlicensed offshore gambling service. Australian ISPs are required to block access. Not used by ORACLE.
- **Kalshi**: US-regulated (CFTC-designated contract market) but restricted for Australian residents. No Australian entity, no ASIC regulation, no compliant access path for AU-based agents. Not used by ORACLE.
- **Betfair Australia**: Licensed Australian betting exchange operated under the Sportsbet umbrella (Flutter Entertainment). Offers limited novelty and politics markets alongside its core sports exchange. However, Betfair's market structure (back/lay fractional odds) is fundamentally different from binary event contracts (YES/NO at 0–1 prices), its non-sports category depth is shallow and inconsistent, and integration would require a separate API abstraction with different order semantics. Not suitable as an execution platform for ORACLE's binary contract strategy. Could theoretically provide supplementary cross-reference signals for overlapping markets in future versions.
- **No new entrants**: As of February 2026, no new ASIC-regulated or ACMA-compliant prediction market platforms have launched in Australia. The regulatory environment remains restrictive, with ACMA continuing aggressive enforcement against unlicensed offshore platforms.
- **Metaculus**: No monetary bets; used as a forecasting data source and cross-reference only. No regulatory concern.
- **Manifold**: Play-money only (Mana currency, no cash-out since March 2025). No regulatory concern.

**Summary**: ForecastEx is not merely the preferred platform — it is the sole viable real-money execution venue for an AU-based prediction market agent. ORACLE's architecture reflects this by treating ForecastEx as the single execution target while leveraging Metaculus and Manifold purely for informational edge (crowd forecasts and sentiment signals). This concentration on a single execution platform is a key architectural constraint and risk factor (see Section 10 and the Development Plan Risk Register).

### 9.2 General

- **LLM costs**: The agent pays for its own inference. If it can't generate alpha, it dies — natural selection for trading strategies.
- **No insider trading**: All data sources are public APIs. The agent has no access to non-public information.
- **Tax implications**: ForecastEx profits may be assessable as income or capital gains under AU tax law. Consult an accountant.
- **Regulatory monitoring**: Given the dynamic nature of AU gambling and financial services regulation, ORACLE operators should periodically monitor ACMA enforcement actions, ASIC licensing updates, and IB product announcements for any changes that could affect ForecastEx availability or introduce new compliant platforms.

---

## 10. Expected Performance Envelope

Based on simulation parameters:

| Scenario | Starting | 48h Target | Win Rate | Daily Burn |
|----------|---------|-----------|----------|-----------|
| Aggressive ($50 start) | $50 | $500-$3,000 | 68-72% | ~$20 |
| Conservative ($100 start) | $100 | $200-$800 | 65-70% | ~$25 |
| Survival test ($10 start) | $10 | Die or $50 | 60-65% | ~$15 |

**Key insight**: The agent needs to find ~2-3 high-edge bets per day to cover costs. ForecastEx has fewer markets than offshore platforms like Polymarket (~50-200 active vs. 500+), so edge density is lower but markets may be less efficiently priced by retail participants. Metaculus cross-references help compensate for the narrower market set.

**ForecastEx concentration risk (2026 note)**: With ForecastEx confirmed as the sole execution venue, ORACLE's opportunity set is bounded by ForecastEx's market catalog. This is the primary strategic constraint. Mitigations include aggressive data enrichment to extract maximum edge from available markets, efficient cost management to lower the survival burn rate, and cross-platform signal aggregation (Metaculus, Manifold) to improve estimate quality even when market count is limited. If ForecastEx expands its category offerings or if new compliant platforms emerge, ORACLE's trait-based architecture allows rapid integration without structural changes.

**ForecastEx market categories available:**
- Economics: CPI, Fed/RBA rates, GDP, unemployment, inflation
- Weather: Hurricane strength, temperature records, tornado counts
- Culture: Billboard 100, Oscars, Grammys, Emmy awards
- Sports: Event outcomes (via IB sports contracts if available)

---

## Appendix A: Glossary

- **Edge**: The difference between estimated fair value and market price.
- **Kelly criterion**: Optimal bet sizing formula that maximizes geometric growth rate.
- **Brier score**: Mean squared error of probabilistic predictions (lower = better; 0 = perfect, 0.25 = chance).
- **ForecastEx**: Interactive Brokers' prediction market exchange, offering event contracts that pay $1 if correct.
- **Mana**: Manifold Markets' play-money currency. No cash-out value.
- **BOM**: Australian Bureau of Meteorology.
- **ACMA**: Australian Communications and Media Authority (gambling/online content regulator).
- **ASIC**: Australian Securities and Investments Commission (financial services regulator).
- **AFSL**: Australian Financial Services Licence (held by IB Australia).

---

*ORACLE v1.2 — Last updated: February 2026*
*"The market can stay irrational longer than you can stay solvent — unless you're a bot."*
