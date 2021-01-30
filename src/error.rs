use std::error::Error;
use std::fmt;

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum ErrorKind {
    IO,
    InvalidCommand,
    InvalidTarget,
    Other,
    Readline,
    S3,
    TargetAlreadyExists,
    UserExit,
}

#[derive(Debug)]
pub struct RBError {
    kind: ErrorKind,
    source_error: Option<Box<dyn Error + 'static>>,
}

impl RBError {
    pub fn new(kind: ErrorKind) -> RBError {
        RBError {
            kind,
            source_error: None,
        }
    }

    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    // These "wrap" functions reduce duplicate code in the common `.map_err(|err| please_turn_this_into_rb_error(err))`
    // type situations
    pub fn wrap_s3<E>(err: E) -> Self
    where
        E: Into<Box<dyn Error + 'static>>,
    {
        RBError {
            kind: ErrorKind::S3,
            source_error: Some(err.into()),
        }
    }

    pub fn wrap_io<E>(err: E) -> Self
    where
        E: Into<Box<dyn Error + 'static>>,
    {
        RBError {
            kind: ErrorKind::IO,
            source_error: Some(err.into()),
        }
    }
}

impl fmt::Display for RBError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Error for RBError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source_error.as_ref().map(|b| b.as_ref())
    }
}
