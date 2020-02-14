//! Indpendent Communication modules
//!
//! This is where dynamic modules are implemented later on using libloading / abi_stable_crates et
//! al.
//! Additionally, FFI modules to other languages (Python/Lua/...) make the most sense in here as
//! well.

mod mqtt;

use slog::Logger;

pub fn init(log: Logger) {
    info!(log, "Initializing submodules");
    mqtt::init(log.new(o!()));
    info!(log, "Finished initializing submodules");
}
