//! Deterministic, bounded wildfire/response illustrations. Never operational guidance.
pub mod demo;
pub mod grid;
pub mod outputs;
pub mod planner;
pub mod severity;
pub mod simulation;
pub mod spread;
pub mod wind;
pub mod worker;

pub use demo::demo_bundle;
pub use grid::{Cell, CellPosition, ComputeError, Grid};
pub use outputs::{LocalProjection, build_briefing, build_drone_plan, build_forecast};
pub use planner::{Mission, MissionRequest, ReplanDecision, plan_mission, replan};
pub use spread::{SpreadInput, SpreadResult, simulate_spread};
pub use wind::{WindSample, WindVector, blend_wind};
pub use worker::ComputePool;
pub type ResponseError = ComputeError;
pub use severity::{SeverityLevel, SeverityProposal, SeverityThresholds, propose_severity};
