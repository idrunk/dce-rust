use dce_cli::protocol::{CliRaw, CliProtocol, CliGet};
use dce_macro::api;
use dce_router::router::Router;
#[cfg(feature = "sync-session")]
use dce_router::api::EventHandler;
#[cfg(feature = "sync-session")]
use crate::apis::session_sync::{before_controller, login, profile};

mod apis {
    #[cfg(feature = "sync-session")]
    pub mod session_sync;
}

fn main() {
    env_logger::init();

    #[allow(unused_mut)]
    let mut router = Router::new().unwrap()
        .push(index)
        .push(suffix)
        .push(var_suffix)
        .push(omission)
        .push(un_omission)
        .push(un_omission2)
        .push(un_omission3)
        .push(un_omission3);

    #[cfg(feature = "sync-session")]
    { router = router.set_event_handlers(Some(EventHandler::Sync(before_controller)), None)
        .push(login)
        .push(profile); }

    let router = router.ready().unwrap();
    CliProtocol::new(1).route(router.clone(), Default::default());
}

/// `cargo run --bin cli --no-default-features -- target=world --arg2 haha`
#[api("")]
pub fn index(req: CliRaw) {
    let raw = format!("{:#?}", req.rp());
    req.raw_resp(raw)
}

/// `cargo run --bin cli --no-default-features -- suffix`                       route unmatched
/// `cargo run --bin cli --no-default-features -- suffix.txt`                   route matched
/// `cargo run --bin cli --no-default-features -- suffix.txt.1`                 route matched
#[api("suffix.txt|txt.1")]
pub fn suffix(mut req: CliGet<String>) {
    let raw = format!("{:#?}\n{:#?}", req.suffix(), req);
    req.raw_resp(raw)
}

/// `cargo run --bin cli --no-default-features -- suffix name`                  route matched
/// `cargo run --bin cli --no-default-features -- suffix name.txt`              route matched
/// `cargo run --bin cli --no-default-features -- suffix part name`             route matched
/// `cargo run --bin cli --no-default-features -- suffix part name.txt`         route matched
#[api("suffix/{filename+}.txt|")]
pub fn var_suffix(mut req: CliRaw) {
    let raw = format!("{:#?}\n{:#?}", req.suffix(), req.params());
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
    let raw = format!(r#"".txt": {:#?}{}{:#?}"#, req.suffix(), "\n", req.params());
    req.raw_resp(raw)
}

/// `cargo run --bin cli --no-default-features -- home omission name`           route unmatched
/// `cargo run --bin cli --no-default-features -- home name`                    route matched
/// `cargo run --bin cli --no-default-features -- home name.1.txt`              route matched
#[api("home/omission/{filename}.|1.txt")]
pub fn un_omission2(mut req: CliRaw) {
    let raw = format!(r#"".|1.txt": {:#?}{}{:#?}"#, req.suffix(), "\n", req.params());
    req.raw_resp(raw)
}


/// `cargo run --bin cli --no-default-features -- home name content.txt`        route unmatched
/// `cargo run --bin cli --no-default-features -- home name content`            route matched
/// `cargo run --bin cli --no-default-features -- home name content.1.txt`      route matched
#[api("home/{filename}/content.|1.txt")]
pub fn un_omission3(mut req: CliRaw) {
    let raw = format!(r#""content.|1.txt": {:#?}{}{:#?}"#, req.suffix(), "\n", req.params());
    req.raw_resp(raw)
}
