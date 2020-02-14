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

pub fn init() {
}

pub async fn process_socket(enforcer: Mutable<Enforcer>, socket: TcpStream) -> Result<(), capnp::Error> {
    let api = Api { e: enforcer };
    let a = api_capnp::bffh_admin::ToClient::new(api).into_client::<::capnp_rpc::Server>();
    let netw = capnp_rpc::twoparty::VatNetwork::new(socket.clone(), socket,
        capnp_rpc::rpc_twoparty_capnp::Side::Server, Default::default());
    let rpc = capnp_rpc::RpcSystem::new(Box::new(netw), Some(a.clone().client));
    rpc.await
}

struct Api {
    e: Mutable<Enforcer>,
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
}
