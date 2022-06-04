use serde::{Serialize, Deserialize};
use std::process::{Command, Stdio};
use std::io::{Write};

#[derive(Serialize, Deserialize, Clone)]
pub struct WireGuardConfigFile {
    pub address: String,
    pub name: String,
    pub post_up: String,
    pub post_down: String,
    pub listen_port: i32,
    pub dns: String,
    pub database_url: String
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