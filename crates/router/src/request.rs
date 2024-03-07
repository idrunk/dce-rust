use std::any::Any;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;
use crate::api::{Api, ApiTrait, Method};
use crate::serializer::{Deserializer, Serializable, Serialized, Serializer};
use crate::router::{CODE_NOT_FOUND, Router};
use crate::protocol::RoutableProtocol;
use dce_util::mixed::{DceErr, DceResult};
use serde::Serialize;
#[cfg(feature = "async")]
use async_trait::async_trait;


#[derive(Debug)]
pub enum PathParam {
    Opt(Option<String>),
    Reqd(String),
    Vec(Vec<String>),
}

impl PathParam {
    pub fn get(&self) -> Option<&str> {
        match self {
            PathParam::Opt(Some(param)) => Some(param.as_str()),
            PathParam::Opt(_) => None,
            PathParam::Reqd(param) => Some(param.as_str()),
            PathParam::Vec(_) => panic!("vec param cannot get as str"),
        }
    }

    pub fn get_vec(&self) -> &Vec<String> {
        match self {
            PathParam::Vec(param) => param,
            _ => panic!("non vec type param cannot get as vec"),
        }
    }
}


#[derive(Debug)]
pub struct RequestContext<Raw: RawRequest + 'static> {
    router: Arc<Router<Raw>>,
    api: Option<&'static (dyn ApiTrait<Raw> + Send + Sync)>,
    raw: Raw,
    path_params: HashMap<&'static str, PathParam>,
    suffix: Option<&'static str>,
    data: HashMap<String, Box<dyn Any + Send>>,
}

impl<Raw: RawRequest + Debug + 'static> RequestContext<Raw> {
    pub fn new(router: Arc<Router<Raw>>, raw: Raw) -> RequestContext<Raw> {
        RequestContext { router, api: None, raw, path_params: Default::default(), suffix: None, data: Default::default(), }
    }

    pub fn router(&self) -> &Arc<Router<Raw>> {
        &self.router
    }

    pub fn api(&self) -> DceResult<&'static (dyn ApiTrait<Raw> + Send + Sync)> {
        Ok(self.api.expect("unreachable, get context api shouldn't before set"))
    }

    pub fn raw(&self) -> &Raw {
        &self.raw
    }

    pub fn params(&self) -> &HashMap<&'static str, PathParam> {
        &self.path_params
    }

    pub fn suffix(&mut self) -> &'static str {
        match self.suffix {
            Some(suffix) => suffix,
            None => {
                self.suffix = self.api.unwrap().suffixes().iter().map(|suffix| &**suffix)
                    .find(|s| self.raw.path().ends_with(format!("{}{}", self.router.suffix_boundary(), s).as_str())).or_else(|| Some(""));
                self.suffix()
            },
        }
    }

    pub fn set_routed_info(mut self, api: &'static (dyn ApiTrait<Raw> + Send + Sync), params: HashMap<&'static str, PathParam>, suffix: Option<&'static str>) -> Self {
        self.api = Some(api);
        self.path_params = params;
        self.suffix = suffix;
        self
    }

    pub fn set_data(mut self, data: HashMap<String, Box<dyn Any + Send>>) -> Self {
        self.data = data;
        self
    }

    pub fn put_data(mut self, key: String, value: Box<dyn Any + Send>) -> Self {
        self.data.insert(key, value);
        self
    }
}


#[derive(Debug)]
pub struct Request<Raw, ReqDto, Req, Resp, RespDto>
where Raw: RawRequest + Debug + 'static,
      ReqDto: 'static,
      Req: From<ReqDto> + 'static,
      Resp: Into<RespDto> + 'static,
      RespDto: 'static
{
    api: &'static Api<Raw, ReqDto, Req, Resp, RespDto>,
    context: RequestContext<Raw>,
}

