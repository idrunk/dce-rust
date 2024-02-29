use rand::random;
use serde::Serialize;
use dce_cli::protocol::{CliConvert, CliRaw};
use dce_macro::{api, openly_err};
use dce_router::serializer::JsonSerializer;

/// `cargo run --package dce --bin app -- hello`
/// `cargo run --package dce --bin app -- hello DCE`
#[api("hello/{target?}")]
pub async fn hello(req: CliRaw) {
    let target = req.param("target")?.get().unwrap_or("RUST").to_owned();
    req.raw_resp(format!("Hello {} !", target))
}

/// `cargo run --package dce --bin app -- session`
/// `cargo run --package dce --bin app -- session --user DCE`
#[api(serializer = JsonSerializer{})]
pub async fn session(mut req: CliConvert<User, UserDto>) {
    let name = req.rpi_mut().args_mut().remove("--user").ok_or_else(|| openly_err!(r#"please pass in the "--user" arg"#))?;
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