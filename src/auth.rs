//! Authentication subsystem
//!
//! Authorization is over in `access.rs`
//! Authentication using SASL

use std::collections::HashMap;
use std::fmt;
use std::error::Error;
use std::path::Path;
use std::fs::File;
use std::io::{Read, Write};
use std::ops::Deref;

use async_std::sync::{Arc, RwLock};
use capnp::capability::Promise;

use futures_signals::signal::Mutable;
use casbin::{Enforcer, Model, FileAdapter};

use slog::Logger;

use crate::error::Result;
use crate::config::Config;

pub async fn init(log: Logger, config: Config) -> Result<AuthenticationProvider> {
    let passdb = open_passdb(&config.passdb).unwrap();

    let m = Model::from_file(&config.access.model).await?;
    let a = FileAdapter::new(config.access.policy);
    let enforcer = Enforcer::new(m, Box::new(a)).await?;

    Ok(AuthenticationProvider::new(passdb, enforcer))
}

#[derive(Debug)]
pub enum SASLError {
    /// Expected UTF-8, got something else
    UTF8,
    /// A bad Challenge was provided
    BadChallenge,
    /// Enforcer Failure
    Enforcer,
}
impl fmt::Display for SASLError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Bad SASL Exchange")
    }
}
impl Error for SASLError {}

type PassDB = HashMap<String, String>;
pub fn open_passdb(path: &Path) -> Option<PassDB> {
    if path.is_file() {
        let mut fp = File::open(path).unwrap();
        let mut content = String::new();
        fp.read_to_string(&mut content).unwrap();
        let map = toml::from_str(&content).ok()?;
        return Some(map);
    } else {
        let mut map = HashMap::new();
        map.insert("Testuser".to_string(), "Testpass".to_string());
        let mut fp = File::create(&path).unwrap();
        let toml = toml::to_string(&map).unwrap();
        fp.write_all(&toml.as_bytes()).unwrap();
        return Some(map);
    }
}

pub struct Plain {
    // FIXME: I don't want to store passwords.
    passdb: PassDB,
    enforcer: Enforcer,
}

impl Plain {
    pub fn step<'a>(&self, data: &'a [u8]) -> Result<(bool, &'a str)> {
        let data = std::str::from_utf8(data).map_err(|_| SASLError::UTF8)?;
        if let Some((authzid, authcid, passwd)) = split_nul(data) {

            // Check if we know about that user
            if let Some(pwd) = self.passdb.get(authcid) {
                // Check the provided password
                // FIXME: At least use hashes
                if pwd == passwd {
                    // authzid is the Identity the user wants to act as.
                    // If that is unset, shortcut to Success
                    if authzid == "" || authzid == authcid {
                        return Ok((true, authcid));
                    }

                    if let Ok(b) = self.enforcer.enforce(vec![authcid, authzid, "su"]) {
                        if b {
                            return Ok((true, authzid));
                        } else {
                            return Ok((false, authzid));
                        }
                    } else {
                        return Err(SASLError::Enforcer.into());
                    }

                }
            }
            Ok((false, authzid))
        } else {
            return Err(SASLError::BadChallenge.into())
        }
    }
}

pub fn split_nul(string: &str) -> Option<(&str, &str, &str)> {
    let mut i = string.split(|b| b == '\0');

    let a = i.next()?;
    let b = i.next()?;
    let c = i.next()?;

    Some((a,b,c))
}


pub struct AuthenticationProvider {
    pub plain: Plain,
}

impl AuthenticationProvider {
        pub fn new(passdb: PassDB, enforcer: Enforcer) -> Self {
        Self {
            plain: Plain { passdb, enforcer }
        }
    }

    pub fn mechs(&self) -> Vec<&'static str> {
        vec!["PLAIN"]
    }
}

#[derive(Clone)]
pub struct Authentication {
    pub state: Arc<RwLock<Option<String>>>,
    provider: Arc<RwLock<AuthenticationProvider>>,
}
impl Authentication {
    pub fn new(provider: Arc<RwLock<AuthenticationProvider>>) -> Self {
        Self {
            state: Arc::new(RwLock::new(None)),
            provider: provider,
        }
    }
}


use crate::api::api;

impl api::authentication::Server for Authentication {
    fn available_mechanisms(&mut self,
        _params: api::authentication::AvailableMechanismsParams,
        mut results: api::authentication::AvailableMechanismsResults)
        -> ::capnp::capability::Promise<(), ::capnp::Error>
    {
        let p = self.provider.clone();
        let f = async move {
            let m = p.read().await.mechs();
            let mut b = results.get()
                .init_mechanisms(m.len() as u32);
            for (i, mech) in m.iter().enumerate() {
                let mut bldr = b.reborrow();
                bldr.set(i as u32, mech);
            }
            Ok(())
        };

        ::capnp::capability::Promise::from_future(f)
    }

    fn initialize_authentication(&mut self,
        params: api::authentication::InitializeAuthenticationParams,
        mut results: api::authentication::InitializeAuthenticationResults)
        -> ::capnp::capability::Promise<(), ::capnp::Error>
    {
        let prov = self.provider.clone();
        let stat = self.state.clone();

        Promise::from_future(async move {
            let params = params.get()?;
            let mechanism = params.get_mechanism()?;

            match mechanism {
                "PLAIN" => {
                    use api::authentication::maybe_data::Which;

                    let data = params.get_initial_data()?;
                    if let Ok(Which::Some(data)) = data.which() {
                        let data = data?;
                        if let Ok((b, name)) = prov.read().await.plain.step(data) {
                            // If login was successful set the authzid
                            if b {
                                stat.write().await.replace(name.to_string());
                            }

                            let outcome = Outcome::value(b);
                            results
                                .get()
                                .init_response()
                                .set_outcome(api::authentication::outcome::ToClient::new(outcome)
                                    .into_client::<::capnp_rpc::Server>());
                        }
                        Ok(())
                    } else {
                        Err(::capnp::Error::unimplemented(
                            "SASL PLAIN requires initial data set".to_string()))
                    }
                },
                m => {
                    Err(::capnp::Error::unimplemented(
                            format!("SASL Mechanism {} is not implemented", m)
                    ))
                }

            }
        })
    }

    fn get_authzid(&mut self,
        _params: api::authentication::GetAuthzidParams,
        mut results: api::authentication::GetAuthzidResults)
        -> ::capnp::capability::Promise<(), ::capnp::Error>
    {
        let state = self.state.clone();
        let f = async move {
            if let Some(zid) = state.read().await.deref() {
                results.get().set_authzid(&zid);
            } else {
                results.get().set_authzid("");
            }

            Ok(())
        };

        Promise::from_future(f)
    }
}

struct Outcome {
    data: Option<Box<[u8]>>,
    value: bool,
}
impl Outcome {
    pub fn value(value: bool) -> Self {
        Self { data: None, value: value }
    }
}

impl api::authentication::outcome::Server for Outcome {
    fn value(&mut self,
        _params: api::authentication::outcome::ValueParams,
        mut results: api::authentication::outcome::ValueResults)
        -> ::capnp::capability::Promise<(), ::capnp::Error>
    {
        results.get().set_granted(self.value);
        ::capnp::capability::Promise::ok(())
    }
}
