use crate::{Result, types::QueryParameters, wireguard::WireGuard};
use warp::Reply;

use super::client_connection;

pub async fn ws_handler(ws: warp::ws::Ws, config: WireGuard, parameters: Option<QueryParameters>) -> Result<impl Reply> {
    Ok(ws.on_upgrade(move |socket| client_connection(socket, config, parameters)))
}