use crate::{Clients, types::{self, Query, QueryParameters, Maximums, Client}};
use futures::{FutureExt, StreamExt};
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use uuid::Uuid;
use warp::ws::{Message, WebSocket};

pub async fn client_connection(ws: WebSocket, clients: Clients, parameters: Option<QueryParameters>) {
    let (client_ws_sender, mut client_ws_rcv) = ws.split();
    let (client_sender, client_rcv) = mpsc::unbounded_channel();

    let client_rcv = UnboundedReceiverStream::new(client_rcv);

    tokio::task::spawn(client_rcv.forward(client_ws_sender).map(|result| {
        if let Err(e) = result {
            println!("[ERROR] In: Sending WebSocket Message '{}'", e);
        }
    }));

    // let uuid = Uuid::new_v4().to_simple().to_string();

    match &parameters {
        Some(obj) => {
            println!("Provided parameters: {:?}", obj);

            // Add with content from connection / query.
            let client = Client {
                author: obj.author.clone(),
                public_key: obj.public_key.clone(),
                sender: Some(client_sender),
                maximums: Maximums { 
                    up: 15, 
                    down: 16 
                }
            };

            println!("Created Client: {:?}", client);

            clients.lock().await.insert(obj.author.clone(), client);

            while let Some(result) = client_ws_rcv.next().await {
                let msg = match result {
                    Ok(msg) => msg,
                    Err(e) => {
                        println!("[ERROR] Receiving message for id {}: {}", obj.author.clone(), e);
                        break;
                    }
                };
        
                client_msg(&obj.author, msg, &clients).await;
            }
        
            clients.lock().await.remove(&obj.author.clone());
        
            println!("[evt]: Client Removed Successfully");
        }
        None => {
            println!("[ERROR] Unable to parse parameters given, {:?}", &parameters);
        }
    };
}

async fn client_msg(client_id: &str, msg: Message, clients: &Clients) {
    let message = match msg.to_str() {
        Ok(v) => v,
        Err(_) => return,
    };

    let json: types::StartQuery = match serde_json::from_str(message) {
        Ok(v) => v,
        Err(e) => {
            return return_to_sender(clients, client_id, format!("{{ \"message\": \"{}\", \"type\": \"error\" }}", e)).await;
        }
    };

    match json.query_type {
        Query::Close => {
            println!("Closing the socket & wireguard conn.");
        },
        Query::Open => {
            println!("Opening the socket & wireguard conn.");
        },
        Query::Resume => {
            println!("Resuming the socket & wireguard conn.");
        },
        _ => {
            return return_to_sender(clients, client_id, format!("{{ \"message\": \"Unknown query_type, expected one of open, close, resume.\", \"type\": \"error\" }}")).await;
        }
    }
}

async fn return_to_sender(clients: &Clients, client_id: &str, message: String) {
    let locked = clients.lock().await;

    match locked.get(client_id) {
        Some(v) => {
            if let Some(sender) = &v.sender {
                let _ = sender.send(Ok(Message::text(message)));
            }
        }
        None => (),
    }
}