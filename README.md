# ORACLE — Autonomous Prediction Market AI Agent

**O**ptimized **R**isk-**A**djusted **C**ross-platform **L**everaged **E**ngine

ORACLE is a fully autonomous AI agent built in Rust that operates across prediction market
and forecasting platforms to detect mispricings, estimate fair-value probabilities via LLM
reasoning, and place Kelly-criterion-sized bets.

## Platform Stack

| Platform | Role | Type |
|----------|------|------|
| **Betfair Exchange** | Real-money execution (primary) | Back/lay betting exchange |
| **Manifold** | Paper-trading, backtesting, validation + sentiment | Play-money |
| **Metaculus** | Crowd forecast cross-reference | Read-only |
| **IBKR ForecastTrader** | Real-money execution (optional secondary) | Event contracts |

Betfair Exchange is the primary live execution venue, offering deep liquidity across sports, politics, and current affairs markets. Manifold provides zero-cost paper trading and historical data for backtesting. Metaculus supplies crowd forecast cross-references. IBKR event contracts are an optional secondary module.

## LLM Stack

All LLM calls route through **OpenRouter** with a single API key:

| Model | Role | Provider |
|-------|------|----------|
| `anthropic/claude-sonnet-4` | Primary estimator | OpenRouter |
| `x-ai/grok-4.1-fast` | Cheap/fast fallback | OpenRouter |

Automatic failover: if the primary model fails after retries, the agent transparently falls back to the secondary model.

## Prerequisites

- **Rust** (stable toolchain, 2021 edition) — install via [rustup](https://rustup.rs)
- **OpenRouter API key** — single key for all LLM models ([openrouter.ai](https://openrouter.ai))
- **Betfair API app key** + funded account (for live execution)
- Optional: Manifold API key (for paper-trading writes), data source API keys

## Quick Start

```bash
# 1. Clone and configure
git clone https://github.com/HolsteredSoul/Oracle.git
cd Oracle
cp .env.example .env
# Fill in your API keys in .env

# 2. Build
cargo build --release

# 3. Run (dry-run / paper trading mode)
cargo run -- --config config.toml

# 4. Run release build
./target/release/oracle --config config.toml
```

## Docker

```bash
docker build -t oracle .
docker run -d --env-file .env --network host -p 8080:8080 oracle
```

## Project Structure

```
oracle/
├── Cargo.toml              # Dependencies and project metadata
├── config.toml             # Runtime configuration (TOML)
├── .env.example            # Environment variable template
├── Dockerfile              # Container deployment
├── docs/
│   ├── WHITEPAPER.md       # Theory, architecture, risk framework
│   ├── DEVELOPMENT_PLAN.md # Phased build roadmap
│   └── QUICKSTART.md       # Step-by-step setup guide
├── src/
│   ├── main.rs             # Entry point, async main loop
│   ├── config.rs           # TOML config + env var resolution
│   ├── types.rs            # Shared types (Market, Side, Trade, etc.)
│   ├── platforms/          # Platform integrations
│   │   ├── mod.rs          # PredictionPlatform trait
│   │   ├── manifold.rs     # Manifold (play-money + paper trading)
│   │   ├── metaculus.rs    # Metaculus (read-only)
│   │   ├── forecastex.rs   # IBKR ForecastTrader (optional, Phase 2)
│   │   └── polymarket.rs   # Polymarket (stub for future)
│   ├── data/               # Data enrichment providers
│   │   ├── mod.rs          # DataProvider trait
│   │   ├── weather.rs      # Open-Meteo (free, global)
│   │   ├── sports.rs       # Keyword extraction + API-Sports
│   │   ├── economics.rs    # FRED (US macro indicators)
│   │   └── news.rs         # NewsAPI + sentiment scoring
│   ├── llm/                # LLM integration
│   │   ├── mod.rs          # LlmEstimator trait
│   │   ├── openrouter.rs   # OpenRouter (primary — routes to all models)
│   │   ├── anthropic.rs    # Anthropic direct (fallback provider)
│   │   ├── openai.rs       # OpenAI direct (fallback provider)
│   │   └── grok.rs         # Grok (stub)
│   ├── strategy/           # Strategy engine
│   │   ├── mod.rs          # Strategy orchestrator
│   │   ├── edge.rs         # Mispricing detection
│   │   ├── kelly.rs        # Kelly criterion sizing
│   │   └── risk.rs         # Risk manager
│   ├── engine/             # Core loop
│   │   ├── mod.rs          # Main scan-estimate-bet loop
│   │   ├── scanner.rs      # Multi-platform market scanner
│   │   ├── enricher.rs     # Data enrichment pipeline
│   │   ├── executor.rs     # Trade execution
│   │   └── accountant.rs   # Cost tracking + survival
│   ├── storage/            # Persistence
│   │   └── mod.rs          # JSON state persistence
│   ├── dashboard/          # Monitoring
│   │   ├── mod.rs          # Axum web server
│   │   └── routes.rs       # API endpoints
│   └── backtest/           # Backtesting framework
│       ├── mod.rs
│       ├── runner.rs        # Strategy replay engine
│       └── calibration.rs   # Brier score + calibration curves
└── tests/
    ├── integration/
    │   ├── mock_platform.rs
    │   └── simulation.rs
    └── unit/
        ├── kelly_tests.rs
        └── edge_tests.rs
```

## Development Status

| Phase | Description | Status | Tests |
|-------|-------------|--------|-------|
| 0 | Project Scaffolding | Complete | — |
| 1 | Core Types & Platform Trait | Complete | 59 |
| 2B | Metaculus Scanner | Complete | 24 |
| 2C | Manifold Scanner | Complete | 17 |
| 2D | Market Router | Complete | 20 |
| 3 | Data Enrichment Pipeline | Complete | 40 |
| 4 | LLM Integration (OpenRouter + Anthropic + OpenAI) | Complete | 30 |
| 5 | Strategy Engine | Complete | 32 |
| 6 | Execution & Survival Loop | Complete | 16 |
| 7 | Dashboard & Monitoring | Complete | 18 |
| 8 | Calibration & Backtesting | Complete | 19 |
| — | Betfair Exchange adapter | Planned | — |
| — | IBKR ForecastTrader adapter | Planned | — |

**Total tests passing: 298**

## Documentation

- [Whitepaper](docs/WHITEPAPER.md) — Theory, architecture, risk framework
- [Development Plan](docs/DEVELOPMENT_PLAN.md) — Phased build roadmap
- [Quick Start Guide](docs/QUICKSTART.md) — Step-by-step setup instructions

## Configuration

Edit `config.toml` for runtime settings. Sensitive values (API keys) are loaded from
environment variables referenced in the config. See `.env.example` for the full list.

Key config sections:
- `[agent]` — scan interval, bankroll, currency
- `[llm]` — provider (`"openrouter"` | `"anthropic"`), model, fallback model, token limits
- `[platforms.*]` — Manifold, Metaculus, Betfair (planned), IBKR (planned)
- `[risk]` — thresholds, Kelly multiplier, exposure limits
- `[data_sources]` — weather, sports, economics API keys
- `[dashboard]` — web UI port
- `[alerts]` — Telegram notifications

## License

Proprietary. All rights reserved.

---

*ORACLE v0.1.0 — February 2026*
