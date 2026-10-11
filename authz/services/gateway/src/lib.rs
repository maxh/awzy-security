//! awzy-gateway: a policy enforcement point.
//!
//! Today: an MCP proxy. Agents connect to `/mcp/{upstream}`; the gateway validates their token,
//! asks awzy-pdp about every `tools/call`, and forwards everything else unchanged. Next: a
//! reverse proxy for HTTP APIs.

mod config;
pub mod containments;
mod mcp;
pub mod metrics;

pub use awzy_identity::{ServiceAccountConfig, TokenConfig, TokenKey};
pub use config::{GatewayConfig, UpstreamConfig};
pub use mcp::{router, GatewayState};
