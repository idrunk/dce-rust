use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;
use futures_util::lock::Mutex;
use futures_util::StreamExt;
use log::{error, info, warn};
use redis::aio::MultiplexedConnection;
use redis::Client;
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::sync::OnceCell;
use tokio_tungstenite::accept_hdr_async;
use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};
use dce_cli::protocol::CliRaw;
use dce_macro::api;
use dce_router::api::EventHandler;
use dce_router::protocol::RoutableProtocol;
use dce_router::request::Context;
use dce_router::router::Router;
use dce_router::serializer::Serialized;
use dce_session::auto::AutoRenew;
use dce_session::connection::Connection;
use dce_session::redis::RedisSession;
use dce_session::session::{DEFAULT_TTL_MINUTES, Session};
use dce_session::user::{UidGetter, User};
use dce_tokio_tungstenite::protocol::{SemiWebsocketProtocol, SemiWebsocketRaw};
use dce_util::mixed::{DceErr, DceResult};


static REDIS_CLIENT: OnceCell<Client> = OnceCell::const_new();

pub fn redis_prepare(host: &str) {
    REDIS_CLIENT.set(Client::open(format!("redis://{host}")).unwrap()).unwrap()
}

pub async fn redis() -> MultiplexedConnection {
    REDIS_CLIENT.get().unwrap().get_multiplexed_async_connection().await.unwrap()
}

