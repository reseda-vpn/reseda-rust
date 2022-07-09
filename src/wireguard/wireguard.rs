use crate::types::{WireGuardConfigFile, Clients, KeyState, Client, Host, Reservation, Slot, Connection};
use std::collections::BTreeMap;
use std::os::raw::c_float;
use std::{collections::HashMap, sync::Arc};
use serde::{Deserialize, Serialize};
use sqlx::mysql::MySqlPoolOptions;
use sqlx::{Pool, MySql};
use tokio::sync::{Mutex};
use std::process::{Command};
use chrono::Utc;
use reqwest;

use std::fs::File;
use std::io::Write;

use std::fs;

pub type WireGuard = Arc<Mutex<WireGuardConfig>>;

#[derive(Clone)]
pub struct WireGuardConfig {
    pub config: WireGuardConfigFile,
    pub keys: KeyState,
    pub clients: Clients,

    pub pool: Pool<MySql>,
    pub registry: BTreeMap<u8, BTreeMap<u8, bool>>,
    pub internal_addr: String,

    pub information: RegistryReturn
}

#[derive(Deserialize, Debug, Serialize, Clone)]
pub struct IpResponse {
    pub country: String,
    pub region: String,
    pub eu: bool,
    pub city: String,
    pub latitude: c_float,
    pub longitude: c_float,
    pub metro: i16,
    pub radius: i16,
    pub timezone: String
}

#[derive(Serialize, Deserialize, Clone)]
pub struct RegistryReturn {
    pub key: String,
    pub cert: String,
    pub ip: String,

    pub record_id: String,
    pub cert_id: String,

    pub res: IpResponse,
    pub id: String
}

impl WireGuardConfig {
    pub async fn initialize() -> Self {
        // Import configuration from environment
        let res = WireGuardConfigFile::from_environment().await;
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

        let registry_return = WireGuardConfig::register_server(&res).await;

        // Return Configuration
        WireGuardConfig {
            config: res,
            keys: keys,
            clients: Arc::new(Mutex::new(HashMap::new())),
            pool: pool,
            registry: registry,
            internal_addr: "10.8.2.1".to_string(),
            information: registry_return
        }
    }

    pub async fn register_server(config: &WireGuardConfigFile) -> RegistryReturn {
        let client = reqwest::Client::new();

        let information = match client.post(format!("https://mesh.reseda.app/register/{}", config.address))
            .body(format!("
            {{
                \"auth\": \"{}\"
            }}", config.access_key))
            .header("Content-Type", "application/json")
            .send().await {
                Ok(content) => {
                    content.text().await.expect("Failed to convert response object to text")
                },
                Err(err) => {
                    panic!("[err]: Error in setting non-proxied DNS {}", err)
                },
            };
        
        let registration_return: RegistryReturn = match serde_json::from_str(&information) {
            Ok(val) => val,
            Err(err) => panic!("{}", format!("Failed to parse server registration: {:?}", err))
        };

        match File::create("key.pem") {
            Ok(mut output) => {
                match write!(output, "{}", registration_return.key) {
                    Ok(_) => {},
                    Err(err) => {
                        println!("[err]: Unable to write file file::key.pem; {}", err);
                    },
                }
            },
            Err(err) => {
                println!("[err]: Unable to open file stream for file::key.pem; {}", err)
            },
        };

        match File::create("cert.pem") {
            Ok(mut output) => {
                match write!(output, "{}", registration_return.cert) {
                    Ok(_) => {},
                    Err(err) => {
                        println!("[err]: Unable to write file file::cert.pem; {}", err);
                    },
                }
            },
            Err(err) => {
                println!("[err]: Unable to open file stream for file::cert.pem; {}", err)
            },
        };

        registration_return
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
            Result::Err(err) => {
                println!("[err]: Unable to write configuration. Reason: {:?}", err);
            },
            Result::Ok(_) => {}
        }
        
        if should_restart {
            WireGuardConfig::restart_config(self).await;
        }

        // Using:: https://gist.github.com/qdm12/4e0e4f9d1a34db9cf63ebb0997827d0d
        // Try to implement localized security policies such that data cannot be shared domestically
        // and users on the VPN cannot access eachother.

        match self.reserve_slot(Host { a: 2, b: 1, conn_time: Utc::now() }) {
            Reservation::Held(reservation) => println!("[reserver]: Default Server Slot held; {:?}", reservation),
            Reservation::Detached(detached) => println!("[reserver]: Error, Slot debounced as detached. {:?}", detached),
            Reservation::Imissable => println!("[reserver]: Error, Slot returned IMISSABLE"),
        }

        self
    }

    pub async fn generate_config_string(&self) -> String {
        let mut elems = vec!["[Interface]".to_string()];
        elems.push(format!("Address = {}/24", &self.internal_addr));
        elems.push(format!("PrivateKey = {}", &self.keys.private_key.trim()));
        elems.push(format!("ListenPort = {}", &self.config.listen_port));
        elems.push(format!("DNS = {}", &self.config.dns));
        elems.push(format!("PostUp = {}", &self.config.post_up));
        elems.push(format!("PostDown = {}", &self.config.post_down));

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
                    println!("[wg]: Remove Peer {:?}", output);
                }
                Err(err) => {
                    println!("[wg]: Failed to bring up reseda server, {:?}", err);
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
                            println!("[wg]: Add Peer: {:?}", output);
                        }
                        Err(err) => {
                            println!("[wg]: Failed to bring up reseda server, {:?}", err);
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
                    println!("[wg]: wg-quick up: {:?}", output);
                    true
                }
                Err(err) => {
                    println!("[wg]: Failed to bring up reseda server, {:?}", err);
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
                println!("[wg] wg-quick down: {:?}", output);
                true
            }
            Err(err) => {
                println!("[wg]: Failed to take down reseda server, {:?}", err);
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
                                match value {
                                    false => {
                                        // Pre-emptive return, we have found an open slot and we can reserve it from here.
                                        return Slot::Open(Host { a: i, b: k, conn_time: Utc::now() })
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
