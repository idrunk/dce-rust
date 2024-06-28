use std::net::SocketAddr;
use http_body_util::{BodyExt, Full};
use hyper::Response;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use log::info;
use sailfish::TemplateOnce;
use dce_hyper::protocol::HttpMethod::{Get, Options, Post};
use dce_router::api::EventHandler;
use dce_router::request::{PathParam, Context};
use dce_router::router::Router;
use dce_router::serializer::JsonSerializer;
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use dce_cli::protocol::CliRaw;
use dce_hyper::protocol::{Http, HttpGet, HttpRaw, HyperHttpProtocol};
use dce_hyper::serializer::SailfishSerializer;
use dce_macro::{api, openly_err};
use dce_util::mixed::DceResult;


/// `set RUST_LOG=debug && cargo run --bin app --target-dir target/http -- http start`
#[api("http/start")]
async fn http_start(_: CliRaw) {
    let addr = SocketAddr::from(([127, 0, 0, 1], 2046));
    let router = Router::new()?
        .set_event_handlers(Some(EventHandler::Async(Box::new(|context| Box::pin(interceptor(context))))), None)
        .push(var1)
        .push(var2)
        .push(var3)
        .push(var4)
        .push(var5)
        .push(var6)
        .push(session)
        .push(hello)
        .push(hello_post)
        .push(home)
        .ready()?;

    let listener = TcpListener::bind(addr).await.expect(format!("cannot bind tcp to {}", addr).as_str());
    info!("Dce started at {}:{} with Hyper", addr.ip(), addr.port());
    loop {
        let (stream, _) = listener.accept().await.expect("cannot accept tcp stream");
        let io = TokioIo::new(stream);

        tokio::task::spawn(async {
            if let Err(err) = http1::Builder::new()
                .serve_connection(io, service_fn(|req| HyperHttpProtocol::from(req).route(router.clone(), Default::default())))
                .await
            {
                println!("Error serving connection: {:?}", err);
            }
        });
    }
}

async fn interceptor(context: &mut Context<HyperHttpProtocol>) -> DceResult<()> {
    if context.api().unwrap().path() == "session/{username?}" {
        if matches!(context.params().get("username"), Some(PathParam::Option(Some(_)))) {
            context.put_data("hello".to_string(), Box::new("attach the to controller"));
        } else {
            return Err(openly_err!(401, "need to login"));
        }
    }
    Ok(())
}

/// `curl http://127.0.0.1:2046/var1`
#[api("{var1}")]
pub fn var1(req: HttpRaw) {
    let path_args = format!("{:#?}", req.params());
    req.raw_resp(Response::new(Full::from(path_args).boxed()))
}

/// `curl http://127.0.0.1:2046/var1/hello`
#[api("{var1}/hello")]
pub fn var2(req: HttpRaw) {
    let path_args = format!("{:#?}", req.params());
    req.raw_resp(Response::new(Full::from(path_args).boxed()))
}

/// `curl http://127.0.0.1:2046/var1/var3`
/// `curl http://127.0.0.1:2046/var1/var3/var3`
#[api("{var1}/var3/{var3?}")]
pub fn var3(req: HttpRaw) {
    let path_args = format!("{:#?}", req.params());
    req.raw_resp(Response::new(Full::from(path_args).boxed()))
}

/// `curl http://127.0.0.1:2046/var4`
/// `curl http://127.0.0.1:2046/var4/var4`
#[api("var4/{var4*}")]
pub fn var4(req: HttpRaw) {
    let path_args = format!("{:#?}", req.params());
    req.raw_resp(Response::new(Full::from(path_args).boxed()))
}

/// `curl http://127.0.0.1:2046/var5/var5/var5`
/// `curl http://127.0.0.1:2046/var5/var5/var5/var5`
#[api("var5/var5/{var5+}")]
pub fn var5(req: HttpRaw) {
    let path_args = format!("{:#?}", req.params());
    req.raw_resp(Response::new(Full::from(path_args).boxed()))
}

/// `curl http://127.0.0.1:2046/var6/var6/var6/var6`
#[api("var6/var6/{var6}/var6")]
pub fn var6(req: HttpRaw) {
    let path_args = format!("{:#?}", req.params());
    req.raw_resp(Response::new(Full::from(path_args).boxed()))
}

/// `curl http://127.0.0.1:2046/session/dce`
/// `curl http://127.0.0.1:2046/session/drunk`
/// `curl -I http://127.0.0.1:2046/session`
#[api("session/{username?}", serializer = JsonSerializer{})]
pub fn session(req: HttpRaw) {
    if matches!(req.params().get("username"), Some(PathParam::Option(Some(username))) if username == "dce") {
        println!("{:#?}", *req.data().get("hello").unwrap().downcast_ref::<&str>().unwrap());
        req.success(None)
    } else {
        println!("{:#?}", req.data());
        req.fail(Some("invalid manager".to_string()), 403)
    }
}

/// `curl http://127.0.0.1:2046/hello`
#[api(method = Get, serializer = JsonSerializer{})]
pub async fn hello(req: HttpRaw) -> DceResult<Option<HttpResp>> {
    println!("{:#?}", req);
    req.success(None)
}

/// `curl -H "Content-Type: application/json" -d "{""user"":""Drunk"",""age"":18}" http://127.0.0.1:2046/hello`
#[api("hello", method = [Post, Options], serializer = [JsonSerializer{}])]
pub async fn hello_post(mut req: Http<GreetingReq, GreetingResp>) {
    let legal_age = 18;
    let body: Greeting = req.req().await?;
    if body.age >= legal_age {
        let mut reqd = body.clone();
        reqd.welcome = format!("Hello {}, welcome", reqd.user);
        req.success(Some(reqd.into()))
    } else {
        req.fail(Some(format!("Sorry, only service for over {} years old peoples", legal_age)), 0)
    }
}

/// `curl http://127.0.0.1:2046/`
#[api(serializer = SailfishSerializer{}, omission = true)]
pub async fn home(req: HttpGet<Greeting>) {
    req.resp(Greeting {
        user: "Dce".to_string(),
        age: 18,
        welcome: "Welcome to Rust".to_string(),
    })
}


#[derive(Debug, Clone, TemplateOnce)]
#[template(path = "home.html")]
pub struct Greeting {
    user: String,
    age: u8,
    welcome: String,
}

#[derive(Deserialize)]
pub struct GreetingReq {
    user: String,
    age: u8,
}

impl From<GreetingReq> for Greeting {
    fn from(value: GreetingReq) -> Self {
        Greeting {
            user: value.user,
            age: value.age,
            welcome: "".to_string(),
        }
    }
}

#[derive(Serialize)]
pub struct GreetingResp {
    welcome: String,
}

impl Into<GreetingResp> for Greeting {
    fn into(self) -> GreetingResp {
        GreetingResp { welcome: self.welcome }
    }
}
