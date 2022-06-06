use std::{collections::HashMap, sync::Arc};
use chrono::{Utc, DateTime};
use tokio::sync::{mpsc, Mutex};
use warp::ws::Message;

use super::Usage;

#[derive(Debug, Clone, PartialEq)]
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
            // 5GB
            Self::Free => 5368709120,

            // 50GB
            Self::Supporter => 53687091200,

            // -1 means IGNORE for the time, such that it does not have a data cap.
            Self::Basic => -1,
            Self::Pro => -1,

            // This is the state that occurs when a user connects but is awaiting their tier to be assigned.
            // We give them a small allowance first, without having a verified account, this is small enough
            // that it cant be abused, but is large enough that it can swallow an up to 500ms wait time 
            // for the query response in data usage. (5mb of information bandwidth)
            Self::Unassigned => 5000000
        } 
    }
}

// By choosing integers with the propper bounds, we cannot go out of bounds of the IP scope.
#[derive(Debug, Clone)]
pub struct Host {
    pub a: u8,
    pub b: u8,
    pub conn_time: DateTime<Utc>
}

#[derive(Debug, Clone)]
pub enum Reservation {
    Held(Host),
    Detached(Host),
    Imissable 
}

#[derive(Debug, Clone)]
pub enum Slot {
    Open(Host),
    Prospective
}

// #[derive(Debug, Clone)]
// struct Connection {
//     pub connected: bool,
//     pub host: Host
// }

#[derive(Debug, Clone)]
pub enum Connection {
    Disconnected,
    Connected(Host)
}

#[derive(Debug, Clone)]
pub struct Client {
    pub author: String,
    pub public_key: String,
    pub sender: Option<mpsc::UnboundedSender<std::result::Result<Message, warp::Error>>>,
    pub maximums: Maximums,
    pub connected: Connection,

    usage: Usage,
    valid_pk: bool,
}

impl Client {
    pub fn set_connectivity(&mut self, new_status: Connection) -> &mut Self {
        self.connected = new_status;

        self
    }

    pub fn get_usage(&self) -> (i128, i128) {
        (self.usage.down, self.usage.up)
    }

    pub fn set_public_key(mut self, public_key: String) -> Self {
        if public_key.len() == 44 && public_key.ends_with("=") {
            self.public_key = public_key.replace(" ", "+");
            self.valid_pk = true;
        }

        self
    }

    pub fn set_author(mut self, author_id: String) -> Self {
        self.author = author_id;

        self
    }

    pub fn expose_client_sender(&self) -> &Option<mpsc::UnboundedSender<std::result::Result<Message, warp::Error>>> {
        &self.sender
    }

    pub fn is_valid(&self) -> bool {
        self.valid_pk
    }

    pub fn set_usage(&mut self, up: &i128, down: &i128) -> bool {
        self.usage.down = *down;
        self.usage.up = *up;

        if self.maximums == Maximums::Pro || self.maximums == Maximums::Basic {
            return true;
        }

        // If plan is not PRO or BASIC, do The following to check if they have exceeded thier bandwith allocation.
        let max: i128 = self.maximums.to_value();

        if max > *up && max > *down {
            false
        }else {
            true
        }
    }

    pub fn set_tier(&mut self, tier: Maximums) -> &mut Self {
        self.maximums = tier;

        self
    }

    pub fn new(sender: Option<mpsc::UnboundedSender<std::result::Result<Message, warp::Error>>>) -> Self {
        Client { 
            author: "".to_string(), 
            public_key: "".to_string(), 
            sender: sender, 
            maximums: Maximums::Unassigned, 
            usage: Usage { 
                up: 0, 
                down: 0 
            },
            connected: Connection::Disconnected,
            valid_pk: false
        }
    }
}

pub type Clients = Arc<Mutex<HashMap<String, Client>>>;