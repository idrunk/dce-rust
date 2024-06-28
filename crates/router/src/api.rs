use std::any::Any;
use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};
use std::fmt::{Debug, Formatter};
use crate::serializer::{Deserializer, Serializer};
use crate::request::{Request, Context, Response};
use dce_util::mixed::DceResult;
#[cfg(feature = "async")]
use std::future::Future;
use std::ops::Deref;
#[cfg(feature = "async")]
use std::pin::Pin;
#[cfg(feature = "async")]
use async_trait::async_trait;
use crate::protocol::RoutableProtocol;
use crate::router::{PATH_PART_SEPARATOR, SUFFIX_BOUNDARY};


const SUFFIX_SEPARATOR: char = '|';


#[derive(Debug)]
pub struct Api<Rp, ReqDto, RespDto>
    where Rp: RoutableProtocol + Send + Sync + Debug + 'static,
          ReqDto: 'static,
          RespDto: 'static
{
    controller: Controller<Rp, ReqDto, RespDto>,
    // 渲染器集，定义节点响应支持的渲染方式，如 apis `GET` 请求当后缀为`.html`时以HTTP渲染器渲染，当后缀为`.xml`时以XML渲染器渲染
    /// Renderer vector, a response could to render by different way,
    /// for example an `GET` request could render to html when url suffix is `.html`, or render to xml when url suffix is `.xml`
    deserializers: Vec<Box<dyn Deserializer<ReqDto> + Send + Sync>>,
    serializers: Vec<Box<dyn Serializer<RespDto> + Send + Sync>>,
    // `method` 用于定义当前节点支持的请求方式，如定义`Http`请求仅支持`["OPTION", "POST"]`
    /// Define supported request methods for current Api, for example define the `Http` request only support `["OPTION", "POST"]` methods
    method: Option<Box<dyn Method<Rp> + Send + Sync>>,
    path: &'static str,
    suffixes: BTreeSet<Suffix>,
    id: &'static str,
    omission: bool,
    redirect: &'static str,
    name: &'static str,
    unresponsive: bool,
    // 扩展属性，可用于定义如校验方式等通用节点配置
    /// Extends properties, can be used to define general api configs such as verification methods
    extras: HashMap<&'static str, Box<dyn Any + Send + Sync>>,
}

impl<Rp, ReqDto, RespDto> Api<Rp, ReqDto, RespDto>
    where Rp: RoutableProtocol + Debug + Send + Sync + 'static,
          ReqDto: 'static,
          RespDto: 'static
{
    pub fn new(
        controller: Controller<Rp, ReqDto, RespDto>,
        deserializers: Vec<Box<dyn Deserializer<ReqDto> + Send + Sync>>,
        serializers: Vec<Box<dyn Serializer<RespDto> + Send + Sync>>,
        method: Option<Box<dyn Method<Rp> + Send + Sync>>,
        path: &'static str,
        id: &'static str,
        omission: bool,
        redirect: &'static str,
        name: &'static str,
        unresponsive: bool,
        extras: HashMap<&'static str, Box<dyn Any + Send + Sync>>,
    ) -> Self {
        let mut path = path.trim_matches(PATH_PART_SEPARATOR);
        let mut suffixes = BTreeSet::from([Suffix("")]);
        if let Some(last_part_from) = path.rfind(PATH_PART_SEPARATOR).map_or_else(|| Some(0), |i| Some(i + 1)) {
            let last_part = &path[last_part_from..];
            if let Some(bound_index) = last_part.find(SUFFIX_BOUNDARY) {
                suffixes = last_part[bound_index + 1 ..].split(SUFFIX_SEPARATOR).map(Suffix).collect();
                path = &path[0.. last_part_from + bound_index];
            }
        }
        Api { controller, deserializers, serializers, method, path, suffixes, id, omission, redirect, name, unresponsive, extras, }
    }

    pub fn controller(&self) -> &Controller<Rp, ReqDto, RespDto> {
        &self.controller
    }

    pub fn deserializers(&self) -> &Vec<Box<dyn Deserializer<ReqDto> + Send + Sync>> {
        &self.deserializers
    }

    pub fn serializers(&self) -> &Vec<Box<dyn Serializer<RespDto> + Send + Sync>> {
        &self.serializers
    }
}

#[cfg_attr(feature = "async", async_trait)]
pub trait ApiTrait<Rp: RoutableProtocol> {
    fn method(&self) -> &Option<Box<dyn Method<Rp> + Send + Sync>>;
    fn path(&self) -> &'static str;
    fn suffixes(&self) -> &BTreeSet<Suffix>;
    fn id(&self) -> &'static str;
    fn omission(&self) -> bool;
    fn redirect(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn unresponsive(&self) -> bool;
    fn extras(&self) -> &HashMap<&'static str, Box<dyn Any + Send + Sync>>;
    fn method_match(&self, rp: &Rp) -> bool;
    #[cfg(feature = "async")]
    async fn call_controller<'a>(&'static self, context: &'a mut Context<Rp>) -> DceResult<()>;
    #[cfg(not(feature = "async"))]
    fn call_controller<'a>(&'static self, context: &'a mut Context<Rp>) -> DceResult<()>;
}

#[cfg_attr(feature = "async", async_trait)]
impl<Rp, ReqDto, RespDto> ApiTrait<Rp> for Api<Rp, ReqDto, RespDto>
    where Rp: RoutableProtocol + Send + Sync + Debug + 'static,
          ReqDto: 'static,
          RespDto: 'static
{
    fn method(&self) -> &Option<Box<dyn Method<Rp> + Send + Sync>> {
        &self.method
    }

    fn path(&self) -> &'static str {
        self.path
    }

