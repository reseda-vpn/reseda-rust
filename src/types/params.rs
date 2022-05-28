use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug)]
pub struct QueryParameters {
    pub author: String,
    pub public_key: String,
}