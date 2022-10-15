use crate::{Clients, types::{self, Query, QueryParameters, Client, Connection, Reservation, Slot}, wireguard::{WireGuard, WireGuardConfig}};
use chrono::Utc;
use futures::{FutureExt, StreamExt};
use tokio::sync::{mpsc, MutexGuard};
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
            let mut client = Client::new(Some(client_sender))
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

                    let exists = config.lock().await.clients.lock().await.contains_key(&pk.clone());

                    match exists {
                        true => {
                            match config.lock().await.clients.lock().await.get_mut(&pk) {
                                Some(config) => {
                                    client.merge_from(&config.clone());
                                },
                                None => {
                                    println!("[err]: Exists, but does not... exist?");
                                },
                            }

                            config.lock().await.clients.lock().await.insert(pk.clone(), client.clone());
                        }
                        false => {
                            config.lock().await.clients.lock().await.insert(pk.clone(), client.clone());
                        }
                    };

                    let mut transaction = match config.lock().await.pool.begin().await {
                        Ok(transaction) => {
                            transaction
                        },
                        Err(err) => {
                            println!("[err]: Unable to perform request, user will be removed and disconnected as server is not in appreciable state to handle user. Reason: {}", err);
                            config.lock().await.clients.lock().await.remove(&obj.author.clone());
                            return;
                        }
                    };

                    let usage = match sqlx::query!("SELECT * FROM Usage WHERE userId = ? AND MONTH(connStart)=MONTH(now())", author_clone)
                        .fetch_all(&mut transaction)
                        .await {
                            Ok(query) => {
                                // Obtained a list of query records for the current billing month.
                                // Enumerate over the list, taking summation of throughput - allowing for the calculation of
                                // the actual appropriate remaining usage for the user.
                                let mut total_accrued = (0, 0);

                                for item in query {
                                    let as_int_down = match str::parse::<i128>(&item.down) {
                                        Ok(val) => val,
                                        Err(error) => {
                                            println!("[err]: In converting string to u64: {}", error);
                                            0
                                        }
                                    };
                                    let as_int_up = match str::parse::<i128>(&item.up) {
                                        Ok(val) => val,
                                        Err(error) => {
                                            println!("[err]: In converting string to u64: {}", error);
                                            0
                                        }
                                    };

                                    total_accrued = (total_accrued.0 + as_int_up, total_accrued.1 + as_int_down);
                                };

                                println!("[ws]: Joining user has accrued {:?} of usage this month.", total_accrued);

                                total_accrued
                            }
                            Err(err) => {
                                println!("[err]: Unable to fetch, possibly no results or invalid user. {}", err);

                                (0, 0)
                            }
                        };

                    let maximums = match sqlx::query!("select tier from Account where userId = ?", author_clone)
                        .fetch_one(&mut transaction)
                        .await {
                            Ok(query) => {
                                let tier = query.tier.as_str();

                                println!("[msg]: User is of {} tier", &tier);

                                let argument_tier = match tier {
                                    "FREE" => types::Maximums::Free(usage.0, usage.1),
                                    "PRO" => types::Maximums::Pro(usage.0, usage.1),
                                    "BASIC" => types::Maximums::Basic(usage.0, usage.1),
                                    "SUPPORTER" => types::Maximums::Supporter(usage.0, usage.1),
                                    _ => types::Maximums::Unassigned
                                };

                                argument_tier
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

pub async fn close_query(client_id: &str, configuration: &mut MutexGuard<'_, WireGuardConfig>) {
    println!("[evt]: Closing connection: Start");

    let mut locked = configuration.clients.lock().await;

    println!("[evt]: Closing connection: Obtained Client Lock");
    
    let connection_to_drop = match locked.get_mut(client_id) {
        Some(client) => {
            match &client.connected {
                Connection::Disconnected => {
                    println!("[err]: User has likely been forcefully disconnected. ");
                    Slot::Prospective
                }
                Connection::Connected(connection) => {
                    println!("[evt]: Closing connection: Found connection to drop");
                    
                    client.to_owned().set_connectivity(Connection::Disconnected);
                    configuration.remove_peer(&client).await;

                    println!("[evt]: Closing connection: Removed Peer");

                    let connection_usage = client.get_usage().clone();
                    let con_time = connection.conn_time.to_rfc3339().clone();

                    let now = Utc::now().to_rfc3339();
                    let session_id = Uuid::new_v4().to_string();

                    let down = connection_usage.0.to_string().clone();
                    let up = connection_usage.1.to_string().clone();

                    let author_id = client.author.clone();

                    println!("[evt]: Closing connection: Creating Transaction");

                    match configuration.pool.begin().await {
                        Ok(mut transaction) => {
                            match sqlx::query!("insert into Usage (id, userId, serverId, up, down, connStart, connEnd) values (?, ?, ?, ?, ?, ?, ?)", session_id, author_id, configuration.config.name, up, down, con_time, now)
                                .execute(&mut transaction)
                                .await {
                                    Ok(_returned_information) => {
                                        println!("[evt]: Closing connection: Committing Transaction");

                                        match transaction.commit().await {
                                            Ok(r2) => {
                                                println!("[sqlx]: Usage Log Transaction Result: {:?}", r2);

                                                match reqwest::Client::new()
                                                    .post("https://reseda.app/api/billing/usage-reccord")
                                                    .json(&serde_json::json!({
                                                        "sessionId": session_id,
                                                    }))
                                                    .send()
                                                    .await {
                                                        Ok(r) => {
                                                            match r.text().await {
                                                                Ok(_) => {
                                                                    // Success!
                                                                    // Here the Reseda API has published the usage-reccord of the service to stripe, thus meaning that the users logging has been billed to them.
                                                                    // Notably, if the user is under a FREE or SUPPORTER tier, they will not be charged anything, as the API will return a ERROR:400, indicating failure to recognise a valid stripe subscription to thier billing profile.
                                                                },
                                                                Err(error) => println!("[api.reseda]: Failed to record usage-record with reseda, API returned: {:?}", error),
                                                            };
                                                        },
                                                        Err(error) => {
                                                            println!("[api.reseda]: Failed to record usage-record with reseda, API returned: {:?}", error)
                                                        },
                                                    }
                                            },
                                            Err(error) => println!("[sqlx]: Transaction Commit Error: {:?}", error),
                                        }
                                    },
                                    Err(error) => println!("[sqlx]: Transaction Commit Error: {:?}", error),
                                };
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

    let message = format!("{{ \"message\": \"Removed client successfully.\", \"type\": \"message\" }}");

    let locked = configuration.clients.lock().await;

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
}

pub async fn open_query(client_id: &str, mut configuration: MutexGuard<'_, WireGuardConfig>) {
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
                    let clone = &valid_slot.clone();

                    v.set_connectivity(Connection::Connected(valid_slot));
                    configuration.add_peer(v).await;

                    let a = &clone.a.clone();
                    let b = &clone.b.clone();

                    let message = format!(
                        "{{ \"message\": {{ \"server_public_key\": \"{}\", \"endpoint\": \"{}:{}\", \"subdomain\": \"{}.{}\" }}, \"type\": \"message\" }}", 
                        configuration.keys.public_key.trim(), 
                        configuration.config.address, 
                        configuration.config.listen_port.trim(),
                        &a, &b
                    );
                    
                    if let Some(sender) = &v.sender {
                        let _ = sender.send(Ok(Message::text(message)));
                    }
         
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
        Query::Open => {
            let configuration = config.lock().await;

            open_query(client_id, configuration).await;
        },
        Query::Close => {
            let mut configuration = config.lock().await;

            close_query(client_id, &mut configuration).await;
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