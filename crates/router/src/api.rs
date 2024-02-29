use std::any::{Any, type_name};
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use crate::serializer::{Deserializer, Serializer};
use crate::request::{RawRequest, Request, RequestContext};
use dce_util::mixed::DceResult;
use std::marker::PhantomData;
#[cfg(feature = "async")]
use std::future::Future;
#[cfg(feature = "async")]
use std::pin::Pin;
#[cfg(feature = "async")]
use async_trait::async_trait;

#[derive(Debug)]
pub struct Api<Raw, ReqDto, Req, Resp, RespDto>
    where Raw: RawRequest + Debug + 'static,
          ReqDto: 'static,
          Req: From<ReqDto> + 'static,
          Resp: Into<RespDto> + 'static,
          RespDto: 'static
{
    controller: Controller<Request<Raw, ReqDto, Req, Resp, RespDto>, Raw::Resp>,
    // 渲染器集，定义节点响应支持的渲染方式，如 apis `GET` 请求当后缀为`.html`时以HTTP渲染器渲染，当后缀为`.xml`时以XML渲染器渲染
    /// Renderer vector, a response could be render by different way,
    /// for example a apis `GET` request could be render to html when url suffix is `.html`, or render to xml when url suffix is `.xml`
    deserializers: Vec<Box<dyn Deserializer<ReqDto> + Send + Sync>>,
    serializers: Vec<Box<dyn Serializer<RespDto> + Send + Sync>>,
    // `method` 用于定义当前节点支持的请求方式，如定义`Http`请求仅支持`["OPTION", "POST"]`
    /// Define supported request methods for current Api, for example define the `Http` request only support `["OPTION", "POST"]` methods
    method: Option<Box<dyn Method<Raw::Rpi> + Send + Sync>>,
    path: &'static str,
    id: &'static str,
    omission: bool,
    redirect: &'static str,
    name: &'static str,
    unresponsive: bool,
    // 扩展属性，可用于定义如校验方式等通用节点配置
    /// Extends properties, can be used to define general api configs such as verification methods
    extras: HashMap<&'static str, Box<dyn Any + Send + Sync>>,
    _marker: PhantomData<Req>,
    _marker2: PhantomData<Resp>,
}

impl<Raw, ReqDto, Req, Resp, RespDto> Api<Raw, ReqDto, Req, Resp, RespDto>
    where Raw: RawRequest + Debug + 'static,
          ReqDto: 'static,
          Req: From<ReqDto> + 'static,
          Resp: Into<RespDto> + 'static,
          RespDto: 'static
{
    pub fn new(
        controller: Controller<Request<Raw, ReqDto, Req, Resp, RespDto>, Raw::Resp>,
        deserializers: Vec<Box<dyn Deserializer<ReqDto> + Send + Sync>>,
        serializers: Vec<Box<dyn Serializer<RespDto> + Send + Sync>>,
        method: Option<Box<dyn Method<Raw::Rpi> + Send + Sync>>,
        path: &'static str,
        id: &'static str,
        omission: bool,
        redirect: &'static str,
        name: &'static str,
        unresponsive: bool,
        extras: HashMap<&'static str, Box<dyn Any + Send + Sync>>,
    ) -> Self {
        Api { controller, deserializers, serializers, method, path, id, omission, redirect, name, unresponsive, extras, _marker: Default::default(), _marker2: Default::default(), }
    }

    pub fn controller(&self) -> &Controller<Request<Raw, ReqDto, Req, Resp, RespDto>, Raw::Resp>{
        &self.controller
    }

    pub fn deserializers(&self) -> &Vec<Box<dyn Deserializer<ReqDto> + Send + Sync>>{
        &self.deserializers
    }

    pub fn serializers(&self) -> &Vec<Box<dyn Serializer<RespDto> + Send + Sync>>{
        &self.serializers
    }
}

#[cfg_attr(feature = "async", async_trait)]
pub trait ApiTrait<Raw: RawRequest> {
    fn method(&self) -> &Option<Box<dyn Method<Raw::Rpi> + Send + Sync>>;
    fn path(&self) -> &'static str;
    fn id(&self) -> &'static str;
    fn omission(&self) -> bool;
    fn redirect(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn unresponsive(&self) -> bool;
    fn extras(&self) -> &HashMap<&'static str, Box<dyn Any + Send + Sync>>;
    fn method_match(&self, raw: &Raw) -> bool;
    #[cfg(feature = "async")]
    async fn call_controller(&'static self, context: RequestContext<Raw>) -> DceResult<Option<Raw::Resp>>;
    #[cfg(not(feature = "async"))]
    fn call_controller(&'static self, context: RequestContext<Raw>) -> DceResult<Option<Raw::Resp>>;
}

#[cfg_attr(feature = "async", async_trait)]
impl<Raw, ReqDto, Req, Resp, RespDto> ApiTrait<Raw> for Api<Raw, ReqDto, Req, Resp, RespDto>
    where Raw: RawRequest + Debug + Send + 'static,
          ReqDto: 'static,
          Req: From<ReqDto> + Debug + Send + Sync + 'static,
          Resp: Into<RespDto> + Send + Sync + 'static,
          RespDto: 'static
{
    fn method(&self) -> &Option<Box<dyn Method<Raw::Rpi> + Send + Sync>> {
        &self.method
    }

    fn path(&self) -> &'static str {
        self.path
    }

    fn id(&self) -> &'static str {
        self.id
    }

    fn omission(&self) -> bool {
        self.omission
    }

    fn redirect(&self) -> &'static str {
        self.redirect
    }

