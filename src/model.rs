use super::error::{QueryError, SerializationError};
use lazy_static::lazy_static;
use mac_address::get_mac_address;
use macaddr::MacAddr6;
use rand::{thread_rng, Rng};
use serde::Deserialize;
use serde_json::{value::RawValue, Value};
use std::string::ToString;
use std::{net::IpAddr, str::FromStr};
use strum_macros::{Display, EnumString};

lazy_static! {}

#[derive(Debug, Clone, Copy, Display, EnumString)]
pub enum MethodNames {
    #[strum(serialize = "getPower")]
    GetPower,
    #[strum(serialize = "getSystemConfig")]
    GetSystemConfig,
    #[strum(serialize = "getModelConfig")]
    GetModelConfig,
    #[strum(serialize = "getUserConfig")]
    GetUserConfig,
    #[strum(serialize = "getWifiConfig")]
    GetWifiConfig,
    #[strum(serialize = "getDevInfo")]
    GetDevInfo,
    #[strum(serialize = "getPilot")]
    GetPilot,
    #[strum(serialize = "setPilot")]
    SetPilot,
    #[strum(serialize = "setState")]
    SetState,
    #[strum(serialize = "setDevInfo")]
    SetDevInfo,
    #[strum(serialize = "setSchd")]
    SetSchd,
    #[strum(serialize = "setSchdPset")]
    SetSchdPset,
    #[strum(serialize = "setWifiConfig")]
    SetWifiConfig,
    #[strum(serialize = "setFavs")]
    SetFavs,
    #[strum(serialize = "reset")]
    Reset,
    #[strum(serialize = "reboot")]
    Reboot,
    #[strum(serialize = "syncPilot")]
    SyncPilot,
    #[strum(serialize = "syncUserConfig")]
    SyncUserConfig,
    #[strum(serialize = "syncSchdPset")]
    SyncSchdPset,
    #[strum(serialize = "syncBroadcastPilot")]
    SyncBroadcastPilot,
    #[strum(serialize = "syncSystemConfig")]
    SyncSystemConfig,
    #[strum(serialize = "syncConfig")]
    SyncConfig,
    #[strum(serialize = "syncAlarm")]
    SyncAlarm,
    #[strum(serialize = "pulse")]
    Pulse,
    #[strum(serialize = "registration")]
    Registration,
}

// TODO provide data structures with Options for ALL potential result keys, method keys, etc to give to the deserializer

#[derive(Debug, Clone)]
pub struct Fingerprint {
    pub device: Option<MacAddr6>,
    pub id: Option<i32>,
}

impl Fingerprint {
    pub fn new() -> Result<Self, QueryError> {
        let mac = MacAddr6::from(
            match get_mac_address() {
                Ok(addr) => Ok(addr),
                Err(_) => Err(QueryError::Serialization(
                    SerializationError::MacAddressError,
                )),
            }?
            .ok_or(QueryError::Serialization(
                SerializationError::MacAddressError,
            ))?
            .bytes(),
        );

        Ok(Self {
            device: Some(mac),
            id: Some(thread_rng().gen()),
        })
    }
}

#[derive(Debug, Clone)]
pub struct Target {
    pub name: String,
    pub address: IpAddr,
    pub device: MacAddr6,
}

impl Target {
    pub fn new(address: IpAddr, mac: MacAddr6, name: Option<String>) -> Self {
        Self {
            name: if let Some(good_name) = name {
                good_name
            } else {
                mac.to_string()
            },
            address: address,
            device: mac,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct RPCErrorData {
    pub code: i32,
    pub message: String,
    pub data: Option<Box<RawValue>>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RPCError {
    pub method: Option<String>,
    pub env: String,
    pub error: RPCErrorData,
    pub id: Option<i32>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RPCResult {
    pub method: String,
    pub env: String,
    pub result: Option<Value>,
    pub id: Option<i32>,
}

impl RPCResult {
    pub fn to_wizres(self: Self, mac: Option<MacAddr6>) -> Result<WizRPCResponse, QueryError> {
        Ok(WizRPCResponse {
            method: if let Ok(method_name) = MethodNames::from_str(self.method.as_str()) {
                method_name
            } else {
                return Err(QueryError::Serialization(
                    SerializationError::MethodNameDeserialization,
                ));
            },
            result: if let Some(res) = self.result {
                match Value::from_str(res.to_string().as_str()) {
                    Ok(val) => Some(val),
                    Err(_) => {
                        return Err(QueryError::Serialization(
                            SerializationError::ValueDeserialization,
                        ))
                    }
                }
            } else {
                None
            },
            fingerprint: if let Some(id) = self.id {
                Some(Fingerprint {
                    device: mac,
                    id: Some(id),
                })
            } else {
                None
            },
        })
    }
}

#[derive(Clone, Debug, Deserialize, Display)]
#[serde(untagged)]
pub enum RPCResponse {
    #[strum(serialize = "result")]
    Ok(RPCResult),
    #[strum(serialize = "error")]
    Err(RPCError),
}

#[derive(Clone, Debug, Deserialize)]
pub struct RPCRequest {
    pub method: String,
    pub params: Option<Box<RawValue>>,
    pub id: Option<i32>,
}

impl RPCRequest {
    pub fn new(method: MethodNames, params: Option<Value>) -> Self {
        // TODO add error handling
        Self {
            method: method.to_string(),
            params: if let Some(parameters) = params {
                Some(RawValue::from_string(parameters.to_string()).unwrap())
            } else {
                None
            },
            id: Some(thread_rng().gen()),
        }
    }

    pub fn from_wizreq(request: WizRPCRequest) -> Self {
        Self::new(request.method, request.params)
    }
}

pub enum WizEnv {
    Pro,
    Unknown,
}

impl WizEnv {
    fn from_string(env: String) -> Self {
        match env.as_str() {
            "pro" => Self::Pro,
            _ => Self::Unknown,
        }
    }
}

pub struct WizRPCRequest {
    pub target: Target,
    pub method: MethodNames,
    pub params: Option<Value>,
}

impl WizRPCRequest {
    pub fn new(address: IpAddr, method: MethodNames, params: Option<Value>) -> Self {
        todo!()
    }
    pub fn to_raw(self: Self) -> Box<[u8]> {
        todo!()
    }
}

pub struct WizRPCResponse {
    pub method: MethodNames,
    pub result: Option<Value>,
    pub fingerprint: Option<Fingerprint>,
}

impl WizRPCResponse {
    pub fn from_rpc_result(result: RPCResult) -> Self {
        todo!()
    }
}
