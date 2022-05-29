use crate::types::{WireGuardConfigFile, UsageMap, Clients, KeyState, Client};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{Mutex};

use std::fs;
use serde_json;

pub type WireGuard = Arc<Mutex<WireGuardConfig>>;

#[derive(Clone)]
pub struct WireGuardConfig {
    pub config: WireGuardConfigFile,
    pub keys: KeyState,
    pub usage_map: UsageMap,
    pub clients: Clients
}

impl WireGuardConfig {
    pub fn load_from_config(file_path: &str) -> Self {
        let data = fs::read_to_string(file_path).expect("Unable to read file");
        let res: WireGuardConfigFile = serde_json::from_str(&data).expect("Unable to parse");

        let keys = KeyState::generate_pair();

        WireGuardConfig {
            config: res,
            keys: keys,
            usage_map: Arc::new(Mutex::new(HashMap::new())),
            clients: Arc::new(Mutex::new(HashMap::new()))
        }
    }

    pub async fn save_config(&mut self) -> &mut Self {
        match fs::write("configs/reseda.conf", &self.generate_config_string().await) {
            Result::Err(_) => {
                println!("Unable to write!");
            },
            Result::Ok(_) => {
                println!("Wrote configuration successfully.");
            }
        }

        self
    }

    pub async fn generate_config_string(&self) -> String {
        let mut elems = vec!["[Interface]".to_string()];
        elems.push(format!("Address = {}", &self.config.address));
        elems.push(format!("PrivateKey = {}", &self.keys.private_key));
        elems.push(format!("ListenPort = {}", &self.config.listen_port));
        elems.push(format!("DNS = {}", &self.config.dns));
        elems.push(format!("PostUp = {}", &self.config.post_up));
        elems.push(format!("PostDown = {}", &self.config.post_down));

        for (key, value) in self.clients.lock().await.iter() {
            elems.push("\n".to_string());
            elems.push("[Peer]".to_string());   
            elems.push(format!("PublicKey = {}", value.public_key));
            elems.push(format!("AllowedIPs = 192.168.69.{}", key));
            elems.push(format!("Endpoint = {}", value.public_key));
        };

        elems.join("\n")
    }
}
