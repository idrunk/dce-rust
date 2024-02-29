use dce_cli::protocol::{CliRaw, CliProtocol};
use dce_router::api;
use dce_router::router::router::Router;

fn main() {
    let router = Router::new()
        .push(hello)
        .ready();

    CliProtocol::new(1).route(router.clone(), Default::default());
}

/// `cargo run --package dce --bin cli --no-default-features -- hello target=world --arg2 haha`
#[api("hello")]
pub fn hello(req: CliRaw) {
    println!("{:#?}", req.raw());
    Ok(None)
}
