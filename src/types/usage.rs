#[derive(Debug, Clone)]
pub struct Usage {
    pub up: i64,
    pub down: i64
}

pub type UsageMap = Arc<Mutex<HashMap<String, Usage>>>;