use std::io;
use toml;

use crate::auth::SASLError;

#[derive(Debug)]
pub enum Error {
    TomlDe(toml::de::Error),
    TomlSer(toml::ser::Error),
    SASL(SASLError),
    IO(io::Error),
}

impl From<SASLError> for Error {
    fn from(e: SASLError) -> Error {
        Error::SASL(e)
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Error {
        Error::IO(e)
    }
}

impl From<toml::de::Error> for Error {
    fn from(e: toml::de::Error) -> Error {
        Error::TomlDe(e)
    }
}

impl From<toml::ser::Error> for Error {
    fn from(e: toml::ser::Error) -> Error {
        Error::TomlSer(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