impl<Raw, ReqDto, Req, Resp, RespDto> Request<Raw, ReqDto, Req, Resp, RespDto>
where Raw: RawRequest + Debug + 'static,
      ReqDto: 'static,
      Req: From<ReqDto> + 'static,
      Resp: Into<RespDto> + 'static,
      RespDto: 'static
{
    pub fn api(&self) -> &Api<Raw, ReqDto, Req, Resp, RespDto> {
        self.api
    }

    pub fn context(&self) -> &RequestContext<Raw> {
        &self.context
    }

    pub fn context_mut(&mut self) -> &mut RequestContext<Raw> {
        &mut self.context
    }

    pub fn params(&self) -> &HashMap<&'static str, PathParam> {
        &self.context.path_params
    }

    pub fn param(&self, key: &str) -> DceResult<&PathParam> {
        self.context.path_params.get(key).ok_or(DceErr::openly(0, format!("no param passed with name '{}'", key)))
    }

    pub fn context_data(&self) -> &HashMap<String, Box<dyn Any + Send>> {
        &self.context.data
    }

    pub fn raw(&self) -> &Raw {
        &self.context.raw
    }

    pub fn rpi(&self) -> &Raw::Rpi {
        self.context.raw.raw()
    }

    pub fn rpi_mut(&mut self) -> &mut Raw::Rpi {
        self.context.raw.raw_mut()
    }

    #[cfg(feature = "async")]
    pub async fn req(&mut self) -> DceResult<Req>  {
        let body = self.context.raw.data().await?;
        self.parse(body, self.api.deserializers())
    }

    #[cfg(not(feature = "async"))]
    pub fn req(&mut self) -> DceResult<Req> {
        let body = self.context.raw.data()?;
        self.parse(body, self.api.deserializers())
    }

    fn parse(&self, serialized: Serialized, deserializers: &[Box<dyn Deserializer<ReqDto> + Send + Sync>]) -> DceResult<Req> {
        Ok(Req::from(Raw::deserialize(deserializers, serialized, &self.context)?))
    }

    pub fn status(self, status: bool, data: Option<Resp>, message: Option<String>, code: isize) -> DceResult<Option<Raw::Resp>> {
        let Self{context, api} = self;
        Ok(Some(Raw::pack_responsible::<RespDto>(context, api.serializers(), Serializable::Status(ResponseStatus {
            status,
            code,
            message: message.unwrap_or("".to_string()),
            data: data.map(|resp| resp.into()),
        }))?.unwrap()))
    }

    pub fn success(self, data: Option<Resp>) -> DceResult<Option<Raw::Resp>> {
        self.status(true, data, None, 0)
    }

    pub fn fail(self, message: Option<String>, code: isize) -> DceResult<Option<Raw::Resp>> {
        self.status(false, None, message, code)
    }

    pub fn resp(self, resp: Resp) -> DceResult<Option<Raw::Resp>> {
        let Self{context, api} = self;
        Ok(Some(Raw::pack_responsible::<RespDto>(context, api.serializers(), Serializable::Dto(resp.into()))?.unwrap()))
    }

    pub fn end(self, resp: Option<Resp>) -> DceResult<Option<Raw::Resp>> {
        if let Some(resp) = resp {
            let Self{context , api} = self;
            Raw::pack_responsible::<RespDto>(context, api.serializers(), Serializable::Dto(resp.into()))
        } else {
            Ok(None)
        }
    }

    pub fn pack_resp(self, resp: Serialized) -> DceResult<Option<Raw::Resp>> {
        let Self{context: RequestContext{raw, ..}, ..} = self;
        Ok(Some(raw.pack_response(resp)?))
    }

    pub fn raw_resp(self, resp: Raw::Resp) -> DceResult<Option<Raw::Resp>> {
        Ok(Some(resp))
    }

    // 解析 Api 的 Method 对象与 extras 扩展属性，协议开发者可在协议实现中实现 parse_api_method 方法，并删掉已被解析到 Method 的 prop_tuples 成员，剩下的成员将作为 extras Map成员
    pub fn parse_api_method_and_extras(prop_tuples: Vec<(&'static str, Box<dyn Any + Send + Sync>)>) -> (Option<Box<dyn Method<Raw::Rpi> + Send + Sync>>, HashMap<&'static str, Box<dyn Any + Send + Sync>>) {
        let mut prop_mapping: HashMap<_, _> = prop_tuples.into_iter().collect();
        (Raw::parse_api_method(&mut prop_mapping), prop_mapping)
    }

    pub fn new(api: &'static Api<Raw, ReqDto, Req, Resp, RespDto>, context: RequestContext<Raw>) -> Request<Raw, ReqDto, Req, Resp, RespDto> {
        Request { api, context }
    }
}

