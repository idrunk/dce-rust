use std::any::Any;
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use async_trait::async_trait;
use http_body_util::{combinators::BoxBody, BodyExt, Empty, Full};
use hyper::body::{Bytes, Incoming};
use hyper::{Method, Request, Response, StatusCode};
use dce_router::protocol::RoutableProtocol;
use dce_router::request::{RawRequest, RequestContext, Request as DceRequest};
use dce_router::router::Router;
use dce_router::serializer::Serialized;
use dce_util::mixed::{DceErr, DceResult};
use crate::request::{HttpMethodGetter, HttpRawRequest};

pub type HttpRaw = DceRequest<HttpRawRequest<HyperHttpProtocol>, (), (), (), ()>;
pub type HttpGet<Dto> = DceRequest<HttpRawRequest<HyperHttpProtocol>, (), (), Dto, Dto>;
pub type HttpSame<Dto> = DceRequest<HttpRawRequest<HyperHttpProtocol>, Dto, Dto, Dto, Dto>;
pub type HttpNoConvert<Req, Resp> = DceRequest<HttpRawRequest<HyperHttpProtocol>, Req, Req, Resp, Resp>;
pub type Http<ReqDto, Req, Resp, RespDto> = DceRequest<HttpRawRequest<HyperHttpProtocol>, ReqDto, Req, Resp, RespDto>;

#[derive(Debug)]
pub struct HyperHttpProtocol {
    req: Option<Request<Incoming>>,
    resp: Option<Response<BoxBody<Bytes, Infallible>>>,
}

impl HyperHttpProtocol {
    pub async fn route(
        self,
        router: Arc<Router<HttpRawRequest<Self>>>,
        context_data: HashMap<String, Box<dyn Any + Send>>,
    ) -> Result<Response<BoxBody<Bytes, Infallible>>, Infallible> {
        Ok(HyperHttpProtocol { req: None, resp: None }
            .handle_result(HttpRawRequest::route(RequestContext::new(router, HttpRawRequest::new(self)).set_data(context_data)).await).unwrap())
    }
}

impl From<Request<Incoming>> for HyperHttpProtocol {
    fn from(value: Request<Incoming>) -> Self {
        Self { req: Some(value), resp: None }
    }
}

impl Into<Response<BoxBody<Bytes, Infallible>>> for HyperHttpProtocol {
    fn into(self) -> Response<BoxBody<Bytes, Infallible>> {
        self.resp.unwrap()
    }
}

#[async_trait]
impl RoutableProtocol for HyperHttpProtocol {
    type Req = Request<Incoming>;
    type Resp = Response<BoxBody<Bytes, Infallible>>;

    fn path(&self) -> &str {
        let Some(req) = &self.req else { unreachable!() };
        req.uri().path().trim_start_matches('/')
    }

    async fn body(&mut self) -> DceResult<Serialized> {
        assert!(matches!(self.req, Some(_)), "req can only take once.");
        let Some(req) = self.req.take() else { unreachable!() };
        Ok(Serialized::Bytes(req.collect().await.or_else(|e| Err(DceErr::closed(0, e.to_string())))?.to_bytes()))
    }

    fn pack_resp(self, serialized: Serialized) -> Self::Resp {
        Response::new(match serialized {
            Serialized::String(str) => Full::from(str).boxed(),
            Serialized::Bytes(bytes) => Full::from(bytes).boxed(),
        })
    }

    fn err_into(self, code: isize, message: String) -> Self::Resp {
        Response::builder().status(code as u16)
            .body(if message.is_empty() { Empty::new().boxed() } else { Full::from(message).boxed() })
            .expect("failed to build a response")
    }

    fn handle_result(self, (_, resp): (Option<bool>, DceResult<Option<Self::Resp>>)) -> Option<Self::Resp> {
        Self::try_print_err(&resp);
        Some(match resp {
            Ok(resp) => resp.unwrap_or_else(|| Response::new(Empty::new().boxed())),
            Err(DceErr::Openly(err)) => self.err_into(if err.code > 0 && err.code < 600 { err.code as u16 } else { StatusCode::SERVICE_UNAVAILABLE.as_u16() } as isize, err.message),
            Err(DceErr::Closed(_)) => self.err_into(StatusCode::SERVICE_UNAVAILABLE.as_u16() as isize, "".to_owned())
        })
    }
}

impl HttpMethodGetter for HyperHttpProtocol {
    fn method(&self) -> &Method {
        let Some(req) = &self.req else { unreachable!() };
        req.method()
    }
}