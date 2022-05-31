use std::borrow::BorrowMut;

use crate::{Clients, types::{self, Query, QueryParameters, Client}, wireguard::{WireGuard}};
use futures::{FutureExt, StreamExt};
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use warp::ws::{Message, WebSocket};

pub async fn client_connection(ws: WebSocket, config: WireGuard, parameters: Option<QueryParameters>) {
    let (client_ws_sender, mut client_ws_rcv) = ws.split();
    let (client_sender, client_rcv) = mpsc::unbounded_channel();

    let client_rcv = UnboundedReceiverStream::new(client_rcv);

    tokio::task::spawn(client_rcv.forward(client_ws_sender).map(|result| {
        if let Err(e) = result {
            println!("[ERROR] In: Sending WebSocket Message '{}'", e);
        }
    }));

    match &parameters {
        Some(obj) => {
            println!("Provided parameters: {:?}", obj);

            let client = Client::new(Some(client_sender))
                .set_public_key(obj.public_key.clone());

            match &client.is_valid() { 
                true => {
                    println!("Created Client: {:?}", client);

                    config.lock().await.clients.lock().await.insert(obj.author.clone(), client);

                    while let Some(result) = client_ws_rcv.next().await {
                        let msg = match result {
                            Ok(msg) => msg,
                            Err(e) => {
                                println!("[ERROR] Receiving message for id {}: {}", obj.author.clone(), e);
                                break;
                            }
                        };
                
                        client_msg(&obj.author, msg, &config).await;
                    }
                
                    config.lock().await.clients.lock().await.remove(&obj.author.clone());
                }
                false => {
                    let owned_client = client.expose_client_sender();

                    match owned_client {
                        Option::Some(sender) => {
                            match sender.send(Ok(Message::text("Invalid public key, expected 44 characters."))) {
                                Ok(_) => {}
                                Err(e) => {
                                    println!("Failed to send message: \'INVALID_PUBLIC_KEY\', reason: {}", e)
                                }
                            }
                        }
                        Option::None => {
                            println!("Client does not contain avaliable webosocket sender.")
                        }
                    }
                    
                    return;
                }
            };

            println!("[evt]: Client Removed Successfully");
        }
        None => {
            println!("[ERROR] Unable to parse parameters given, {:?}", &parameters);
        }
    };
}

async fn client_msg(client_id: &str, msg: Message, config: &WireGuard) {
    let message = match msg.to_str() {
        Ok(v) => v,
        Err(_) => return,
    };

    let json: types::StartQuery = match serde_json::from_str(message) {
        Ok(v) => v,
        Err(e) => {
            return return_to_sender(&config.lock().await.clients, client_id, format!("{{ \"message\": \"{}\", \"type\": \"error\" }}", e)).await;
        }
    };

    match json.query_type {
        Query::Close => {
            let configuration = config.lock().await;

            println!("Closing the socket & wireguard conn.");
            let mut locked = configuration.clients.lock().await;

            match locked.get_mut(client_id) {
                Some(v) => {
                    v.set_connectivity(false);
                }
                None => (),
            }

            drop(locked);
            drop(configuration);

            println!("Set. restarting...");
            // Remove Client / Kill Websocket Connection, then update config.
            config.lock().await.save_config(true).await;
            println!("Done!");
        },
        Query::Open => {
            let configuration = config.lock().await;

            println!("Opening the socket & wireguard conn.");
            let mut locked = configuration.clients.lock().await;

            match locked.get_mut(client_id) {
                Some(v) => {
                    v.set_connectivity(true);
                }
                None => (),
            }

            drop(locked);
            drop(configuration);

            println!("Set. restarting...");
            config.lock().await.borrow_mut().save_config(true).await;
            println!("Done!");
        },
        Query::Resume => {
            println!("Resuming the socket & wireguard conn.");
            // No need to adjust config if resuming...
        },
        _ => {
            return return_to_sender(&config.lock().await.clients, client_id, format!("{{ \"message\": \"Unknown query_type, expected one of open, close, resume.\", \"type\": \"error\" }}")).await;
        }
    }

    let configuration = config.lock().await;

    let locked = configuration.clients.lock().await;
    match locked.get(client_id) {
        Some(v) => {
            println!("Client: {:?}", v);
        }
        None => println!("Unable to find client."),
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