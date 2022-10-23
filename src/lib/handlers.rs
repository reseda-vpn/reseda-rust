use std::convert::Infallible;

use crate::{Result as WsResult, types::QueryParameters, wireguard::WireGuard};
use serde::Serialize;
use warp::reply::json as json_reply;
use warp::Reply;
use warp::{http::StatusCode};

use super::client_connection;

pub async fn ws_handler(ws: warp::ws::Ws, config: WireGuard, parameters: Option<QueryParameters>) -> WsResult<impl Reply> {
    Ok(ws.on_upgrade(move |socket| client_connection(socket, config, parameters)))
}

#[derive(Serialize, Debug)]
pub struct NodeResponse {
    // The nodes current information so we can verify it is ready to be publicized 
    pub status: String,
    pub usage: String,

    // This is information the client has which we request back so that we can verify the server which was booted **matches** the one we have in the local storage
    pub ip: String,
    pub cert: String,
    pub record_id: String
}

pub async fn health_status(config: WireGuard) -> Result<Box<dyn warp::Reply>, Infallible> {
    println!("Received health call, obtaining datalock...");
    let data = config.lock().await;
    println!("Obtained datalock...");
    
    let health_response = NodeResponse { 
        status: "OK".to_string(),
        usage: data.clients.lock().await.len().to_string(),

        ip: data.information.ip.clone(),
        cert: data.information.mim.clone(),
        record_id: data.information.record_id.clone()
    };

    Ok(Box::new(json_reply(&health_response)))
}

pub async fn echo() -> Result<Box<dyn warp::Reply>, Infallible> {
    Ok(Box::new(StatusCode::OK))
}