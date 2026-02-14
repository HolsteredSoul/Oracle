-- ORACLE Database Schema
-- SQLite

-- Agent state (single row, updated each cycle)
CREATE TABLE IF NOT EXISTS agent_state (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    bankroll REAL NOT NULL,
    total_pnl REAL NOT NULL DEFAULT 0.0,
    cycle_count INTEGER NOT NULL DEFAULT 0,
    trades_placed INTEGER NOT NULL DEFAULT 0,
    trades_won INTEGER NOT NULL DEFAULT 0,
    trades_lost INTEGER NOT NULL DEFAULT 0,
    total_api_costs REAL NOT NULL DEFAULT 0.0,
    total_ib_commissions REAL NOT NULL DEFAULT 0.0,
    start_time TEXT NOT NULL,
    peak_bankroll REAL NOT NULL,
    status TEXT NOT NULL DEFAULT 'Alive',
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Trade history
CREATE TABLE IF NOT EXISTS trades (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    order_id TEXT NOT NULL,
    market_id TEXT NOT NULL,
    platform TEXT NOT NULL,
    side TEXT NOT NULL,
    amount REAL NOT NULL,
    fill_price REAL NOT NULL,
    fees REAL NOT NULL DEFAULT 0.0,
    pnl REAL,
    resolved INTEGER NOT NULL DEFAULT 0,
    timestamp TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- LLM probability estimates (for calibration tracking)
CREATE TABLE IF NOT EXISTS estimates (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    market_id TEXT NOT NULL,
    platform TEXT NOT NULL,
    question TEXT NOT NULL,
    llm_estimate REAL NOT NULL,
    market_price REAL NOT NULL,
    metaculus_forecast REAL,
    manifold_price REAL,
    actual_outcome INTEGER,  -- 0 or 1, filled on resolution
    confidence REAL,
    reasoning TEXT,
    tokens_used INTEGER,
    cost REAL,
    llm_model TEXT,
    timestamp TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Cycle reports
CREATE TABLE IF NOT EXISTS cycle_reports (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    cycle_number INTEGER NOT NULL,
    markets_scanned INTEGER NOT NULL DEFAULT 0,
    edges_found INTEGER NOT NULL DEFAULT 0,
    bets_placed INTEGER NOT NULL DEFAULT 0,
    cycle_cost REAL NOT NULL DEFAULT 0.0,
    cycle_pnl REAL NOT NULL DEFAULT 0.0,
    bankroll_after REAL NOT NULL,
    timestamp TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Balance history (for charting)
CREATE TABLE IF NOT EXISTS balance_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    bankroll REAL NOT NULL,
    total_pnl REAL NOT NULL,
    timestamp TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_trades_market ON trades(market_id);
CREATE INDEX IF NOT EXISTS idx_trades_platform ON trades(platform);
CREATE INDEX IF NOT EXISTS idx_estimates_market ON estimates(market_id);
CREATE INDEX IF NOT EXISTS idx_estimates_model ON estimates(llm_model);
CREATE INDEX IF NOT EXISTS idx_cycle_reports_number ON cycle_reports(cycle_number);
