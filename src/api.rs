// module needs to be top level for generated functions to be in scope:
// https://github.com/capnproto/capnproto-rust/issues/16
pub(crate) mod api_capnp {
    include!(concat!(env!("OUT_DIR"), "/schema/api_capnp.rs"));
}

use std::default::Default;
use async_std::net::TcpStream;

use futures_signals::signal::Mutable;
use casbin::Enforcer;
use casbin::MgmtApi;

use crate::machine::{MachineDB, Machine, Status, save};
use crate::auth::Authentication;

pub fn init() {
}

pub async fn process_socket(e: Mutable<Enforcer>, m: Mutable<MachineDB>, a: Authentication, socket: TcpStream) 
    -> Result<(), capnp::Error> 
{
    let auth = api_capnp::authentication::ToClient::new(a).into_client::<::capnp_rpc::Server>();
    let api = Api { e, m, auth };
    let a = api_capnp::bffh_admin::ToClient::new(api).into_client::<::capnp_rpc::Server>();
    let netw = capnp_rpc::twoparty::VatNetwork::new(socket.clone(), socket,
        capnp_rpc::rpc_twoparty_capnp::Side::Server, Default::default());
    let rpc = capnp_rpc::RpcSystem::new(Box::new(netw), Some(a.clone().client));
    rpc.await
}

struct Api {
    e: Mutable<Enforcer>,
    m: Mutable<MachineDB>,
    auth: api_capnp::authentication::Client,
}

impl api_capnp::bffh_admin::Server for Api {
    fn get_all_subjects(&mut self,
        _params: api_capnp::bffh_admin::GetAllSubjectsParams,
        mut results: api_capnp::bffh_admin::GetAllSubjectsResults)
        -> ::capnp::capability::Promise<(), ::capnp::Error>
    {
        let subjs = self.e.lock_ref().get_all_subjects();
        let mut b = results.get()
            .init_subjects(subjs.len() as u32);
        for (i, s) in subjs.into_iter().enumerate() {
            let bldr = b.reborrow();
            let mut sub = bldr.get(i as u32);
            sub.set_id(&s);
            sub.set_domain("");
        }

        ::capnp::capability::Promise::ok(())
    }

    fn get_all_machines(&mut self,
        _params: api_capnp::bffh_admin::GetAllMachinesParams,
        mut results: api_capnp::bffh_admin::GetAllMachinesResults)
        -> ::capnp::capability::Promise<(), ::capnp::Error>
    {
        let machs = self.m.lock_ref();

        let mut b = results.get()
            .init_machines(machs.len() as u32);

        for (i, (name, m)) in machs.iter().enumerate() {
            let bldr = b.reborrow();
            let mut mach = bldr.get(i as u32);
            mach.set_name(&name);
            mach.set_location(&m.location);
            mach.set_status(match m.status {
                Status::Blocked => api_capnp::Status::Blocked,
                Status::Free => api_capnp::Status::Free,
                Status::Occupied => api_capnp::Status::Occupied,
            });
        }
        ::capnp::capability::Promise::ok(())
    }

    fn add_machine(&mut self,
        params: api_capnp::bffh_admin::AddMachineParams,
        mut results: api_capnp::bffh_admin::AddMachineResults)
        -> ::capnp::capability::Promise<(), ::capnp::Error>
    {
        let params = pry!(params.get());

        let name = pry!(params.get_name());
        let location = pry!(params.get_location());

        let m = Machine::new(location.to_string());

        let mut mdb = self.m.lock_mut();
        mdb.insert(name.to_string(), m);

        ::capnp::capability::Promise::ok(())
    }

    fn authentication(&mut self,
        _params: api_capnp::bffh_admin::AuthenticationParams,
        mut results: api_capnp::bffh_admin::AuthenticationResults)
        -> ::capnp::capability::Promise<(), ::capnp::Error>
    {
        let mut b = results.get();
        b.set_auth(self.auth.clone());
        ::capnp::capability::Promise::ok(())
    }
}
