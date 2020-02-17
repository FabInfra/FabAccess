use std::collections::HashMap;
use std::fs::File;
use std::path::Path;
use std::io::{Read, Write};

use slog::Logger;

use serde::{Serialize, Deserialize};
use toml;

use futures_signals::signal::Mutable;

use crate::error::Result;
use crate::config::Config;
use crate::api::api;
use crate::access::Permissions;

use capnp::capability::Promise;
use capnp::Error;
use capnp_rpc::Server;

use uuid::Uuid;

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

#[derive(Clone)]
pub struct Machines {
    log: Logger,
    mdb: Mutable<MachineDB>,
    perm: Permissions
}

impl Machines {
    pub fn new(log: Logger, mdb: Mutable<MachineDB>, perm: Permissions) -> Self {
        Self { log, mdb, perm }
    }
}
impl api::machines::Server for Machines {
    fn manage(&mut self,
        params: api::machines::ManageParams,
        mut results: api::machines::ManageResults)
        -> Promise<(), Error>
    {
        let params = pry!(params.get());
        let uuid_s = pry!(params.get_uuid());

        let uuid = uuid_from_api(uuid_s);

        let db = self.mdb.lock_ref();

        if let Some(m) = db.get(&uuid) {
            let manager = MachineManager::new(uuid, self.mdb.clone());

            if self.perm.enforce(&m.perm, "manage") {
                let mut b = results.get();
                let mngr = api::machines::manage::ToClient::new(manager).into_client::<Server>();
                b.set_manage(mngr);
                trace!(self.log, "Granted manage on machine {}", uuid);
                Promise::ok(())
            } else {
                Promise::err(Error::failed("Permission denied".to_string()))
            }
        } else {
            info!(self.log, "Attempted manage on invalid machine {}", uuid);
            Promise::err(Error::failed("No such machine".to_string()))
        }
    }

    fn use_(&mut self,
        params: api::machines::UseParams,
        mut results: api::machines::UseResults)
        -> Promise<(), Error>
    {
        let params = pry!(params.get());
        let uuid_s = pry!(params.get_uuid());
        let uuid = uuid_from_api(uuid_s);

        let mdb = self.mdb.lock_ref();
        if let Some(m) = mdb.get(&uuid) {
            match m.status {
                Status::Free => {
                    trace!(self.log, "Granted use on machine {}", uuid);

                    let mut b = results.get();

                    let gb = api::machines::give_back::ToClient::new(
                            GiveBack::new(self.log.new(o!()), uuid, self.mdb.clone())
                        ).into_client::<Server>();

                    b.set_giveback(gb);

                    Promise::ok(())
                },
                Status::Occupied => {
                    info!(self.log, "Attempted use on an occupied machine {}", uuid);
                    Promise::err(Error::failed("Machine is occupied".to_string()))
                },
                Status::Blocked => {
                    info!(self.log, "Attempted use on a blocked machine {}", uuid);
                    Promise::err(Error::failed("Machine is blocked".to_string()))
                }
            }
        } else {
            info!(self.log, "Attempted use on invalid machine {}", uuid);
            Promise::err(Error::failed("No such machine".to_string()))
        }
    }
}

pub struct GiveBack {
    log: Logger,
    mdb: Mutable<MachineDB>,
    uuid: Uuid,
}
impl GiveBack {
    pub fn new(log: Logger, uuid: Uuid, mdb: Mutable<MachineDB>) -> Self {
        trace!(log, "Giveback initialized for {}", uuid);
        Self { log, mdb, uuid }
    }
}

impl api::machines::give_back::Server for GiveBack {
    fn giveback(&mut self,
        _params: api::machines::give_back::GivebackParams,
        _results: api::machines::give_back::GivebackResults)
        -> Promise<(), Error>
    {
        trace!(log, "Returning {}...", uuid);
        let mut mdb = self.mdb.lock_mut();
        if let Some(m) = mdb.get_mut(&self.uuid) {
            m.status = Status::Free;
        } else {
            warn!(self.log, "A giveback was issued for a unknown machine {}", self.uuid);
        }

        Promise::ok(())
    }
}

// FIXME: Test this exhaustively!
fn uuid_from_api(uuid: api::u_u_i_d::Reader) -> Uuid {
    let uuid0 = uuid.get_uuid0() as u128;
    let uuid1 = uuid.get_uuid1() as u128;
    let num: u128 = (uuid1 << 64) + uuid0;
    Uuid::from_u128(num)
}
fn api_from_uuid(uuid: Uuid, mut wr: api::u_u_i_d::Builder) {
    let num = uuid.to_u128_le();
    let uuid0 = num as u64;
    let uuid1 = (num >> 64) as u64;
    wr.set_uuid0(uuid0);
    wr.set_uuid1(uuid1);
}

#[derive(Clone)]
pub struct MachineManager {
    mdb: Mutable<MachineDB>,
    uuid: Uuid,
}

impl MachineManager {
    pub fn new(uuid: Uuid, mdb: Mutable<MachineDB>) -> Self {
        Self { mdb, uuid }
    }
}

impl api::machines::manage::Server for MachineManager {
    fn set_blocked(&mut self,
        params: api::machines::manage::SetBlockedParams,
        mut results: api::machines::manage::SetBlockedResults)
        -> Promise<(), Error>
    {
        let mut db = self.mdb.lock_mut();
        if let Some(m) = db.get_mut(&self.uuid) {
            let params = pry!(params.get());
            let blocked = params.get_blocked();

            m.set_blocked(blocked);
            Promise::ok(())
        } else {
            Promise::err(Error::failed("No such machine".to_string()))
        }
    }

}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Machine {
    pub name: String,
    pub location: String,
    pub status: Status,
    pub perm: String,
}

impl Machine {
    pub fn new(name: String, location: String, perm: String) -> Machine {
        Machine {
            name: name,
            location: location,
            status: Status::Free,
            perm: perm,
        }
    }

    pub fn set_blocked(&mut self, blocked: bool) {
        if blocked {
            self.status = Status::Blocked;
        } else {
            self.status = Status::Free;
        }
    }
}

pub type MachineDB = HashMap<Uuid, Machine>;

pub fn init(config: &Config) -> Result<MachineDB> {
    if config.machinedb.is_file() {
        let mut fp = File::open(&config.machinedb)?;
        let mut content = String::new();
        fp.read_to_string(&mut content)?;
        let map = toml::from_str(&content)?;
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
