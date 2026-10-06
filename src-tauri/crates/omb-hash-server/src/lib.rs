pub mod config;
mod server;

pub use omb_hash::protocol::*;
pub use server::{router, HashServer};

pub const VERSION: &str = match option_env!("OMB_HASH_VERSION") {
    Some(version) if !version.is_empty() => version,
    _ => env!("CARGO_PKG_VERSION"),
};
