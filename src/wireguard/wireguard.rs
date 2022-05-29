use crate::types::{WireGuardConfigFile, UsageMap, Clients};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{Mutex};

use std::fs;
use serde_json;

pub type WireGuard = Arc<Mutex<WireGuardConfig>>;

pub struct WireGuardConfig {
    pub config: WireGuardConfigFile,
    pub usage_map: UsageMap,
    pub clients: Clients
}

impl WireGuardConfig {
    pub fn load_from_config(file_path: &str) -> Self {
        let data = fs::read_to_string(file_path).expect("Unable to read file");
        let mut res: WireGuardConfigFile = serde_json::from_str(&data).expect("Unable to parse");

        // GENERATE THE KEYS
        res.private_key = "".to_string();
        res.public_key = "".to_string();

        WireGuardConfig {
            config: res,
            usage_map: Arc::new(Mutex::new(HashMap::new())),
            clients: Arc::new(Mutex::new(HashMap::new()))
        }
    }

    pub fn save_config(self) {
        fs::write("./configs/reseda.conf", self.generate_config_string());
    }

    pub fn generate_config_string(self) -> String {
        let mut elems = vec!["[Interface]".to_string()];
        elems.push(format!("Address = {}", &self.config.address));
        elems.push(format!("PrivateKey = {}", &self.config.private_key));
        elems.push(format!("ListenPort = {}", &self.config.listen_port));
        elems.push(format!("DNS = {}", &self.config.dns));


        elems.join("\n")
    }
}