// module needs to be top level for generated functions to be in scope:
// https://github.com/capnproto/capnproto-rust/issues/16
pub mod api {
    include!(concat!(env!("OUT_DIR"), "/schema/api_capnp.rs"));
}

use std::default::Default;
use async_std::net::TcpStream;

use futures_signals::signal::Mutable;
use casbin::Enforcer;
use casbin::MgmtApi;

use crate::machine::Machines;
use crate::auth::Authentication;
use crate::access::Permissions;

use capnp::{Error};
use capnp::capability::Promise;
use capnp_rpc::RpcSystem;
use capnp_rpc::twoparty::VatNetwork;
use capnp_rpc::rpc_twoparty_capnp::Side;

use api::diflouroborane;

pub fn init() {
}

pub async fn process_socket(auth: Authentication, perm: Permissions, mach: Machines, socket: TcpStream) 
    -> Result<(), Error> 
{
    let api = Api { auth, perm, mach };
    let a = api::diflouroborane::ToClient::new(api).into_client::<capnp_rpc::Server>();
    let netw = VatNetwork::new(socket.clone(), socket, Side::Server, Default::default());
    let rpc = RpcSystem::new(Box::new(netw), Some(a.clone().client));
    rpc.await
}

struct Api {
    auth: Authentication,
    perm: Permissions,
    mach: Machines,
}

impl diflouroborane::Server for Api {
    fn authentication(&mut self,
        _params: diflouroborane::AuthenticationParams,
        mut results: diflouroborane::AuthenticationResults)
        -> Promise<(), Error>
    {
        let mut b = results.get();
        let auth = api::authentication::ToClient::new(self.auth.clone()).into_client::<capnp_rpc::Server>();
        b.set_auth(auth);
        Promise::ok(())
    }

    fn permissions(&mut self,
        _params: diflouroborane::PermissionsParams,
        mut results: diflouroborane::PermissionsResults) 
        -> Promise<(), Error>
    {
        let mut b = results.get();
        let perm = api::permissions::ToClient::new(self.perm.clone()).into_client::<capnp_rpc::Server>();
        b.set_perm(perm);
        Promise::ok(())
    }

    fn machines(&mut self,
        _params: diflouroborane::MachinesParams,
        mut results: diflouroborane::MachinesResults) 
        -> Promise<(), Error>
    {
        let mut b = results.get();
        let mach = api::machines::ToClient::new(self.mach.clone()).into_client::<capnp_rpc::Server>();
        b.set_mach(mach);
        Promise::ok(())
    }
}