    fn suffixes(&self) -> &BTreeSet<Suffix> {
        &self.suffixes
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

    fn method_match(&self, rp: &Rp) -> bool {
        match &self.method {
            Some(method) => method.req_match(rp),
            _ => true
        }
    }

    #[cfg(feature = "async")]
    async fn call_controller<'a>(&'static self, context: &'a mut Context<Rp>) -> DceResult<()> {
        if context.router().before_controller().is_some() {
            match context.router().clone().before_controller() {
                Some(EventHandler::Sync(func)) => func(context)?,
                Some(EventHandler::Async(func)) => func(context).await?,
                _ => {},
            };
        }
        let req = Request::new(self, context);
        *context.rp_mut().resp_mut() = match &self.controller {
            Controller::Async(controller) => controller(req).await,
            Controller::Sync(controller) => controller(req),
        }?;
        if context.router().after_controller().is_some() {
            match context.router().clone().after_controller() {
                Some(EventHandler::Sync(func)) => func(context)?,
                Some(EventHandler::Async(func)) => func(context).await?,
                _ => {},
            };
        }
        Ok(())
    }

    #[cfg(not(feature = "async"))]
    fn call_controller<'a>(&'static self, context: &'a mut Context<Rp>) -> DceResult<()> {
        if context.router().before_controller().is_some() {
            if let Some(EventHandler::Sync(func)) = context.router().clone().before_controller() { func(context)?; }
        }
        let req = Request::new(self, context);
        let Controller::Sync(controller) = &self.controller;
        *context.rp_mut().resp_mut() = controller(req)?;
        if context.router().after_controller().is_some() {
            if let Some(crate::api::EventHandler::Sync(func)) = context.router().clone().after_controller() { func(context)?; }
        }
        Ok(())
    }
}

impl<Rp: RoutableProtocol> Debug for dyn ApiTrait<Rp> + Send + Sync + 'static {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(format!(r#"Api{{method: {:?}, path: "{}", suffixes: {:?}, id: "{}", omission: {}, redirect: "{}", name: "{}", unresponsive: {}, extras: {:?}}}"#,
                            self.method(), self.path(), self.suffixes(), self.id(), self.omission(), self.redirect(), self.name(), self.unresponsive(), self.extras()).as_str())
    }
}


#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Suffix(&'static str);

impl Deref for Suffix {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl PartialOrd<Self> for Suffix {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Suffix {
    fn cmp(&self, other: &Self) -> Ordering {
        let compound_diff = self.0.chars().filter(|c| SUFFIX_BOUNDARY.eq_ignore_ascii_case(c)).count() as isize
            - other.0.chars().filter(|c| SUFFIX_BOUNDARY.eq_ignore_ascii_case(c)).count() as isize;
        // put complex suffix front, and simple back
        if compound_diff > 0 {
            return Ordering::Less;
        } else if compound_diff < 0 {
            return Ordering::Greater;
        }
        self.0.cmp(other.0)
    }
}


pub trait Method<Rp> {
    fn to_string(&self) -> String;
    fn req_match(&self, raw: &Rp) -> bool;
}

impl<Rp> Debug for dyn Method<Rp> + Send + Sync + 'static {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.to_string().as_str())
    }
}


pub enum Controller<Rp, ReqDto, RespDto>
    where Rp: RoutableProtocol + Debug + Send + Sync + 'static,
          ReqDto: 'static,
          RespDto: 'static {
    Sync(fn(Request<'_, Rp, ReqDto, RespDto>) -> DceResult<Option<Response<Rp::Resp>>>),
    #[cfg(feature = "async")]
    Async(Box<dyn Fn(Request<'_, Rp, ReqDto, RespDto>) -> Pin<Box<dyn Future<Output = DceResult<Option<Response<Rp::Resp>>>> + Send + '_>> + Send + Sync>),
}

impl<Rp, ReqDto, RespDto> Debug for Controller<Rp, ReqDto, RespDto>
    where Rp: RoutableProtocol + Debug + Send + Sync + 'static,
          ReqDto: 'static,
          RespDto: 'static {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        formatter.write_str(format!("{} controller", match &self {
            Controller::Sync(_) => "A sync",
            #[cfg(feature = "async")]
            _ => "An async"
        }).as_str())
    }
}


pub enum EventHandler<Rp: RoutableProtocol + 'static> {
    Sync(fn(&mut Context<Rp>) -> DceResult<()>),
    #[cfg(feature = "async")]
    Async(Box<dyn for <'a> Fn(&'a mut Context<Rp>) -> Pin<Box<dyn Future<Output = DceResult<()>> + Send + 'a>> + Send + Sync>),
}

impl<Rp: RoutableProtocol + 'static> Debug for EventHandler<Rp> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        formatter.write_str(format!("{} function", match &self {
            EventHandler::Sync(_) => "A sync",
            #[cfg(feature = "async")]
            _ => "An async"
        }).as_str())
    }
}
