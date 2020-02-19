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

use signal_hook::iterator::Signals;

use clap::{App, Arg};

use api::api as api_capnp;

use futures::prelude::*;
use futures::executor::{LocalPool, ThreadPool};
use futures::compat::Stream01CompatExt;
use futures::join;
use futures::task::{SpawnExt, LocalSpawn};

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

use api::API;

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
    let machinedb_f = machine::init(log.new(o!("system" => "machines")), &config);
    let permission_f = access::init(log.new(o!("system" => "permissions")), &config);
    let authentication_f = auth::init(log.new(o!("system" => "authentication")), config.clone());

    // Bind to each address in config.listen.
    // This is a Stream over Futures so it will do absolutely nothing unless polled to completion
    let listeners_s: futures::stream::Collect<_, Vec<TcpListener>> 
        = stream::iter((&config).listen.iter())
        .map(|l| {
            let addr = l.address.clone();
            let port = l.port.unwrap_or(config::DEFAULT_PORT);
            TcpListener::bind((l.address.as_str(), port))
                // If the bind errors, include the address so we can log it
                // Since this closure is lazy we need to have a cloned addr
                .map_err(move |e| { (addr, port, e) })
        })
        .filter_map(|f| async {
            match f.await {
                Ok(l) => Some(l),
                Err((addr, port, e)) => {
                    error!(&log, "Could not setup socket on {} port {}: {}", addr, port, e);
                    None
                }
            }
        }).collect();

    let (mach, pdb, auth) = exec.run_until(async {
        // Rull all futures to completion in parallel.
        // This will block until all three are done starting up.
        join!(machinedb_f, permission_f, authentication_f)
    });

    // Error out if any of the subsystems failed to start.
    let mach = mach?;
    let pdb = pdb.unwrap();
    let auth = auth?;

    // Since the below closures will happen at a much later time we need to make sure all pointers
    // are still valid. Thus, Arc.
    let start_log = log.clone();
    let stop_log = log.clone();

    // Create a thread pool to run tasks on
    let pool = ThreadPool::builder()
        .after_start(move |i| {
            info!(start_log.new(o!("system" => "threadpool")), "Starting Thread <{}>", i)
        })
        .before_stop(move |i| {
            info!(stop_log.new(o!("system" => "threadpool")), "Stopping Thread <{}>", i)
        })
        .create()?;
    let local_spawn = exec.spawner();


    // The API has access to all subsystems it needs and the Threadpool as capability to spawn new
    // tasks for CPU-intensive work
    let api = API::new(auth, pdb, mach, pool);

    // Closure inefficiencies. Lucky cloning an Arc is pretty cheap.
    let inner_log = log.clone();
    let loop_log = log.clone();

    exec.run_until(async move {
        // Generate a stream of TcpStreams appearing on any of the interfaces we listen to
        let listeners = listeners_s.await;
        let incoming = stream::select_all(listeners.iter().map(|l| l.incoming()));

        // For each incoming connection start a new task to handle it and throw it on the thread
        // pool
        let handle_sockets = incoming.map(|socket| {
            // incoming.next() is an error when the underlying `accept` call yielded an error
            // In POSIX those are protocol errors we can't really handle, so we just log the error
            // and the move on
            match socket {
                Ok(socket) => {
                    // If we have it available add the peer's address to all log messages
                    let log =
                        if let Ok(addr) = socket.peer_addr() {
                            inner_log.new(o!("address" => addr))
                        } else {
                            inner_log.new(o!())
                        };

                    // Clone a log for potential error handling
                    let elog = log.clone();

                    // We handle the error using map_err, `let _` is used to quiet the compiler
                    // warning
                    let f = api::handle_connection(api.clone(), log.clone(), socket)
                        .map_err(move |e| {
                            error!(log, "Error occured during protocol handling: {}", e);
                        })
                        // Void any and all results since pool.spawn allows no return value.
                        .map(|_| ());

                    // In this case only the error is relevant since the Value is always ()
                    // The future is Boxed to make it the `LocalFutureObj` that LocalSpawn expects
                    if let Err(e) = local_spawn.spawn_local_obj(Box::new(f).into()) {
                        error!(elog, "Failed to spawn connection handler: {}", e);
                        // Failing to spawn a handler means we are most likely overloaded
                        return LoopResult::Overloaded;
                    }
                },
                Err(e) => {
                    error!(inner_log, "Socket `accept` error: {}", e);
                }
            }

            // Unless we are overloaded we just want to keep going.
            return LoopResult::Continue;
        });

        // Check each signal as it arrives
        // signals is a futures-0.1 stream, compat() makes it a futures-0.3 (which we use) stream
        let handle_signals = signals.compat().map(|_signal| {
            // _signal is the signal c_int.
            // But since we only listen for SIGINT at the moment we don't really need to look at
            // it.
            return LoopResult::Stop;
        });

        // Now actually check if a connection was opened or a signal recv'd
        let mut combined = stream::select(handle_signals, handle_sockets);
        loop {
            match combined.next().await {
                // When the result says to continue, do exactly that
                Some(LoopResult::Continue) => {}
                Some(LoopResult::Overloaded) => {
                    // In case over server overload we should install a replacement handler that
                    // would instead just return `overloaded` for all connections until the
                    // situation is remedied.
                    //
                    // For now, just log the overload and keep going.
                    error!(loop_log, "Server overloaded");
                }
                // None should never be returned because it would mean all sockets were closed and
                // we can not receive any further signals. Still, in that case shut down cleanly
                // anyway, the only reason this could happen are some heavy bugs in the runtime
                Some(LoopResult::Stop) | None => {
                    warn!(loop_log, "Stopping server");
                    break;
                }
            }
        }
    });

    // TODO: Run actual shut down code here
    info!(log, "Shutting down...");

    // Returning () is an implicit success so this will properly set the exit code as well
    Ok(())
}

/// The result of one iteration of the core loop
enum LoopResult {
    /// Everything was fine, keep going
    Continue,
    /// Something happened that means we should shut down
    Stop,
    /// The Server is currently overloaded
    Overloaded,
}
