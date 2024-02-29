use sailfish::TemplateOnce;
use dce_router::closed_err;
use dce_router::router::request::ResponseStatus;
use dce_router::router::serializer::{Deserializer, Serializable, Serialized, Serializer};
use dce_router::util::DceResult;

pub struct SailfishSerializer {

}

impl<Dto> Deserializer<Dto> for SailfishSerializer {
    fn deserialize<'a>(&self, _value: Serialized) -> DceResult<Dto> {
        unreachable!("Not support template deserialize yet")
    }
}

impl<Dto: TemplateOnce> Serializer<Dto> for SailfishSerializer {
    fn serialize(&self, value: Serializable<Dto>) -> DceResult<Serialized> {
        let dto = match value {
            Serializable::Dto(dto) => dto,
            Serializable::Status(ResponseStatus { data: Some(dto), .. }) => dto,
            _ => unreachable!("None sailfishable Dto"),
        };
        Ok(Serialized::String(dto.render_once().map_err(|e| closed_err!("{}", e.to_string()))?))
    }
}
