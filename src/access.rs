//! Access control logic
//!

use slog::Logger;

use casbin::prelude::*;

use futures_signals::signal::Mutable;

use crate::api::api;
use crate::config::Config;
use crate::auth::Authentication;
use crate::error::Result;

use std::rc::Rc;
use async_std::sync::{Arc, RwLock};

use std::ops::Deref;

pub struct PermissionsProvider {
    log: Logger,
    pdb: Enforcer,
}

impl PermissionsProvider {
    pub fn new(log: Logger, pdb: Enforcer) -> Self {
        Self { log, pdb }
    }

    pub fn enforce(&self, actor: &str, object: &str, action: &str) -> Result<bool> {
        let b = self.pdb.enforce(vec![actor, object, action])?;
        if b {
            trace!(self.log, "Granted {} on {} for {}", action, object, actor);
        } else {
            trace!(self.log, "Denied {} on {} for {}", action, object, actor);
        }
        Ok(b)
    }
}

#[derive(Clone)]
pub struct Permissions {
    inner: Arc<RwLock<PermissionsProvider>>,
    auth: Rc<Authentication>,
}

impl Permissions {
    pub fn new(inner: Arc<RwLock<PermissionsProvider>>, auth: Rc<Authentication>) -> Self {
        Self { inner, auth }
    }

    pub async fn enforce(&self, object: &str, action: &str) -> Result<bool> {
        if let Some(actor) = self.auth.state.read().await.deref() {
            self.inner.read().await.enforce(&actor, object, action)
        } else {
            Ok(false)
        }
    }
}

impl api::permissions::Server for Permissions {

}

/// This line documents init
pub async fn init(log: Logger, config: &Config) -> std::result::Result<PermissionsProvider, Box<dyn std::error::Error>> {
    let model = Model::from_file(config.access.model.clone()).await?;
    let adapter = Box::new(FileAdapter::new(config.access.policy.clone()));

    let e = Enforcer::new(model, adapter).await?;

    return Ok(PermissionsProvider::new(log, e));
}
