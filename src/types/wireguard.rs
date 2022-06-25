use serde::{Serialize, Deserialize};
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
    pub auth_token: String,

    pub location: String,
    pub country: String,
    pub flag: String
}

impl WireGuardConfigFile {
    pub async fn from_environment() -> Self {
        let mut settings = config::Config::default();

        let base_path = std::env::current_dir().expect("Failed to determine the current directory");
        let configuration_directory = base_path.join("configuration");

        match settings.merge(config::File::from(configuration_directory.join("base")).required(true)) {
            Ok(config) => {},
            Err(err) => println!("[err]: Loading environment. Reason: {:?}", err)
        }

        match settings.merge(config::File::from(configuration_directory.join("region")).required(true)) {
            Ok(config) => {},
            Err(err) => println!("[err]: Loading environment. Reason: {:?}", err)
        }

        let database_url = match settings.get_str("database_auth") {
            Ok(val) => val,
            Err(_) => panic!()
        };

        let auth_token = match settings.get_str("mesh_auth") {
            Ok(val) => val,
            Err(_) => panic!()
        };

        let location = match settings.get_str("location") {
            Ok(val) => val,
            Err(_) => panic!()
        };

        let country = match settings.get_str("country") {
            Ok(val) => val,
            Err(_) => panic!()
        };

        let flag = match settings.get_str("flag") {
            Ok(val) => val,
            Err(_) => panic!()
        };

        let name = match settings.get_str("region") {
            Ok(val) => val,
            Err(_) => panic!()
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
                    auth_token: auth_token,

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