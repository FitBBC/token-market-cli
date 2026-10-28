pub mod adapters;
pub mod auth;
pub mod config;
pub mod models;
pub mod setup;
pub mod verification;

pub use adapters::{
    adapters, get_adapter, tool_id, Adapter, AdapterContext, AdapterWriteResult, Protocol, ToolId,
};
pub use setup::{run_setup, SetupOutcome, SetupProgress};

pub mod desktop;

pub mod inventory;
