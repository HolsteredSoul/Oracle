//! Persistence layer.
//!
//! Saves and loads agent state to/from a JSON file.
//! SQLite integration can be added later for trade history and calibration
//! data, but JSON is sufficient for the core state persistence requirement.

use anyhow::{Context, Result};
use std::path::Path;
use tracing::{debug, info};

use crate::types::AgentState;

/// Default state file path.
const DEFAULT_STATE_FILE: &str = "oracle_state.json";

/// Save agent state to a JSON file.
pub fn save_state(state: &AgentState, path: Option<&str>) -> Result<()> {
    let path = path.unwrap_or(DEFAULT_STATE_FILE);
    let json = serde_json::to_string_pretty(state)
        .context("Failed to serialise agent state")?;

    std::fs::write(path, &json)
        .context(format!("Failed to write state to {path}"))?;

    debug!(path, bankroll = %state.bankroll, "State saved");
    Ok(())
}

/// Load agent state from a JSON file.
/// Returns None if the file doesn't exist (fresh start).
pub fn load_state(path: Option<&str>) -> Result<Option<AgentState>> {
    let path = path.unwrap_or(DEFAULT_STATE_FILE);

    if !Path::new(path).exists() {
        info!(path, "No saved state found, starting fresh");
        return Ok(None);
    }

    let json = std::fs::read_to_string(path)
        .context(format!("Failed to read state from {path}"))?;

    let state: AgentState = serde_json::from_str(&json)
        .context(format!("Failed to parse state from {path}"))?;

    info!(
        path,
        bankroll = %state.bankroll,
        cycle_count = state.cycle_count,
        trades = state.trades_placed,
        "State loaded from disk"
    );

    Ok(Some(state))
}

/// Delete the state file (for testing or reset).
pub fn delete_state(path: Option<&str>) -> Result<()> {
    let path = path.unwrap_or(DEFAULT_STATE_FILE);
    if Path::new(path).exists() {
        std::fs::remove_file(path)
            .context(format!("Failed to delete state file {path}"))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::AgentStatus;
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;
    use std::path::PathBuf;

    fn temp_path() -> String {
        let mut p = std::env::temp_dir();
        p.push(format!("oracle_test_state_{}.json", uuid::Uuid::new_v4()));
        p.to_string_lossy().to_string()
    }

    #[test]
    fn test_save_and_load() {
        let path = temp_path();
        let state = AgentState::new(dec!(100));
        save_state(&state, Some(&path)).unwrap();

        let loaded = load_state(Some(&path)).unwrap();
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.bankroll, dec!(100));
        assert_eq!(loaded.status, AgentStatus::Alive);

        delete_state(Some(&path)).unwrap();
    }

    #[test]
    fn test_load_nonexistent() {
        let path = "/tmp/oracle_nonexistent_state_12345.json";
        let loaded = load_state(Some(path)).unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn test_save_preserves_fields() {
        let path = temp_path();
        let mut state = AgentState::new(dec!(500));
        state.cycle_count = 42;
        state.trades_placed = 10;
        state.trades_won = 7;
        state.total_pnl = dec!(25);
        state.bankroll = dec!(525);
        state.peak_bankroll = dec!(550);

        save_state(&state, Some(&path)).unwrap();
        let loaded = load_state(Some(&path)).unwrap().unwrap();

        assert_eq!(loaded.cycle_count, 42);
        assert_eq!(loaded.trades_placed, 10);
        assert_eq!(loaded.trades_won, 7);
        assert_eq!(loaded.total_pnl, dec!(25));
        assert_eq!(loaded.bankroll, dec!(525));
        assert_eq!(loaded.peak_bankroll, dec!(550));

        delete_state(Some(&path)).unwrap();
    }

    #[test]
    fn test_delete_state() {
        let path = temp_path();
        save_state(&AgentState::new(dec!(50)), Some(&path)).unwrap();
        assert!(Path::new(&path).exists());

        delete_state(Some(&path)).unwrap();
        assert!(!Path::new(&path).exists());
    }

    #[test]
    fn test_delete_nonexistent_ok() {
        let result = delete_state(Some("/tmp/oracle_does_not_exist_xyz.json"));
        assert!(result.is_ok());
    }
}
