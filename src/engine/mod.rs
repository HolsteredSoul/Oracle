//! Core engine — the main scan → estimate → bet loop.

pub mod scanner;
pub mod enricher;
pub mod executor;
pub mod accountant;

// TODO (Phase 5–6): Implement the cycle orchestrator that ties together
// scanning, enrichment, estimation, edge detection, sizing, and execution.
