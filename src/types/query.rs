use serde::{Deserialize};

#[derive(Debug, Deserialize)]
pub enum Query {
    Open,
    Close,
    Resume
}

#[derive(Debug, Deserialize)]
pub struct StartQuery {
    pub query_type: Query,
    // Mapping the following two values together.
    pub client_pub_key: String,
    pub author: String,
}
