use std::fmt::Debug;
use dce_util::mixed::{DceErr, DceResult};
use crate::request::{Context, Response};
use crate::serializer::{Deserializer, Serializable, Serialized, Serializer};
use log::{error, warn};
use std::any::Any;
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use crate::api::{ApiTrait, Method};
use crate::router::{CODE_NOT_FOUND, Router};
#[cfg(feature = "async")]
use async_trait::async_trait;


pub const HEAD_PATH_NAME: &'static str = "$#path#";
pub const HEAD_ID_NAME: &'static str = "$#id#";
#[cfg(feature = "session")]
pub const HEAD_SID_NAME: &'static str = "Session-Id";


#[derive(Debug)]
pub struct Meta<Req, Resp> {
    req: Option<Req>,
    resp: Option<Response<Resp>>,
    heads: HashMap<String, String>,
    resp_heads: HashMap<String, String>,
}

impl<Req, Resp> Meta<Req, Resp> {
    pub fn new(req: Req, heads: HashMap<String, String>) -> Self {
        Self { req: Some(req), resp: None, heads, resp_heads: Default::default() }
    }
    
    pub fn req(&self) -> DceResult<&Req> {
        self.req.as_ref().ok_or_else(|| DceErr::closed0("Invalid empty request"))
    }
    
    pub fn req_mut(&mut self) -> &mut Option<Req> {
        &mut self.req
    }
    
    pub fn resp_mut(&mut self) -> &mut Option<Response<Resp>> {
        &mut self.resp
    }
    
    pub fn heads(&self) -> &HashMap<String, String> {
        &self.heads
    }
    
    pub fn resp_heads(&self) -> &HashMap<String, String> {
        &self.resp_heads
    }
    
    pub fn resp_heads_mut(&mut self) -> &mut HashMap<String, String> {
        &mut self.resp_heads
    }
}


#[cfg_attr(feature = "async", async_trait)]
pub trait RoutableProtocol: From<Self::Req> + Into<Self::Resp> + Deref<Target = Meta<Self::Req, Self::Resp>> + DerefMut + Debug {
    type Req;
    type Resp: Debug;

    #[cfg(feature = "async")]
    async fn body(&mut self) -> DceResult<Serialized>;
    #[cfg(not(feature = "async"))]
    fn body(&mut self) -> DceResult<Serialized>;
    fn pack_resp(&self, serialized: Serialized) -> Self::Resp;

    fn path(&self) -> &str {
        self.heads.get(HEAD_PATH_NAME).map_or("", |v| v.as_str())
    }

    fn id(&self) -> Option<&str> {
        self.heads.get(HEAD_ID_NAME).map(|v| v.as_str())
    }

    #[cfg(feature = "async")]
    async fn handle(self, router: Arc<Router<Self>>, context_data: HashMap<String, Box<dyn Any + Send>>) -> Option<Self::Resp> {
        let mut context = Context::new(router, self, context_data);
        let result = Router::route(&mut context).await;
        context.take_rp()?.handle_result(result, &mut context)
    }

    #[cfg(not(feature = "async"))]
    fn handle(self, router: Arc<Router<Self>>, context_data: HashMap<String, Box<dyn Any + Send>>) -> Option<Self::Resp> {
        let mut context = Context::new(router, self, context_data);
        let result = Router::route(&mut context);
        context.take_rp()?.handle_result(result, &mut context)
    }

