use std::any::Any;
use std::collections::HashMap;
use std::fmt::Debug;
use async_trait::async_trait;
use hyper::Method;
use dce_router::api::{Method as DceMethod};
use dce_router::protocol::RoutableProtocol;
use dce_router::request::RawRequest;
use dce_router::serializer::Serialized;
use dce_util::mixed::DceResult;


#[derive(Debug)]
pub struct HttpRawRequest<T: RoutableProtocol>(T);

#[async_trait]
impl<T: RoutableProtocol + HttpMethodGetter + Debug + Send + 'static> RawRequest for HttpRawRequest<T> {
    type Rpi = T;
    type Req = T::Req;
    type Resp = T::Resp;

    fn new(req: T) -> Self {
        Self(req)
    }

    fn path(&self) -> &str {
        self.0.path()
    }

    fn raw(&self) -> &Self::Rpi {
        &self.0
    }

    fn raw_mut(&mut self) -> &mut Self::Rpi {
        &mut self.0
    }

    async fn data(&mut self) -> DceResult<Serialized> {
        self.0.body().await
    }

    fn pack_response(self, serialized: Serialized) -> DceResult<Self::Resp> {
        Ok(self.0.pack_resp(serialized))
    }

    fn parse_api_method(prop_mapping: &mut HashMap<&'static str, Box<dyn Any + Send + Sync>>) -> Option<Box<dyn DceMethod<Self::Rpi> + Send + Sync>> {
        Some(Box::new(HttpMethodSet(if prop_mapping.contains_key("method") {
            let method = prop_mapping.remove("method").unwrap();
            if method.is::<Method>() {
                vec![*method.downcast::<Method>().unwrap()]
            } else {
                *method.downcast::<Vec<Method>>().unwrap()
            }
        } else {
            vec![Method::GET]
        })))
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
pub struct HttpMethodSet(Vec<Method>);

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
