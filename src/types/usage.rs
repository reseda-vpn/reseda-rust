#[derive(Debug, Clone, Copy)]
pub struct Usage {
    pub up: i128,
    pub down: i128
}

// pub type UsageMap = Arc<Mutex<HashMap<String, Usage>>>;