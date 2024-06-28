use std::any::Any;
use std::collections::HashMap;
use std::fmt::Debug;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use async_trait::async_trait;
use futures_util::SinkExt;
use log::error;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;
use dce_router::protocol::{HEAD_ID_NAME, HEAD_PATH_NAME, Meta, RoutableProtocol};
use dce_router::request::{Request, Response};
use dce_router::router::Router;
use dce_router::serializer::Serialized;
use dce_util::mixed::{DceErr, DceResult};

pub type SemiWebsocketRaw<'a> = Request<'a, SemiWebsocketProtocol, (), ()>;
pub type SemiWebsocketGet<'a, Dto> = Request<'a, SemiWebsocketProtocol, (), Dto>;
pub type SemiWebsocketSame<'a, Dto> = Request<'a, SemiWebsocketProtocol, Dto, Dto>;
pub type SemiWebsocket<'a, ReqDto, RespDto> = Request<'a, SemiWebsocketProtocol, ReqDto, RespDto>;


const ID_PATH_SEPARATOR: char = ';';
const HEAD_BODY_SEPARATOR: &str = ">BODY>>>";


#[derive(Debug)]
pub struct SemiWebsocketProtocol {
    meta: Meta<Message, Message>,
    body_index: usize,
    binary_response: bool,
}

impl SemiWebsocketProtocol {
    pub fn binary(mut self) -> Self {
        self.binary_response = true;
        self
    }

    pub async fn route(
        self,
        router: Arc<Router<Self>>,
        ws_stream: &mut WebSocketStream<TcpStream>,
        context_data: HashMap<String, Box<dyn Any + Send>>,
    ) {
        if let Some(handled) = Self::handle(self, router, context_data).await {
            let _ = ws_stream.send(handled).await.map_err(|e| error!("{e}"));
        }
    }
}

impl From<Message> for SemiWebsocketProtocol {
    fn from(value: Message) -> Self {
        let mut body_index = 0;
        let mut heads = HashMap::new();
        let mut path = value.to_text().map_or("", &str::trim).to_string();
        if let Some(index) = path.find(HEAD_BODY_SEPARATOR) {
            let mut head_lines: Vec<_> = path[0..index].lines().map(ToString::to_string).collect();
            path = head_lines.remove(0);
            if let Some((tmp_id, tmp_path)) = path.split_once(ID_PATH_SEPARATOR) {
                heads.insert(HEAD_ID_NAME.to_string(), tmp_id.to_string());
                path = tmp_path.to_string();
            }
            heads.extend(head_lines.iter().map(|line| line.split_once(':')
                .map_or_else(|| (line.to_string(), "".to_string()), |(k, v)| (k.to_string(), v.to_string()))));
            body_index = index + HEAD_BODY_SEPARATOR.len();
        }
        heads.insert(HEAD_PATH_NAME.to_string(), path);
        Self { meta: Meta::new(value, heads), body_index, binary_response: false, }
    }
}

impl Into<Message> for SemiWebsocketProtocol {
    fn into(mut self) -> Message {
        let resp = self.resp_mut().take();
        let (id, path) = (self.id(), self.path());
        match resp {
            Some(Response::Raw(resp)) => resp,
            resp => {
                if self.binary_response {
                    let mut binary = vec![];
                    if let Some(id) = id {
                        binary.extend(id.as_bytes());
                        binary.push(ID_PATH_SEPARATOR as u8);
                    }
                    binary.extend(path.as_bytes());
                    for (k, v) in self.resp_heads() {
                        binary.extend(format!("\n{k}:{v}").as_bytes());
                    }
                    binary.extend(format!("\n{}\n", HEAD_BODY_SEPARATOR).as_bytes());
                    if let Some(Response::Serialized(sd)) = resp {
                        binary.extend(match sd {
                            Serialized::Bytes(bts) => bts.to_vec(),
                            Serialized::String(str) => str.into_bytes(),
                        });
                    }
                    Message::Binary(binary)
                } else {
                    let mut text = "".to_string();
                    if let Some(id) = id {
                        text.push_str(id);
                        text.push(ID_PATH_SEPARATOR);
                    }
                    text.push_str(path);
                    for (k, v) in self.resp_heads() {
                        text.push_str(format!("\n{k}:{v}").as_str());
                    }
                    text.push_str(format!("\n{}\n", HEAD_BODY_SEPARATOR).as_str());
                    if let Some(Response::Serialized(sd)) = resp {
                        text.push_str(match sd {
                            Serialized::Bytes(bts) => String::from_utf8_lossy(bts.deref()).to_string(),
                            Serialized::String(str) => str,
                        }.as_str());
                    }
                    Message::Text(text)
                }
            }
        }
    }
}

impl Deref for SemiWebsocketProtocol {
    type Target = Meta<Message, Message>;

    fn deref(&self) -> &Self::Target {
        &self.meta
    }
}

impl DerefMut for SemiWebsocketProtocol {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.meta
    }
}

#[async_trait]
impl RoutableProtocol for SemiWebsocketProtocol {
    type Req = Message;
    type Resp = Self::Req;

    async fn body(&mut self) -> DceResult<Serialized> {
        self.req_mut().take().ok_or_else(|| DceErr::closed0("Empty request"))?.to_text()
            .map(|t| Serialized::String(t[self.body_index ..].to_string())).map_err(DceErr::closed0)
    }

    fn pack_resp(&self, serialized: Serialized) -> Self::Resp {
        Message::Text(match serialized {
            Serialized::Bytes(bts) => String::from_utf8_lossy(bts.deref()).to_string(),
            Serialized::String(str) => str,
        })
    }
}
