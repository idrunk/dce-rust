use rand::random;
use serde::Serialize;
use dce_cli::protocol::{CliRaw, CliGet};
use dce_macro::{api, openly_err};
use dce_router::serializer::JsonSerializer;

/// `cargo run --bin app`
#[api("")]
pub async fn index(req: CliRaw) {
    req.raw_resp("Welcome to DCE !".to_string())
}

/// `cargo run --bin app -- hello`
/// `cargo run --bin app -- hello DCE`
#[api("hello/{target?}")]
pub async fn hello(req: CliRaw) {
    let target = req.param("target")?.as_str().unwrap_or("RUST").to_owned();
    req.raw_resp(format!("Hello {} !", target))
}

/// `cargo run --bin app -- session`
/// `cargo run --bin app -- session --user DCE`
#[api(serializer = JsonSerializer{})]
pub async fn session(mut req: CliGet<UserDto>) {
    let name = req.rp_mut().args_mut().remove("--user").ok_or_else(|| openly_err!(r#"please pass in the "--user" arg"#))?;
    let resp = User {
        nickname: name.clone(),
        gender: if random::<u8>() > 127 { Gender::Female } else { Gender::Male },
        name,
        cellphone: "+8613344445555".to_owned(),
    };
    req.resp(resp)
}

#[derive(Serialize)]
pub enum Gender {
    Male,
    Female,
}

pub struct User {
    nickname: String,
    gender: Gender,
    #[allow(dead_code)]
    name: String,
    #[allow(dead_code)]
    cellphone: String,
}

#[derive(Serialize)]
pub struct UserDto {
    nickname: String,
    gender: Gender,
}

impl From<User> for UserDto {
    fn from(value: User) -> Self {
        Self {
            nickname: value.nickname,
            gender: value.gender,
        }
    }
}