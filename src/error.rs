use std::io;

#[derive(Debug)]
pub enum Error {
    IO(io::Error)
}

pub type Result<T> = std::result::Result<T, Error>;
