use std::{convert::Infallible, sync::Arc, time::Duration, process::Command};
use tokio::sync::Mutex;
use warp::ws::Message;
use warp::{Filter, Rejection};
use crate::types::{QueryParameters, Clients, Connection};
use crate::wireguard::{WireGuardConfig, WireGuard};
use futures_timer::Delay;

mod lib;
mod types;
mod wireguard;

type Result<T> = std::result::Result<T, Rejection>;

#[tokio::main]
async fn main() {
    let config: WireGuard = Arc::new(
        Mutex::new(
            WireGuardConfig::load_from_config("config.reseda").await
                .save_config(true).await
                .to_owned()
        )
    );

    println!("[service] ws_handler::starting");

    let opt_query = warp::query::<QueryParameters>()
        .map(Some)
        .or_else(|_| async { Ok::<(Option<QueryParameters>,), std::convert::Infallible>((None,)) });

    let ws_route = warp::path::end()
        .and(warp::ws())
        .and(with_config(config.clone()))
        .and(opt_query)
        .and_then(lib::ws_handler);

    let routes = ws_route.with(warp::cors().allow_any_origin());

    tokio::spawn(async move {
        loop {
            // Task will run ever *1s*
            match Command::new("wg")
                .args(["show", "reseda", "transfer"])
                .output() {
                    Ok(output) => {
                        match String::from_utf8(output.stdout) {
                            Ok(mut string) => {
                                string = string.trim().to_string();
                                for line in string.split("\n").into_iter() {
                                    if line == "" { break };
                                    
                                    let split_vector = line.trim().split("\t");
                                    let vec: Vec<&str> = split_vector.collect();

                                    match config.lock().await.clients.lock().await.get_mut(&vec[0].to_string()) {
                                        Some(client) => {
                                            let up = vec[1].parse::<i128>().unwrap();
                                            let down = vec[2].parse::<i128>().unwrap();

                                            match client.set_usage(&up, &down) {
                                                true => {
                                                    println!("[warn]: Exceeded maximum usage, given {}, had {}/{}", client.maximums.to_value(), up, down);
                                                    match &client.connected {
                                                        Connection::Disconnected => {
                                                            println!("[err]: Something went wrong, attempted to remove user for exceeding limits who is not connected...")
                                                        },
                                                        Connection::Connected(connection) => {
                                                            config.lock().await.free_slot(connection);
                                                            client.set_connectivity(Connection::Disconnected);
                                                            config.lock().await.remove_peer(&client).await;
                                                        },
                                                    }
                                                }
                                                false => {
                                                    let message = format!("{{ \"message:\": {{ \"up\": {}, \"down\": {} }}, \"type\": \"update\"}}", &up, &down);

                                                    if let Some(sender) = &client.sender {
                                                        match sender.send(Ok(Message::text(message))) {
                                                            Ok(_) => {
                                                                println!("[usage]: User {} is given {}, has used up::{}, down::{}", client.public_key, client.maximums.to_value(), up, down);
                                                            }
                                                            Err(e) => {
                                                                println!("[err]: Failed to send message: \'INVALID_SENDER\', reason: {}", e)
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        },
                                        None => {
                                            println!("[err]: No user matched for this!")
                                        },
                                    }
                                }
                            }
                            Err(err) => {
                                println!("[err]: Parsing UTF8: {}", err)
                            }
                        }
                    }
                    Err(err) => {
                        println!("[err]: Failed to bring up reseda server, {:?}", err);
                    }
                }

            // End of Task
            Delay::new(Duration::from_millis(1000)).await;
        }
    });

    warp::serve(routes)
        .tls()
        .cert_path("cert.pem")
        .key_path("key.pem")
        .run(([0, 0, 0, 0], 443)).await;
}

fn with_config(config: WireGuard) -> impl Filter<Extract = (WireGuard,), Error = Infallible> + Clone {
    warp::any().map(move || config.clone())
}