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
            let client = Client::new(Some(client_sender))
                .set_public_key(obj.public_key.clone());

            match &client.is_valid() { 
                true => {
                    println!("Created Client: {:?}", client);

                    let owned_client = &client.expose_client_sender();

                    match owned_client {
                        Option::Some(sender) => {
                            match sender.send(Ok(Message::text(format!("{{ \"message\": \"PUBLIC_KEY_OK\", \"type\": \"message\" }}")))) {
                                Ok(_) => {}
                                Err(e) => {
                                    println!("Failed to send message: \'INVALID_PUBLIC_KEY\', reason: {}", e)
                                }
                            }
                        }
                        Option::None => {
                            println!("Client does not contain available websocket sender.")
                        }
                    }

                    let pk = client.public_key.clone();

                    config.lock().await.clients.lock().await.insert(pk.clone(), client);

                    while let Some(result) = client_ws_rcv.next().await {
                        let msg = match result {
                            Ok(msg) => msg,
                            Err(e) => {
                                println!("[ERROR] Receiving message for id {}: {}", obj.author.clone(), e);
                                break;
                            }
                        };
                
                        client_msg(&pk, msg, &config).await;
                    }
                
                    config.lock().await.clients.lock().await.remove(&obj.author.clone());
                }
                false => {
                    let owned_client = client.expose_client_sender();

                    match owned_client {
                        Option::Some(sender) => {
                            match sender.send(Ok(Message::text(format!("{{ \"message\": \"Invalid public key, expected 44 characters.\", \"type\": \"error\" }}")))) {
                                Ok(_) => {}
                                Err(e) => {
                                    println!("Failed to send message: \'INVALID_PUBLIC_KEY\', reason: {}", e)
                                }
                            }
                        }
                        Option::None => {
                            println!("Client does not contain available websocket sender.")
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
            let mut locked = configuration.clients.lock().await;

            match locked.get_mut(client_id) {
                Some(v) => {
                    v.set_connectivity(false);
                    configuration.remove_peer(v).await;
                }
                None => (),
            }

            drop(locked);
            drop(configuration);

            let temp = &config.lock().await;
            let message = format!("{{ \"message\": \"Removed client successfully.\", \"type\": \"success\" }}");

            let locked = temp.clients.lock().await;

            match locked.get(client_id) {
                Some(v) => {
                    if let Some(sender) = &v.sender {
                        let _ = sender.send(Ok(Message::text(message)));
                    }
                }
                None => {
                    println!("Failed to find user with id: {}", client_id);
                },
            }
        },
        Query::Open => {
            let configuration = config.lock().await;
            let mut locked = configuration.clients.lock().await;

            match locked.get_mut(client_id) {
                Some(v) => {
                    v.set_connectivity(true);
                    configuration.add_peer(v).await;
                }
                None => (),
            }

            drop(locked);
            drop(configuration);

            let temp = &config.lock().await;
            let message = format!("{{ \"message\": {{ \"server_public_key\": \"{}\", \"endpoint\": \"{}:{}\" }}, \"type\": \"message\" }}", temp.keys.public_key, temp.config.address, temp.config.listen_port);

            let locked = temp.clients.lock().await;

            match locked.get(client_id) {
                Some(v) => {
                    if let Some(sender) = &v.sender {
                        let _ = sender.send(Ok(Message::text(message)));
                    }
                }
                None => {
                    println!("Failed to find user with id: {}", client_id);
                },
            }
        },
        Query::Resume => {
            println!("Resuming the socket & wireguard conn.");
            // No need to adjust config if resuming...
        },
        _ => {
            return return_to_sender(&config.lock().await.clients, client_id, format!("{{ \"message\": \"Unknown query_type, expected one of open, close, resume.\", \"type\": \"error\" }}")).await;
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