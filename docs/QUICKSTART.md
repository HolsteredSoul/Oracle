# ORACLE Quick Start Guide

**Welcome!** This guide will walk you through setting up ORACLE from scratch — even if you've never used a command line or built software before. Every step includes an explanation of *what* you're doing and *why*.

---

## Table of Contents

1. [What Is ORACLE?](#1-what-is-oracle)
2. [What You'll Need (Prerequisites)](#2-what-youll-need-prerequisites)
3. [Step-by-Step Setup](#3-step-by-step-setup)
4. [Configuration — Telling ORACLE How to Behave](#4-configuration--telling-oracle-how-to-behave)
5. [Your First Run — Simulated Mode (No Real Money)](#5-your-first-run--simulated-mode-no-real-money)
6. [Going Live — Real Funds on Polymarket](#6-going-live--real-funds-on-polymarket)
7. [Monitoring — Keeping an Eye on Things](#7-monitoring--keeping-an-eye-on-things)
8. [Troubleshooting — Common Issues and Fixes](#8-troubleshooting--common-issues-and-fixes)
9. [Glossary](#9-glossary)

---

## 1. What Is ORACLE?

ORACLE (**O**ptimized **R**isk-**A**djusted **C**ross-platform **L**everaged **E**ngine) is an autonomous AI agent that trades on **prediction markets** — websites where people bet on the outcomes of real-world events (elections, weather, sports, etc.).

Here's what ORACLE does, in plain English:

1. **Scans** prediction markets for open questions (e.g., "Will it rain in Sydney tomorrow?")
2. **Gathers data** from news sources, weather services, economic databases, and sports APIs
3. **Asks an AI** (like Claude or GPT-4) to estimate the true probability of each event
4. **Compares** the AI's estimate to the market's current price — if the market says 40% but the AI says 60%, that's a potential opportunity
5. **Places bets** (or simulates them) using a mathematically sound sizing formula called the Kelly criterion
6. **Repeats** this cycle every 10 minutes, 24/7

You can run ORACLE in **simulation mode** (no real money) to see how it performs before risking anything.

---

## 2. What You'll Need (Prerequisites)

### Your Computer

ORACLE runs on most modern computers:

- **Operating system**: macOS, Linux, or Windows (with WSL2 — see below)
- **RAM**: 512 MB free (most computers have far more than this)
- **Disk space**: About 500 MB for the tools and compiled program
- **Internet**: A stable connection (ORACLE needs to reach online APIs)

### Software to Install

You need two programs installed before you can build ORACLE. Don't worry — we'll walk through installing each one in [Step-by-Step Setup](#3-step-by-step-setup).

| Software | What It Is | Why ORACLE Needs It |
|----------|-----------|-------------------|
| **Git** | A tool for downloading and tracking code | To download ORACLE's source code from GitHub |
| **Rust** | A programming language and its build tools | ORACLE is written in Rust, so you need Rust's tools to compile it into a program your computer can run |

> **What does "compile" mean?** Source code is human-readable text. Your computer can't run text files directly — it needs a *compiled binary* (a machine-readable program). The Rust compiler translates ORACLE's source code into an executable program.

### API Keys — Your Access Passes

ORACLE connects to several online services. Each service requires an **API key** — a unique password that identifies you and authorizes access. Think of it like a library card: you sign up, they give you a card (key), and you present it every time you borrow a book (make a request).

| API Key | What It's For | Required? | Cost | Where to Sign Up |
|---------|--------------|-----------|------|-----------------|
| **Anthropic** | The AI that estimates probabilities | **Yes** (or use OpenAI instead) | Pay-per-use (~$0.003/request) | [console.anthropic.com](https://console.anthropic.com/) |
| **FRED** | U.S. economic data (GDP, unemployment, etc.) | Recommended | Free | [fred.stlouisfed.org](https://fred.stlouisfed.org/docs/api/api_key.html) |
| **NewsAPI** | Recent news articles for context | Recommended | Free (100 requests/day) | [newsapi.org](https://newsapi.org/register) |
| OpenAI | Alternative AI provider | Optional (if not using Anthropic) | Pay-per-use | [platform.openai.com](https://platform.openai.com/api-keys) |
| API-Sports | Sports data | Optional | Free (100 requests/day) | [api-sports.io](https://api-sports.io/) |

**At minimum**, you need one AI key (Anthropic *or* OpenAI). The data source keys (FRED, NewsAPI) make ORACLE smarter but aren't strictly required.

> **How to get an Anthropic API key (step by step):**
> 1. Go to [console.anthropic.com](https://console.anthropic.com/)
> 2. Click **Sign Up** and create an account (email + password)
> 3. Once logged in, go to **API Keys** in the left sidebar
> 4. Click **Create Key**
> 5. Give it a name (e.g., "oracle") and click **Create**
> 6. **Copy the key immediately** — it starts with `sk-ant-` and you won't be able to see it again
> 7. Save it somewhere safe (a password manager is ideal)

---

## 3. Step-by-Step Setup

### 3.1 Open Your Terminal

The **terminal** (also called "command line" or "shell") is a text-based way to interact with your computer. Instead of clicking icons, you type commands.

**How to open it:**

- **macOS**: Press `Cmd + Space`, type "Terminal", and press Enter
- **Linux**: Press `Ctrl + Alt + T` (on most distributions), or search for "Terminal" in your app menu
- **Windows**: You need **WSL2** (Windows Subsystem for Linux), which lets you run Linux inside Windows:
  1. Open **PowerShell as Administrator** (right-click Start menu > "Windows PowerShell (Admin)")
  2. Run: `wsl --install`
  3. Restart your computer when prompted
  4. After restart, a Linux terminal (Ubuntu) will open — use this for all following steps

> **Tip**: Throughout this guide, lines starting with `$` are commands you type into the terminal. Don't type the `$` itself — it just represents the prompt. Lines *without* `$` show the expected output.

### 3.2 Install Git

**Git** is a tool that downloads code from GitHub (where ORACLE's code is hosted) and tracks changes to files.

**Check if you already have it:**

```bash
$ git --version
git version 2.43.0    # Any version 2.x is fine
```

If you see a version number, you're all set — skip to the next section.

**If you don't have Git:**

- **macOS**: Run `xcode-select --install` in your terminal. A popup will appear — click "Install".
- **Linux (Ubuntu/Debian)**: Run `sudo apt update && sudo apt install git`
- **Linux (Fedora)**: Run `sudo dnf install git`
- **Windows (WSL)**: Run `sudo apt update && sudo apt install git`

### 3.3 Install Rust

**Rust** is the programming language ORACLE is written in. Installing Rust gives you `cargo` — Rust's build tool that compiles source code into a runnable program.

**Run this command:**

```bash
$ curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

> **What does this command do?** It downloads the official Rust installer (`rustup`) from the internet and runs it. The flags (`--proto`, `--tlsv1.2`, `-sSf`) ensure the download is secure and silent (no progress bars cluttering your screen).

The installer will ask you a question:

```
1) Proceed with standard installation (default — just press enter)
2) Customize installation
3) Cancel installation
```

**Press Enter** to accept the default (option 1). This is the right choice for most people.

After installation completes, you need to load Rust's tools into your current terminal session:

```bash
$ source "$HOME/.cargo/env"
```

> **What does `source` do?** It reloads your terminal's configuration so it knows where to find the Rust tools you just installed. You only need to do this once — future terminal windows will find Rust automatically.

**Verify it worked:**

```bash
$ rustc --version
rustc 1.93.1 (6b0048927 2025-06-24)    # Version may differ — any 1.70+ is fine

$ cargo --version
cargo 1.93.1 (509dd861e 2025-06-18)
```

If you see version numbers, Rust is installed correctly.

### 3.4 Download ORACLE

Now you'll use Git to download ("clone") ORACLE's source code from GitHub:

```bash
$ git clone https://github.com/HolsteredSoul/Oracle.git
```

This creates a folder called `Oracle` in your current directory containing all of ORACLE's code.

**Navigate into the project folder:**

```bash
$ cd Oracle
```

> **What does `cd` mean?** It stands for "change directory." You're telling the terminal to move into the `Oracle` folder so that future commands run inside it.

**Verify you're in the right place:**

```bash
$ ls
Cargo.lock  Cargo.toml  Dockerfile  config.toml  docs  src  tests  ...
```

You should see files like `Cargo.toml`, `config.toml`, and folders like `src` and `docs`.

### 3.5 Set Up Your API Keys

ORACLE reads your API keys from a file called `.env` (short for "environment"). This file is private — it never gets shared or uploaded.

**Create your `.env` file by copying the template:**

```bash
$ cp .env.example .env
```

> **What's a `.env` file?** It's a simple text file that stores secret values (like your API keys) as `KEY=value` pairs. The leading dot in `.env` makes it a "hidden file" on macOS/Linux — it won't show up in normal file listings, which helps keep it out of sight.

**Now open `.env` in a text editor and add your keys.** You can use any text editor:

- **Terminal-based editor** (simplest): `nano .env`
- **macOS**: `open -a TextEdit .env`
- **Linux**: `gedit .env` or `xdg-open .env`
- **VS Code** (if installed): `code .env`

Inside the file, find the line that says `ANTHROPIC_API_KEY=sk-ant-...` and replace `sk-ant-...` with your actual key:

```bash
# --- LLM Providers ---
ANTHROPIC_API_KEY=sk-ant-abc123-your-actual-key-here

# --- Data Sources (optional but recommended) ---
FRED_API_KEY=your-fred-key-here
NEWS_API_KEY=your-newsapi-key-here
```

**Save the file and close the editor.** If you're using `nano`, press `Ctrl + O` (to save), then `Enter`, then `Ctrl + X` (to exit).

> **Important**: Never share your `.env` file or post your API keys publicly. The `.gitignore` file already prevents `.env` from being accidentally uploaded to GitHub.

### 3.6 Build ORACLE

Now you'll compile ORACLE's source code into a runnable program. This is like translating a book from one language to another — the Rust compiler reads the source code and produces a fast, optimized program.

```bash
$ cargo build --release
```

> **What does `--release` mean?** It tells the compiler to produce an optimized build. This takes longer to compile (2–5 minutes on first run) but the resulting program runs much faster. Without `--release`, you get a "debug" build that compiles quickly but runs slowly.

**The first build takes a while** because it needs to download and compile all of ORACLE's dependencies (libraries it uses). You'll see output like:

```
   Compiling serde v1.0.200
   Compiling tokio v1.37.0
   ... (many more lines)
   Compiling oracle v0.1.0 (/home/you/Oracle)
    Finished `release` profile [optimized] target(s) in 3m 22s
```

Future builds will be much faster since dependencies are cached.

**Verify the build succeeded:**

```bash
$ ./target/release/oracle --help
```

You should see ORACLE's help text. If you do, the build worked.

> **Where is the program?** The compiled binary lives at `./target/release/oracle`. The `./` means "in the current directory," `target/release/` is where Rust puts optimized builds, and `oracle` is the program name.

---

## 4. Configuration — Telling ORACLE How to Behave

ORACLE has two configuration files:

| File | What It Controls | Contains Secrets? |
|------|-----------------|-------------------|
| `.env` | API keys and passwords | **Yes** — never share this |
| `config.toml` | Behavior settings (scan speed, bet sizes, risk limits) | No — safe to share |

You already set up `.env` in the previous step. Now let's look at `config.toml`.

### Understanding config.toml

Open `config.toml` in your text editor. Here are the most important settings, explained:

```toml
[agent]
name = "ORACLE-001"              # A name for your agent (cosmetic only)
scan_interval_secs = 600         # How often to scan markets (600 = every 10 minutes)
initial_bankroll = 100.0         # Starting simulated balance (in the currency below)
currency = "AUD"                 # Currency for display purposes

[llm]
provider = "anthropic"           # Which AI to use: "anthropic", "openai", or "grok"
model = "claude-sonnet-4-20250514"  # Specific AI model — leave as-is for now
max_tokens = 500                 # Maximum length of AI responses
batch_size = 10                  # How many markets to send to the AI at once

[risk]
mispricing_threshold = 0.08      # Only bet when the AI sees at least an 8% edge
kelly_multiplier = 0.25          # Bet 25% of the "mathematically optimal" amount (safer)
max_bet_pct = 0.06               # Never bet more than 6% of bankroll on one market
max_exposure_pct = 0.60          # Never have more than 60% of bankroll at risk total

[dashboard]
enabled = true                   # Turn the web dashboard on/off
port = 8080                      # Which port the dashboard runs on
```

**For your first run, you don't need to change anything.** The defaults are conservative and safe for experimentation.

> **What is TOML?** It's a simple configuration file format (like JSON or YAML, but more readable). Lines starting with `#` are comments — they're ignored by the program and exist only to explain things to you.

---

## 5. Your First Run — Simulated Mode (No Real Money)

This is the exciting part — you'll run ORACLE for the first time. In simulation mode, the agent scans *real* markets and makes *real* AI-powered estimates, but all bets are fake. No money is involved.

### 5.1 Start ORACLE

Make sure you're in the Oracle directory, then run:

```bash
$ ./target/release/oracle --config config.toml
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

Followed by log output showing what ORACLE is doing:

```
INFO oracle: ORACLE starting up agent_name=ORACLE-001 scan_interval_secs=600
INFO oracle: Fresh start bankroll=100.0
INFO oracle: Entering main loop. Press Ctrl+C to stop.
INFO oracle: Starting cycle cycle=1
INFO oracle: Markets scanned count=47
INFO oracle: [DRY RUN] Would place bet market_id=0xabc side=Yes amount=$4.20 edge=12.3%
INFO oracle: Cycle complete cycle=1 scanned=47 edges=3 bets=2 bankroll=$98.50
```

> **What does `[DRY RUN]` mean?** It means the bet is simulated. ORACLE calculated what it *would* bet, but didn't actually place a trade. This is the safe, simulation-only mode.

### 5.2 Open the Dashboard

While ORACLE is running, open your web browser and go to:

```
http://localhost:8080
```

> **What is `localhost`?** It refers to your own computer. Port `8080` is the "door number" where ORACLE's dashboard is listening. So `localhost:8080` means "connect to the dashboard running on my own machine."

The dashboard shows:

- **Status** — whether the agent is alive, its current bankroll
- **Performance** — profit/loss, win rate, trade history
- **Activity** — balance over time, recent cycles
- **Costs** — how much the AI API calls are costing

### 5.3 What to Look For

During your trial run, watch for these things:

| What to Check | Healthy Values | If Something Seems Wrong |
|---------------|---------------|------------------------|
| Markets scanned per cycle | 30–200+ | Check your internet connection |
| Edges found per cycle | 0–10 (zero is normal!) | Markets may be efficiently priced — nothing wrong |
| Bet sizes | 1–6% of bankroll | If much larger, check `config.toml` risk settings |
| Bankroll trend | Gradual decrease from LLM costs is normal | Big drops may indicate too-aggressive settings |

### 5.4 Stopping ORACLE

To stop the agent, press `Ctrl + C` in the terminal where it's running. ORACLE saves its state automatically, so you can restart later without losing progress.

### 5.5 Let It Run for a While

For a meaningful trial, let ORACLE run for **24–48 hours**. This gives you enough data to:

- See how the AI estimates compare to market prices
- Observe the agent's betting behavior over many cycles
- Track operational costs (how much your API keys are costing)
- Decide whether to adjust settings or eventually try real money

### 5.6 Running with Docker (Alternative)

If you have Docker installed and prefer containerized deployment:

```bash
$ docker build -t oracle .
$ docker run -d --name oracle-trial --env-file .env -p 8080:8080 oracle
```

View logs:

```bash
$ docker logs -f oracle-trial
```

Stop:

```bash
$ docker stop oracle-trial
```

---

## 6. Going Live — Real Funds on Polymarket

> **Only proceed here after you've run in simulation mode and are comfortable with how ORACLE works.** Real money is at risk in this section.

ORACLE trades on **Polymarket**, a prediction market that uses **USDC** (a cryptocurrency pegged to the US dollar) on the **Polygon** blockchain. Setting this up involves three steps:

1. Create a cryptocurrency wallet
2. Fund it with USDC
3. Connect it to Polymarket

### 6.1 Create a Polygon Wallet

A **wallet** is like a digital bank account for cryptocurrency. You'll create one specifically for ORACLE.

**Option A: MetaMask (beginner-friendly)**

1. Install the [MetaMask](https://metamask.io/) browser extension
2. Click "Create a new wallet"
3. Set a password
4. **Write down your seed phrase on paper and store it somewhere safe** — this is the only way to recover your wallet if something goes wrong. Never store it digitally or share it with anyone.
5. Add the Polygon network to MetaMask:
   - Click the network selector (top of MetaMask) > "Add network" > "Add a network manually"
   - Fill in:
     - Network name: `Polygon Mainnet`
     - RPC URL: `https://polygon-rpc.com`
     - Chain ID: `137`
     - Currency symbol: `POL`
     - Block explorer: `https://polygonscan.com`
6. Copy your wallet address (click your address at the top — it starts with `0x...`)

**Option B: Dedicated wallet via command line (for advanced users)**

```bash
# Install foundry toolkit first: https://getfoundry.sh
$ cast wallet new
```

This outputs a private key and address. Store the private key securely.

> **Security tip**: Use a wallet *exclusively* for ORACLE. Only put in the amount you're willing to lose. Never use your main personal wallet.

### 6.2 Fund Your Wallet with USDC

You need USDC (a stablecoin worth ~$1 each) on the Polygon network.

**How to get USDC on Polygon:**

1. Create an account on a cryptocurrency exchange (Coinbase, Kraken, or Binance)
2. Buy USDC (you can buy with a debit card or bank transfer)
3. Withdraw USDC to your Polygon wallet:
   - Select **Polygon** as the withdrawal network (not Ethereum — Polygon has much lower fees)
   - Paste your wallet address from MetaMask
   - Double-check the address before confirming
4. Also send a tiny amount of **POL** (Polygon's currency) for transaction fees — $0.50 worth is more than enough (transactions on Polygon cost fractions of a cent)

**Recommended starting amount**: $50–$200 USDC. ORACLE needs enough to cover operational costs (AI API fees of ~$15–50/day) while finding profitable bets.

### 6.3 Connect to Polymarket

1. Go to [polymarket.com](https://polymarket.com)
2. Click "Sign Up / Log In"
3. Connect your MetaMask wallet
4. Deposit USDC from your wallet into your Polymarket account

### 6.4 Configure ORACLE for Live Trading

Add your wallet's private key to `.env`:

```bash
# Open .env and add this line
POLYGON_PRIVATE_KEY=0xyour_private_key_here
```

Update `config.toml` to reflect your actual deposit:

```toml
[agent]
initial_bankroll = 100.0    # Match your actual USDC deposit amount
currency = "USD"
```

### 6.5 Launch

```bash
$ ./target/release/oracle --config config.toml
```

ORACLE will now execute real trades. Monitor it closely via the dashboard at `http://localhost:8080`.

### 6.6 Built-in Risk Protections

ORACLE has several automatic safety features to limit losses:

| Protection | What It Does |
|-----------|-------------|
| **Quarter-Kelly sizing** | Bets are 1/4 of the theoretically optimal amount — dramatically reduces risk |
| **Max 6% per bet** | No single bet can risk more than 6% of your bankroll |
| **Max 60% total exposure** | At most 60% of your money is at risk at any one time |
| **Category caps** | No more than 30% in any single category (sports, weather, etc.) |
| **Drawdown adjustment** | As your bankroll drops, ORACLE automatically gets more conservative |
| **Survival halt** | If bankroll hits $0, the agent stops trading entirely |

**How drawdown adjustment works:**

| Your Bankroll vs Starting Amount | ORACLE's Behavior |
|----------------------------------|-------------------|
| Over 200% (doubled your money) | More aggressive — seeking growth |
| 100–200% (in profit) | Normal operation |
| 50–100% (some losses) | Gets more cautious — smaller bets |
| 25–50% (significant losses) | Survival mode — very small bets |
| Under 25% (heavy losses) | Ultra-conservative — minimal activity |

### 6.7 Withdrawing Profits

1. Withdraw USDC from Polymarket back to your Polygon wallet (via the Polymarket website)
2. Send USDC from your wallet to your exchange account
3. Sell USDC for regular currency and withdraw to your bank

---

## 7. Monitoring — Keeping an Eye on Things

### 7.1 The Web Dashboard

Open `http://localhost:8080` in your browser (or `http://your-server-ip:8080` if running on a remote server).

The dashboard shows:
- **Status** — ALIVE / DIED / PAUSED and current bankroll
- **Performance** — Profit & loss, win rate, trade history
- **Activity** — Balance chart over time, recent cycles
- **Costs** — Cumulative API costs and daily burn rate

It refreshes automatically every 30 seconds.

### 7.2 Log Output

ORACLE prints structured logs to the terminal. For long-running sessions, you can save logs to a file:

```bash
$ ./target/release/oracle --config config.toml 2>&1 | tee -a oracle.log
```

> **What does this command do?** `2>&1` combines all output (normal + errors) into one stream. `tee -a oracle.log` sends that output to both the screen *and* a file called `oracle.log`. The `-a` means "append" — it adds to the file instead of overwriting it.

For more detailed logs (useful for debugging):

```bash
$ RUST_LOG=oracle=debug ./target/release/oracle --config config.toml
```

### 7.3 State Persistence

ORACLE saves its state to `oracle_state.json` after every cycle. If you stop the agent or your computer crashes, ORACLE picks up where it left off when you restart — no progress is lost.

---

## 8. Troubleshooting — Common Issues and Fixes

### "command not found: cargo" or "command not found: rustc"

Rust isn't in your terminal's PATH (the list of places your terminal looks for programs).

**Fix**: Run `source "$HOME/.cargo/env"` and try again. If that doesn't work, close your terminal, open a new one, and try again.

### "command not found: git"

Git isn't installed. See [Section 3.2](#32-install-git) for installation instructions.

### Build fails with errors

Make sure you're in the `Oracle` directory (`cd Oracle`) and that you have internet access (the first build downloads dependencies). Try:

```bash
$ cargo clean
$ cargo build --release
```

`cargo clean` removes previous build artifacts so you get a fresh start.

### "No LLM API key configured"

Your Anthropic (or OpenAI) API key isn't being read. Check:

1. Your `.env` file exists in the Oracle directory: `ls -la .env`
2. The key is set correctly (no extra spaces or quotes):
   ```
   ANTHROPIC_API_KEY=sk-ant-your-actual-key
   ```
   Not:
   ```
   ANTHROPIC_API_KEY="sk-ant-your-actual-key"   # No quotes!
   ANTHROPIC_API_KEY= sk-ant-your-actual-key     # No space after =
   ```

### No markets scanned (count=0)

- Check your internet connection
- Verify platform settings in `config.toml`:
  ```toml
  [platforms.metaculus]
  enabled = true
  [platforms.manifold]
  enabled = true
  ```
- Try running with debug logs: `RUST_LOG=oracle=debug ./target/release/oracle --config config.toml`

### Zero edges found

This is **completely normal**. It means the markets are efficiently priced and the AI doesn't see any opportunities. ORACLE will keep scanning every cycle and will bet when it finds genuine mispricings.

If you want the agent to be more sensitive (at the cost of potentially noisier bets), lower the threshold:

```toml
[risk]
mispricing_threshold = 0.06    # 6% edge instead of default 8%
```

### LLM API costs are too high

Reduce costs by adjusting `config.toml`:

```toml
[agent]
scan_interval_secs = 900    # Scan every 15 minutes instead of 10

[llm]
batch_size = 5              # Analyze fewer markets per cycle
model = "claude-haiku-4-5-20251001"   # Use a cheaper, faster model
```

### Agent stopped — "bankroll reached $0"

The simulated bankroll ran out (usually from LLM API costs exceeding simulated gains). To start fresh:

1. Delete the saved state: `rm oracle_state.json`
2. Optionally increase the starting bankroll in `config.toml`:
   ```toml
   [agent]
   initial_bankroll = 500.0
   ```
3. Restart ORACLE

### Dashboard not loading in browser

- Make sure ORACLE is still running in the terminal
- Check that `dashboard.enabled = true` in `config.toml`
- Try `http://127.0.0.1:8080` instead of `http://localhost:8080`
- If using Docker, ensure you included `-p 8080:8080` in the `docker run` command

### Permission denied when running `./target/release/oracle`

Make the file executable:

```bash
$ chmod +x ./target/release/oracle
```

---

## 9. Glossary

| Term | Definition |
|------|-----------|
| **API** | Application Programming Interface — a way for programs to talk to online services |
| **API key** | A unique code that identifies you when connecting to an API (like a password) |
| **Binary** | A compiled program file that your computer can run directly |
| **Cargo** | Rust's build tool — it compiles code and manages dependencies |
| **Clone** | Downloading a Git repository (a project's code and history) to your computer |
| **Compile** | Translating source code into a program your computer can run |
| **config.toml** | ORACLE's configuration file (controls behavior, not secrets) |
| **Dependency** | A library (pre-written code) that ORACLE uses — like building blocks |
| **.env** | A hidden file that stores your secret API keys |
| **Edge** | The difference between what the AI estimates and what the market says — a potential opportunity |
| **Git** | A tool for downloading, tracking, and managing code |
| **Kelly criterion** | A mathematical formula for calculating optimal bet sizes |
| **LLM** | Large Language Model — the AI that estimates probabilities (e.g., Claude, GPT-4) |
| **localhost** | Your own computer, when accessed through a web browser |
| **PATH** | A list of directories where your terminal looks for programs |
| **Polygon** | A blockchain network where Polymarket transactions happen |
| **Port** | A numbered "door" on your computer that a program listens on (ORACLE uses 8080) |
| **Prediction market** | A platform where people bet on the outcomes of real-world events |
| **Release build** | An optimized compiled program (slower to build, faster to run) |
| **Rust** | The programming language ORACLE is written in |
| **Terminal** | A text-based interface for running commands on your computer |
| **TOML** | A configuration file format used by `config.toml` |
| **USDC** | A cryptocurrency stablecoin worth ~$1, used by Polymarket |
| **Wallet** | A digital account for holding cryptocurrency |
| **WSL2** | Windows Subsystem for Linux — lets you run Linux tools on Windows |

---

*ORACLE Quick Start Guide — February 2026*
