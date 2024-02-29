use std::any::Any;
use std::collections::HashMap;
use std::env::args;
use std::sync::Arc;
use dce_router::protocol::{CustomizedProtocolRawRequest, RoutableProtocol};
use dce_router::request::{RawRequest, Request, RequestContext};
use dce_router::router::Router;
use dce_router::serializer::Serialized;
use dce_util::mixed::DceResult;
#[cfg(feature = "async")]
use async_trait::async_trait;


pub type CliRaw = Request<CustomizedProtocolRawRequest<CliProtocol>, (), (), (), ()>;
pub type CliRequest<Resp> = Request<CustomizedProtocolRawRequest<CliProtocol>, (), (), Resp, Resp>;
pub type CliConvert<Resp, RespDto> = Request<CustomizedProtocolRawRequest<CliProtocol>, (), (), Resp, RespDto>;

const PASS_SEPARATOR: &str = "--";

enum ArgType {
    AssignExpr(String, String), // arg is an assign expr, 0th is name, 1st is value
    PrefixName, // - prefix name
    DownFlowSeparator, // a separator to separate remain args to down flow cli
    Path, // path type arg
}

#[derive(Debug, Default)]
pub struct CliProtocol {
    raw: Vec<String>,
    path: String,
    pass: Vec<String>,
    args: HashMap<String, String>,
}

impl CliProtocol {
    pub fn raw(&self) -> &Vec<String> {
        &self.raw
    }

    pub fn pass(&self) -> &Vec<String> {
        &self.pass
    }

    pub fn args(&self) -> &HashMap<String, String> {
        &self.args
    }

    pub fn args_mut(&mut self) -> &mut HashMap<String, String> {
        &mut self.args
    }

    #[cfg(feature = "async")]
    pub async fn route(self, router: Arc<Router<CustomizedProtocolRawRequest<Self>>>, context_data: HashMap<String, Box<dyn Any + Send>>) {
        if let Some(resp) = CliProtocol::default().handle_result(CustomizedProtocolRawRequest::route(
            RequestContext::new(router, CustomizedProtocolRawRequest::new(self)).set_data(context_data)
        ).await) {
            println!("{resp}");
        }
    }

    #[cfg(not(feature = "async"))]
    pub fn route(self, router: Arc<Router<CustomizedProtocolRawRequest<Self>>>, context_data: HashMap<String, Box<dyn Any + Send>>) {
        if let Some(resp) = CliProtocol::default().handle_result(CustomizedProtocolRawRequest::route(
            RequestContext::new(router, CustomizedProtocolRawRequest::new(self)).set_data(context_data)
        )) {
            println!("{resp}");
        }
    }

    pub fn new(base: usize) -> Self {
        let raw = args().collect::<Vec<_>>();
        let mut cli = Self::from(raw.iter().skip(base).map(|a| a.clone()).collect::<Vec<_>>());
        cli.raw = raw;
        cli
    }

    fn parse_type(arg: &str) -> ArgType {
        return if let Some((left, right)) = arg.split_once("=") {
            ArgType::AssignExpr(left.to_string(), right.to_string())
        } else if arg.starts_with("-") {
            if arg == PASS_SEPARATOR { return ArgType::DownFlowSeparator }
            ArgType::PrefixName
        } else {
            ArgType::Path
        }
    }
}

impl From<Vec<String>> for CliProtocol {
    fn from(mut value: Vec<String>) -> Self {
        let mut pass = vec![];
        let mut paths = vec![];
        let mut args = HashMap::<String, String>::new();

        while ! value.is_empty() {
            let arg = value.remove(0);
            match Self::parse_type(&arg) {
                ArgType::AssignExpr(name, value) => { args.insert(name, value); },
                ArgType::PrefixName => {
                    args.insert(arg, match value.get(0) {
                        Some(next) if matches!(Self::parse_type(next), ArgType::Path) => value.remove(0),
                        _ => String::new(),
                    });
                },
                ArgType::DownFlowSeparator => {
                    while ! value.is_empty() { pass.push(value.remove(0)); }
                },
                _ => { paths.push(arg); },
            }
        }

        CliProtocol { raw: vec![], path: paths.join("/"), pass, args }
    }
}

impl Into<String> for CliProtocol {
    fn into(self) -> String {
        unreachable!()
    }
}

#[cfg_attr(feature = "async", async_trait)]
impl RoutableProtocol for CliProtocol {
    type Req = Vec<String>;
    type Resp = String;

    fn path(&self) -> &str {
        &self.path
    }

    #[cfg(feature = "async")]
    async fn body(&mut self) -> DceResult<Serialized> {
        unreachable!("not support cli body yet")
    }

    #[cfg(not(feature = "async"))]
    fn body(&mut self) -> DceResult<Serialized> {
        unreachable!("not support cli body yet")
    }

    fn pack_resp(self, serialized: Serialized) -> Self::Resp {
        match serialized {
            Serialized::String(str) => str,
            Serialized::Bytes(bytes) => String::from_utf8_lossy(bytes.as_ref()).to_string(),
        }
    }

    fn handle_result(self, (unresponsive, response): (Option<bool>, DceResult<Option<Self::Resp>>)) -> Option<String>{
        Self::try_print_err(&response);
        if let Ok(Some(resp)) = if unresponsive.unwrap_or(true) { Ok(None) } else { response } {
            return Some(resp);
        }
        None
    }
}
