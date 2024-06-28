use std::collections::HashSet;
use std::net::SocketAddr;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use log::info;
use redis::aio::MultiplexedConnection;
use redis::Client;
use dce_hyper::protocol::HttpMethod::{Patch, Post};
use dce_router::api::{EventHandler};
use dce_router::request::{Context, Response};
use dce_router::router::Router;
use dce_router::serializer::{Serialized};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::sync::OnceCell;
use dce_cli::protocol::CliRaw;
use dce_hyper::protocol::{HttpRaw, HyperHttpProtocol};
use dce_macro::{api};
use dce_router::protocol::RoutableProtocol;
use dce_session::auto::AutoRenew;
use dce_session::redis::RedisSession;
use dce_session::session::{DEFAULT_TTL_MINUTES, Session};
use dce_session::user::{UidGetter, User};
use dce_util::mixed::{DceErr, DceResult};


static REDIS_CLIENT: OnceCell<Client> = OnceCell::const_new();

pub fn redis_prepare(host: &str) {
    REDIS_CLIENT.set(Client::open(format!("redis://{host}")).unwrap()).unwrap()
}

pub async fn redis() -> MultiplexedConnection {
    REDIS_CLIENT.get().unwrap().get_multiplexed_async_connection().await.unwrap()
}

/// `set RUST_LOG=debug && cargo run --bin app --target-dir target/session -- http start session redis=127.0.0.1:6379`
#[api("http/start/session")]
async fn http_start_session(req: CliRaw) {
    let redis_host = req.rp().args().get("redis").ok_or(DceErr::closed0(r#"You must specific the redis host, for example "http start session redis=127.0.0.1:6379""#))?;
    redis_prepare(redis_host);
    let addr = SocketAddr::from(([127, 0, 0, 1], 2050));
    let router = Router::new()?
        .set_event_handlers(Some(EventHandler::Async(Box::new(|context| Box::pin(before_controller(context))))),
                            Some(EventHandler::Async(Box::new(|context| Box::pin(after_controller(context))))))
        .push(index)
        .push(login)
        .push(profile)
        .push(modify)
        .push(user)
        .ready()?;
    
    let listener = TcpListener::bind(addr).await.expect(format!("cannot bind tcp to {}", addr).as_str());
    info!("Dce started at {}:{} with session feature and Hyper crate", addr.ip(), addr.port());
    loop {
        let (stream, _) = listener.accept().await.expect("cannot accept tcp stream");
        let io = TokioIo::new(stream);

        tokio::task::spawn(async {
            if let Err(err) = http1::Builder::new()
                .serve_connection(io, service_fn(|req| HyperHttpProtocol::from(req).route(router.clone(), Default::default()))).await
            {
                println!("Error serving connection: {:?}", err);
            }
        });
    }
}

async fn after_controller(context: &mut Context<HyperHttpProtocol>) -> DceResult<()> {
    if let Some(new_sid) = context.rp_mut().get_resp_sid().map(|s| s.to_string()) {
        if let Some(Response::Serialized(Serialized::String(body))) = context.rp_mut().resp_mut() {
            body.push_str(format!("\n\nGot new sid, you can use it to access private page:\n{}", new_sid).as_str());
        }
    }
    Ok(())
}

async fn before_controller(context: &mut Context<HyperHttpProtocol>) -> DceResult<()> {
    let mut session = match context.rp().sid() {
        Some(sid) => RedisSession::new_with_id(vec![sid.to_string()]),
        _ => RedisSession::<MultiplexedConnection, Member>::new(60),
    }?.with(redis().await).auto().config(Some(240), None, None, None);

    let mut auth = AppAuth::new(context, &mut session);
    auth.valid().await?;
    if auth.is_auto_login()? {
        auth.auto_login().await?;
    } else if ! auth.is_login() {
        auth.try_renew().await?;
    }

    HyperHttpProtocol::set_session(context, Box::new(session.unwrap()));
    Ok(())
}

struct AppAuth<'a> {
    context: &'a mut Context<HyperHttpProtocol>,
    session: &'a mut AutoRenew<RedisSession<MultiplexedConnection, Member>>,
    roles_needs: HashSet<u16>,
}

impl<'a> AppAuth<'a> {
    fn new(context: &'a mut Context<HyperHttpProtocol>, session: &'a mut AutoRenew<RedisSession<MultiplexedConnection, Member>>) -> Self {
        let roles_needs = context.api().unwrap().extras().get("roles").into_iter()
            .flat_map(|v| v.downcast_ref::<Vec<_>>().map_or_else(|| vec![], |r| r.clone())).collect();
        Self {
            context,
            session,
            roles_needs,
        }
    }
    
    fn is_private(&self) -> bool {
        ! self.roles_needs.is_empty()
    }
    
    fn is_login(&self) -> bool {
        self.context.api().unwrap().path().ends_with("login")
    }
    
    fn is_auto_login(&self) -> DceResult<bool> {
        self.context.rp().req().map(|r| r.uri().query().map_or(false, |q| q.contains("autologin")))
    }

    async fn auto_login(&mut self) -> DceResult<()> {
        if self.session.auto_login().await? {
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
        if self.is_private() {
            if let Some(user) = self.session.user().await {
                if ! self.roles_needs.contains(&user.role_id) {
                    return Err(DceErr::openly(403, "Forbidden".to_string()))
                }
            } else {
                return Err(DceErr::openly(401, "Unauthorized".to_string()))
            }
        }
        Ok(())
    }
}

/// `curl http://127.0.0.1:2050/`
#[api("")]
async fn index(req: HttpRaw) {
    req.pack(Serialized::String("This is a public page, you can access without a token".to_string()))
}

/// `curl http://127.0.0.1:2050/login -d "{""name"":""Drunk""}"`, role 1
/// `curl http://127.0.0.1:2050/login -d "{""name"":""Dce""}"`, role 2
#[api(method = [Post])]
async fn login(mut req: HttpRaw) {
    let data = req.rp_mut().body().await?.json_value()?;
    let name = data["name"].as_str().ok_or(DceErr::openly(1000, "Name required".to_string()))?;
    let members = members();
    let member = members.iter().find(|m| m.name.eq_ignore_ascii_case(name)).ok_or(DceErr::openly(1001, "Wrong name".to_string()))?;
    let session = HyperHttpProtocol::session::<RedisSession<MultiplexedConnection, Member>, _>(&mut req)?;
    if session.login(member.clone(), DEFAULT_TTL_MINUTES).await? {
        let new_sid = session.id().to_string();
        req.rp_mut().set_resp_sid(new_sid);
        return req.pack(Serialized::String(format!("Succeed login with:\n{:?}", member)))
    }
    req.pack(Serialized::String("Failed to login".to_string()))
}

/// `curl http://127.0.0.1:2050/manage/profile`, without sid, cannot access got 401
/// `curl http://127.0.0.1:2050/manage/profile -H "X-Session-Id: $session_id"`, pass sid on header, can access if sid is valid
/// `curl http://127.0.0.1:2050/manage/profile -b "session_id=$session_id"`, pass sid in cookies, can access if sid is valid
/// `curl http://127.0.0.1:2050/manage/profile?autologin=1 -H "X-Session-Id: $session_id"`, use long life sid to do auto login, will get new sid and the old will destroy
#[api("manage/profile", roles = [1u16, 2])]
async fn profile(mut req: HttpRaw) {
    let session = HyperHttpProtocol::session::<RedisSession<MultiplexedConnection, Member>, _>(&mut req)?;
    let member = session.user().await.unwrap().clone();
    req.pack(Serialized::String(format!("Your profile:\n{:?}", member)))
}

/// `curl -X PATCH http://127.0.0.1:2050/manage/profile -H "X-Session-Id: $session_id" -d "{}"`, none required fields, got openly err response
/// `curl -X PATCH http://127.0.0.1:2050/manage/profile -H "X-Session-Id: $session_id" -d "{""name"":""Foo"",""role_id"":2}"`, with required, curren session user will update to role 2
#[api("manage/profile", method = [Patch], roles = [1u16, 2])]
async fn modify(mut req: HttpRaw) {
    let data = req.rp_mut().body().await?.json_value()?;
    let new_name = data["name"].as_str();
    let new_role_id = data["role_id"].as_u64();   
    if new_name.is_none() && new_role_id.is_none() {
        return Err(DceErr::openly(1010, "Must specified something to modify".to_string()));
    }
    let session = HyperHttpProtocol::session::<RedisSession<MultiplexedConnection, Member>, _>(&mut req)?;
    let mut member = session.user().await.unwrap().clone();
    let _ = new_name.map(|v| member.name = v.to_string());
    let _ = new_role_id.map(|v| member.role_id = v as u16);
    session.sync(&member).await?;
    req.pack(Serialized::String(format!("You have succeed to modified profile to:\n{:?}", member)))
}

/// `curl http://127.0.0.1:2050/manage/user -H "X-Session-Id: $session_id"`, got 403 if the session user role is 1, you can use role 2 user login to access
#[api("manage/user", roles = [2u16])]
async fn user(mut req: HttpRaw) {
    let session = HyperHttpProtocol::session::<RedisSession<MultiplexedConnection, Member>, _>(&mut req)?;
    let member = session.user().await.unwrap().clone();
    req.pack(Serialized::String(format!("You are role {}, so you can access, your profile:\n{:?}", member.role_id, member)))
}


#[derive(Serialize, Deserialize, Clone, Debug)]
struct Member {
    id: u64,
    name: String,
    role_id: u16,
}

impl UidGetter for Member {
    fn id(&self) -> u64 {
        self.id
    }
}

fn members() -> Vec<Member> {
    vec![
        Member { id: 1000, name: "Drunk".to_string(), role_id: 1, },
        Member { id: 1001, name: "Dce".to_string(), role_id: 2, },
        Member { id: 1002, name: "Rust".to_string(), role_id: 2, },
    ]
}