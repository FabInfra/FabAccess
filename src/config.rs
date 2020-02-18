use std::str::FromStr;
use std::path::{Path, PathBuf};
use serde::{Serialize, Deserialize};
use std::io::Read;
use std::fs::File;

use crate::error::Result;

use std::default::Default;

pub fn read(path: &Path) -> Result<Config> {
    let mut fp = File::open(path)?;
    let mut contents = String::new();
    fp.read_to_string(&mut contents)?;

    let config = toml::from_str(&contents)?;
    Ok(config)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub machinedb: PathBuf,
    pub passdb: PathBuf,
    pub(crate) access: Access,
    pub listen: Box<[Listen]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Access {
    pub(crate) model: PathBuf,
    pub(crate) policy: PathBuf
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Listen {
    pub address: String,
    pub port: Option<u16>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            machinedb: PathBuf::from_str("/tmp/machines.db").unwrap(),
            access: Access {
                model: PathBuf::from_str("/tmp/model.conf").unwrap(),
                policy: PathBuf::from_str("/tmp/policy.csv").unwrap(),
            },
            passdb: PathBuf::from_str("/tmp/passwd.db").unwrap(),
            listen: Box::new([Listen {
                    address: "127.0.0.1".to_string(),
                    port: Some(DEFAULT_PORT)
                },
                Listen {
                    address: "::1".to_string(),
                    port: Some(DEFAULT_PORT)
            }]),
        }
    }
}

// The default port in the non-assignable i.e. free-use area
pub const DEFAULT_PORT: u16 = 59661;
