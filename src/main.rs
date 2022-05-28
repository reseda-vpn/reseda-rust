use std::{collections::HashMap, convert::Infallible, sync::Arc, time::Duration};
use tokio::sync::Mutex;
use warp::{Filter, Rejection};
use crate::types::{QueryParameters, Clients};
use std::thread;
use tokio::runtime::Handle;
use tokio::runtime::Runtime;

mod lib;
mod types;

type Result<T> = std::result::Result<T, Rejection>;

#[tokio::main]
async fn main() {
    let clients: Clients = Arc::new(Mutex::new(HashMap::new()));

    println!("[SERVICE] ws_handler::start");

    let opt_query = warp::query::<QueryParameters>()
        .map(Some)
        .or_else(|_| async { Ok::<(Option<QueryParameters>,), std::convert::Infallible>((None,)) });

    let ws_route = warp::path::end()
        .and(warp::ws())
        .and(with_clients(clients.clone()))
        .and(opt_query)
        .and_then(lib::ws_handler);

    let routes = ws_route.with(warp::cors().allow_any_origin());

    tokio::spawn(async {
        loop {
            println!("Hi from second thread!");
            thread::sleep(Duration::from_millis(100));
        }
    });

    warp::serve(routes)
        // .tls()
        // .cert_path("cert.pem")
        // .key_path("key.pem")
        .run(([0, 0, 0, 0], 8000)).await;
}

fn with_clients(clients: Clients) -> impl Filter<Extract = (Clients,), Error = Infallible> + Clone {
    warp::any().map(move || clients.clone())
}