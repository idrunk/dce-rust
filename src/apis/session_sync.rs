use redis::{Client, Connection};
use serde::{Deserialize, Serialize};
use tokio::sync::OnceCell;
use dce_cli::protocol::{CliRaw, CliProtocol};
use dce_macro::api;
use dce_router::protocol::RoutableProtocol;
use dce_router::request::Context;
use dce_router::serializer::Serialized;
use dce_session::auto::AutoRenew;
use dce_session::redis::RedisSession;
use dce_session::session::{DEFAULT_TTL_MINUTES, Session};
use dce_session::user::{UidGetter, User};
use dce_util::mixed::{DceErr, DceResult};

static REDIS_CLIENT: OnceCell<Client> = OnceCell::const_new();

pub fn redis_prepare(host: &str) {
    REDIS_CLIENT.set(Client::open(format!("redis://{host}")).unwrap()).unwrap()
}

fn redis() -> Connection {
    REDIS_CLIENT.get().unwrap().get_connection().unwrap()
}


pub fn before_controller(context: &mut Context<CliProtocol>) -> DceResult<()> {
    redis_prepare(context.rp().args().get("redis")
        .ok_or(DceErr::closed0(r#"You must specific the redis host, for example "http start session redis=127.0.0.1:6379""#))?);
    let mut session = match context.rp().sid() {
        Some(sid) => RedisSession::new_with_id(vec![sid.to_string()]),
        None => RedisSession::<Connection, Member>::new(DEFAULT_TTL_MINUTES),
    }?.with(redis()).auto();

    let mut auth = AppAuth::new(context, &mut session);
    auth.valid()?;
    if ! auth.is_login() {
        auth.try_renew()?;
    }

    CliProtocol::set_session(context, Box::new(session.unwrap()));
    Ok(())
}

pub struct AppAuth<'a> {
    context: &'a mut Context<CliProtocol>,
    session: &'a mut AutoRenew<RedisSession<Connection, Member>>,
}

impl<'a> AppAuth<'a> {
    fn new(context: &'a mut Context<CliProtocol>, session: &'a mut AutoRenew<RedisSession<Connection, Member>>) -> Self {
        Self { context, session, }
    }
    
    fn is_login(&self) -> bool {
        self.context.api().unwrap().path().ends_with("login")
    }
    
    fn try_renew(&mut self) -> DceResult<()> {
        if self.session.try_renew()? {
            self.context.rp_mut().set_resp_sid(self.session.id().to_string());
        }
        Ok(())
    }

    fn valid(&mut self) -> DceResult<()> {
        if ! self.is_login() {
            if self.session.user().is_none() {
                return Err(DceErr::openly(401, "Unauthorized".to_string()))
            }
        }
        Ok(())
    }
}



/// `set RUST_LOG=debug && cargo run --bin cli --no-default-features --features sync-session -- login redis=127.0.0.1:6379`, 1000: Name required
/// `set RUST_LOG=debug && cargo run --bin cli --no-default-features --features sync-session -- login redis=127.0.0.1:6379 --name Dce`, 1001: Wrong name
/// `set RUST_LOG=debug && cargo run --bin cli --no-default-features --features sync-session -- login redis=127.0.0.1:6379 --name Drunk`, Succeed to logged and got new sid
#[api]
pub fn login(mut req: CliRaw) {
    let name = req.rp().args().get("--name").ok_or(DceErr::openly(1000, "Name required".to_string()))?;
    let member = Some(Member{ id: 100, name: "Drunk".to_string() }).into_iter()
        .find(|m| m.name.eq_ignore_ascii_case(name)).ok_or(DceErr::openly(1001, "Wrong name".to_string()))?;
    let session = CliProtocol::session::<RedisSession<Connection, Member>, _>(&mut req)?;
    if session.login(member.clone(), DEFAULT_TTL_MINUTES)? {
        let new_sid = session.id().to_string();
        req.rp_mut().set_resp_sid(new_sid);
        return req.pack(Serialized::String(format!("Succeed login with:\n{:?}", member)))
    }
    req.pack(Serialized::String("Failed to login".to_string()))
}

/// `set RUST_LOG=debug && cargo run --bin cli --no-default-features --features sync-session -- profile redis=127.0.0.1:6379`, 401: Unauthorized
/// `set RUST_LOG=debug && cargo run --bin cli --no-default-features --features sync-session -- profile redis=127.0.0.1:6379 --sid WRONG_SID`, code 0, failed to receive message (0: invalid sid "WRONG_SID", less then 76 chars)
/// `set RUST_LOG=debug && cargo run --bin cli --no-default-features --features sync-session -- profile redis=127.0.0.1:6379 --sid $SESSION_ID`, Succeed to auto logged and got the new sid
#[api]
pub fn profile(mut req: CliRaw) {
    let session = CliProtocol::session::<RedisSession<Connection, Member>, _>(&mut req)?;
    let member = session.user().unwrap().clone();
    // you can make a breakpoint here to check the server address is logged on session in redis
    req.pack(Serialized::String(format!("Your profile:\n{:?}", member)))
}


#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Member {
    id: u64,
    name: String,
}

impl UidGetter for Member {
    fn id(&self) -> u64 {
        self.id
    }
}