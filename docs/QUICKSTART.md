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

### System Requirements

- **OS**: Linux, macOS, or Windows (WSL2 recommended)
- **RAM**: 512 MB minimum
- **Disk**: 500 MB (for Rust toolchain + build artifacts)
- **Network**: Stable internet connection

### Software

| Tool | Purpose | Install |
|------|---------|---------|
| **Rust** (stable, 2021 edition) | Build ORACLE | [rustup.rs](https://rustup.rs) |
| **Git** | Clone the repository | Your package manager |
| **Docker** (optional) | Container deployment | [docker.com](https://docs.docker.com/get-docker/) |

Install Rust if you haven't already:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

### API Keys You'll Need

| Key | Required For | Free Tier | Where to Get |
|-----|-------------|-----------|--------------|
| **Anthropic API key** | LLM probability estimates | Pay-per-use | [console.anthropic.com](https://console.anthropic.com/) |
| **FRED API key** | Economic data enrichment | Yes (free) | [fred.stlouisfed.org/docs/api](https://fred.stlouisfed.org/docs/api/api_key.html) |
| **NewsAPI key** | News sentiment data | Yes (free, 100 req/day) | [newsapi.org](https://newsapi.org/register) |
| OpenAI API key | Alternative LLM (optional) | Pay-per-use | [platform.openai.com](https://platform.openai.com/api-keys) |
| API-Sports key | Sports data (optional) | Yes (free, 100 req/day) | [api-sports.io](https://api-sports.io/) |

At minimum you need an **Anthropic API key** (or OpenAI) to run the agent. The data source keys improve estimate quality but are not strictly required — the agent falls back to keyword-based analysis without them.

---

## 2. Installation

```bash
# Clone the repository
git clone https://github.com/HolsteredSoul/Oracle.git
cd Oracle

# Build in release mode
cargo build --release
```

The binary is at `./target/release/oracle`.

Verify it compiled:

```bash
./target/release/oracle --help
```

---

## 3. Configuration

### 3.1 Environment Variables

Copy the template and fill in your keys:

```bash
cp .env.example .env
```

Edit `.env` with your API keys:

```bash
# --- Required for operation ---
ANTHROPIC_API_KEY=sk-ant-your-key-here

# --- Recommended (improves estimate quality) ---
FRED_API_KEY=your-fred-key-here
NEWS_API_KEY=your-newsapi-key-here

# --- Optional ---
OPENAI_API_KEY=sk-your-openai-key       # Alternative LLM
API_SPORTS_KEY=your-sports-key           # Sports data
```

### 3.2 Config File

The default `config.toml` ships with sensible defaults for trial mode. Key settings to review:

| Setting | Default | Meaning |
|---------|---------|---------|
| `agent.scan_interval_secs` | `600` | Scan every 10 minutes |
| `agent.initial_bankroll` | `100.0` | Starting simulated bankroll |
| `llm.provider` | `"anthropic"` | LLM provider (`"anthropic"` or `"openai"`) |
| `llm.model` | `"claude-sonnet-4-20250514"` | Model to use for estimates |
| `risk.kelly_multiplier` | `0.25` | Quarter-Kelly (conservative) |
| `risk.max_bet_pct` | `0.06` | Max 6% of bankroll per bet |
| `dashboard.port` | `8080` | Web dashboard port |

You can leave `config.toml` as-is for your first run.

---

## 4. Trial Run — Simulated Money

This section walks you through running ORACLE with zero financial risk. The agent scans real markets and makes real probability estimates, but all bets are simulated.

### 4.1 How Simulation Works

ORACLE has two layers of simulated operation:

1. **Dry-run mode** (default) — The agent scans Polymarket, Metaculus, and Manifold for live markets, enriches them with real data, estimates probabilities via your LLM, detects mispricings, and sizes bets using Kelly criterion — but logs trades instead of placing them. The internal bankroll is tracked in memory.

2. **Manifold paper trading** — Optionally, bets are placed on Manifold Markets using play-money (Mana). This gives you real execution feedback without financial risk.

### 4.2 Start a Dry Run

No wallet setup needed. Just your LLM API key:

```bash
# Ensure .env has at least ANTHROPIC_API_KEY set
# Then run:
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
INFO oracle: Fresh start bankroll=100.0
INFO oracle: Entering main loop. Press Ctrl+C to stop.
INFO oracle: Starting cycle cycle=1
INFO oracle: Markets scanned count=47
INFO oracle: [DRY RUN] Would place bet market_id=0xabc side=Yes amount=$4.20 edge=12.3%
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

Once you're satisfied with trial performance, this section covers switching to real-money execution on Polymarket.

### 5.1 Overview

ORACLE executes real-money trades on **Polymarket**, which operates on the **Polygon** blockchain using **USDC** (a USD-pegged stablecoin). The flow is:

```
Your bank account
      ↓  (fiat on-ramp)
Crypto exchange (e.g. Coinbase, Kraken, Binance)
      ↓  (withdraw USDC to Polygon)
Polygon wallet (MetaMask or similar)
      ↓  (deposit to Polymarket)
Polymarket account
      ↓  (ORACLE places trades via API)
Profit / loss settled in USDC
```

### 5.2 Step 1 — Create a Polygon Wallet

You need a wallet that can hold USDC on the Polygon network.

**Option A: MetaMask (recommended for beginners)**

1. Install [MetaMask](https://metamask.io/) browser extension
2. Create a new wallet — **save your seed phrase securely offline**
3. Add the Polygon network:
   - Network name: `Polygon Mainnet`
   - RPC URL: `https://polygon-rpc.com`
   - Chain ID: `137`
   - Currency: `MATIC`
   - Explorer: `https://polygonscan.com`
4. Copy your wallet address (starts with `0x...`)

**Option B: Dedicated wallet (recommended for production)**

For unattended operation, generate a dedicated wallet for ORACLE:

```bash
# Using cast (from the foundry toolkit: https://getfoundry.sh)
cast wallet new
```

This outputs a private key and address. Store the private key securely — you'll need it for the `POLYGON_PRIVATE_KEY` env var.

> **Security**: Use a dedicated wallet for ORACLE with only the funds you're willing to risk. Never use your main wallet.

### 5.3 Step 2 — Fund the Wallet with USDC

You need USDC on the Polygon network in your wallet.

**Method A: Via a centralised exchange**

1. Buy USDC on a crypto exchange (Coinbase, Kraken, Binance, etc.)
2. Withdraw USDC to your Polygon wallet address
   - Select **Polygon** as the withdrawal network (not Ethereum — fees are much lower on Polygon)
   - Double-check the destination address
3. You also need a small amount of **POL** (previously MATIC) for gas fees (~$0.50 worth is plenty — Polygon transactions cost fractions of a cent)

**Method B: Bridge from Ethereum**

If you already have USDC on Ethereum:

1. Go to [portal.polygon.technology/bridge](https://portal.polygon.technology/bridge)
2. Bridge USDC from Ethereum to Polygon
3. Wait for confirmation (~15-30 minutes)

**Recommended starting amount**: $50-$200 USDC. The agent needs enough bankroll to cover operational costs (LLM inference ~$15-50/day) while building up returns.

### 5.4 Step 3 — Connect Wallet to Polymarket

1. Go to [polymarket.com](https://polymarket.com)
2. Click **Sign Up / Log In**
3. Connect your wallet (MetaMask or WalletConnect)
4. Deposit USDC from your wallet into your Polymarket account
5. Navigate to **Settings → API Keys** and generate API credentials (if required for CLOB access)

### 5.5 Step 4 — Configure ORACLE for Live Trading

Add your wallet's private key to `.env`:

```bash
# .env — add this line
POLYGON_PRIVATE_KEY=0xyour_private_key_here
```

> **Never commit `.env` to version control.** It's already in `.gitignore`.

Update `config.toml` to reflect your real starting bankroll:

```toml
[agent]
initial_bankroll = 100.0    # Match your actual USDC deposit
currency = "USD"
```

### 5.6 Step 5 — Launch with Real Funds

```bash
./target/release/oracle --config config.toml
```

The agent will now:

1. Scan Polymarket, Metaculus, and Manifold for live markets
2. Enrich candidates with weather, sports, economics, and news data
3. Send enriched markets to the LLM for probability estimation
4. Detect mispricings where the LLM estimate diverges from market price
5. Size bets using Kelly criterion (capped at 6% of bankroll)
6. Execute trades on Polymarket via the CLOB API
7. Track costs, P&L, and update the dashboard
8. Repeat every 10 minutes

### 5.7 Risk Controls (Built-in)

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

### 5.8 Withdrawing Funds

To withdraw profits:

1. Withdraw USDC from Polymarket back to your Polygon wallet (via Polymarket UI)
2. Send USDC from your Polygon wallet to your exchange
3. Sell USDC for fiat and withdraw to your bank

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

Enable JSON logging for log aggregation:

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

Your `ANTHROPIC_API_KEY` (or `OPENAI_API_KEY`) is not set or empty. Check `.env` and ensure the key is valid.

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
- Use a cheaper model (e.g. `"claude-haiku-4-5-20251001"`)

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
