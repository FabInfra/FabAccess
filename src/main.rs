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
mod session;

use signal_hook::iterator::Signals;

use clap::{App, Arg};

use api::api as api_capnp;

use session::Session;

use futures::prelude::*;
use futures::executor::{LocalPool, ThreadPool};
use futures::join;

use capnp_rpc::twoparty::{VatNetwork, VatId};
use capnp_rpc::rpc_twoparty_capnp::Side;

use async_std::net::{TcpListener, TcpStream};

use std::io;
use std::io::Write;
use std::path::PathBuf;
use std::str::FromStr;
use std::mem::drop;

use std::sync::Arc;

use error::Error;

// Returning a `Result` from `main` allows us to use the `?` shorthand.
// In the case of an Err it will be printed using `fmt::Debug`
fn main() -> Result<(), Error> {
    // Initialize signal handler.
    // Specifically, this is a Stream of c_int representing received signals
    // We currently only care about Ctrl-C so SIGINT it is.
    // TODO: Make this do SIGHUP and a few others too.
    let signals = Signals::new(&[signal_hook::SIGINT])?.into_async()?;

    use clap::{crate_version, crate_description, crate_name};

    // Argument parsing
    // values for the name, description and version are pulled from `Cargo.toml`.
    let matches = App::new(crate_name!())
        .about(crate_description!())
        .version(crate_version!())
        .arg(Arg::with_name("config")
            .help("Path to the config file to use")
            .long("config")
            .short("c")
            .takes_value(true)
        )
        .arg(Arg::with_name("print default")
            .help("Print a default config to stdout instead of running")
            .long("print-default")
        )
        .get_matches();

    // Check for the --print-default option first because we don't need to do anything else in that
    // case.
    if matches.is_present("print default") {
        let config = config::Config::default();
        let encoded = toml::to_vec(&config)?;

        // Direct writing to fd 1 is faster but also prevents any print-formatting that could
        // invalidate the generated TOML
        let stdout = io::stdout();
        let mut handle = stdout.lock();
        handle.write_all(&encoded)?;

        // Early return to exit.
        return Ok(())
    }

    // If no `config` option is given use a preset default.
    let configpath = matches.value_of("config").unwrap_or("/etc/diflouroborane.toml");
    let config = config::read(&PathBuf::from_str(configpath).unwrap())?;

    // Initialize the logging subsystem first to be able to better document the progress from now
    // on.
    // TODO: Now would be a really good time to close stdin/out and move logging to syslog
    // Log is in an Arc so we can do very cheap clones in closures.
    let log = Arc::new(log::init(&config));
    info!(log, "Starting");

    // Kick up an executor
    // Most initializations from now on do some amount of IO and are much better done in an
    // asyncronous fashion.
    let mut exec = LocalPool::new();

    // Start loading the machine database, authentication system and permission system
    // All of those get a custom logger so the source of a log message can be better traced and
    // filtered
    let machinedb_f = machine::init(log.new(o!("system" => "machinedb")), &config);
    let permission_f = access::init(log.new(o!("system" => "permissions")), &config);
    let authentication_f = auth::init(log.new(o!("system" => "authentication")), config.clone());

    // Bind to each address in config.listen.
    // This is a Stream over Futures so it will do absolutely nothing unless polled to completion
    let listeners_s: futures::stream::Collect<_, Vec<TcpListener>> = stream::iter((&config).listen.iter())
        .map(|l| {
            let addr = l.address.clone();
            TcpListener::bind((l.address.as_str(), l.port.unwrap_or(config::DEFAULT_PORT)))
                // If the bind errors, include the address so we can log it
                // Since this closure is lazy we need to have a cloned addr
                .map_err(|e| { (addr, e) })
        })
        .filter_map(|f| async {
            match f.await {
                Ok(l) => Some(l),
                Err((addr, e)) => {
                    error!(&log, "Could not connect to {}: {}", addr, e);
                    None
                }
            }
        }).collect();

    let (mdb, pdb, auth) = exec.run_until(async {
        // Rull all futures to completion in parallel.
        // This will "block" until all three are done starting up.
        join!(machinedb_f, permission_f, authentication_f)
    });

    // Error out if any of the subsystems failed to start.
    let mdb = mdb?;
    let pdb = pdb.unwrap();
    let auth = auth?;

    // Since the below closures will happen at a much later time we need to make sure all pointers
    // are still valid. Thus, Arc.
    let l2 = log.clone();
    let l3 = log.clone();

    // Create a thread pool to run tasks on
    let mut pool = ThreadPool::builder()
        .after_start(move |i| { info!(l2.new(o!("system" => "threadpool")), "Starting Thread <{}>", i)})
        .before_stop(move |i| { info!(l3.new(o!("system" => "threadpool")), "Stopping Thread <{}>", i)})
        .create()?;

    // Spawner is a handle to the shared ThreadPool forwarded into each connection
    let spawner = pool.clone();

    let result: Result<(), Box<dyn std::error::Error>> = exec.run_until(async move {
        // Generate a stream of TcpStreams appearing on any of the interfaces we listen to
        let listeners = listeners_s.await;
        let mut incoming = stream::select_all(listeners.iter().map(|l| l.incoming()));

        // Runn
        while let Some(socket) = incoming.next().await {
        }

        Ok(())
    });

    Ok(())
}
