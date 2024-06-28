use std::any::Any;
use std::collections::HashMap;
use std::env::args;
use std::fmt::Debug;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use dce_router::protocol::{HEAD_PATH_NAME, Meta, RoutableProtocol};
use dce_router::request::{Request, Context, Response};
use dce_router::router::Router;
use dce_router::serializer::Serialized;
use dce_util::mixed::DceResult;
#[cfg(feature = "async")]
use async_trait::async_trait;

pub type CliRaw<'a> = Request<'a, CliProtocol, (), ()>;
pub type CliGet<'a, Resp> = Request<'a, CliProtocol, (), Resp>;

const PASS_SEPARATOR: &str = "--";

enum ArgType {
    AssignExpr(String, String), // arg is an assign expr, 0th is name, 1st is value
    PrefixName, // - prefix name
    DownFlowSeparator, // a separator to separate remain args to down flow cli
    Path, // path type arg
}

#[derive(Debug)]
pub struct CliProtocol {
    meta: Meta<Vec<String>, String>,
    pass: Vec<String>,
    args: HashMap<String, String>,
}

impl CliProtocol {
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
    pub async fn route(self, router: Arc<Router<Self>>, context_data: HashMap<String, Box<dyn Any + Send>>) {
        if let Some(resp) = Self::handle(self, router, context_data).await {
            println!("{resp}");
        }
    }

    #[cfg(not(feature = "async"))]
    pub fn route(self, router: Arc<Router<Self>>, context_data: HashMap<String, Box<dyn Any + Send>>) {
        if let Some(resp) = Self::handle(self, router, context_data) {
            println!("{resp}");
        }
    }

    pub fn new(base: usize) -> Self {
        let raw = args().collect::<Vec<_>>();
        let mut cli = Self::from(raw.iter().skip(base).map(|a| a.clone()).collect::<Vec<_>>());
        *cli.req_mut() = Some(raw);
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
                ArgType::DownFlowSeparator => while ! value.is_empty() {
                    pass.push(value.remove(0));
                },
                _ => paths.push(arg),
            }
        }

        CliProtocol { meta: Meta::new(vec![], HashMap::from([(HEAD_PATH_NAME.to_string(), paths.join("/"))])), pass, args }
    }
}

impl Into<String> for CliProtocol {
    fn into(mut self) -> String {
        #[allow(unused_mut)]
        let mut resp = match self.resp_mut().take() {
            Some(Response::Serialized(sd)) => self.pack_resp(sd),
            Some(Response::Raw(resp)) => resp,
            _ => "".to_string(),
        };
        #[cfg(feature = "session")]
        if let Some(resp_sid) = self.get_resp_sid() {
            resp.push_str(format!("\n\nNew sid: {}", resp_sid).as_str());
        }
        resp
    }
}

impl Deref for CliProtocol {
    type Target = Meta<Vec<String>, String>;

    fn deref(&self) -> &Self::Target {
        &self.meta
    }
}

impl DerefMut for CliProtocol {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.meta
    }
}

#[cfg_attr(feature = "async", async_trait)]
impl RoutableProtocol for CliProtocol {
    type Req = Vec<String>;
    type Resp = String;

    #[cfg(feature = "async")]
    async fn body(&mut self) -> DceResult<Serialized> {
        unreachable!("not support cli body yet")
    }

    #[cfg(not(feature = "async"))]
    fn body(&mut self) -> DceResult<Serialized> {
        unreachable!("not support cli body yet")
    }

    fn pack_resp(&self, serialized: Serialized) -> Self::Resp {
        match serialized {
            Serialized::String(str) => str,
            Serialized::Bytes(bytes) => String::from_utf8_lossy(bytes.as_ref()).to_string(),
        }
    }

    fn handle_result(self, result: DceResult<()>, context: &mut Context<Self>) -> Option<Self::Resp> {
        Self::try_print_err(&result);
        if ! result.is_err() && ! context.api().map_or(false, |a| a.unresponsive()) {
            return Some(self.into());
        }
        None
    }

    #[cfg(feature = "session")]
    fn sid(&self) -> Option<&str> {
        self.args.get("--sid").map(|a| a.as_str())
    }
}
