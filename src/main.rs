use std::{convert::Infallible, sync::Arc, time::Duration, process::Command};
use tokio::sync::Mutex;
use warp::ws::Message;
use warp::{Filter, Rejection};
use crate::types::{QueryParameters, Clients};
use crate::wireguard::{WireGuardConfig, WireGuard};
use std::thread;

mod lib;
mod types;
mod wireguard;

type Result<T> = std::result::Result<T, Rejection>;

#[tokio::main]
async fn main() {
    let config: WireGuard = Arc::new(
        Mutex::new(
            WireGuardConfig::load_from_config("config.reseda")
                .save_config(true).await
                .config_sync().await
                .to_owned()
        )
    );

    println!("[SERVICE] ws_handler::start");

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
            // Task will run ever *10s*
            match Command::new("wg")
                .args(["show", "reseda", "transfer"])
                .output() {
                    Ok(output) => {
                        println!("Raw Output: {:?}", output);
                        match String::from_utf8(output.stdout) {
                            Ok(mut string) => {
                                string = string.trim().to_string();
                                for line in string.split("\n").into_iter() {
                                    if line == "" { break };
                                    
                                    let split_vector = line.trim().split("\t");
                                    let vec: Vec<&str> = split_vector.collect();

                                    match config.lock().await.clients.lock().await.get_mut(&vec[0].to_string()) {
                                        Some(client) => {
                                            let up = vec[1].parse::<i64>().unwrap();
                                            let down = vec[2].parse::<i64>().unwrap();

                                            client.set_usage(&up, &down);
                                            let message = format!("{{\"message:\": {{ \"up\": \"{}\", \"down\": {} }}, \"type\": \"update\"}}", &up, &down);

                                            if let Some(sender) = &client.sender {
                                                match sender.send(Ok(Message::text(message))) {
                                                    Ok(_) => {
                                                        println!("Sent update of usage to user.");
                                                    }
                                                    Err(e) => {
                                                        println!("Failed to send message: \'INVALID_PUBLIC_KEY\', reason: {}", e)
                                                    }
                                                }
                                            }

                                            // println!("Sending update to user: {:?}", client);
                                            // match owned_client {
                                            //     Option::Some(sender) => {
                                            //         match sender.send(Ok(Message::text(message))) {
                                            //             Ok(_) => {
                                            //                 println!("Sent update of usage to user.");
                                            //             }
                                            //             Err(e) => {
                                            //                 println!("Failed to send message: \'INVALID_PUBLIC_KEY\', reason: {}", e)
                                            //             }
                                            //         }
                                            //     }
                                            //     Option::None => {
                                            //         println!("Client does not contain available websocket sender.")
                                            //     }
                                            // }
                                        },
                                        None => {
                                            println!("No user matched for this!")
                                        },
                                    }
                                }
                            }
                            Err(err) => {
                                println!("Error: {}", err)
                            }
                        }
                    }
                    Err(err) => {
                        println!("Failed to bring up reseda server, {:?}", err);
                    }
                }

            // End of Task
            thread::sleep(Duration::from_millis(10000));
        }
    });

    warp::serve(routes)
        // .tls()
        // .cert_path("cert.pem")
        // .key_path("key.pem")
        .run(([0, 0, 0, 0], 8000)).await;
}

fn with_config(config: WireGuard) -> impl Filter<Extract = (WireGuard,), Error = Infallible> + Clone {
    warp::any().map(move || config.clone())
}