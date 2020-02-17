//! Access control logic
//!

use slog::Logger;

use casbin::prelude::*;

use super::config::Config;

use futures_signals::signal::Mutable;

use crate::api::api;
use crate::auth::Authentication;
use crate::error::Result;

#[derive(Clone)]
pub struct Permissions {
    log: Logger,
    pdb: Mutable<Enforcer>,
    auth: Authentication,
}

impl Permissions {
    pub fn new(log: Logger, pdb: Mutable<Enforcer>, auth: Authentication) -> Self {
        Self { log, pdb, auth }
    }

    pub fn enforce(&self, object: &str, action: &str) -> bool {
        if let Some(actor) = self.auth.get_authzid() {
            trace!(self.log, "Checking permission {} for {} on {}", action, actor, object);
            let r = self.pdb.lock_ref().enforce(vec![&actor,object,action]).unwrap();
            if !r {
                info!(self.log, "Failed permission {} for {} on {}", action, actor, object);
            }
            return r;
        } else {
            info!(self.log, "Attempted anonymous access: {} on {}", action, object);
            false
        }
    }
}

impl api::permissions::Server for Permissions {

}

/// This line documents init
pub async fn init(config: &Config) -> std::result::Result<Enforcer, Box<dyn std::error::Error>> {
    let model = Model::from_file(config.access.model.clone()).await?;
    let adapter = Box::new(FileAdapter::new(config.access.policy.clone()));

    let e = Enforcer::new(model, adapter).await?;

    return Ok(e);
}
