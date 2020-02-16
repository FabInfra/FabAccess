use std::collections::HashMap;
use std::fs::File;
use std::path::Path;
use std::io::{Read, Write};

use serde::{Serialize, Deserialize};
use toml;

use futures_signals::signal::{ReadOnlyMutable};
use casbin::Enforcer;

use crate::error::Result;
use crate::config::Config;

/// Status of a Machine
#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Status {
    /// Not currently used by anybody
    Free,
    /// Used by somebody
    Occupied,
    /// Not used by anybody but also can not be used. E.g. down for maintenance
    Blocked,
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Machine {
    pub location: String,
    pub status: Status,
}

impl Machine {
    pub fn new(location: String) -> Machine {
        Machine {
            location: location,
            status: Status::Free,
        }
    }
}

pub type MachineDB = HashMap<Name, Machine>;

type Name = String;

pub fn init(config: &Config) -> Result<MachineDB> {
    if config.machinedb.is_file() {
        let mut fp = File::open(&config.machinedb)?;
        let mut content = String::new();
        fp.read_to_string(&mut content)?;
        let map: HashMap<Name, Machine> = toml::from_str(&content)?;
        return Ok(map);
    } else {
        return Ok(HashMap::new());
    }
}

pub fn save(config: &Config, mdb: &MachineDB) -> Result<()> {
    let mut fp = File::create(&config.machinedb)?;
    let toml = toml::to_string(mdb)?;
    fp.write_all(&toml.as_bytes())?;
    Ok(())
}
