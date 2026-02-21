# ORACLE — Autonomous Prediction Market AI Agent

**O**ptimized **R**isk-**A**djusted **C**ross-platform **L**everaged **E**ngine

ORACLE is a fully autonomous AI agent built in Rust that operates across prediction market
and forecasting platforms to detect mispricings, estimate fair-value probabilities via LLM
reasoning, and place Kelly-criterion-sized bets.

## Platform Stack (AU-Compliant, February 2026)

| Platform | Role | Type |
|----------|------|------|
| **IB ForecastEx** | Real-money execution | ASIC-regulated exchange |
| **Metaculus** | Crowd forecast cross-reference | Read-only |
| **Manifold** | Paper-trading validation + sentiment | Play-money |

ForecastEx is the sole viable real-money prediction market platform accessible from
Australia as of February 2026.

## Prerequisites

- **Rust** (stable toolchain, 2021 edition) — install via [rustup](https://rustup.rs)
- **IB Gateway** or **TWS** running locally (paper: port 4002, live: port 4001)
- API keys for LLM provider(s) and data sources (see `.env.example`)

## Quick Start

```bash
# 1. Clone and configure
cd oracle
cp .env.example .env
# Fill in your API keys in .env

# 2. Ensure IB Gateway is running (paper mode, port 4002)

# 3. Build
cargo build --release

# 4. Run (paper trading)
cargo run -- --config config.toml

# 5. Run release build
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
│   ├── WHITEPAPER.md       # Theory, architecture, risk framework (v1.2)
│   └── DEVELOPMENT_PLAN.md # Phased build roadmap (v1.2)
├── src/
│   ├── main.rs             # Entry point, async main loop
│   ├── config.rs           # TOML config + env var resolution
│   ├── types.rs            # Shared types (Market, Side, Trade, etc.)
│   ├── platforms/          # Platform integrations
│   │   ├── mod.rs          # PredictionPlatform trait
│   │   ├── forecastex.rs   # IB ForecastEx (TWS API)
│   │   ├── metaculus.rs    # Metaculus (read-only)
│   │   └── manifold.rs     # Manifold (play-money)
│   ├── data/               # Data enrichment providers
│   │   ├── mod.rs          # DataProvider trait
│   │   ├── weather.rs      # Open-Meteo (free, global)
│   │   ├── sports.rs       # Keyword extraction + API-Sports
│   │   ├── economics.rs    # FRED (US macro indicators)
│   │   └── news.rs         # NewsAPI + sentiment scoring
│   ├── llm/                # LLM integration
│   │   ├── mod.rs          # LlmEstimator trait + prompt builder
│   │   ├── anthropic.rs    # Claude
│   │   ├── openai.rs       # GPT-4
│   │   └── grok.rs         # Grok
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
│   │   ├── mod.rs          # SQLite operations
│   │   └── schema.sql      # Database schema
│   └── dashboard/          # Monitoring
│       ├── mod.rs          # Axum web server
│       ├── routes.rs       # API endpoints
│       └── templates/      # HTML templates
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
| 0 | Project Scaffolding | ✅ Complete | — |
| 1 | Core Types & Platform Trait | ✅ Complete | 59 |
| 2A | IB ForecastEx Scanner | ❌ Deferred | — |
| 2B | Metaculus Scanner | ✅ Complete | 24 |
| 2C | Manifold Scanner | ✅ Complete | 17 |
| 2D | Market Router | ✅ Complete | 20 |
| 3 | Data Enrichment Pipeline | ✅ Complete | 40 |
| 4 | LLM Integration | ✅ Complete | 21 |
| 5 | Strategy Engine | ✅ Complete | 32 |
| 6 | Execution & Survival Loop | ✅ Complete | 16 |
| 7 | Dashboard & Monitoring | ⭕ Next | — |
| 8 | Calibration & Backtesting | — | — |

**Total tests passing: 229**

## Documentation

- [Whitepaper v1.2](docs/WHITEPAPER.md) — Theory, architecture, risk framework
- [Development Plan v1.2](docs/DEVELOPMENT_PLAN.md) — Phased build roadmap

## Configuration

Edit `config.toml` for runtime settings. Sensitive values (API keys) are loaded from
environment variables referenced in the config. See `.env.example` for the full list.

Key config sections:
- `[agent]` — scan interval, bankroll, currency
- `[llm]` — provider, model, token limits
- `[platforms.*]` — ForecastEx (IB), Metaculus, Manifold
- `[risk]` — thresholds, Kelly multiplier, exposure limits
- `[data_sources]` — weather, sports, economics API keys
- `[dashboard]` — web UI port
- `[alerts]` — Telegram/Discord notifications

## License

Proprietary. All rights reserved.

---

*ORACLE v0.1.0 — February 2026*
