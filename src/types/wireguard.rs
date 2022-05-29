use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct WireGuardConfigFile {
    pub address: String,
    pub name: String,
    pub post_up: String,
    pub post_down: String,
    pub listen_port: i32,
    pub private_key: String,
    pub public_key: String,
    pub dns: String
}