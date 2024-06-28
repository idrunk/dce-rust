use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::convert::Infallible;
use std::fmt::Debug;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use async_trait::async_trait;
use http_body_util::{combinators::BoxBody, BodyExt, Empty, Full};
use hyper::body::{Bytes, Incoming};
use hyper::{Method, Request, Response, StatusCode};
#[allow(unused)]
use hyper::header::{COOKIE, HeaderValue};
use dce_router::protocol::{Meta, RoutableProtocol};
use dce_router::request::{Context, Request as DceRequest, Response as DceResponse};
use dce_router::router::Router;
use dce_router::serializer::Serialized;
use dce_util::mixed::{DceErr, DceResult};
use dce_router::api::Method as DceMethod;

pub type HttpRaw<'a> = DceRequest<'a, HyperHttpProtocol, (), ()>;
pub type HttpGet<'a, Dto> = DceRequest<'a, HyperHttpProtocol, (), Dto>;
pub type HttpSame<'a, Dto> = DceRequest<'a, HyperHttpProtocol, Dto, Dto>;
pub type Http<'a, ReqDto, RespDto> = DceRequest<'a, HyperHttpProtocol, ReqDto, RespDto>;

#[derive(Debug)]
pub struct HyperHttpProtocol {
    meta: Meta<Request<Incoming>, Response<BoxBody<Bytes, Infallible>>>,
}

impl HyperHttpProtocol {
    pub async fn route(
        self,
        router: Arc<Router<Self>>,
        context_data: HashMap<String, Box<dyn Any + Send>>,
    ) -> Result<Response<BoxBody<Bytes, Infallible>>, Infallible> {
        Self::handle(self, router, context_data).await.ok_or_else(|| unreachable!("http route should always return Some(Resp)"))
    }
}

impl From<Request<Incoming>> for HyperHttpProtocol {
    fn from(value: Request<Incoming>) -> Self {
        Self { meta: Meta::new(value, Default::default()) }
    }
}

impl Into<Response<BoxBody<Bytes, Infallible>>> for HyperHttpProtocol {
    fn into(mut self) -> Response<BoxBody<Bytes, Infallible>> {
        let resp = self.resp_mut().take();
        #[allow(unused_mut)]
        let mut resp = match resp {
            None => Response::new(Empty::new().boxed()),
            Some(DceResponse::Serialized(sd)) => self.pack_resp(sd),
            Some(DceResponse::Raw(rr)) => rr,
        };
        #[cfg(feature = "session")]
        if let Some(resp_sid) = self.get_resp_sid() {
            resp.headers_mut().insert("X-Session-Id", HeaderValue::from_str(resp_sid.as_str()).unwrap());
        }
        resp
    }
}

impl Deref for HyperHttpProtocol {
    type Target = Meta<Request<Incoming>, Response<BoxBody<Bytes, Infallible>>>;

    fn deref(&self) -> &Self::Target {
        &self.meta
    }
}

impl DerefMut for HyperHttpProtocol {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.meta
    }
}

#[async_trait]
impl RoutableProtocol for HyperHttpProtocol {
    type Req = Request<Incoming>;
    type Resp = Response<BoxBody<Bytes, Infallible>>;

    async fn body(&mut self) -> DceResult<Serialized> {
        let req = self.req_mut().take().ok_or_else(|| DceErr::closed0("Empty request"))?;
        Ok(Serialized::Bytes(req.collect().await.or_else(DceErr::closed0_wrap)?.to_bytes()))
    }

    fn pack_resp(&self, serialized: Serialized) -> Self::Resp {
        Response::new(match serialized {
            Serialized::String(str) => Full::from(str).boxed(),
            Serialized::Bytes(bytes) => Full::from(bytes).boxed(),
        })
    }

    fn path(&self) -> &str {
        self.req().map_or("", |r| r.uri().path().trim_start_matches('/'))
    }

    fn handle_result(self, result: DceResult<()>, _: &mut Context<Self>) -> Option<Self::Resp> {
        Self::try_print_err(&result);
        Some(match result {
            Ok(_) => self.into(),
            Err(err) => {
                let code = err.value().code;
                let is_openly = matches!(err, DceErr::Openly(_));
                let mut resp = self.err_into(err);
                if is_openly && code < 600 {
                    *resp.status_mut() = StatusCode::from_u16(code as u16).unwrap_or(StatusCode::SERVICE_UNAVAILABLE);
                }
                resp
            },
        })
    }

    #[cfg(feature = "session")]
    fn sid(&self) -> Option<&str> {
        self.req().map_or(None, |r| r.headers().get("X-Session-Id").map(|v| v.to_str().unwrap())
            .or_else(|| r.headers().get(COOKIE).iter().find_map(|v| {
                let mut cookies = v.to_str().unwrap_or("").split(';');
                cookies.find_map(|kv| if let Some(index) = kv.find("session_id=") { Some(&kv[index + 11 ..]) } else { None } )
            })))
    }

    fn parse_api_method(prop_mapping: &mut HashMap<&'static str, Box<dyn Any + Send + Sync>>) -> Option<Box<dyn DceMethod<Self> + Send + Sync>> {
        Self::parse_http_method(prop_mapping)
    }
}


impl HttpProtocol for HyperHttpProtocol {}

impl HttpMethodGetter for HyperHttpProtocol {
    fn method(&self) -> &Method {
        self.req().map_or_else(|_| &Method::GET, |r| r.method())
    }
}

pub trait HttpProtocol: RoutableProtocol + HttpMethodGetter {
    fn parse_http_method(prop_mapping: &mut HashMap<&'static str, Box<dyn Any + Send + Sync>>) -> Option<Box<dyn DceMethod<Self> + Send + Sync>> {
        Some(Box::new(prop_mapping.remove("method").map(|ms| if ms.is::<Method>() {
            ms.downcast::<Method>().map(|m| HashSet::from([*m])).ok()
        } else {
            ms.downcast::<Vec<Method>>().map(|m| m.into_iter().collect::<HashSet<_>>()).ok()
        }).unwrap_or_else(|| Some(HashSet::from([Method::GET, Method::HEAD, Method::OPTIONS]))).map(HttpMethodSet)?))
    }
}

#[allow(non_snake_case, non_upper_case_globals)]
pub mod HttpMethod {
    use hyper::Method;

    pub const Get: Method = Method::GET;
    pub const Post: Method = Method::POST;
    pub const Put: Method = Method::PUT;
    pub const Delete: Method = Method::DELETE;
    pub const Head: Method = Method::HEAD;
    pub const Options: Method = Method::OPTIONS;
    pub const Connect: Method = Method::CONNECT;
    pub const Patch: Method = Method::PATCH;
    pub const Trace: Method = Method::TRACE;
}

#[derive(Debug)]
pub struct HttpMethodSet(HashSet<Method>);

impl<T: HttpMethodGetter> DceMethod<T> for HttpMethodSet {
    fn to_string(&self) -> String {
        format!("[{}]", self.0.iter().map(|m| m.to_string()).fold(String::new(), |a, b| format!("{a}, {b}")))
    }

    fn req_match(&self, raw: &T) -> bool {
        self.0.contains(raw.method())
    }
}

pub trait HttpMethodGetter {
    fn method(&self) -> &Method;
}
