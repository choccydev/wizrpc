use crate::error::{QueryError, SerializationError};
use lazy_static::lazy_static;
use macaddr::MacAddr6;
use rand::{thread_rng, Rng};
use serde::Deserialize;
use serde::Serialize;
use serde_json::Number;
use serde_json::Value;
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

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseParams {
    pub mac: Option<String>,
    pub home_id: Option<Number>,
    pub room_id: Option<Number>,
    pub group_id: Option<Number>,
    pub rgn: Option<String>,
    pub module_name: Option<String>,
    pub fw_version: Option<String>,
    pub drv_conf: Option<Vec<Number>>,
    pub ping: Option<Number>,
    pub fade_in: Option<Number>,
    pub fade_out: Option<Number>,
    pub fade_night: Option<Number>,
    pub dft_dim: Option<Number>,
    pub po: Option<bool>,
    pub min_dimming: Option<Number>,
    pub rssi: Option<Number>,
    pub src: Option<String>,
    pub state: Option<bool>,
    pub scene_id: Option<Number>,
    pub temp: Option<Number>,
    pub dimming: Option<Number>,
    pub success: Option<bool>,
}

// TODO provide data structures with Options for ALL potential result keys, method keys, etc to give to the deserializer

#[derive(Debug, Clone)]
pub struct Fingerprint {
    pub device: Option<MacAddr6>,
    pub id: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct Target {
    pub name: String,
    pub address: IpAddr,
    pub mac: MacAddr6,
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
            mac,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct RPCErrorData {
    pub code: i32,
    pub message: String,
    pub data: Option<Value>,
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
    pub result: Option<ResponseParams>,
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
                Some(res)
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

#[derive(Clone, Debug, Serialize)]
pub struct RPCRequest {
    pub method: String,
    pub params: Option<Value>,
    pub id: Option<i32>,
}

impl RPCRequest {
    pub fn new(method: MethodNames, params: Option<Value>) -> Self {
        // TODO add error handling
        Self {
            method: method.to_string(),
            params: if let Some(parameters) = params {
                Some(parameters)
            } else {
                None
            },
            id: Some(thread_rng().gen()),
        }
    }
}

pub struct WizRPCRequest {
    pub device: String,
    pub method: MethodNames,
    pub params: Option<Value>,
}

impl WizRPCRequest {
    pub fn new(device: String, method: MethodNames, params: Option<Value>) -> Self {
        WizRPCRequest {
            device: device,
            method: method,
            params: params,
        }
    }
    pub fn to_raw(self: Self) -> Result<Vec<u8>, QueryError> {
        let request = RPCRequest::new(self.method, self.params);
        Ok(Vec::from(serde_json::to_string(&request)?.as_bytes()))
    }
}

pub struct WizRPCResponse {
    pub method: MethodNames,
    pub result: Option<ResponseParams>,
    pub fingerprint: Option<Fingerprint>,
}
