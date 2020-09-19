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
    UserExit,
}

#[derive(Debug)]
pub struct RBError {
    kind: ErrorKind,
    source_error: Option<Box<dyn Error + 'static>>,
}

impl RBError {
    pub fn new_with_source<E>(kind: ErrorKind, source_error: E) -> RBError
    where
        E: Into<Box<dyn Error + 'static>>,
    {
        RBError {
            kind,
            source_error: Some(source_error.into()),
        }
    }

    pub fn new(kind: ErrorKind) -> RBError {
        RBError {
            kind,
            source_error: None,
        }
    }

    pub fn kind(&self) -> ErrorKind {
        self.kind
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
