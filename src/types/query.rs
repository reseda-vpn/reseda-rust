use serde::{Deserialize, Deserializer};

#[derive(Debug)]
pub enum Query {
    Open,
    Close,
    None
}

#[derive(Debug, Deserialize)]
pub struct StartQuery {
    pub query_type: Query
}

impl<'de> Deserialize<'de> for Query {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?.to_lowercase();
        let state = match s.as_str() {
            "open" => Query::Open,
            "close" => Query::Close,
            _ => Query::None,
        };
        Ok(state)
    }
}