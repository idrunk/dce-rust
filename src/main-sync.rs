use dce_cli::protocol::{CliRaw, CliProtocol};
use dce_macro::api;
use dce_router::router::Router;

fn main() {
    env_logger::init();

    let router = Router::new()
        .push(index)
        .push(suffix)
        .push(var_suffix)
        .push(omission)
        .push(un_omission)
        .push(un_omission2)
        .push(un_omission3)
        .ready();

    CliProtocol::new(1).route(router.clone(), Default::default());
}

/// `cargo run --bin cli --no-default-features -- target=world --arg2 haha`
#[api("")]
pub fn index(req: CliRaw) {
    let raw = format!("{:#?}", req.raw());
    req.raw_resp(raw)
}

/// `cargo run --bin cli --no-default-features -- suffix`                       route unmatched
/// `cargo run --bin cli --no-default-features -- suffix.txt`                   route matched
/// `cargo run --bin cli --no-default-features -- suffix.txt.1`                 route matched
#[api("suffix.txt|txt.1")]
pub fn suffix(mut req: CliRaw) {
    let raw = format!("{:#?}\n{:#?}", req.context_mut().suffix(), req);
    req.raw_resp(raw)
}

/// `cargo run --bin cli --no-default-features -- suffix name`                  route matched
/// `cargo run --bin cli --no-default-features -- suffix name.txt`              route matched
/// `cargo run --bin cli --no-default-features -- suffix part name`             route matched
/// `cargo run --bin cli --no-default-features -- suffix part name.txt`         route matched
#[api("suffix/{filename+}.txt|")]
pub fn var_suffix(mut req: CliRaw) {
    let raw = format!("{:#?}\n{:#?}", req.context_mut().suffix(), req.params());
    req.raw_resp(raw)
}

/// `cargo run --bin cli --no-default-features -- home`
#[api("home/omission", omission = true)]
pub fn omission(req: CliRaw) {
    req.raw_resp("home".to_string())
}

/// `cargo run --bin cli --no-default-features -- home omission name.txt`       route unmatched
/// `cargo run --bin cli --no-default-features -- home name.txt`                route matched
#[api("home/omission/{filename}.txt")]
pub fn un_omission(mut req: CliRaw) {
    let raw = format!(r#"".txt": {:#?}{}{:#?}"#, req.context_mut().suffix(), "\n", req.params());
    req.raw_resp(raw)
}

/// `cargo run --bin cli --no-default-features -- home omission name`           route unmatched
/// `cargo run --bin cli --no-default-features -- home name`                    route matched
/// `cargo run --bin cli --no-default-features -- home name.1.txt`              route matched
#[api("home/omission/{filename}.|1.txt")]
pub fn un_omission2(mut req: CliRaw) {
    let raw = format!(r#"".|1.txt": {:#?}{}{:#?}"#, req.context_mut().suffix(), "\n", req.params());
    req.raw_resp(raw)
}


/// `cargo run --bin cli --no-default-features -- home name content.txt`        route unmatched
/// `cargo run --bin cli --no-default-features -- home name content`            route matched
/// `cargo run --bin cli --no-default-features -- home name content.1.txt`      route matched
#[api("home/{filename}/content.|1.txt")]
pub fn un_omission3(mut req: CliRaw) {
    let raw = format!(r#""content.|1.txt": {:#?}{}{:#?}"#, req.context_mut().suffix(), "\n", req.params());
    req.raw_resp(raw)
}
