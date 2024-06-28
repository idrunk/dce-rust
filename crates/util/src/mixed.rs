use std::fmt::{Display, Formatter};

pub type DceResult<T> = Result<T, DceErr>;

#[derive(Debug)]
pub enum DceErr {
    /// openly error, will print in console and send to client
    Openly(DceError),
    Closed(DceError),
}

impl DceErr {
    pub fn value(&self) -> &DceError {
        match self {
            DceErr::Openly(v) => v,
            DceErr::Closed(v) => v,
        }
    }

    pub fn openly<T: ToString>(code: isize, message: T) -> Self {
        Self::Openly(DceError {code, message: message.to_string()})
    }

    pub fn closed<T: ToString>(code: isize, message: T) -> Self {
        Self::Closed(DceError {code, message: message.to_string()})
    }

    pub fn openly0<T: ToString>(message: T) -> Self {
        Self::Openly(DceError {code: 0, message: message.to_string()})
    }

    pub fn closed0<T: ToString>(message: T) -> Self {
        Self::Closed(DceError {code: 0, message: message.to_string()})
    }

    pub fn none() -> Self {
        Self::Closed(DceError {code: 0, message: "Need Some but got None".to_string()})
    }

    pub fn openly0_wrap<M: ToString, R>(message: M) -> Result<R, Self> {
        Err(Self::Openly(DceError {code: 0, message: message.to_string()}))
    }

    pub fn closed0_wrap<M: ToString, R>(message: M) -> Result<R, Self> {
        Err(Self::Closed(DceError {code: 0, message: message.to_string()}))
    }

    pub fn to_responsible(&self) -> String {
        match self {
            DceErr::Openly(e) => format!("{}: {}", e.code, e.message),
            DceErr::Closed(_) => format!("{}: {}", SERVICE_UNAVAILABLE, SERVICE_UNAVAILABLE_MESSAGE),
        }
    }
}

#[derive(Debug)]
pub struct DceError {
    pub code: isize,
    pub message: String,
}


impl Display for DceErr {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let (code, message) = match self {
            DceErr::Openly(DceError{code, message}) => (code, message),
            DceErr::Closed(DceError{code, message}) => (code, message),
        };
        f.write_str(format!("{}: {}", code, message).as_str())
    }
}

impl std::error::Error for DceErr {}

pub const SERVICE_UNAVAILABLE: isize = 503;
pub const SERVICE_UNAVAILABLE_MESSAGE: &str = "Service Unavailable";
