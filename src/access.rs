//! Access control logic
//!

use casbin::prelude::*;

use super::config::Config;

use futures_signals::signal::Mutable;

use crate::api::api;
use crate::auth::Authentication;
use crate::error::Result;

#[derive(Clone)]
pub struct Permissions {
    pdb: Mutable<Enforcer>,
    auth: Authentication,
}

impl Permissions {
    pub fn new(pdb: Mutable<Enforcer>, auth: Authentication) -> Self {
        Self { pdb, auth }
    }

    pub fn enforce(&self, object: &str, action: &str) -> bool {
        if let Some(actor) = self.auth.get_authzid() {
            self.pdb.lock_ref().enforce(vec![&actor,object,action]).unwrap()
        } else {
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
