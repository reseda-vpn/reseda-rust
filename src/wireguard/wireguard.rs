use crate::types::{WireGuardConfigFile, UsageMap, Clients, KeyState};
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
    pub usage_map: UsageMap,
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

        // let exec_process = Command::new("wg syncconf ./configs/reseda.conf <(wg-quick strip ./configs/reseda.conf)")
        //     .stdin(Stdio::piped())
        //     .stdout(Stdio::piped())
        //     .spawn()
        //     .expect("Failed to generate private key.");

        // let output = exec_process.wait_with_output().expect("Failed to read stdout");
        // println!("SYNC: {:?}", output);

        // Return Configuration
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

        WireGuardConfig::restart_config(self, false).await;

        self
    }

    pub async fn generate_config_string(&self) -> String {
        let mut elems = vec!["[Interface]".to_string()];
        elems.push(format!("Address = {}", &self.config.address));
        elems.push(format!("PrivateKey = {}", &self.keys.private_key.trim()));
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

    pub async fn restart_config(&mut self, take_down: bool) {
        if take_down {
            let down_status = &self.config_down().await;
            println!("Taking Down: {}", down_status);
        }

        let up_status = &self.config_up().await;
        println!("Bringing Up: {}", up_status);
    }

    pub async fn config_up(&self) -> bool {
        match Command::new("wg-quick")
            .env("export WG_I_PREFER_BUGGY_USERSPACE_TO_POLISHED_KMOD", "1")    
            .args(["up", "./configs/reseda.conf"]).output() {
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

    async fn config_down(&self) -> bool {
        match Command::new("wg-quick")
            .env("export WG_I_PREFER_BUGGY_USERSPACE_TO_POLISHED_KMOD", "1")    
            .args(["down", "./configs/reseda.conf"])
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
