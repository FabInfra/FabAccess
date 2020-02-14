//! Access control logic
//!

use casbin::prelude::*;

use super::config::Config;

pub async fn init(config: &Config) -> Result<Enforcer, Box<dyn std::error::Error>> {
    let model = Model::from_file(config.access.model.clone()).await?;
    let adapter = Box::new(FileAdapter::new(config.access.policy.clone()));

    let e = Enforcer::new(model, adapter).await?;

    return Ok(e);
}
