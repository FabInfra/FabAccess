use std::str::FromStr;
use std::path::PathBuf;
use serde_derive::Deserialize;

use crate::error::Result;

pub fn read() -> Result<Config> {
    Ok(Config {
        access: Access {
            model: PathBuf::from_str("/tmp/model.conf").unwrap(),
            policy: PathBuf::from_str("/tmp/policy.csv").unwrap(),
        }
    })
}

#[derive(Deserialize)]
pub struct Config {
    pub(crate) access: Access
}

#[derive(Deserialize)]
pub struct Access {
    pub(crate) model: PathBuf,
    pub(crate) policy: PathBuf
}
