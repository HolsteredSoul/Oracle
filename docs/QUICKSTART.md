# ORACLE Quick Start Guide

From zero to autonomous prediction market agent — step by step.

---

## Table of Contents

1. [Prerequisites](#1-prerequisites)
2. [Installation](#2-installation)
3. [Configuration](#3-configuration)
4. [Trial Run — Simulated Money](#4-trial-run--simulated-money)
5. [Full Operation — Real Funds](#5-full-operation--real-funds)
6. [Monitoring](#6-monitoring)
7. [Troubleshooting](#7-troubleshooting)

---

## 1. Prerequisites

### What You'll Need

- **Windows 10/11, macOS, or Linux**
- **RAM**: 512 MB minimum
- **Disk**: 500 MB free space (for Rust toolchain + build files)
- **Internet connection**

### Step 1.1 — Install Rust

Rust is the programming language ORACLE is built in. You need it to compile and run the project.

1. Go to [rustup.rs](https://rustup.rs)
2. Download and run the installer for your platform — the defaults are fine
3. Once installed, **close and reopen** your terminal so the changes take effect
4. Verify it worked by running:

```bash
rustc --version
```

You should see something like `rustc 1.77.0 (...)`. If you do, Rust is ready.

### Step 1.2 — Install Git

Git lets you download the ORACLE source code.

1. Go to [git-scm.com/downloads](https://git-scm.com/downloads)
2. Download and run the installer for your platform — the defaults are fine
3. Verify it worked:

```bash
git --version
```

### Step 1.3 — API Keys You'll Need

API keys are like passwords that give ORACLE access to external services. Here's what you need:

| Key | Required? | What It's For | Where to Get It |
|-----|-----------|---------------|-----------------|
| **OpenRouter API key** | Required | The AI brain — routes to Claude, Grok, and other LLMs | [openrouter.ai/keys](https://openrouter.ai/keys) |
| **Betfair API app key** | For live trading | Real-money execution on Betfair Exchange | [developer.betfair.com](https://developer.betfair.com/) |
| **FRED API key** | Recommended | Economic data (US macro indicators) | [fred.stlouisfed.org/docs/api](https://fred.stlouisfed.org/docs/api/api_key.html) — free |
| **NewsAPI key** | Recommended | News headlines & sentiment | [newsapi.org/register](https://newsapi.org/register) — free (100 req/day) |
| Manifold API key | Optional | Paper-trading writes on Manifold | [manifold.markets](https://manifold.markets/) — in settings |
| API-Sports key | Optional | Live sports scores | [api-sports.io](https://api-sports.io/) — free (100 req/day) |

> **Minimum to get started:** You only need the **OpenRouter API key**. The others improve ORACLE's accuracy or enable live trading but aren't required for a first run.

---

## 2. Installation

Open a terminal and run these commands one at a time:

```bash
# Download the ORACLE source code
git clone https://github.com/HolsteredSoul/Oracle.git

# Navigate into the project folder
cd Oracle

# Compile ORACLE (this takes 1-3 minutes the first time)
cargo build --release
```

When the build finishes, the compiled program is saved at `target/release/oracle`.

Verify it compiled correctly:

```bash
./target/release/oracle --help
```

You should see a list of available options. If you do, the build succeeded.

---

## 3. Configuration

Before running ORACLE, you need to tell it your API keys. These are stored in a file called `.env` that lives in the project folder.

### Step 3.1 — Create Your .env File

From inside the `Oracle` folder:

```bash
# Create your .env file from the template
cp .env.example .env
```

Now open `.env` in any text editor and fill in your keys:

```
# --- Required ---
OPENROUTER_API_KEY=sk-or-your-key-here

# --- For live trading (Betfair) ---
BETFAIR_APP_KEY=your-betfair-app-key
BETFAIR_USERNAME=your-betfair-username
BETFAIR_PASSWORD=your-betfair-password

# --- Recommended (improves accuracy) ---
FRED_API_KEY=your-fred-key-here
NEWS_API_KEY=your-newsapi-key-here

# --- Optional ---
MANIFOLD_API_KEY=your-manifold-key
API_SPORTS_KEY=your-sports-key
```

Save and close the file.

> **Important:** Never share your `.env` file or commit it to GitHub — it contains your private API keys. It's already listed in `.gitignore` so Git will ignore it automatically.

### 3.2 Config File

The default `config.toml` ships with sensible defaults for trial mode. Key settings to review:

| Setting | Default | Meaning |
|---------|---------|---------|
| `agent.scan_interval_secs` | `600` | Scan every 10 minutes |
| `agent.initial_bankroll` | `100.0` | Starting simulated bankroll |
| `llm.provider` | `"openrouter"` | LLM provider (`"openrouter"` or `"anthropic"`) |
| `llm.model` | `"anthropic/claude-sonnet-4"` | Primary model for estimates |
| `llm.fallback_model` | `"x-ai/grok-4.1-fast"` | Fallback when primary fails |
| `risk.kelly_multiplier` | `0.25` | Quarter-Kelly (conservative) |
| `risk.max_bet_pct` | `0.06` | Max 6% of bankroll per bet |
| `dashboard.port` | `8080` | Web dashboard port |

You can leave `config.toml` as-is for your first run.

---

## 4. Trial Run — Simulated Money

This is the best way to start. ORACLE will scan real markets, make real probability estimates using AI, but **not place or risk any real money**. All bets are logged to screen only.

### 4.1 How the Trial Mode Works

By default, ORACLE runs in **dry-run mode**:

- It scans live markets on Manifold and Metaculus (and Betfair if configured)
- It pulls in real-world data (news, weather, sports scores, economic indicators)
- It sends market details to Claude 4 Sonnet (via OpenRouter) to estimate the true probability
- If the primary model fails, it automatically falls back to Grok-4.1-fast
- It detects mispricings and calculates bet sizes using the Kelly criterion
- **But it logs the trades instead of executing them** — no real money needed

You can also optionally enable **Manifold paper trading**, which places bets using play-money (called "Mana") on Manifold Markets — real execution, zero financial risk.

### 4.2 Start a Dry Run

Make sure your `.env` file has at least `OPENROUTER_API_KEY` set, then:

```bash
./target/release/oracle --config config.toml
```

You'll see the startup banner:

```
  ___  ____      _    ____ _     _____
 / _ \|  _ \    / \  / ___| |   | ____|
| | | | |_) |  / _ \| |   | |   |  _|
| |_| |  _ <  / ___ \ |___| |___| |___
 \___/|_| \_\/_/   \_\____|_____|_____|

  Optimized Risk-Adjusted Cross-platform Leveraged Engine
  v0.1.0 — Autonomous Agent
```

Followed by structured logs showing each cycle:

```
INFO oracle: ORACLE starting up agent_name=ORACLE-001 scan_interval_secs=600
INFO oracle: Using OpenRouter LLM provider model=anthropic/claude-sonnet-4 fallback=Some("x-ai/grok-4.1-fast")
INFO oracle: Fresh start bankroll=100.0
INFO oracle: Entering main loop. Press Ctrl+C to stop.
INFO oracle: Starting cycle cycle=1
INFO oracle: Markets scanned count=47
INFO oracle: [DRY RUN] Would place bet market_id=abc side=Yes amount=$4.20 edge=12.3%
INFO oracle: Cycle complete cycle=1 scanned=47 edges=3 bets=2 bankroll=$98.50
```

### 4.3 What to Watch During Trial

- **Markets scanned** — Should show 30-200+ markets per cycle depending on platform availability.
- **Edges found** — Mispricings detected. Expect 0-10 per cycle. Zero edges is normal when markets are efficient.
- **Bets (dry-run)** — Logged with `[DRY RUN]` prefix. Check that bet sizes look reasonable (typically 1-6% of bankroll).
- **Bankroll** — Tracks simulated balance. Deducts LLM inference costs as real operational overhead.
- **Dashboard** — Open `http://localhost:8080` in your browser to see the live web UI.

### 4.4 Trial Duration

Let the agent run for **at least 24-48 hours** before making decisions about live operation. This gives you enough cycles to:

- Verify LLM estimates are reasonable
- Check that edge detection thresholds aren't too aggressive or too conservative
- Confirm operational costs (LLM API spend) are within your budget
- Build familiarity with the dashboard and log output

### 4.5 Adjusting Parameters During Trial

Edit `config.toml` while the agent is stopped (Ctrl+C), then restart:

```toml
# More conservative — fewer but higher-conviction bets
[risk]
mispricing_threshold = 0.12    # Require 12% edge (default: 8%)
kelly_multiplier = 0.15        # Reduce bet sizing

# Faster/slower scanning
[agent]
scan_interval_secs = 300       # Every 5 minutes (more API cost)

# Use a cheaper primary model
[llm]
model = "x-ai/grok-4.1-fast"  # Cheaper but less accurate
```

### 4.6 Running with Docker (Simulated)

```bash
docker build -t oracle .
docker run -d \
  --name oracle-trial \
  --env-file .env \
  -p 8080:8080 \
  oracle
```

View logs:

```bash
docker logs -f oracle-trial
```

Stop:

```bash
docker stop oracle-trial
```

---

## 5. Full Operation — Real Funds

Once you're satisfied with trial performance, this section covers switching to real-money execution on Betfair Exchange.

### 5.1 Overview

ORACLE executes real-money trades on **Betfair Exchange**, one of the world's largest betting exchanges. The flow is:

```
Your bank account
      |  (deposit)
Betfair account (funded)
      |  (ORACLE places trades via REST API)
Profit / loss settled in your Betfair balance
      |  (withdraw)
Your bank account
```

### 5.2 Step 1 — Create a Betfair Account

1. Go to [betfair.com](https://www.betfair.com) and create an account
2. Complete identity verification as required
3. Deposit funds via bank transfer, card, or other payment methods

### 5.3 Step 2 — Get Betfair API Credentials

1. Go to [developer.betfair.com](https://developer.betfair.com/)
2. Register for a **Betfair API app key** (free for personal use)
3. You'll need:
   - **App key** — identifies your application
   - **Username & password** — your Betfair login credentials
   - Optional: SSL certificate for non-interactive login

### 5.4 Step 3 — Configure ORACLE for Live Trading

Add your Betfair credentials to `.env`:

```bash
# .env — add these lines
BETFAIR_APP_KEY=your-app-key-here
BETFAIR_USERNAME=your-betfair-username
BETFAIR_PASSWORD=your-betfair-password
```

> **Never commit `.env` to version control.** It's already in `.gitignore`.

Update `config.toml` to reflect your real starting bankroll:

```toml
[agent]
initial_bankroll = 100.0    # Match your actual Betfair deposit
currency = "GBP"            # Or AUD, EUR — match your Betfair account currency
```

### 5.5 Step 4 — Launch with Real Funds

```bash
./target/release/oracle --config config.toml
```

The agent will now:

1. Scan Betfair, Manifold, and Metaculus for live markets
2. Enrich candidates with weather, sports, economics, and news data
3. Send enriched markets to Claude 4 Sonnet (via OpenRouter) for probability estimation
4. Detect mispricings where the LLM estimate diverges from market price
5. Size bets using Kelly criterion (capped at 6% of bankroll)
6. Execute trades on Betfair via the REST API
7. Track costs, P&L, and update the dashboard
8. Repeat every 10 minutes

### 5.6 Risk Controls (Built-in)

ORACLE enforces multiple safety layers automatically:

| Protection | Default | Effect |
|-----------|---------|--------|
| Quarter-Kelly sizing | `kelly_multiplier = 0.25` | Bets are 1/4 of theoretically optimal — reduces variance by ~75% |
| Max single bet | `max_bet_pct = 0.06` | No bet exceeds 6% of bankroll |
| Max total exposure | `max_exposure_pct = 0.60` | At most 60% of bankroll at risk at once |
| Category exposure caps | 30% per category | No over-concentration in one domain |
| Drawdown-adjusted sizing | Automatic | Agent gets more conservative as bankroll drops |
| Survival halt | `survival_threshold = 0.0` | Agent stops if bankroll hits $0 |

**Drawdown protection tiers:**

| Bankroll vs Starting | Behaviour | Kelly Multiplier |
|---------------------|-----------|-----------------|
| > 200% | Aggressive growth | 0.35 |
| 100-200% | Normal | 0.25 |
| 50-100% | Conservative | 0.15 |
| 25-50% | Survival mode | 0.10 |
| < 25% | Ultra-conservative | 0.05 |

### 5.7 Adding IBKR Event Contracts (Optional)

For additional market coverage, you can optionally enable IBKR ForecastTrader event contracts:

1. Open an Interactive Brokers account with event contract permissions
2. Configure TWS/IB Gateway (paper port 4002, live port 4001)
3. Add credentials to `config.toml` under `[platforms.forecastex]`

Event contracts on IBKR are treated like options with YES/NO strikes.

### 5.8 Withdrawing Funds

To withdraw profits from Betfair:

1. Log in to your Betfair account
2. Navigate to **My Account > Withdraw**
3. Select your withdrawal method and amount
4. Funds typically arrive in 1-3 business days

---

## 6. Monitoring

### 6.1 Web Dashboard

Open `http://localhost:8080` (or your server's IP) to access the live dashboard.

The dashboard displays:

- **Status** — ALIVE / DIED / PAUSED with current bankroll
- **Performance** — P&L, win rate, Sharpe ratio, trade history
- **Activity** — Balance history chart, recent cycles, recent trades
- **Costs** — Cumulative LLM/data/commission costs and burn rate

It auto-refreshes every 30 seconds.

### 6.2 Logs

Structured logs are written to stdout. For production, pipe to a file:

```bash
./target/release/oracle --config config.toml 2>&1 | tee -a oracle.log
```

Enable JSON logging:

```bash
ORACLE_LOG_JSON=1 ./target/release/oracle --config config.toml
```

Adjust log verbosity:

```bash
RUST_LOG=oracle=debug ./target/release/oracle --config config.toml
```

### 6.3 State Persistence

Agent state is saved to `oracle_state.json` after every cycle. If the agent crashes or you stop it, it resumes from the last saved state on restart — no progress is lost.

---

## 7. Troubleshooting

### "No LLM API key configured — running in dry-run/scan-only mode"

Your `OPENROUTER_API_KEY` is not set or empty. Check `.env` and ensure the key is valid.

### "Primary model failed, falling back"

The primary model (Claude 4 Sonnet) is temporarily unavailable. The agent automatically falls back to Grok-4.1-fast. This is normal — check OpenRouter status if it persists.

### "Both primary and fallback models failed"

Both LLM models are unavailable. Check your OpenRouter API key and account balance at [openrouter.ai/activity](https://openrouter.ai/activity).

### No markets scanned

- Verify your internet connection
- Check that `platforms.metaculus.enabled` and `platforms.manifold.enabled` are `true` in `config.toml`
- Try increasing log verbosity: `RUST_LOG=oracle=debug`

### Zero edges found

This is normal when markets are efficiently priced. The agent will keep scanning every cycle. You can lower the threshold in `config.toml`:

```toml
[risk]
mispricing_threshold = 0.06    # Lower = more sensitive (but noisier)
```

### High LLM costs

- Reduce `llm.batch_size` in `config.toml` (fewer markets per LLM call)
- Increase `agent.scan_interval_secs` (scan less frequently)
- Switch to a cheaper primary model: `model = "x-ai/grok-4.1-fast"`
- Use a cheaper Claude variant: `model = "anthropic/claude-haiku-4"`

### Agent died (bankroll reached $0)

The agent halts when operational costs exceed its bankroll. Options:

1. Delete `oracle_state.json` to reset state and restart with a fresh bankroll
2. Increase `agent.initial_bankroll` in `config.toml`
3. Reduce costs by using a cheaper LLM model or longer scan intervals

### Docker: dashboard not accessible

Ensure you're mapping the port and using host networking:

```bash
docker run -d --env-file .env --network host oracle
# Or explicitly map the port:
docker run -d --env-file .env -p 8080:8080 oracle
```

---

*ORACLE Quick Start Guide — February 2026*
