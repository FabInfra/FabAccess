#[macro_use]
extern crate slog;

#[macro_use]
extern crate capnp_rpc;

mod auth;
mod access;
mod modules;
mod log;
mod api;
mod config;
mod error;
mod machine;

use api::api as api_capnp;

use futures::prelude::*;
use futures_signals::signal::Mutable;
use futures::task::LocalSpawn;

fn main() {
    let log = log::init();
    info!(log, "Starting");

    let config = config::read().unwrap();

    modules::init(log.new(o!()));
    api::init();

    let m = machine::init(&config).unwrap();
    let m = Mutable::new(m);
    let m2 = m.clone();
    let c2 = config.clone();


    let mut exec = futures::executor::LocalPool::new();

    let enf = exec.run_until(async {
        let e = access::init(&config).await.unwrap();
        Mutable::new(e)
    });

    let p = auth::open_passdb(&config.passdb).unwrap();
    let p = Mutable::new(p);
    let authp = auth::AuthenticationProvider::new(p, enf.clone());
    let authp = Mutable::new(authp);

    use std::net::ToSocketAddrs;
    let args: Vec<String> = ::std::env::args().collect();
    if args.len() != 2 {
        println!("usage: {} ADDRESS[:PORT]", args[0]);
        return;
    }

    let addr = args[1].to_socket_addrs().unwrap().next().expect("could not parse address");


    let spawner = exec.spawner();
    let result: Result<(), Box<dyn std::error::Error>> = exec.run_until(async move {
        let listener = async_std::net::TcpListener::bind(&addr).await?;
        let mut incoming = listener.incoming();
        while let Some(socket) = incoming.next().await {
            let socket = socket?;
            // TODO: Prettify session handling
            let auth = auth::Authentication::new(authp.clone());
            let perm = access::Permissions::new(enf.clone(), auth.clone());
            let mach = machine::Machines::new(m.clone(), perm.clone());

            let rpc_system = api::process_socket(auth, perm, mach, socket);
            spawner.spawn_local_obj(
                Box::pin(rpc_system.map_err(|e| println!("error: {:?}", e)).map(|_|())).into()).expect("spawn")
        }
        Ok(())
    });
    result.expect("main");

    machine::save(&c2, &m2.lock_ref()).expect("MachineDB save");
}
