use serde::{Deserialize, Serialize};

pub mod bedrock;
pub mod pack;
pub mod world;

pub const GAME_TITLE: &str = "Unnamed Craft";
pub const PROTOCOL_VERSION: u32 = 1;
pub const PACK_NAMESPACE: &str = "unamed";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetworkConfig {
    pub server_address: String,
    pub protocol_version: u32,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            server_address: "127.0.0.1:5000".to_string(),
            protocol_version: PROTOCOL_VERSION,
        }
    }
}
