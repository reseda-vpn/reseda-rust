use crate::lib::close_query;
use crate::types::{Clients, Connection, QueryParameters};
use crate::wireguard::{WireGuard, WireGuardConfig};
use futures_timer::Delay;
use std::{convert::Infallible, process::Command, sync::Arc, time::Duration};
use tokio::sync::Mutex;
use warp::ws::Message;
use warp::{Filter, Rejection};

mod lib;
mod types;
mod wireguard;

type Result<T> = std::result::Result<T, Rejection>;

#[tokio::main]
async fn main() {
    let config: WireGuard = Arc::new(Mutex::new(
        WireGuardConfig::initialize()
            .await
            .save_config(true)
            .await
            .to_owned(),
    ));

    println!("[service] ws_handler::starting");

    let opt_query = warp::query::<QueryParameters>()
        .map(Some)
        .or_else(|_| async { Ok::<(Option<QueryParameters>,), std::convert::Infallible>((None,)) });

    let ws_route = warp::path::path("ws")
        .and(warp::ws())
        .and(with_config(config.clone()))
        .and(opt_query)
        .and_then(lib::ws_handler);

    let echo_route = warp::path::end().and(warp::get()).and_then(lib::echo);

    let health_route = warp::path("health")
        .and(with_config(config.clone()))
        .and_then(lib::health_status);

    let routes = ws_route
        .or(echo_route)
        .or(health_route)
        .with(warp::cors().allow_any_origin());

    tokio::spawn(async move {
        loop {
            // Task will run ever *1s*
            match Command::new("wg")
                .args(["show", "reseda", "transfer"])
                .output()
            {
                Ok(output) => {
                    match String::from_utf8(output.stdout) {
                        Ok(mut string) => {
                            string = string.trim().to_string();
                            for line in string.split("\n").into_iter() {
                                if line == "" {
                                    break;
                                };

                                let split_vector = line.trim().split("\t");
                                let vec: Vec<&str> = split_vector.collect();

                                let mut config_lock = config.lock().await;

                                let config_clone = config_lock.clone();
                                let mut clients_lock = config_clone.clients.lock().await;

                                let client = match clients_lock.get_mut(&vec[0].to_string()) {
                                    Some(client) => client,
                                    None => {
                                        println!("[err]: No user matched for this!");
                                        break;
                                    }
                                };

                                let up = vec[1].parse::<i128>().unwrap();
                                let down = vec[2].parse::<i128>().unwrap();

                                let usage_query = client.set_usage(&up, &down);

                                // If a usage could be set...
                                if usage_query == false {
                                    let message = format!("{{ \"message\": {{ \"up\": {}, \"down\": {} }}, \"type\": \"update\"}}", &up, &down);

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

                                    break;
                                }

                                // If usage could not be sent...
                                println!(
                                    "[warn]: Exceeded maximum usage, given {}, had {}/{}",
                                    client.maximums.to_value(),
                                    up,
                                    down
                                );

                                let conn = client.connected.clone();

                                if conn == Connection::Disconnected {
                                    println!("[err]: Something went wrong, attempted to directly remove user for exceeding limits who is not connected...");
                                    Delay::new(Duration::from_millis(1000)).await;
                                    config_lock.remove_peer(&client).await;

                                    let public_key = &client.public_key.clone();

                                    drop(client);
                                    drop(clients_lock);

                                    println!("[evt]: Closing Service for user, config is arc-locked for this process.");

                                    close_query(&public_key, &mut config_lock).await;

                                    println!("[evt]: Closed Service for user, preparing to unlock config.");

                                    break;
                                }

                                if let Connection::Connected(_val) = conn {
                                    // Message: UserDisConnection-ExceededUsage
                                    let message = format!(
                                        "{{ \"message\": \"UDC-EU\", \"type\": \"error\"}}"
                                    );

                                    // Inform user of upcoming disconnection.
                                    if let Some(sender) = &client.sender {
                                        match sender.send(Ok(Message::text(message))) {
                                            Ok(_) => {
                                                println!("[messaging]: User exceeded usage and was send a disconnection warning.");
                                            }
                                            Err(e) => {
                                                println!("[err]: Failed to send message: \'INVALID_SENDER\', reason: {}", e)
                                            }
                                        }
                                    };

                                    // Wait 200ms, to allow for throughput from buffer to leave and inform before pulling  (non-thread-blocking wait)
                                    Delay::new(Duration::from_millis(200)).await;

                                    let public_key = &client.public_key.clone();

                                    drop(client);
                                    drop(clients_lock);

                                    println!("[evt]: Closing Service for user, config is arc-locked for this process.");

                                    close_query(&public_key, &mut config_lock).await;

                                    println!("[evt]: Closed Service for user, preparing to unlock config.");
                                };
                            }

                            println!("[evt]: Configuration unlocked.");
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
        .run(([0, 0, 0, 0], 443))
        .await;
}

fn with_config(
    config: WireGuard,
) -> impl Filter<Extract = (WireGuard,), Error = Infallible> + Clone {
    warp::any().map(move || config.clone())
}
