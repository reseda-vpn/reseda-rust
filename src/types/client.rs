use std::{collections::HashMap, sync::Arc};
use tokio::sync::{mpsc, Mutex};
use warp::ws::Message;

#[derive(Debug, Clone)]
pub struct Maximums {
    pub up: i64,
    pub down: i64
}

#[derive(Debug, Clone)]
pub struct Client {
    pub author: String,
    pub public_key: String,
    pub sender: Option<mpsc::UnboundedSender<std::result::Result<Message, warp::Error>>>,
    pub maximums: Maximums
}

pub type Clients = Arc<Mutex<HashMap<String, Client>>>;