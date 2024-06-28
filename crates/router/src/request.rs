use std::any::{Any, type_name};
use std::collections::HashMap;
use std::fmt::Debug;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use crate::api::{Api, ApiTrait};
use crate::serializer::{Deserializer, Serializable, Serialized};
use crate::router::Router;
use crate::protocol::RoutableProtocol;
use dce_util::mixed::{DceErr, DceResult};
use serde::Serialize;


#[derive(Debug)]
pub struct Context<Rp: RoutableProtocol + 'static> {
    router: Arc<Router<Rp>>,
    api: Option<&'static (dyn ApiTrait<Rp> + Send + Sync)>,
    rp: Option<Rp>,
    path_params: HashMap<&'static str, PathParam>,
    suffix: Option<&'static str>,
    data: HashMap<String, Box<dyn Any + Send>>,
}

impl<Rp: RoutableProtocol + Debug + 'static> Context<Rp> {
    pub fn new(router: Arc<Router<Rp>>, rp: Rp, data: HashMap<String, Box<dyn Any + Send>>) -> Context<Rp> {
        Context { router, api: None, rp: Some(rp), path_params: Default::default(), suffix: None, data, }
    }

    pub fn router(&self) -> &Arc<Router<Rp>> {
        &self.router
    }

    pub fn api(&self) -> Option<&'static (dyn ApiTrait<Rp> + Send + Sync)> {
        self.api
    }

    pub fn rp(&self) -> &Rp {
        self.rp.as_ref().expect("Routable protocol data has been taken, should not borrow it anymore")
    }

    pub fn rp_mut(&mut self) -> &mut Rp {
        self.rp.as_mut().expect("Routable protocol data has been taken, should not borrow mut anymore")
    }
    
    pub fn take_rp(&mut self) -> Option<Rp> {
        self.rp.take()
    }

    pub fn params(&self) -> &HashMap<&'static str, PathParam> {
        &self.path_params
    }

    pub fn param(&self, key: &str) -> DceResult<&PathParam> {
        self.path_params.get(key).ok_or(DceErr::openly0(format!("no param passed with name '{}'", key)))
    }

    pub fn suffix(&mut self) -> &'static str {
        match self.suffix {
            Some(suffix) => suffix,
            None => {
                self.suffix = self.api.iter().find_map(|a| a.suffixes().iter().map(|suffix| &**suffix)
                    .find(|s| self.rp().path().ends_with(format!("{}{}", self.router.suffix_boundary(), s).as_str()))).or_else(|| Some(""));
                self.suffix()
            },
        }
    }

    pub fn data(&self) -> &HashMap<String, Box<dyn Any + Send>> {
        &self.data
    }

    pub fn put_data(&mut self, key: String, value: Box<dyn Any + Send>) {
        self.data.insert(key, value);
    }

    pub fn get_as<S: 'static>(&self, key: &str) -> DceResult<&S> {
        let type_name = type_name::<S>();
        self.data.get(key).ok_or_else(|| DceErr::closed0(format!("{} has not bound yet", type_name)))?
            .downcast_ref().ok_or_else(|| DceErr::closed0(format!("Box cannot downcast to {} ref", type_name)))
    }

    pub fn get_as_mut<S: 'static>(&mut self, key: &str) -> DceResult<&mut S> {
        let type_name = type_name::<S>();
        self.data.get_mut(key).ok_or_else(|| DceErr::closed0(format!("{} has not bound yet", type_name)))?
            .downcast_mut().ok_or_else(|| DceErr::closed0(format!("Box cannot downcast to {} ref", type_name)))
    }

    pub fn set_routed_info(&mut self, api: &'static (dyn ApiTrait<Rp> + Send + Sync), params: HashMap<&'static str, PathParam>, suffix: Option<&'static str>) {
        self.api = Some(api);
        self.path_params = params;
        self.suffix = suffix;
    }
}

#[derive(Debug)]
pub enum PathParam {
    Option(Option<String>),
    Required(String),
    Vector(Vec<String>),
}

impl PathParam {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            PathParam::Option(Some(param)) => Some(param.as_str()),
            PathParam::Option(_) => None,
            PathParam::Required(param) => Some(param.as_str()),
            PathParam::Vector(_) => panic!("Vec param cannot get as str"),
        }
    }

    pub fn as_vec(&self) -> &Vec<String> {
        match self {
            PathParam::Vector(param) => param,
            _ => panic!("Non vec type param cannot get as vec"),
        }
    }
}



#[derive(Debug)]
pub struct Request<'a, Rp, ReqDto, RespDto>
where Rp: RoutableProtocol + Send + Sync + Debug + 'static,
      ReqDto: 'static,
      RespDto: 'static
{
    api: &'static Api<Rp, ReqDto, RespDto>,
    context: &'a mut Context<Rp>,
}

