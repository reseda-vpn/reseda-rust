use crate::types::{WireGuardConfigFile, Clients, KeyState, Client};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{Mutex};
use std::process::{Command};

use std::fs;
use serde_json;

pub type WireGuard = Arc<Mutex<WireGuardConfig>>;

#[derive(Clone)]
pub struct WireGuardConfig {
    pub config: WireGuardConfigFile,
    pub keys: KeyState,
    pub clients: Clients
}

impl WireGuardConfig {
    pub fn load_from_config(file_path: &str) -> Self {
        // Load local config file as string
        let data = fs::read_to_string(file_path).expect("Unable to read file");
        // Convert to JSON
        let res: WireGuardConfigFile = serde_json::from_str(&data).expect("Unable to parse");
        // Generate Keys
        let keys = KeyState::generate_pair();

        // Return Configuration
        WireGuardConfig {
            config: res,
            keys: keys,
            clients: Arc::new(Mutex::new(HashMap::new()))
        }
    }

    pub async fn save_config(&mut self, should_restart: bool) -> &mut Self {
        let config = &self.generate_config_string().await;

        match fs::write("/etc/wireguard/reseda.conf", config) {
            Result::Err(_) => {
                println!("Unable to write!");
            },
            Result::Ok(_) => {}
        }
        
        if should_restart {
            WireGuardConfig::restart_config(self).await;
        }

        self
    }

    pub async fn generate_config_string(&self) -> String {
        let mut elems = vec!["[Interface]".to_string()];
        elems.push(format!("Address = {}/24", &self.config.address));
        elems.push(format!("PrivateKey = {}", &self.keys.private_key.trim()));
        elems.push(format!("ListenPort = {}", &self.config.listen_port));
        elems.push(format!("DNS = {}", &self.config.dns));
        elems.push(format!("PostUp = {}", &self.config.post_up));
        elems.push(format!("PostDown = {}", &self.config.post_down));

        for (_, value) in self.clients.lock().await.iter() {
            if value.connected {
                elems.push("\n".to_string());
                elems.push("[Peer]".to_string());   
                elems.push(format!("PublicKey = {}", value.public_key));
                // TODO: Replace allowed IP address with a dynamically assigned address
                elems.push(format!("AllowedIPs = 192.168.69.{}/24", 2));
                elems.push(format!("PersistentKeepalive = 25"));
            }
        };

        elems.join("\n")
    }

    pub async fn restart_config(&mut self) -> bool {
        let down_status = &self.config_down().await;
        let up_status = &self.config_up().await;

        *up_status && *down_status
    }
    
    pub async fn remove_peer(&self, client: &Client) {

    }

    pub async fn add_peer(&self, client: &Client) {
        match Command::new("wg")
            .env("export WG_I_PREFER_BUGGY_USERSPACE_TO_POLISHED_KMOD", "1")
            .args(["set", "reseda", "peer", &client.public_key, "allowed-ips", "10.8.0.2", "persistent-keepalive", "25"]).output() {
                Ok(output) => {
                    println!("Output: {:?}", output);
                }
                Err(err) => {
                    println!("Failed to bring up reseda server, {:?}", err);
                }
        }
    }

    #[deprecated = "OLD_CODE"]
    pub async fn config_sync(&mut self) -> &mut Self {
        match Command::new("wg")
            .env("export WG_I_PREFER_BUGGY_USERSPACE_TO_POLISHED_KMOD", "1")
            .args(["syncconf", "reseda", "<(wg-quick strip reseda)"]).output() {
                Ok(output) => {
                    println!("Output: {:?}", output);
                }
                Err(err) => {
                    println!("Failed to bring up reseda server, {:?}", err);
                }
        }

        self
    }

    pub async fn config_up(&self) -> bool {
        match Command::new("wg-quick")
            .env("export WG_I_PREFER_BUGGY_USERSPACE_TO_POLISHED_KMOD", "1")    
            .args(["up", "reseda"]).output() {
                Ok(output) => {
                    println!("Output: {:?}", output);
                    true
                }
                Err(err) => {
                    println!("Failed to bring up reseda server, {:?}", err);
                    false
                }
        }
    }

    pub async fn config_down(&self) -> bool {
        match Command::new("wg-quick")
            .env("export WG_I_PREFER_BUGGY_USERSPACE_TO_POLISHED_KMOD", "1")    
            .args(["down", "reseda"])
            .output() {
            Ok(output) => {
                println!("Output: {:?}", output);
                true
            }
            Err(err) => {
                println!("Failed to take down reseda server, {:?}", err);
                false
            }
        }
    }
}
