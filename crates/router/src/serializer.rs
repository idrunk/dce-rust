use std::any::{Any, type_name};
use std::fmt::{Debug, Display, Formatter};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use crate::request::ResponseStatus;
use dce_util::mixed::{DceErr, DceResult};

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
    pub fn to_string(&self) -> String {
        match self {
            Serialized::String(v) => v.to_string(),
            Serialized::Bytes(v) => String::from_utf8_lossy(&v).to_string(),
        }
    }
    
    pub fn json_value(&self) -> DceResult<Value> {
        serde_json::from_str(self.to_string().as_str()).map_err(DceErr::closed0)
    }
}

impl Display for Serialized {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.to_string().as_str())
    }
}



pub trait Deserializer<Dto> {
    fn deserialize<'a>(&self, value: Serialized) -> DceResult<Dto>;
}

pub trait Serializer<Dto> {
    fn serialize(&self, value: Serializable<Dto>) -> DceResult<Serialized>;
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

impl From<Vec<(&str, Box<dyn Any>)>> for StringSerializer {
    fn from(_: Vec<(&str, Box<dyn Any>)>) -> Self {
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
        }).or(DceErr::closed0_wrap("Serialized cannot deserialize to ReqDto"))?)
    }
}

impl<Dto: Serialize + 'static> Serializer<Dto> for JsonSerializer {
    fn serialize(&self, value: Serializable<Dto>) -> DceResult<Serialized> {
        Ok(Serialized::String(match value {
            Serializable::Dto(v) => serde_json::to_string::<Dto>(&v),
            Serializable::Status(v) => serde_json::to_string::<ResponseStatus<Dto>>(&v),
        }.or(DceErr::closed0_wrap("RespDto not a jsonable"))?))
    }
}

impl From<Vec<(&'static str, Box<dyn Any>)>> for JsonSerializer {
    fn from(_: Vec<(&'static str, Box<dyn Any>)>) -> Self {
        JsonSerializer {}
    }
}