impl<'a, Rp, ReqDto, RespDto> Request<'a, Rp, ReqDto, RespDto>
where Rp: RoutableProtocol + Send + Sync + Debug + 'static,
{
    #[cfg(feature = "async")]
    pub async fn req<Req: From<ReqDto>>(&mut self) -> DceResult<Req>  {
        let body = self.context.rp_mut().body().await?;
        self.parse(body, self.api.deserializers()).map(Req::from)
    }

    #[cfg(not(feature = "async"))]
    pub fn req<Req: From<ReqDto>>(&mut self) -> DceResult<Req> {
        let body = self.context.rp_mut().body()?;
        self.parse(body, self.api.deserializers()).map(Req::from)
    }

    #[cfg(feature = "async")]
    pub async fn dto(&mut self) -> DceResult<ReqDto>  {
        let body = self.context.rp_mut().body().await?;
        self.parse(body, self.api.deserializers())
    }

    #[cfg(not(feature = "async"))]
    pub fn dto(&mut self) -> DceResult<ReqDto> {
        let body = self.context.rp_mut().body()?;
        self.parse(body, self.api.deserializers())
    }

    fn parse(&self, serialized: Serialized, deserializers: &[Box<dyn Deserializer<ReqDto> + Send + Sync>]) -> DceResult<ReqDto> {
        Rp::deserialize(deserializers, serialized, &self.context)
    }

    pub fn status<Resp: Into<RespDto>>(self, status: bool, data: Option<Resp>, message: Option<String>, code: isize) -> DceResult<Option<Response<Rp::Resp>>> {
        let Self{context, api} = self;
        Rp::pack_responsible::<RespDto>(context, api.serializers(), Serializable::Status(ResponseStatus {
            status,
            code,
            message: message.unwrap_or("".to_string()),
            data: data.map(|resp| resp.into()),
        }))
    }

    pub fn success(self, data: Option<RespDto>) -> DceResult<Option<Response<Rp::Resp>>> {
        self.status(true, data, None, 0)
    }

    pub fn fail(self, message: Option<String>, code: isize) -> DceResult<Option<Response<Rp::Resp>>> {
        self.status::<RespDto>(false, None, message, code)
    }

    pub fn resp<Resp: Into<RespDto>>(self, resp: Resp) -> DceResult<Option<Response<Rp::Resp>>> {
        let Self{context, api} = self;
        Rp::pack_responsible(context, api.serializers(), Serializable::Dto(resp.into()))
    }

    pub fn end(self, resp: Option<RespDto>) -> DceResult<Option<Response<Rp::Resp>>> {
        if let Some(resp) = resp {
            let Self{context , api} = self;
            Rp::pack_responsible::<RespDto>(context, api.serializers(), Serializable::Dto(resp.into()))
        } else {
            Ok(None)
        }
    }

    pub fn pack(self, serialized: Serialized) -> DceResult<Option<Response<Rp::Resp>>> {
        Ok(Some(Response::Serialized(serialized)))
    }

    pub fn raw_resp(self, resp: Rp::Resp) -> DceResult<Option<Response<Rp::Resp>>> {
        Ok(Some(Response::Raw(resp)))
    }
    
    pub fn new(api: &'static Api<Rp, ReqDto, RespDto>, context: &'a mut Context<Rp>) -> Request<'a, Rp, ReqDto, RespDto> {
        Request { api, context }
    }
}

impl<Rp, ReqDto, RespDto> Deref for Request<'_, Rp, ReqDto, RespDto>
    where Rp: RoutableProtocol + Send + Sync + Debug + 'static,  {
    type Target = Context<Rp>;

    fn deref(&self) -> &Self::Target {
        &self.context
    }
}

impl<Rp, ReqDto, RespDto> DerefMut for Request<'_, Rp, ReqDto, RespDto>
    where Rp: RoutableProtocol + Send + Sync + Debug + 'static, {

    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.context
    }
}

pub trait RequestTrait {
    type Rp: RoutableProtocol;
    type ReqDto;
    type RespDto;
}

impl<Rp, ReqDto, RespDto> RequestTrait for Request<'_, Rp, ReqDto, RespDto>
    where Rp: RoutableProtocol + Send + Sync + Debug + 'static, {
    type Rp = Rp;
    type ReqDto = ReqDto;
    type RespDto = RespDto;
}


#[derive(Debug)]
pub enum Response<Resp> {
    Serialized(Serialized),
    Raw(Resp),
}


#[derive(Debug, Serialize)]
pub struct ResponseStatus<Dto> {
    pub status: bool,
    pub code: isize,
    pub message: String,
    pub data: Option<Dto>,
}
