//! Strategy engine — edge detection, Kelly sizing, and risk management.

pub mod edge;
pub mod kelly;
pub mod risk;

// TODO (Phase 5): Implement strategy orchestrator that pipelines
// edge detection → Kelly sizing → risk checks → bet selection.
