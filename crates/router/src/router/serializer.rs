use std::any::{Any, type_name};
use std::fmt::{Debug, Display, Formatter};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use dce_router_macro::closed_err;
use crate::router::api::ToStruct;
use crate::util::DceResult;
use crate::router::request::ResponseStatus;
use crate::util::mem::self_transmute;

#[derive(Debug)]
pub enum Serializable<Dto> {
    Dto(Dto),
    Status(ResponseStatus<Dto>),
}


#[derive(Debug)]
pub enum Serialized {
    String(String),
    Bytes(Bytes),
}

impl Serialized {
    pub fn unwrap<T: 'static>(self) -> T {
        match self {
            Serialized::String(v) => self_transmute::<String, T>(v),
            Serialized::Bytes(v) => self_transmute::<Bytes, T>(v),
        }
    }
}

impl Display for Serialized {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&match self {
            Serialized::String(str) => str.to_string(),
            Serialized::Bytes(bytes) => String::from_utf8_lossy(bytes).to_string(),
        })
    }
}



pub trait Deserializer<Dto> {
    fn deserialize<'a>(&self, value: Serialized) -> DceResult<Dto>;
}

pub trait Serializer<Dto> {
    fn serialize(&self, value: Serializable<Dto>) -> DceResult<Serialized>;
}



// 无用序列器，用于不解析或不序列化的请求数据，默认序列器
/// unreachable serializer, just for no parse or no serialize request data, default serializer
pub struct UnreachableSerializer;

impl<Dto> Deserializer<Dto> for UnreachableSerializer {
    fn deserialize<'a>(&self, _: Serialized) -> DceResult<Dto> {
        panic!("missing serializer in api configuration, or try not using a serializable Request")
    }
}

impl<Dto> Serializer<Dto> for UnreachableSerializer {
    fn serialize(&self, _: Serializable<Dto>) -> DceResult<Serialized> {
        panic!("missing serializer in api configuration, or do not try respond in a not responsible Request")
    }
}

impl ToStruct for UnreachableSerializer {
    fn from<const N: usize>(_: [(&str, Box<dyn Any>); N]) -> Self {
        UnreachableSerializer
    }
}



pub struct StringSerializer;

impl<Dto: From<Serialized>> Deserializer<Dto> for StringSerializer {
    fn deserialize<'a>(&self, value: Serialized) -> DceResult<Dto> {
        Ok(Dto::from(value))
    }
}

impl<Dto: Into<Serialized> + 'static> Serializer<Dto> for StringSerializer {
    fn serialize(&self, value: Serializable<Dto>) -> DceResult<Serialized> {
        Ok(match value {
            Serializable::Dto(v) => v.into(),
            Serializable::Status(s) => Serialized::String((if s.status { "succeeded" } else { "failed" }).to_string())
        })
    }
}

impl ToStruct for StringSerializer {
    fn from<const N: usize>(_: [(&str, Box<dyn Any>); N]) -> Self {
        StringSerializer
    }
}



pub struct JsonSerializer {

}

impl<Dto: for<'a> Deserialize<'a>> Deserializer<Dto> for JsonSerializer {
    fn deserialize<'a>(&self, value: Serialized) -> DceResult<Dto> {
        Ok((match value {
            Serialized::String(v) => serde_json::from_str(v.as_str()),
            Serialized::Bytes(v) => serde_json::from_slice(v.as_ref()),
        }).or(Err(closed_err!("Serialized cannot deserialize to ReqDto")))?)
    }
}

impl<Dto: Serialize + 'static> Serializer<Dto> for JsonSerializer {
    fn serialize(&self, value: Serializable<Dto>) -> DceResult<Serialized> {
        Ok(Serialized::String(match value {
            Serializable::Dto(v) => serde_json::to_string::<Dto>(&v),
            Serializable::Status(v) => serde_json::to_string::<ResponseStatus<Dto>>(&v),
        }.or(Err(closed_err!("RespDto not a jsonable")))?))
    }
}

impl ToStruct for JsonSerializer {
    fn from<const N: usize>(_: [(&str, Box<dyn Any>); N]) -> Self {
        JsonSerializer {}
    }
}


impl<Dto> Debug for dyn Deserializer<Dto> + Send + Sync {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(format!("Deserializer: {}{{}}", type_name::<Self>()).as_str())
    }
}

impl<Dto> Debug for dyn Serializer<Dto> + Send + Sync {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(format!("Serializer: {}{{}}", type_name::<Self>()).as_str())
    }
}

