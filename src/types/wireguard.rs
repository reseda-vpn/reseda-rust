use serde::{Serialize, Deserialize};
use std::env;
use std::process::{Command, Stdio};
use std::io::{Write};

#[derive(Serialize, Deserialize, Clone)]
pub struct WireGuardConfigFile {
    pub address: String,
    pub name: String,
    pub post_up: String,
    pub post_down: String,
    pub listen_port: String,
    pub dns: String,
    pub database_url: String,

    pub location: String,
    pub country: String,
    pub flag: String
}

impl WireGuardConfigFile {
    pub async fn from_environment() -> Self {
        let location = match env::var("LOCATION") {
            Ok(value) => value,
            Err(_) => panic!("Could not identify environment variable: LOCATION")
        };

        let country = match env::var("COUNTRY") {
            Ok(value) => value,
            Err(_) => panic!("Could not identify environment variable: COUNTRY")
        };

        let flag = match env::var("FLAG") {
            Ok(value) => value,
            Err(_) => panic!("Could not identify environment variable: COUNTRY")
        };

        let name = match env::var("NAME") {
            Ok(value) => value,
            Err(_) => panic!("Could not identify environment variable: NAME")
        };

        let database_url = match env::var("DATABASE_URL") {
            Ok(value) => value,
            Err(_) => panic!("Could not identify environment variable: DATABASE_URL")
        };

        match public_ip::addr().await {
            Some(ip) => {
                let ip_addr = ip.to_string();

                Self {
                    name: name,
                    address: ip_addr,
                    post_up: "iptables -A FORWARD -i reseda -j ACCEPT; iptables -t nat -A POSTROUTING -o eth0 -j MASQUERADE".to_string(),
                    post_down: "iptables -A FORWARD -i reseda -j ACCEPT; iptables -t nat -A POSTROUTING -o eth0 -j MASQUERADE".to_string(),
                    dns: "1.1.1.1".to_string(),
                    listen_port: "51820".to_string(),
                    database_url: database_url,

                    location: location,
                    country: country,
                    flag: flag
                }
            },
            None => panic!("[err]: Unable to retrieve IP address.")
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct KeyState {
    pub private_key: String,
    pub public_key: String,
}

impl KeyState {
    pub fn generate_pair() -> Self {
        // Generate Private Key
        let exec_process = Command::new("wg")
            .arg("genkey")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("Failed to generate private key.");

        let output = exec_process.wait_with_output().expect("Failed to read stdout");
        let private_key = String::from_utf8(output.stdout.to_vec()).unwrap();

        let clone_key = private_key.clone();

        // Generate Public Key
        let mut exec_process = Command::new("wg")
            .arg("pubkey")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("Failed to generate public key");

        let mut stdin = exec_process.stdin.take().expect("Failed to open stdin");
        std::thread::spawn(move || {
            stdin.write_all(&clone_key.as_bytes()).expect("Failed to write to stdin");
        });

        let output = exec_process.wait_with_output().expect("Failed to read stdout");
        let public_key = String::from_utf8(output.stdout.to_vec()).unwrap();

        KeyState {
            private_key: private_key,
            public_key: public_key
        }
    }
}