    fn name(&self) -> &'static str {
        self.name
    }

    fn unresponsive(&self) -> bool {
        self.unresponsive
    }

    fn extras(&self) -> &HashMap<&'static str, Box<dyn Any + Send + Sync>> {
        &self.extras
    }

    fn method_match(&self, raw: &Raw) -> bool {
        match &self.method {
            Some(method) => method.req_match(raw.raw()),
            _ => true
        }
    }

    #[cfg(feature = "async")]
    async fn call_controller(&'static self, mut context: RequestContext<Raw>) -> DceResult<Option<Raw::Resp>> {
        if context.router().get_before_controller().is_some() {
            context = match &context.router().clone().get_before_controller() {
                Some(BeforeController::Sync(func)) => func(context)?,
                #[cfg(feature = "async")]
                Some(BeforeController::Async(func)) => func(context).await?,
                _ => context,
            };
        }
        let req = Request::new(self, context);
        match &self.controller {
            Controller::Async(controller) => controller(req).await,
            Controller::Sync(controller) => controller(req),
        }
    }

    #[cfg(not(feature = "async"))]
    fn call_controller(&'static self, mut context: RequestContext<Raw>) -> DceResult<Option<Raw::Resp>> {
        if context.router().get_before_controller().is_some() {
            context = match &context.router().clone().get_before_controller() {
                Some(BeforeController::Sync(func)) => func(context)?,
                _ => context,
            };
        }
        let Controller::Sync(controller) = &self.controller;
        controller(Request::new(self, context))
    }
}

impl<Raw: RawRequest> Debug for dyn ApiTrait<Raw> + Send + Sync + 'static {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(format!(r#"Api {{ path: "{}", omission: {}, redirect: "{}" }}"#, self.path(), self.omission(), self.redirect()).as_str())
    }
}


pub trait Method<Raw> {
    fn to_string(&self) -> String;
    fn req_match(&self, raw: &Raw) -> bool;
}

impl<Raw> Debug for dyn Method<Raw> + Send + Sync + 'static {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.to_string().as_str())
    }
}


pub enum Controller<Req, Ret> {
    Sync(fn(Req) -> DceResult<Option<Ret>>),
    #[cfg(feature = "async")]
    Async(Box<dyn Fn(Req) -> Pin<Box<dyn Future<Output = DceResult<Option<Ret>>> + Send>> + Send + Sync>),
}

impl<Req, Ret> Debug for Controller<Req, Ret> {
    fn fmt(&self, fomatter: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        fomatter.write_str(format!("{} controller", match &self {
            Controller::Sync(_) => "A sync",
            #[cfg(feature = "async")]
            _ => "An async"
        }).as_str())
    }
}


pub enum BeforeController<Raw: RawRequest + 'static> {
    Sync(fn(RequestContext<Raw>) -> DceResult<RequestContext<Raw>>),
    #[cfg(feature = "async")]
    Async(Box<dyn Fn(RequestContext<Raw>) -> Pin<Box<dyn Future<Output = DceResult<RequestContext<Raw>>> + Send>> + Send + Sync>),
}

impl<Raw: RawRequest + 'static> Debug for BeforeController<Raw> {
    fn fmt(&self, fomatter: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        fomatter.write_str(format!("{} function", match &self {
            BeforeController::Sync(_) => "A sync",
            #[cfg(feature = "async")]
            _ => "An async"
        }).as_str())
    }
}


pub trait ToStruct {
    fn from<const N: usize>(value: [(&str, Box<dyn Any>); N]) -> Self;

    fn map_remove_downcast<T: 'static>(map: &mut HashMap<&str, Box<dyn Any + Send + Sync>>, key: &str) -> T {
        map.remove(key).map(|v| *v.downcast::<T>().unwrap_or_else(|_| panic!("'{}' property cannot cast to '{}'", key, type_name::<T>()))).unwrap()
    }
}
