use std::fmt::{Debug, Display, Formatter};

pub mod string;
pub mod atom_tree;
pub mod mem;


pub struct PlaceHolder;


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

    pub fn openly(code: isize, message: String) -> Self {
        Self::Openly(DceError {code, message})
    }

    pub fn closed(code: isize, message: String) -> Self {
        Self::Closed(DceError {code, message})
    }
}

#[derive(Debug)]
pub struct DceError {
    pub code: isize,
    pub message: String,
}


impl Display for DceErr {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let (message, code) = match self {
            DceErr::Openly(DceError{code, message}) => (code, message),
            DceErr::Closed(DceError{code, message}) => (code, message),
        };
        f.write_str(format!("{}: {}", code, message).as_str())
    }
}

impl std::error::Error for DceErr {}

pub const SERVICE_UNAVAILABLE: isize = 503;
pub const SERVICE_UNAVAILABLE_MESSAGE: &str = "Service Unavailable";
