use std::{convert::Infallible, sync::Arc, time::Duration, process::Command};
use tokio::sync::Mutex;
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
                .save_config(false).await.to_owned()
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

    tokio::spawn(async {
        loop {
            // Task will run ever *10s*
            println!("Hi from second thread!");
            let command_output = Command::new("wg")
                .args(["show", "reseda", "transfer"])
                .output()
                .expect("Failed to see wireguard status.");

            println!("Output: {:?}", command_output);
            let string_version: String = String::from_utf8(command_output.stdout).expect("Output was not valid utf8.");

            println!("As String: {:?}", string_version);

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