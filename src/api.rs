// module needs to be top level for generated functions to be in scope:
// https://github.com/capnproto/capnproto-rust/issues/16
pub mod api {
    include!(concat!(env!("OUT_DIR"), "/schema/api_capnp.rs"));
}

use std::default::Default;
use async_std::net::TcpStream;

use futures::task::Spawn;
use futures::FutureExt;
use futures_signals::signal::Mutable;
use casbin::Enforcer;
use casbin::MgmtApi;

use slog::Logger;

use std::rc::Rc;
use async_std::sync::{Arc, RwLock};

use crate::machine::{MachinesProvider, Machines};
use crate::auth::{AuthenticationProvider, Authentication};
use crate::access::{PermissionsProvider, Permissions};

use capnp::{Error};
use capnp::capability::Promise;
use capnp_rpc::RpcSystem;
use capnp_rpc::twoparty::VatNetwork;
use capnp_rpc::rpc_twoparty_capnp::Side;

use std::ops::Deref;

use api::diflouroborane;

#[derive(Clone)]
pub struct API<S> {
    auth: Arc<RwLock<AuthenticationProvider>>,
    perm: Arc<RwLock<PermissionsProvider>>,
    mach: Arc<RwLock<MachinesProvider>>,

    spawner: S,
}
impl<S: Spawn> API<S> {
    pub fn new(auth: AuthenticationProvider, 
       perm: PermissionsProvider,
       mach: MachinesProvider,
       spawner: S)
        -> Self
    {
        let auth = Arc::new(RwLock::new(auth));
        let perm = Arc::new(RwLock::new(perm));
        let mach = Arc::new(RwLock::new(mach));

        Self { auth, perm, mach, spawner }
    }

    pub fn into_connection(self) -> Bootstrap {
        let auth = Rc::new(Authentication::new(self.auth));
        let perm = Rc::new(Permissions::new(self.perm, auth.clone()));
        let mach = Machines::new(self.mach, perm.clone());
        Bootstrap {
            auth: auth,
            perm: perm,
            mach: mach,
        }
    }
}

pub async fn handle_connection<S: Spawn>(api: API<S>, log: Logger, socket: TcpStream) -> Result<(), Error> {
    info!(log, "A new connection");
    let client = api.into_connection();
    let a = api::diflouroborane::ToClient::new(client).into_client::<capnp_rpc::Server>();

    let netw = VatNetwork::new(socket.clone(), socket, Side::Server, Default::default());

    let rpc = RpcSystem::new(Box::new(netw), Some(a.clone().client)).map(|_| ());

    rpc.await;
    
    Ok(())
}

/// Bootstrap capability of the Diflouroborane API
///
/// This is the starting point for any client connecting
#[derive(Clone)]
pub struct Bootstrap {
    auth: Rc<Authentication>,
    perm: Rc<Permissions>,
    mach: Machines,
}

impl diflouroborane::Server for Bootstrap {
    fn authentication(&mut self,
        _params: diflouroborane::AuthenticationParams,
        mut results: diflouroborane::AuthenticationResults)
        -> Promise<(), Error>
    {
        let mut b = results.get();
        let auth = api::authentication::ToClient::new(self.auth.deref().clone()).into_client::<capnp_rpc::Server>();
        b.set_auth(auth);
        Promise::ok(())
    }

    fn permissions(&mut self,
        _params: diflouroborane::PermissionsParams,
        mut results: diflouroborane::PermissionsResults) 
        -> Promise<(), Error>
    {
        //let mut b = results.get();
        //let perm = api::permissions::ToClient::new(self.perm).into_client::<capnp_rpc::Server>();
        //b.set_perm(perm);
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
