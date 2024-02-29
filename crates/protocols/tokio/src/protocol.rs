use std::any::Any;
use std::collections::HashMap;
use std::fmt::Write;
use std::net::SocketAddr;
use std::sync::Arc;
use async_trait::async_trait;
use futures_util::SinkExt;
use futures_util::stream::SplitSink;
use tokio::net::TcpStream;
use tokio_util::codec::{BytesCodec, Framed};
use tokio_util::udp::UdpFramed;
use bytes::{BufMut, BytesMut};
use dce_router::router::protocol::{CustomizedProtocolRawRequest, RoutableProtocol};
use dce_router::router::request::{RawRequest, Request, RequestContext};
use dce_router::router::router::Router;
use dce_router::router::serializer::Serialized;
use dce_router::util::DceResult;


pub type SemiTcpRaw = Request<CustomizedProtocolRawRequest<SemiTcpProtocol>, (), (), (), ()>;
pub type SemiTcpGet<Dto> = Request<CustomizedProtocolRawRequest<SemiTcpProtocol>, (), (), Dto, Dto>;
pub type SemiTcpSame<Dto> = Request<CustomizedProtocolRawRequest<SemiTcpProtocol>, Dto, Dto, Dto, Dto>;
pub type SemiTcpNoConvert<Req, Resp> = Request<CustomizedProtocolRawRequest<SemiTcpProtocol>, Req, Req, Resp, Resp>;
pub type SemiTcp<ReqDto, Req, Resp, RespDto> = Request<CustomizedProtocolRawRequest<SemiTcpProtocol>, ReqDto, Req, Resp, RespDto>;


const ID_PATH_SEPARATOR: char = ';';
const HEAD_BODY_SEPARATOR: &str = ">BODY>>>";


#[derive(Debug)]
pub struct SemiTcpProtocol {
    id: Option<String>,
    path: Option<String>,
    body: Option<Serialized>,
}

impl SemiTcpProtocol {
    fn new_resp(&self) -> Self {
        Self {
            id: self.id.clone(),
            path: self.path.clone(),
            body: None,
        }
    }

    pub async fn route(
        self,
        router: Arc<Router<CustomizedProtocolRawRequest<Self>>>,
        stream: &mut SplitSink<Framed<TcpStream, BytesCodec>, BytesMut>,
        context_data: HashMap<String, Box<dyn Any + Send>>,
    ) {
        if let Some(handled) = self.new_resp().handle_result(CustomizedProtocolRawRequest::route(
            RequestContext::new(router, CustomizedProtocolRawRequest::new(self)).set_data(context_data)
        ).await) {
            stream.send(handled).await.unwrap();
        }
    }

    pub async fn udp_route(
        self,
        router: Arc<Router<CustomizedProtocolRawRequest<Self>>>,
        stream: &mut SplitSink<UdpFramed<BytesCodec>, (BytesMut, SocketAddr)>,
        addr: SocketAddr,
        context_data: HashMap<String, Box<dyn Any + Send>>,
    ) {
        if let Some(handled) = self.new_resp().handle_result(CustomizedProtocolRawRequest::route(
            RequestContext::new(router, CustomizedProtocolRawRequest::new(self)).set_data(context_data)
        ).await) {
            stream.send((handled, addr)).await.unwrap();
        }
    }
}

impl From<BytesMut> for SemiTcpProtocol {
    fn from(value: BytesMut) -> Self {
        let mut head = "";
        let body_hold = String::from_utf8(value.to_vec()).unwrap();
        let mut body = body_hold.trim();
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
        }
    }
}

impl Into<BytesMut> for SemiTcpProtocol {
    fn into(self) -> BytesMut {
        let SemiTcpProtocol {id, path, body} = self;
        let body = body.unwrap_or_else(|| Serialized::String("".to_string()));
        let mut text = BytesMut::new();
        if let Some(id) = id {
            text.put_slice(id.as_bytes());
            text.write_char(ID_PATH_SEPARATOR).unwrap();
        }
        if let Some(path) = path { text.put_slice(path.as_bytes()) }
        text.put_slice(format!("\n{}\n", HEAD_BODY_SEPARATOR).as_bytes());
        match body {
            Serialized::String(str) => text.put_slice(str.as_bytes()),
            Serialized::Bytes(bytes) => text.put_slice(bytes.as_ref()),
        };
        text
    }
}

#[async_trait]
impl RoutableProtocol for SemiTcpProtocol {
    type Req = BytesMut;
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
