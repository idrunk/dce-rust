use std::fmt::Debug;
use crate::util::{DceErr, DceResult, SERVICE_UNAVAILABLE, SERVICE_UNAVAILABLE_MESSAGE};
use crate::router::request::RawRequest;
use crate::router::serializer::Serialized;
use log::{error, warn};
#[cfg(feature = "async")]
use async_trait::async_trait;


#[cfg_attr(feature = "async", async_trait)]
pub trait RoutableProtocol: From<Self::Req> + Into<Self::Resp> {
    type Req;
    type Resp: Debug;
    fn path(&self) -> &str;
    #[cfg(feature = "async")]
    async fn body(&mut self) -> DceResult<Serialized>;
    #[cfg(not(feature = "async"))]
    fn body(&mut self) -> DceResult<Serialized>;
    fn pack_resp(self, serialized: Serialized) -> Self::Resp;

    fn id(&self) -> Option<&str> {
        None
    }

    fn try_print_err(response: &DceResult<Option<Self::Resp>>) {
        if response.is_err() {
            match response.as_ref().err() {
                Some(DceErr::Openly(err)) => warn!("code {}, {}", err.code, err.message),
                Some(DceErr::Closed(err)) => error!("code {}, {}", err.code, err.message),
                _ => unreachable!(),
            };
        }
    }

    fn err_into(self, code: isize, message: String) -> Self::Resp {
        self.pack_resp(Serialized::String(format!("{}, {}", code, message)))
    }

    fn handle_result(self, (unresponsive, response): (Option<bool>, DceResult<Option<Self::Resp>>)) -> Option<Self::Resp> {
        Self::try_print_err(&response);
        if ! unresponsive.unwrap_or_else(|| self.id().is_none()) {
            return Some(match response {
                Ok(Some(resp)) => resp,
                Ok(None) => self.into(),
                Err(DceErr::Openly(err)) => self.err_into(err.code, err.message),
                Err(DceErr::Closed(_)) => self.err_into(SERVICE_UNAVAILABLE, SERVICE_UNAVAILABLE_MESSAGE.to_string()),
            });
        }
        None
    }
}


#[derive(Debug)]
pub struct CustomizedProtocolRawRequest<T> (T);

#[cfg_attr(feature = "async", async_trait)]
impl<T: RoutableProtocol + Send + Debug + 'static> RawRequest for CustomizedProtocolRawRequest<T> {
    type Rpi = T;
    type Req = T::Req;
    type Resp = T::Resp;

    fn new(raw: T) -> Self {
        CustomizedProtocolRawRequest(raw)
    }

    fn path(&self) -> &str {
        self.0.path()
    }

    fn raw(&self) -> &Self::Rpi {
        &self.0
    }

    fn raw_mut(&mut self) -> &mut Self::Rpi {
        &mut self.0
    }

    #[cfg(feature = "async")]
    async fn data(&mut self) -> DceResult<Serialized> {
        self.0.body().await
    }

    #[cfg(not(feature = "async"))]
    fn data(&mut self) -> DceResult<Serialized> {
        self.0.body()
    }

    fn pack_response(self, serialized: Serialized) -> DceResult<Self::Resp> {
        Ok(self.0.pack_resp(serialized))
    }
}
