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

use std::rc::Rc;
use async_std::sync::{Arc, RwLock};

use capnp::capability::Promise;
use capnp::Error;
use capnp_rpc::Server;

use uuid::Uuid;
use std::ops::DerefMut;

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

pub struct MachinesProvider {
    log: Logger,
    mdb: MachineDB,
}

impl MachinesProvider {
    pub fn new(log: Logger, mdb: MachineDB) -> Self {
        Self { log, mdb }
    }

    pub fn use_(&mut self, uuid: &Uuid) -> std::result::Result<(), capnp::Error> {
        if let Some(m) = self.mdb.get_mut(uuid) {
            match m.status {
                Status::Free => {
                    trace!(self.log, "Granted use on machine {}", uuid);

                    m.status = Status::Occupied;

                    Ok(())
                },
                Status::Occupied => {
                    info!(self.log, "Attempted use on an occupied machine {}", uuid);
                    Err(Error::failed("Machine is occupied".to_string()))
                },
                Status::Blocked => {
                    info!(self.log, "Attempted use on a blocked machine {}", uuid);
                    Err(Error::failed("Machine is blocked".to_string()))
                }
            }
        } else {
            info!(self.log, "Attempted use on invalid machine {}", uuid);
            Err(Error::failed("No such machine".to_string()))
        }
    }

    pub fn give_back(&mut self, uuid: &Uuid) -> std::result::Result<(), capnp::Error> {
        if let Some(m) = self.mdb.get_mut(uuid) {
            m.status = Status::Free;
        } else {
            warn!(self.log, "A giveback was issued for a unknown machine {}", uuid);
        }

        Ok(())
    }

    pub fn get_perm_req(&self, uuid: &Uuid) -> Option<String> {
        self.mdb.get(uuid).map(|m| m.perm.clone())
    }

    pub fn set_blocked(&mut self, uuid: &Uuid, blocked: bool) -> std::result::Result<(), capnp::Error> {
        // If the value can not be found map doesn't run and ok_or changes it into a Err with the
        // given error value
        self.mdb.get_mut(uuid).map(|m| m.set_blocked(blocked))
            .ok_or(capnp::Error::failed("No such machine".to_string()))
    }
}

#[derive(Clone)]
pub struct Machines {
    inner: Arc<RwLock<MachinesProvider>>,
    perm: Rc<Permissions>,
}
impl Machines {
    pub fn new(inner: Arc<RwLock<MachinesProvider>>, perm: Rc<Permissions>) -> Self {
        Self { inner, perm }
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

        // We need to copy the Arc here because we don't have access to it from within the closure
        // witout moving it out of self.
        let i = self.inner.clone();
        let p = self.perm.clone();

        let f = async move {
            // We only need a read lock at first there's no reason to aquire a write lock.
            let i_lock = i.read().await;

            if let Some(ps) = i_lock.get_perm_req(&uuid) {
                // drop the lock as soon as possible to prevent locking as much as possible
                drop(i_lock);
                if let Ok(true) = p.enforce(&ps, "manage").await {
                    // We're here and have not returned an error yet - that means we're free to
                    // send a successful manage back.
                    let mut b = results.get();

                    // Magic incantation to get a capability to send
                    // Also since we move i in here we at this point *must* have dropped
                    // all locks we may still have on it.
                    b.set_manage(api::machines::manage::ToClient::new(
                            MachineManager::new(uuid, i)).into_client::<Server>());
                }
            }
            Ok(())
        };

        Promise::from_future(f)
    }

    fn use_(&mut self,
        params: api::machines::UseParams,
        mut results: api::machines::UseResults)
        -> Promise<(), capnp::Error>
    {
        let params = pry!(params.get());
        let uuid_s = pry!(params.get_uuid());
        let uuid = uuid_from_api(uuid_s);

        // We need to copy the Arc here because we don't have access to it from within the closure
        // witout moving it out of self.
        let i = self.inner.clone();
        let p = self.perm.clone();

        let f = async move {
            // We only need a read lock at first there's no reason to aquire a write lock.
            let i_lock = i.read().await;

            if let Some(ps) = i_lock.get_perm_req(&uuid) {
                // drop the lock as soon as possible to prevent locking as much as possible
                drop(i_lock);
                if let Ok(true) = p.enforce(&ps, "write").await {
                    {
                        // If use_() returns an error that is our error. If it doesn't that means we can use
                        // the machine
                        // Using a subscope to again make the time the lock is valid as short as
                        // possible. Less locking == more good
                        let mut i_lock = i.write().await;
                        i_lock.use_(&uuid)?;
                    }

                    // We're here and have not returned an error yet - that means we're free to
                    // send a successful use back.
                    let mut b = results.get();

                    // Magic incantation to get a capability to send
                    // Also since we move i in here we at this point *must* have dropped
                    // all locks we may still have on it.
                    b.set_giveback(api::machines::give_back::ToClient::new(
                            GiveBack::new(i, uuid)).into_client::<Server>());
                }
            }
            Ok(())
        };

        Promise::from_future(f)
    }
}

#[derive(Clone)]
pub struct GiveBack {
    mdb: Arc<RwLock<MachinesProvider>>,
    uuid: Uuid,
}
impl GiveBack {
    pub fn new(mdb: Arc<RwLock<MachinesProvider>>, uuid: Uuid) -> Self {
        Self { mdb, uuid }
    }
}

impl api::machines::give_back::Server for GiveBack {
    fn giveback(&mut self,
        _params: api::machines::give_back::GivebackParams,
        _results: api::machines::give_back::GivebackResults)
        -> Promise<(), Error>
    {
        let mdb = self.mdb.clone();
        let uuid = self.uuid.clone();
        let f = async move {
            mdb.write().await.give_back(&uuid)
        };

        Promise::from_future(f)
    }
}

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
    mdb: Arc<RwLock<MachinesProvider>>,
    uuid: Uuid,
}

impl MachineManager {
    pub fn new(uuid: Uuid, mdb: Arc<RwLock<MachinesProvider>>) -> Self {
        Self { mdb, uuid }
    }
}

impl api::machines::manage::Server for MachineManager {
    fn set_blocked(&mut self,
        params: api::machines::manage::SetBlockedParams,
        results: api::machines::manage::SetBlockedResults)
        -> Promise<(), Error>
    {
        let uuid = self.uuid.clone();
        let mdb = self.mdb.clone();
        let f = async move {
            let params = params.get()?;
            let blocked = params.get_blocked();
            mdb.write().await.set_blocked(&uuid, blocked)?;
            Ok(())
        };

        Promise::from_future(f)
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

pub async fn init(log: Logger, config: &Config) -> Result<MachinesProvider> {
    let mdb = if config.machinedb.is_file() {
        let mut fp = File::open(&config.machinedb)?;
        let mut content = String::new();
        fp.read_to_string(&mut content)?;
        let map = toml::from_str(&content)?;
        map
    } else {
        HashMap::new()
    };

    Ok(MachinesProvider::new(log, mdb))
}

pub fn save(config: &Config, mdb: &MachineDB) -> Result<()> {
    let mut fp = File::create(&config.machinedb)?;
    let toml = toml::to_string(mdb)?;
    fp.write_all(&toml.as_bytes())?;
    Ok(())
}
