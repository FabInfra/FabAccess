//! Mock impl of MQTT as transport.
//!
//! Specific Protocol implementations (Sonoff/Card2Go/...) would be located here

use slog::Logger;

pub fn init(log: Logger) {
    info!(log, "MQTT Module initialized.")
}
