use std::any::Any;
use std::collections::HashMap;
use std::fmt::Debug;
use std::net::SocketAddr;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use async_trait::async_trait;
use futures_util::SinkExt;
use futures_util::stream::SplitSink;
use tokio::net::TcpStream;
use tokio_util::codec::{BytesCodec, Framed};
use tokio_util::udp::UdpFramed;
use bytes::{BufMut, BytesMut};
use log::{error, warn};
use dce_router::protocol::{HEAD_ID_NAME, HEAD_PATH_NAME, Meta, RoutableProtocol};
use dce_router::request::{Request, Response};
use dce_router::router::Router;
use dce_router::serializer::Serialized;
use dce_util::mixed::{DceErr, DceResult};


pub type SemiTcpRaw<'a> = Request<'a, SemiTcpProtocol, (), ()>;
pub type SemiTcpGet<'a, Dto> = Request<'a, SemiTcpProtocol, (), Dto>;
pub type SemiTcpSame<'a, Dto> = Request<'a, SemiTcpProtocol, Dto, Dto>;
pub type SemiTcp<'a, ReqDto, RespDto> = Request<'a, SemiTcpProtocol, ReqDto, RespDto>;


const ID_PATH_SEPARATOR: char = ';';
const HEAD_BODY_SEPARATOR: &str = ">BODY>>>";


#[derive(Debug)]
pub struct SemiTcpProtocol {
    meta: Meta<BytesMut, BytesMut>,
    body_index: usize,
}

impl SemiTcpProtocol {
    pub async fn route(
        self,
        router: Arc<Router<Self>>,
        stream: &mut SplitSink<Framed<TcpStream, BytesCodec>, BytesMut>,
        context_data: HashMap<String, Box<dyn Any + Send>>,
    ) {
        if let Some(handled) = Self::handle(self, router, context_data).await {
            let _ = stream.send(handled).await.map_err(|e| error!("{e}"));
        }
    }

    pub async fn udp_route(
        self,
        router: Arc<Router<Self>>,
        stream: &mut SplitSink<UdpFramed<BytesCodec>, (BytesMut, SocketAddr)>,
        addr: SocketAddr,
        context_data: HashMap<String, Box<dyn Any + Send>>,
    ) {
        if let Some(handled) = Self::handle(self, router, context_data).await {
            let _ = stream.send((handled, addr)).await.map_err(|e| error!("{e}"));
        }
    }
}

impl From<BytesMut> for SemiTcpProtocol {
    fn from(value: BytesMut) -> Self {
        let mut body_index = 0;
        let mut heads = HashMap::new();
        let mut path = String::from_utf8(value.to_vec()).map_err(|e| warn!("{e}")).map_or(Default::default(), |v| v.trim().to_string());
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
        Self { meta: Meta::new(value, heads), body_index, }
    }
}

impl Into<BytesMut> for SemiTcpProtocol {
    fn into(mut self) -> BytesMut {
        let resp = self.meta.resp_mut().take();
        let (id, path) = (self.id(), self.path());
        match resp {
            Some(Response::Raw(resp)) => resp,
            resp => {
                let mut text = BytesMut::new();
                if let Some(id) = id {
                    text.put_slice(id.as_bytes());
                    text.put_slice(ID_PATH_SEPARATOR.to_string().as_bytes());
                }
                text.put_slice(path.as_bytes());
                for (k, v) in self.resp_heads() {
                    text.put_slice(format!("\n{k}:{v}").as_bytes())
                }
                text.put_slice(format!("\n{}\n", HEAD_BODY_SEPARATOR).as_bytes());
                if let Some(Response::Serialized(sd)) = resp {
                    text.put(self.pack_resp(sd));
                }
                text
            }
        }        
    }
}

impl Deref for SemiTcpProtocol {
    type Target = Meta<BytesMut, BytesMut>;

    fn deref(&self) -> &Self::Target {
        &self.meta
    }
}

impl DerefMut for SemiTcpProtocol {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.meta
    }
}

#[async_trait]
impl RoutableProtocol for SemiTcpProtocol {
    type Req = BytesMut;
    type Resp = Self::Req;

    async fn body(&mut self) -> DceResult<Serialized> {
        String::from_utf8(self.req_mut().take().ok_or_else(|| DceErr::closed0("Empty request"))?.to_vec())
            .map(|t| Serialized::String(t[self.body_index ..].to_string())).map_err(DceErr::closed0)
    }

    fn pack_resp(&self, serialized: Serialized) -> Self::Resp {
        let mut text = BytesMut::new();
        match serialized {
            Serialized::String(str) => text.put_slice(str.as_bytes()),
            Serialized::Bytes(bytes) => text.put_slice(bytes.as_ref()),
        }
        text
    }
}
