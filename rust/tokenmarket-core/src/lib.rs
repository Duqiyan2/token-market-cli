pub mod adapters;
pub mod auth;
pub mod config;
mod model_catalog;
mod model_projection;
pub mod models;
pub mod verification;

pub use adapters::{
    adapters, get_adapter, tool_id, Adapter, AdapterContext, AdapterWriteResult, Protocol, ToolId,
};

pub mod client;
pub mod credential_vault;
pub mod desktop;

pub mod desktop_workspace;
pub mod installation;
pub mod inventory;
pub mod usage;

pub mod connections;
mod dumate;
mod qwenwork;
pub mod qwenwork_runtime;
mod workbuddy;
mod yaml;

pub mod client_update;