pub trait RequestTrait {
    type Raw: RawRequest;
}

impl<Raw, ReqDto, Req, Resp, RespDto> RequestTrait for Request<Raw, ReqDto, Req, Resp, RespDto>
    where Raw: RawRequest + Debug + 'static,
          ReqDto: 'static,
          Req: From<ReqDto> + 'static,
          Resp: Into<RespDto> + 'static,
          RespDto: 'static
{
    type Raw = Raw;
}


#[derive(Debug, Serialize)]
pub struct ResponseStatus<Dto> {
    pub status: bool,
    pub code: isize,
    pub message: String,
    pub data: Option<Dto>,
}


/// Package for raw request data and agent for protocol
#[cfg_attr(feature = "async", async_trait)]
pub trait RawRequest: Sized {
    type Rpi: RoutableProtocol + Debug;
    type Req: 'static;
    type Resp: Debug + 'static;

    fn new(proto: Self::Rpi) -> Self;
    fn path(&self) -> &str;
    fn raw(&self) -> &Self::Rpi;
    fn raw_mut(&mut self) -> &mut Self::Rpi;
    #[cfg(feature = "async")]
    async fn data(&mut self) -> DceResult<Serialized>;
    #[cfg(not(feature = "async"))]
    fn data(&mut self) -> DceResult<Serialized>;

    fn pack_response(self, serialized: Serialized) -> DceResult<Self::Resp>;

    fn pack_responsible<RespDto: 'static>(
        context: RequestContext<Self>,
        serializers: &[Box<dyn Serializer<RespDto> + Send + Sync>],
        responsible: Serializable<RespDto>,
    ) -> DceResult<Option<Self::Resp>> {
        let body = Self::serialize(serializers, responsible, &context)?;
        Ok(Some(Self::pack_response(context.raw, body)?))
    }

    fn api_match<Raw: RawRequest>(raw: &Raw, apis: &[&'static (dyn ApiTrait<Raw> + Send + Sync)]) -> DceResult<&'static (dyn ApiTrait<Raw> + Send + Sync)> {
        Ok(*apis.iter().find(|n| n.method_match(raw)).ok_or(DceErr::closed(CODE_NOT_FOUND, format!(r#"Path "{}" cannot match any Api by Method"#, raw.path())))?)
    }

    // 协议开发者可在协议实现中实现此方法，并删掉已被解析的 prop_tuples 成员
    fn parse_api_method(_prop_mapping: &mut HashMap<&'static str, Box<dyn Any + Send + Sync>>) -> Option<Box<dyn Method<Self::Rpi> + Send + Sync>> {
        None
    }

    #[cfg(feature = "async")]
    async fn route(context: RequestContext<Self>) -> (Option<bool>, DceResult<Option<Self::Resp>>) where Self: Debug + Sized {
        Router::route(context).await
    }

    #[cfg(not(feature = "async"))]
    fn route(context: RequestContext<Self>) -> (Option<bool>, DceResult<Option<Self::Resp>>) where Self: Debug + Sized {
        Router::route(context)
    }

    fn get_input_serializer<'a, ReqDto>(deserializers: &'a [Box<dyn Deserializer<ReqDto> + Send + Sync>], _context: &RequestContext<Self>) -> &'a Box<dyn Deserializer<ReqDto> + Send + Sync> {
        deserializers.first().unwrap()
    }

    fn get_output_serializer<'a, RespDto>(serializers: &'a [Box<dyn Serializer<RespDto> + Send + Sync>], _context: &RequestContext<Self>) -> &'a Box<dyn Serializer<RespDto> + Send + Sync> {
        serializers.last().unwrap()
    }

    fn deserialize<ReqDto>(serializers: &[Box<dyn Deserializer<ReqDto> + Send + Sync>], seq: Serialized, context: &RequestContext<Self>) -> DceResult<ReqDto> {
        Self::get_input_serializer(serializers, context).deserialize(seq)
    }

    fn serialize<RespDto>(serializers: &[Box<dyn Serializer<RespDto> + Send + Sync>], dto: Serializable<RespDto>, context: &RequestContext<Self>) -> DceResult<Serialized> {
        Self::get_output_serializer(serializers, context).serialize(dto)
    }
}
