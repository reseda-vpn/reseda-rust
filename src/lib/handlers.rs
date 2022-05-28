use crate::{Clients, Result, types::QueryParameters};
use warp::Reply;

use super::client_connection;

pub async fn ws_handler(ws: warp::ws::Ws, clients: Clients, parameters: Option<QueryParameters>) -> Result<impl Reply> {
    Ok(ws.on_upgrade(move |socket| client_connection(socket, clients, parameters)))
}