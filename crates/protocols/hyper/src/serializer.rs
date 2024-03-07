use std::ops::Deref;
use sailfish::TemplateOnce;
use dce_macro::closed_err;
use dce_router::request::ResponseStatus;
use dce_router::serializer::{Deserializer, Serializable, Serialized, Serializer};
use dce_util::mixed::DceResult;


pub struct SailfishSerializer {}

impl<Dto> Deserializer<Dto> for SailfishSerializer {
    fn deserialize<'a>(&self, _value: Serialized) -> DceResult<Dto> {
        unreachable!("Not support template deserialize yet")
    }
}

impl<Dto: TemplateOnce> Serializer<Dto> for SailfishSerializer {
    fn serialize(&self, value: Serializable<Dto>) -> DceResult<Serialized> {
        let rendered = match value {
            Serializable::Dto(dto) | Serializable::Status(ResponseStatus { data: Some(dto), .. }) => dto.render_once(),
            Serializable::Status(status) => match status.code {
                404 => NotFound::from(status).render_once(),
                _ => Status::from(status).render_once(),
            },
        }.map_err(|e| closed_err!("{}", e.to_string()))?;
        Ok(Serialized::String(rendered))
    }
}


#[derive(TemplateOnce)]
#[template(path = "notfound.html")]
pub struct NotFound<Dto> {
    pub s: ResponseStatus<Dto>,
}

impl<Dto> From<ResponseStatus<Dto>> for NotFound<Dto> {
    fn from(value: ResponseStatus<Dto>) -> Self {
        NotFound {s: value}
    }
}


#[derive(TemplateOnce)]
#[template(path = "status.html")]
pub struct Status<Dto> {
    pub s: ResponseStatus<Dto>,
}

impl<Dto> From<ResponseStatus<Dto>> for Status<Dto> {
    fn from(value: ResponseStatus<Dto>) -> Self {
        Status {s: value}
    }
}
