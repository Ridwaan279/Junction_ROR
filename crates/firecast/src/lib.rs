//! Physics-based wildfire spread and smoke forecasting for EmberWatch.
//!
//! Pipeline: [`environment::acquire`] gathers terrain, land cover, a spatial
//! weather field, Context.dev web intelligence and IPMA warnings (each with
//! provenance and an explicit synthetic fallback); [`forecast::compute`] runs a
//! Rothermel/ellipse minimum-travel-time ensemble and a Gaussian smoke plume and
//! publishes GeoJSON layers; [`hazards::plan_drone`] turns them into drone-routing
//! hazards for the shared response planner.
//!
//! Experimental research software: outputs are uncalibrated for Portugal and are
//! never operational guidance.
pub mod contour;
pub mod environment;
pub mod forecast;
pub mod fuel;
pub mod geo;
pub mod hazards;
pub mod intel;
pub mod landcover;
pub mod moisture;
pub mod rothermel;
pub mod smoke;
pub mod solar;
pub mod sources;
pub mod spread;
pub mod terrain;
pub mod weather;

pub use environment::{AcquireRequest, Basis, FireEnvironment, acquire};
pub use forecast::{FirecastConfig, FirecastResult, compute, to_domain};
pub use hazards::plan_drone;
pub use intel::ContextDev;
pub use sources::HttpSources;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum FirecastError {
    #[error("network error reaching {0}")]
    Network(String),
    #[error("upstream error: {0}")]
    Upstream(String),
    #[error("could not parse {0}")]
    Parse(String),
    #[error("configuration: {0}")]
    Config(String),
    #[error("computation: {0}")]
    Compute(String),
}
