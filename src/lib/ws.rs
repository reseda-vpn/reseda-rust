use crate::{Clients, types::{self, Query, QueryParameters, Client, Connection, Reservation, Slot}, wireguard::{WireGuard}};
use chrono::Utc;
use futures::{FutureExt, StreamExt};
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use uuid::Uuid;
use warp::ws::{Message, WebSocket};

pub async fn client_connection(ws: WebSocket, config: WireGuard, parameters: Option<QueryParameters>) {
    let (client_ws_sender, mut client_ws_rcv) = ws.split();
    let (client_sender, client_rcv) = mpsc::unbounded_channel();

    let client_rcv = UnboundedReceiverStream::new(client_rcv);

    tokio::task::spawn(client_rcv.forward(client_ws_sender).map(|result| {
        if let Err(e) = result {
            println!("[service] Sending WebSocket Message '{}'", e);
        }
    }));

    match &parameters {
        Some(obj) => {
            let client = Client::new(Some(client_sender))
                .set_public_key(obj.public_key.clone())
                .set_author(obj.author.clone());

            match &client.is_valid() { 
                true => {
                    let owned_client = &client.expose_client_sender();

                    match owned_client {
                        Option::Some(sender) => {
                            match sender.send(Ok(Message::text(format!("{{ \"message\": \"PUBLIC_KEY_OK\", \"type\": \"message\" }}")))) {
                                Ok(_) => {}
                                Err(e) => {
                                    println!("[err]: Failed to send message: \'INVALID_PUBLIC_KEY\', reason: {}", e)
                                }
                            }
                        }
                        Option::None => {
                            println!("[err]: Client does not contain available websocket sender.")
                        }
                    }

                    let pk = client.public_key.clone();
                    let author_clone = client.author.clone(); //format!("\'{}\'", client.author.clone().trim_matches('\"'));

                    config.lock().await.clients.lock().await.insert(pk.clone(), client);

                    let maximums = match config.lock().await.pool.begin().await {
                        Ok(mut transaction) => {
                            match sqlx::query!("select tier from Account where userId = ?", author_clone)
                                .fetch_one(&mut transaction)
                                .await {
                                    Ok(query) => {
                                        let tier = String::from_utf8(query.tier);
                                        
                                        match tier {
                                            Ok(t) => { 
                                                let tier_string = t.as_str();

                                                let argument_tier = match tier_string {
                                                    "FREE" => types::Maximums::Free,
                                                    "PRO" => types::Maximums::Pro,
                                                    "BASIC" => types::Maximums::Basic,
                                                    "SUPPORTER" => types::Maximums::Supporter,
                                                    _ => types::Maximums::Unassigned
                                                };

                                                argument_tier
                                            },
                                            Err(_) => {
                                                println!("[err]: Unable to parse");
                                                types::Maximums::Unassigned
                                            },
                                        }
                                    },
                                    Err(err) => {
                                        println!("[err]: Unable to perform request, user will remain unassigned. Reason: {}", err);
                                        types::Maximums::Unassigned
                                    }
                            }
                        },
                        Err(err) => {
                            println!("[err]: Unable to perform request, user will remain unassigned. Reason: {}", err);
                            types::Maximums::Unassigned
                        }
                    };

                    match config.lock().await.clients.lock().await.get_mut(&pk) {
                        Some(client) => {
                            client.set_tier(maximums);
                        }
                        None => {}
                    }
                    
                    while let Some(result) = client_ws_rcv.next().await {
                        let msg = match result {
                            Ok(msg) => msg,
                            Err(e) => {
                                println!("[err]: Receiving message for id {}: {}", obj.author.clone(), e);
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
                                    println!("[err]: Failed to send message: \'INVALID_PUBLIC_KEY\', reason: {}", e)
                                }
                            }
                        }
                        Option::None => {
                            println!("[err]: Client does not contain available websocket sender.")
                        }
                    }
                    
                    return;
                }
            };

            println!("[evt]: Client Removed Successfully");
        }
        None => {
            println!("[err]: Unable to parse parameters given, {:?}", &parameters);
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
            let mut configuration = config.lock().await;
            let mut locked = configuration.clients.lock().await;

            let connection_to_drop = match locked.get_mut(client_id) {
                Some(client) => {
                    match &client.connected {
                        Connection::Disconnected => {
                            println!("[err]: Something went wrong, attempted to remove user for exceeding limits who is not connected...");
                            Slot::Prospective
                        }
                        Connection::Connected(connection) => {
                            client.to_owned().set_connectivity(Connection::Disconnected);
                            configuration.remove_peer(&client).await;

                            let connection_usage = client.get_usage();
                            let session_id = Uuid::new_v4().to_string();
                            let con_time = connection.conn_time.to_rfc3339();
                            let now = Utc::now().to_rfc3339();

                            let down = connection_usage.0.to_string();
                            let up = connection_usage.1.to_string();
                            
                            match configuration.pool.begin().await {
                                Ok(mut transaction) => {
                                    match sqlx::query!("insert into Usage (id, userId, serverId, up, down, connStart, connEnd) values (?, ?, ?, ?, ?, ?, ?)", session_id, client.author, configuration.config.name, up, down, con_time, now)
                                        .execute(&mut transaction)
                                        .await {
                                            Ok(result) => {
                                                match transaction.commit().await {
                                                    Ok(r2) => {
                                                        println!("[sqlx]: Usage Log Transaction Result: {:?}, {:?}", result, r2);
                                                    },
                                                    Err(error) => println!("[sqlx]: Transaction Commitance Error: {:?}", error),
                                                }

                                            },
                                            Err(error) => println!("[sqlx]: Transaction Error: {:?}", error),
                                        }
                                },
                                Err(err) => {
                                    println!("[err]: Unable to perform request, user will remain unassigned. Reason: {}", err);
                                }
                            };

                            Slot::Open(connection.clone())
                        }
                    }
                }
                None => {
                    Slot::Prospective
                },
            };

            drop(locked);
            match connection_to_drop {
                Slot::Open(drop) => {
                    println!("[reserver]: Freeing up now unused slot; {:?}", drop);
                    configuration.free_slot(&drop);
                },
                Slot::Prospective => println!("[reserver]: Error, Could not drop"),
            }
            drop(configuration);

            let temp = &config.lock().await;
            let message = format!("{{ \"message\": \"Removed client successfully.\", \"type\": \"message\" }}");

            let locked = temp.clients.lock().await;

            match locked.get(client_id) {
                Some(v) => {
                    if let Some(sender) = &v.sender {
                        let _ = sender.send(Ok(Message::text(message)));
                    }
                }
                None => {
                    println!("[err]: Failed to find user with id: {}", client_id);
                },
            }
        },
        Query::Open => {
            let mut configuration = config.lock().await;

            let slot = configuration.find_open_slot();
            println!("[reserver]: Found slot: {:?}", slot);

            let reserved_slot = match slot {
                types::Slot::Open(open_slot) => configuration.reserve_slot(open_slot),
                types::Slot::Prospective => Reservation::Imissable,
            };
            println!("[reserver]: Reserved Slot: {:?}", reserved_slot);

            match reserved_slot {
                Reservation::Held(valid_slot) => {
                    let mut lock = configuration.clients.lock().await;
                    let client = lock.get_mut(client_id);

                    match client {
                        Some(v) => {
                            v.set_connectivity(Connection::Connected(valid_slot));
                            configuration.add_peer(v).await;

                            println!("[evt]: Success, Created Peer {:?} on slot {:?}", v.public_key, v.connected);
                        }
                        None => {
                            drop(lock);
                            // Found and reserved slot, however was not able to get lock on client, so we free the slot as it is not assigned to any user.
                            configuration.free_slot(&valid_slot);
                        },
                    }
                }
                Reservation::Imissable => {
                    println!("[reserver]: Error, Unable to add user to slot (Imissable)");
                }
                Reservation::Detached(err) => {
                    println!("[reserver]: Error, Unable to add user to slot (Detached): {:?}", err);
                },
            }

            drop(configuration);

            let temp = &config.lock().await;
            let message = format!("{{ \"message\": {{ \"server_public_key\": \"{}\", \"endpoint\": \"{}:{}\" }}, \"type\": \"message\" }}", temp.keys.public_key.trim(), temp.config.address, temp.config.listen_port.trim());

            let locked = temp.clients.lock().await;

            match locked.get(client_id) {
                Some(v) => {
                    if let Some(sender) = &v.sender {
                        let _ = sender.send(Ok(Message::text(message)));
                    }
                }
                None => {
                    println!("[err]: Failed to find user with id: {}", client_id);
                },
            }
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