    fn api_match(&self, apis: &[&'static (dyn ApiTrait<Self> + Send + Sync)]) -> DceResult<&'static (dyn ApiTrait<Self> + Send + Sync)> {
        apis.iter().find(|n| n.method_match(self)).map(|a| *a)
            .ok_or_else(|| DceErr::openly(CODE_NOT_FOUND, format!(r#"Path "{}" cannot match any Api by Method"#, self.path())))
    }

    fn deserializer<'a, ReqDto>(deserializers: &'a [Box<dyn Deserializer<ReqDto> + Send + Sync>], _context: &Context<Self>) -> DceResult<&'a Box<dyn Deserializer<ReqDto> + Send + Sync>> {
        deserializers.first().ok_or_else(|| DceErr::closed0("No deserializer configured"))
    }

    fn serializer<'a, RespDto>(serializers: &'a [Box<dyn Serializer<RespDto> + Send + Sync>], _context: &Context<Self>) -> DceResult<&'a Box<dyn Serializer<RespDto> + Send + Sync>> {
        serializers.last().ok_or_else(|| DceErr::closed0("No serializer configured"))
    }

    fn deserialize<ReqDto>(serializers: &[Box<dyn Deserializer<ReqDto> + Send + Sync>], seq: Serialized, context: &Context<Self>) -> DceResult<ReqDto> {
        Self::deserializer(serializers, context)?.deserialize(seq)
    }

    fn serialize<RespDto>(serializers: &[Box<dyn Serializer<RespDto> + Send + Sync>], dto: Serializable<RespDto>, context: &Context<Self>) -> DceResult<Serialized> {
        Self::serializer(serializers, context)?.serialize(dto)
    }

    fn pack_responsible<RespDto: 'static>(
        context: &Context<Self>,
        serializers: &[Box<dyn Serializer<RespDto> + Send + Sync>],
        responsible: Serializable<RespDto>,
    ) -> DceResult<Option<Response<Self::Resp>>> {
        Self::serialize(serializers, responsible, &context).map(|sd| Some(Response::Serialized(sd)))
    }

    // Parse the "method" object and "extras" properties of Api. Protocol developers can implement the "parse_api_method" method in the protocol implementation 
    // and delete the prop_tuples member that has been parsed into the Method. The remaining members will be used as extras Map members
    fn parse_api_method_and_extras(prop_tuples: Vec<(&'static str, Box<dyn Any + Send + Sync>)>) -> (Option<Box<dyn Method<Self> + Send + Sync>>, HashMap<&'static str, Box<dyn Any + Send + Sync>>) {
        let mut prop_mapping: HashMap<_, _> = prop_tuples.into_iter().collect();
        (Self::parse_api_method(&mut prop_mapping), prop_mapping)
    }

    // Protocol developers could override implement this method and should remove the parsed prop_tuples member
    fn parse_api_method(_prop_mapping: &mut HashMap<&'static str, Box<dyn Any + Send + Sync>>) -> Option<Box<dyn Method<Self> + Send + Sync>> {
        None
    }

    fn try_print_err(response: &DceResult<()>) {
        if let Err(error) = response {
            match error {
                DceErr::Openly(err) => warn!("code {}, {}", err.code, err.message),
                DceErr::Closed(err) => error!("code {}, {}", err.code, err.message),
            };
        }
    }

    fn err_into(mut self, err: DceErr) -> Self::Resp {
        self.resp = Some(Response::Raw(self.pack_resp(Serialized::String(err.to_responsible()))));
        self.into()
    }

    fn handle_result(self, result: DceResult<()>, context: &mut Context<Self>) -> Option<Self::Resp> {
        Self::try_print_err(&result);
        if ! context.api().map_or_else(|| self.id().is_none(), |a| a.unresponsive()) {
            return Some(match result {
                Ok(_) => self.into(),
                Err(err) => self.err_into(err),
            });
        }
        None
    }

    #[cfg(feature = "session")]
    fn sid(&self) -> Option<&str> {
        self.heads.get(HEAD_SID_NAME).map(String::as_str)
    }

    #[cfg(feature = "session")]
    fn set_resp_sid(&mut self, sid: String) {
        self.resp_heads.insert(HEAD_SID_NAME.to_string(), sid);
    }

    #[cfg(feature = "session")]
    fn get_resp_sid(&mut self) -> Option<&String> {
        self.resp_heads.get(HEAD_SID_NAME)
    }

    #[cfg(feature = "session")]
    fn set_session<Rp: RoutableProtocol + Debug + 'static>(context: &mut Context<Rp>, value: Box<dyn Any + Send>) {
        context.put_data("$#session#".to_string(), value);
    }

    #[cfg(feature = "session")]
    fn session<S: 'static, Rp: RoutableProtocol + Debug + 'static>(context: &mut Context<Rp>) -> DceResult<&mut S> {
        context.get_as_mut("$#session#")
    }
}