/// `set RUST_LOG=debug && cargo run --bin app --features connection-session --target-dir target/session_ws -- websocket start session redis=127.0.0.1:6379`
#[api("websocket/start/session")]
pub async fn websocket_start_session(req: CliRaw) {
    let redis_host = req.rp().args().get("redis").ok_or(DceErr::closed0(r#"You must specific the redis host, for example "http start session redis=127.0.0.1:6379""#))?;
    redis_prepare(redis_host);
    let addr = "0.0.0.0:2051";
    let server = TcpListener::bind(addr).await.unwrap();
    let router = Router::new()?
        .set_event_handlers(Some(EventHandler::Async(Box::new(|c| Box::pin(before_controller(c))))), None)
        .push(login)
        .push(profile)
        .ready()?;

    info!("Dce started at {} with session feature and tokio-tungstenite", addr);

    while let Ok((stream, _)) = server.accept().await {
        let server_addr = server.local_addr().map_or_else(|_| "".to_string(), |a| a.to_string());
        tokio::spawn(async move {
            let mut sid: Option<String> = None;
            let mut ws_stream = accept_hdr_async(stream, |req: &Request, response: Response| {
                let _ = req.headers().get("X-Session-Id").map(|v| sid = v.to_str().map_or(None, |v| Some(v.to_string())));
                Ok(response)
            }).await.expect("Error during the websocket handshake occurred");

            match match sid {
                Some(sid) => RedisSession::new_with_id(vec![sid]),
                None => RedisSession::<MultiplexedConnection, Member>::new(DEFAULT_TTL_MINUTES),
            }.map(|r| r.connect(server_addr)) {
                Ok(root) => {
                    while let Some(msg) = ws_stream.next().await {
                        match msg {
                            Ok(msg) if msg.is_text() || msg.is_binary() => SemiWebsocketProtocol::from(msg).binary()
                                .route(router.clone(), &mut ws_stream, HashMap::from([("root_session".to_string(), Box::new(root.clone()) as Box<dyn Any + Send>)])).await,
                            Err(err) => error!("{err}"),
                            _ => {} // when ping pong or other do nothing
                        };
                    }
                    let _ = root.lock().await.redis_then(redis().await).disconnect().await;
                },
                Err(e) => warn!("{e}"),
            }
        });
    }
    req.end(None)
}


async fn before_controller(context: &mut Context<SemiWebsocketProtocol>) -> DceResult<()> {
    let root = context.data().get("root_session")
        .map(|a| a.downcast_ref::<Arc<Mutex<RedisSession<MultiplexedConnection, Member>>>>().unwrap().clone()).unwrap();
    let is_first_request = matches!(root.lock().await.conn_meta().server_unbound(), Some(Some(_)));
    let mut session = RedisSession::clone_for_request(Arc::downgrade(&root), context.rp().sid().map(ToString::to_string)).await?.with(redis().await).auto();

    let mut auth = AppAuth::new(context, &mut session);
    auth.valid().await?;
    if auth.is_auto_login(is_first_request)? {
        auth.auto_login().await?;
    } else if ! auth.is_login() {
        auth.try_renew().await?;
    }

    SemiWebsocketProtocol::set_session(context, Box::new(session.unwrap()));
    Ok(())
}

struct AppAuth<'a> {
    context: &'a mut Context<SemiWebsocketProtocol>,
    session: &'a mut AutoRenew<RedisSession<MultiplexedConnection, Member>>,
}

impl<'a> AppAuth<'a> {
    fn new(context: &'a mut Context<SemiWebsocketProtocol>, session: &'a mut AutoRenew<RedisSession<MultiplexedConnection, Member>>) -> Self {
        Self { context, session, }
    }
    
    fn is_login(&self) -> bool {
        self.context.api().unwrap().path().ends_with("login")
    }

    fn is_auto_login(&self, is_first_request: bool) -> DceResult<bool> {
        Ok(is_first_request)
    }

    async fn auto_login(&mut self) -> DceResult<()> {
        if self.session.auto_login().await.is_ok() {
            self.context.rp_mut().set_resp_sid(self.session.id().to_string());
        }
        Ok(())
    }

    async fn try_renew(&mut self) -> DceResult<()> {
        if self.session.try_renew().await? {
            self.context.rp_mut().set_resp_sid(self.session.id().to_string());
        }
        Ok(())
    }

    async fn valid(&mut self) -> DceResult<()> {
        if ! self.is_login() {
            if self.session.user().await.is_none() {
                return Err(DceErr::openly(401, "Unauthorized".to_string()))
            }
        }
        Ok(())
    }
}



/// `cargo run --bin app --features connection-session -- websocket 127.0.0.1:2051 -- login`, 1000: Name required
/// `cargo run --bin app --features connection-session -- websocket 127.0.0.1:2051 --data "{""name"":""Dce""}" -- login`, 1001: Wrong name
/// `cargo run --bin app --features connection-session -- websocket 127.0.0.1:2051 --data "{""name"":""Drunk""}" -- login`, Succeed to logged and got new sid
#[api]
pub async fn login(mut req: SemiWebsocketRaw) {
    let data = req.rp_mut().body().await?.json_value()?;
    let name = data["name"].as_str().ok_or(DceErr::openly(1000, "Name required".to_string()))?;
    let member = Some(Member{ id: 100, name: "Drunk".to_string() }).into_iter()
        .find(|m| m.name.eq_ignore_ascii_case(name)).ok_or(DceErr::openly(1001, "Wrong name".to_string()))?;
    let session = SemiWebsocketProtocol::session::<RedisSession<MultiplexedConnection, Member>, _>(&mut req)?;
    if session.login(member.clone(), DEFAULT_TTL_MINUTES).await? {
        let new_sid = session.id().to_string();
        req.rp_mut().set_resp_sid(new_sid);
        return req.pack(Serialized::String(format!("Succeed login with:\n{:?}", member)))
    }
    req.pack(Serialized::String("Failed to login".to_string()))
}

/// `cargo run --bin app --features connection-session -- websocket 127.0.0.1:2051 -- profile`, 401: Unauthorized
/// `cargo run --bin app --features connection-session -- websocket 127.0.0.1:2051 --sid WRONG_SID -- profile`, code 0, failed to receive message (0: invalid sid "WRONG_SID", less then 76 chars)
/// `cargo run --bin app --features connection-session -- websocket 127.0.0.1:2051 --sid $SESSION_ID -- profile`, Succeed to auto logged and got the new sid
#[api]
pub async fn profile(mut req: SemiWebsocketRaw) {
    let session = SemiWebsocketProtocol::session::<RedisSession<MultiplexedConnection, Member>, _>(&mut req)?;
    let member = session.user().await.unwrap().clone();
    // you can make a breakpoint here to check the server address is logged on session in redis
    req.pack(Serialized::String(format!("Your profile:\n{:?}", member)))
}


#[derive(Serialize, Deserialize, Clone, Debug)]
struct Member {
    id: u64,
    name: String,
}

impl UidGetter for Member {
    fn id(&self) -> u64 {
        self.id
    }
}