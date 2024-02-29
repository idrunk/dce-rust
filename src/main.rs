use std::io::Write;
use chrono::Local;
use env_logger::Builder;
use log::LevelFilter;
use dce_cli::protocol::CliProtocol;
use dce_router::router::Router;
use crate::apis::cli::{hello, session};
use crate::apis::clients::append;
use crate::apis::http::http_start;
use crate::apis::tcp::tcp_start;
use crate::apis::udp::udp_start;
use crate::apis::websocket::websocket_start;

mod apis {
    pub mod cli;
    pub mod clients;
    pub mod http;
    pub mod websocket;
    pub mod tcp;
    pub mod udp;
}

#[tokio::main]
async fn main() {
    Builder::new()
        .format(|buf, record| {
            writeln!(buf,
                     "{} [{}] - {}",
                     Local::now().format("%Y-%m-%dT%H:%M:%S"),
                     record.level(),
                     record.args()
            )
        })
        .filter(None, LevelFilter::Info)
        .parse_default_env()
        .init();

    let router = Router::new()
        .push(hello)
        .push(session)
        .push(http_start)
        .push(http_start)
        .push(websocket_start)
        .push(tcp_start)
        .push(udp_start)
        .consumer_push(append)
        .ready();

    CliProtocol::new(1).route(router.clone(), Default::default()).await;
}
