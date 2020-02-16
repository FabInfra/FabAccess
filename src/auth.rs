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

use futures_signals::signal::Mutable;
use casbin::Enforcer;

use crate::error::Result;

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

#[derive(Clone)]
struct Plain {
    // FIXME: I don't want to store passwords.
    passdb: Mutable<PassDB>,
    enforcer: Mutable<Enforcer>,
}

impl Plain {
    pub fn step<'a>(&self, data: &'a [u8]) -> Result<(bool, &'a str)> {
        let data = std::str::from_utf8(data).map_err(|_| SASLError::UTF8)?;
        if let Some((authzid, authcid, passwd)) = split_nul(data) {

            // Check if we know about that user
            if let Some(pwd) = self.passdb.lock_ref().get(authcid) {
                // Check the provided password
                // FIXME: At least use hashes
                if pwd == passwd {
                    // authzid is the Identity the user wants to act as.
                    // If that is unset, shortcut to Success
                    if authzid == "" || authzid == authcid {
                        return Ok((true, authcid));
                    }

                    let e = self.enforcer.lock_ref();
                    if let Ok(b) = e.enforce(vec![authcid, authzid, "su"]) {
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

#[derive(Clone)]
pub struct Authentication {
    state: Option<String>,
    plain: Plain,
}

impl Authentication {
    pub fn new(passdb: Mutable<PassDB>, enforcer: Mutable<Enforcer>) -> Self {
        Authentication {
            state: None,
            plain: Plain { passdb, enforcer }
        }
    }

    pub fn mechs(&self) -> Vec<&'static str> {
        vec!["PLAIN"]
    }
}

use crate::api_capnp;

impl api_capnp::authentication::Server for Authentication {
    fn available_mechanisms(&mut self,
        _params: api_capnp::authentication::AvailableMechanismsParams,
        mut results: api_capnp::authentication::AvailableMechanismsResults)
        -> ::capnp::capability::Promise<(), ::capnp::Error>
    {
        let m = self.mechs();
        let mut b = results.get()
            .init_mechanisms(m.len() as u32);
        for (i, mech) in m.iter().enumerate() {
            let mut bldr = b.reborrow();
            bldr.set(i as u32, mech);
        }

        ::capnp::capability::Promise::ok(())
    }

    fn initialize_authentication(&mut self,
        params: api_capnp::authentication::InitializeAuthenticationParams,
        mut results: api_capnp::authentication::InitializeAuthenticationResults)
        -> ::capnp::capability::Promise<(), ::capnp::Error>
    {
        let params = pry!(params.get());
        let mechanism = pry!(params.get_mechanism());
        match mechanism {
            "PLAIN" => {
                use api_capnp::maybe::Which;

                let data = pry!(params.get_initial_data());
                if let Ok(Which::Some(data)) = data.which() {
                    let data = pry!(data);
                    if let Ok((b, name)) = self.plain.step(data) {

                        // If login was successful, also set the current authzid
                        if b {
                            self.state = Some(name.to_string());
                        }

                        let outcome = Outcome::value(b);
                        results
                            .get()
                            .init_response()
                            .set_right(api_capnp::authentication::outcome::ToClient::new(outcome)
                                .into_client::<::capnp_rpc::Server>()).unwrap();
                    }
                    ::capnp::capability::Promise::ok(())
                } else {
                    return
                        ::capnp::capability::Promise::err(::capnp::Error::unimplemented(
                        "SASL PLAIN requires initial data set".to_string()));
                }
            },
            m => {
                return
                    ::capnp::capability::Promise::err(::capnp::Error::unimplemented(
                    format!("SASL Mechanism {} is not implemented", m)));
            }
        }
    }

    fn get_authzid(&mut self,
        _params: api_capnp::authentication::GetAuthzidParams,
        mut results: api_capnp::authentication::GetAuthzidResults)
        -> ::capnp::capability::Promise<(), ::capnp::Error>
    {
        if let Some(zid) = &self.state {
            results.get().set_authzid(zid);
        } else {
            results.get().set_authzid("");
        }
        ::capnp::capability::Promise::ok(())
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

impl api_capnp::authentication::outcome::Server for Outcome {
    fn value(&mut self,
        _params: api_capnp::authentication::outcome::ValueParams,
        mut results: api_capnp::authentication::outcome::ValueResults)
        -> ::capnp::capability::Promise<(), ::capnp::Error>
    {
        results.get().set_granted(self.value);
        ::capnp::capability::Promise::ok(())
    }
}
