use std::str::FromStr;
use std::path::PathBuf;
use serde::{Serialize, Deserialize};

use crate::error::Result;

pub fn read() -> Result<Config> {
    Ok(Config {
        machinedb: PathBuf::from_str("/tmp/machines.db").unwrap(),
        access: Access {
            model: PathBuf::from_str("/tmp/model.conf").unwrap(),
            policy: PathBuf::from_str("/tmp/policy.csv").unwrap(),
        },
        passdb: PathBuf::from_str("/tmp/passwd.db").unwrap(),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub(crate) access: Access,
    pub machinedb: PathBuf,
    pub passdb: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Access {
    pub(crate) model: PathBuf,
    pub(crate) policy: PathBuf
}
