//! Application support crate for the Moli CLI binary.
//!
//! This crate groups CLI parsing, runtime/server configuration, telemetry
//! setup, and the embedded protocol server used by the `moli` executable.

pub mod app;
mod cdp_frontend;
mod cdp_frontend_router;
mod cdp_scheduler;
mod cdp_writer;
pub mod cli;
pub mod config;
pub mod cookie_cache;
pub mod fetch_dump;
pub mod mcp_server;
mod network_trace;
pub mod protocol_server;
pub mod runtime_thread_budget;
pub mod telemetry;
