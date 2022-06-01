use std::{collections::HashMap, sync::Arc};
use tokio::sync::{mpsc, Mutex};
use warp::ws::Message;

use super::Usage;

#[derive(Debug, Clone)]
pub enum Maximums {
    Free,
    Supporter,
    Basic,
    Pro,
    Unassigned
} 

impl Maximums {
    pub fn to_value(&self) -> i128 {
        match self {
            Self::Free => 5368709120,
            Self::Supporter => 53687091200,
            Self::Basic => -1,
            Self::Pro => -1,
            Self::Unassigned => 0
        } 
    }
}

#[derive(Debug, Clone)]
pub struct Client {
    pub author: String,
    pub public_key: String,
    pub sender: Option<mpsc::UnboundedSender<std::result::Result<Message, warp::Error>>>,
    pub maximums: Maximums,
    usage: Usage,
    valid_pk: bool,
    pub connected: bool
}

impl Client {
    pub fn set_connectivity(&mut self, new_status: bool) -> &mut Self {
        self.connected = new_status;

        self
    }

    pub fn set_public_key(mut self, public_key: String) -> Self {
        if public_key.len() == 44 && public_key.ends_with("=") {
            self.public_key = public_key.replace(" ", "+");
            self.valid_pk = true;
        }

        self
    }

    pub fn expose_client_sender(&self) -> &Option<mpsc::UnboundedSender<std::result::Result<Message, warp::Error>>> {
        &self.sender
    }

    pub fn is_valid(&self) -> bool {
        self.valid_pk
    }

    pub fn set_usage(&mut self, up: &i64, down: &i64) -> &mut Self {
        self.usage.down = *down;
        self.usage.up = *up;

        self
    }

    pub fn new(sender: Option<mpsc::UnboundedSender<std::result::Result<Message, warp::Error>>>) -> Self {
        Client { 
            author: "".to_string(), 
            public_key: "".to_string(), 
            sender: sender, 
            maximums: Maximums::Unassigned, 
            usage: Usage { up: 0, down: 0 },
            connected: false,
            valid_pk: false
        }
    }
}

pub type Clients = Arc<Mutex<HashMap<String, Client>>>;