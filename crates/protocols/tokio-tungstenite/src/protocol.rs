use std::any::Any;
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;
use async_trait::async_trait;
use futures_util::SinkExt;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;
use dce_router::protocol::{CustomizedProtocolRawRequest, RoutableProtocol};
use dce_router::request::{RawRequest, Request, RequestContext};
use dce_router::router::Router;
use dce_router::serializer::Serialized;
use dce_util::mixed::DceResult;


pub type SemiWebsocketRaw = Request<CustomizedProtocolRawRequest<SemiWebsocketProtocol>, (), (), (), ()>;
pub type SemiWebsocketGet<Dto> = Request<CustomizedProtocolRawRequest<SemiWebsocketProtocol>, (), (), Dto, Dto>;
pub type SemiWebsocketSame<Dto> = Request<CustomizedProtocolRawRequest<SemiWebsocketProtocol>, Dto, Dto, Dto, Dto>;
pub type SemiWebsocketNoConvert<Req, Resp> = Request<CustomizedProtocolRawRequest<SemiWebsocketProtocol>, Req, Req, Resp, Resp>;
pub type SemiWebsocket<ReqDto, Req, Resp, RespDto> = Request<CustomizedProtocolRawRequest<SemiWebsocketProtocol>, ReqDto, Req, Resp, RespDto>;


const ID_PATH_SEPARATOR: char = ';';
const HEAD_BODY_SEPARATOR: &str = ">BODY>>>";


#[derive(Debug)]
pub struct SemiWebsocketProtocol {
    id: Option<String>,
    path: Option<String>,
    body: Option<Serialized>,
    binary_response: bool,
}

impl SemiWebsocketProtocol {
    pub fn binary(mut self) -> Self {
        self.binary_response = true;
        self
    }

    pub async fn route(
        self,
        router: Arc<Router<CustomizedProtocolRawRequest<Self>>>,
        ws_stream: &mut WebSocketStream<TcpStream>,
        context_data: HashMap<String, Box<dyn Any + Send>>,
    ) {
        let resp = Self {
            id: self.id.clone(),
            path: self.path.clone(),
            body: None,
            binary_response: self.binary_response.clone(),
        };
        if let Some(handled) = resp.handle_result(CustomizedProtocolRawRequest::route(
            RequestContext::new(router, CustomizedProtocolRawRequest::new(self)).set_data(context_data)
        ).await) {
            ws_stream.send(handled).await.unwrap();
        }
    }
}

impl From<Message> for SemiWebsocketProtocol {
    fn from(value: Message) -> Self {
        assert!(value.is_binary() || value.is_text(), "can only convert a text message frame");
        let mut head = "";
        let mut body = value.to_text().unwrap().trim();
        if let Some((tmp_head, tmp_body)) = body.split_once(HEAD_BODY_SEPARATOR) {
            head = tmp_head.trim_end();
            body = tmp_body.trim_start();
        }
        let mut req_id = "";
        let mut path = head.lines().nth(0).unwrap_or("");
        if let Some((tmp_reqid, tmp_path)) = path.split_once(ID_PATH_SEPARATOR) {
            req_id = tmp_reqid.trim_end();
            path = tmp_path.trim_start();
        }
        Self {
            id: if req_id.is_empty() { None } else { Some(req_id.to_string()) },
            path: if path.is_empty() { None } else { Some(path.to_string()) },
            body: if body.is_empty() { None } else { Some(Serialized::String(body.to_string())) },
            binary_response: false,
        }
    }
}

impl Into<Message> for SemiWebsocketProtocol {
    fn into(self) -> Message {
        let SemiWebsocketProtocol{id, path, body, binary_response} = self;
        let body = body.unwrap_or_else(|| Serialized::String("".to_string()));
        if binary_response {
            let mut binary = vec![];
            if let Some(mut id) = id {
                unsafe { binary.append(id.as_mut_vec()); }
                binary.push(ID_PATH_SEPARATOR as u8);
            }
            if let Some(mut path) = path { unsafe { binary.append(path.as_mut_vec()) } }
            binary.append(unsafe { format!("\n{}\n", HEAD_BODY_SEPARATOR).as_mut_vec() });
            binary.append(&mut match body {
                Serialized::Bytes(bts) => bts.to_vec(),
                Serialized::String(str) => str.into_bytes(),
            });
            Message::Binary(binary)
        } else {
            let mut text = "".to_string();
            if let Some(id) = id {
                text.push_str(id.as_str());
                text.push(ID_PATH_SEPARATOR);
            }
            if let Some(path) = path { text.push_str(path.as_str()) }
            text.push_str(format!("\n{}\n", HEAD_BODY_SEPARATOR).as_str());
            text.push_str(match body {
                Serialized::Bytes(bts) => String::from_utf8_lossy(bts.deref()).to_string(),
                Serialized::String(str) => str,
            }.as_str());
            Message::Text(text)
        }
    }
}

#[async_trait]
impl RoutableProtocol for SemiWebsocketProtocol {
    type Req = Message;
    type Resp = Self::Req;

    fn path(&self) -> &str {
        match &self.path {
            Some(path) => path,
            None => "",
        }
    }

    async fn body(&mut self) -> DceResult<Serialized> {
        assert!(matches!(self.body, Some(_)), "body can only take once");
        let Some(body) = self.body.take() else { unreachable!() };
        Ok(body)
    }

    fn pack_resp(mut self, serialized: Serialized) -> Self::Resp {
        self.body = Some(serialized);
        self.into()
    }

    fn id(&self) -> Option<&str> {
        self.id.as_ref().map_or(None, |id| Some(id))
    }
}
