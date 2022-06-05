use crate::types::{WireGuardConfigFile, Clients, KeyState, Client, Host, Reservation, Slot, Connection};
use std::collections::BTreeMap;
use std::{collections::HashMap, sync::Arc};
use sqlx::mysql::MySqlPoolOptions;
use sqlx::{Pool, MySql};
use tokio::sync::{Mutex};
use std::process::{Command};

use std::fs;
use serde_json;

pub type WireGuard = Arc<Mutex<WireGuardConfig>>;

#[derive(Clone)]
pub struct WireGuardConfig {
    pub config: WireGuardConfigFile,
    pub keys: KeyState,
    pub clients: Clients,
    pub pool: Pool<MySql>,
    pub registry: BTreeMap<u8, BTreeMap<u8, bool>>
}

impl WireGuardConfig {
    pub async fn load_from_config(file_path: &str) -> Self {
        // Load local config file as string
        let data = fs::read_to_string(file_path).expect("Unable to read file");
        // Convert to JSON
        let res: WireGuardConfigFile = serde_json::from_str(&data).expect("Unable to parse");
        // Generate Keys
        let keys = KeyState::generate_pair();
        // Initialize IP Registry (maps 65025 possible IP addresses)
        let registry = WireGuardConfig::init_registry(12);

        let pool = match MySqlPoolOptions::new()
            .max_connections(5)
            .connect(&res.database_url).await {
                Ok(pool) => {
                    println!("[service] sqlx::success Successfully started pool.");
                    pool
                },
                Err(error) => {
                    panic!("[service] sqlx::error Failed to initialize SQLX pool. Reason: {}", error);
                }
        };

        // Return Configuration
        WireGuardConfig {
            config: res,
            keys: keys,
            clients: Arc::new(Mutex::new(HashMap::new())),
            pool: pool,
            registry: registry
        }
    }

    pub fn init_registry(highest: u8) -> BTreeMap<u8, BTreeMap<u8, bool>> {
        let mut registry: BTreeMap<u8, BTreeMap<u8, bool>> = BTreeMap::new();

        for i in 2..highest {
            let mut new_map = BTreeMap::new();

            for k in 1..255 {
                new_map.insert(k, false);
            }

            registry.insert(i, new_map);
        }

        registry
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

        match self.reserve_slot(Host { a: 2, b: 1 }) {
            Reservation::Held(reservation) => println!("Default Server Slot held; {:?}", reservation),
            Reservation::Detached(detached) => println!("Slot debounced as detached. {:?}", detached),
            Reservation::Imissable => println!("Slot returned IMISSABLE"),
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

        // Only used on initialization, no peers should be added this way.
        // for (_, value) in self.clients.lock().await.iter() {
        //     match value.connected {
        //         Connection::Connected(_) => {
        //             elems.push("\n".to_string());
        //             elems.push("[Peer]".to_string());   
        //             elems.push(format!("PublicKey = {}", value.public_key));
        //             // TODO: Replace allowed IP address with a dynamically assigned address
        //             elems.push(format!("AllowedIPs = 192.168.69.{}/24", 2));
        //             elems.push(format!("PersistentKeepalive = 25"));
        //         }
        //         Connection::Disconnected => {}
        //     }
        // };

        elems.join("\n")
    }

    pub async fn restart_config(&mut self) -> bool {
        let down_status = &self.config_down().await;
        let up_status = &self.config_up().await;

        *up_status && *down_status
    }
    
    pub async fn remove_peer(&self, client: &Client) {
        match Command::new("wg")
            .env("export WG_I_PREFER_BUGGY_USERSPACE_TO_POLISHED_KMOD", "1")
            .args(["set", "reseda", "peer", &client.public_key, "remove"]).output() {
                Ok(output) => {
                    println!("Output: {:?}", output);
                }
                Err(err) => {
                    println!("Failed to bring up reseda server, {:?}", err);
                }
        }
    }

    pub async fn add_peer(&self, client: &Client) {
        match &client.connected {
            Connection::Disconnected => {
                println!("[err]: Attempted to add peer that was DISCONNECTED.")
            },
            Connection::Connected(connection) => {
                match Command::new("wg")
                    .env("export WG_I_PREFER_BUGGY_USERSPACE_TO_POLISHED_KMOD", "1")
                    .args(["set", "reseda", "peer", &client.public_key, "allowed-ips", &format!("10.8.{}.{}", connection.a, connection.b), "persistent-keepalive", "25"]).output() {
                        Ok(output) => {
                            println!("Output: {:?}", output);
                        }
                        Err(err) => {
                            println!("Failed to bring up reseda server, {:?}", err);
                        }
                }
            },
        }
        
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

    pub fn find_open_slot(&self) -> Slot {
        for i in 2..self.registry.len() as u8 {
            for k in 1..255 {
                match self.registry.get(&i) {
                    Some(a_val) => {
                        match a_val.get(&k) {
                            Some(value) => {
                                println!("Choosing {:?}::{}", Host { a: i, b: k }, value);

                                match value {
                                    false => {
                                        // Pre-emptive return, we have found an open slot and we can reserve it from here.
                                        return Slot::Open(Host { a: i, b: k })
                                    }
                                    true => {}
                                }
                            }
                            None => {}
                        }
                    },
                    None => {},
                }
            }
        }

        Slot::Prospective
    }

    pub fn reserve_slot(&mut self, requested_slot: Host) -> Reservation {
        match self.registry.get_mut(&requested_slot.a) {
            Some(slot_a) => {
                match slot_a.get_key_value(&requested_slot.b) {
                    Some(slot_b) => {
                        match slot_b.1 {
                            &false => {
                                slot_a.entry(requested_slot.b).and_modify(| val | { *val = true });
                                Reservation::Held(requested_slot)
                            }
                            &true => { 
                                println!("[err]: Assigning slot {:?} failed. Reason: Slot had value TRUE", requested_slot);
                                Reservation::Detached(requested_slot)
                            }
                        }
                    }
                    None => {
                        println!("[err]: Assigning slot {:?} failed. Reason: Slot did not have a valid/existing b value.", requested_slot);
                        Reservation::Detached(requested_slot)
                    }
                }
            }
            None => {
                println!("[err]: Assigning slot {:?} failed. Reason: Slot did not have a valid/existing a value.", requested_slot);
                Reservation::Detached(requested_slot)
            },
        }
    }

    pub fn free_slot(&mut self, freeing_slot: &Host) {
        self.registry.entry(freeing_slot.a)
            .and_modify(| val | { 
                val.entry(freeing_slot.b)
                    .and_modify(| val2 |  {
                         *val2 = false 
                    }); 
            });
    }
}
