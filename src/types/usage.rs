use std::{collections::HashMap, sync::Arc};
use tokio::sync::{Mutex};

#[derive(Debug, Clone, Copy)]
pub struct Usage {
    pub up: i64,
    pub down: i64
}

pub type UsageMap = Arc<Mutex<HashMap<String, Usage>>>;