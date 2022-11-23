use super::error::{QueryError, SerializationError, WizNetError};
use lazy_static::lazy_static;
use mac_address::get_mac_address;
use macaddr::MacAddr6;
use optional_struct::OptionalStruct;
use serde::{Deserialize, Serialize};
use serde_json::{value::RawValue, Number, Value};
use std::collections::HashMap;
use std::str::FromStr;
use std::string::ToString;
use strum_macros::Display;
use uuid::Uuid;

lazy_static! {}

#[derive(Debug, Clone, Copy, Display)]
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
    #[strum(serialize = "syncUserConfig")]
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

#[derive(Debug, Clone)]
pub struct Fingerprint {
    prefix: String,
    sender: MacAddr6,
    id: Uuid,
}

impl Fingerprint {
    pub fn from_string(string: String) -> Self {
        // TODO add error handling
        let vec_data = Vec::from(string.as_bytes());
        let mut iter_first = vec_data.split(|character| character.to_string() == ':'.to_string());
        let prefix = String::from_utf8(Vec::from(iter_first.next().unwrap())).unwrap();

        let sender = MacAddr6::from_str(
            String::from_utf8(Vec::from(iter_first.next().unwrap()))
                .unwrap()
                .as_str(),
        )
        .unwrap();

        let id = Uuid::parse_str(
            String::from_utf8(Vec::from(iter_first.next().unwrap()))
                .unwrap()
                .as_str(),
        )
        .unwrap();

        Self {
            prefix: prefix,
            sender: sender,
            id: id,
        }
    }

    pub fn to_string(self: Self) -> String {
        let mut text = String::new();
        text.push_str(self.prefix.as_str());
        text.push(':');
        text.push_str(format!("{:-}", self.sender).as_str());
        text.push(':');
        text.push_str(self.id.as_simple().to_string().as_str());
        text
    }

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
            prefix: "WizRPC".to_string(),
            sender: mac,
            id: Uuid::new_v4(),
        })
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
    pub method: String,
    pub env: String,
    pub error: RPCErrorData,
    pub id: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RPCResult {
    pub method: String,
    pub env: String,
    pub result: Box<RawValue>,
    pub id: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RPCRequest {
    pub method: String,
    pub params: Option<Box<RawValue>>,
    pub id: Option<String>,
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
            id: Some(Fingerprint::new().unwrap().to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamsFilter {
    pub state: bool,
    pub speed: bool,
    pub ratio: bool,
    pub scene: bool,
    pub brightness: bool,
}

impl ParamsFilter {
    fn to_map(self: Self) -> HashMap<String, bool> {
        // TODO Error handling
        serde_json::from_value(serde_json::to_value(self).unwrap()).unwrap()
    }
}

#[derive(Debug, Clone, OptionalStruct, Serialize, Deserialize)]
#[optional_derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, OptionalStruct, Serialize, Deserialize)]
#[optional_derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestParams {
    pub state: Option<bool>,
    pub speed: Option<Number>,
    pub ratio: Option<Number>,
    pub scene: Option<Number>,
    pub brightness: Option<Number>,
    // TODO missing params
}

impl RequestParams {
    pub fn new() -> Self {
        RequestParams {
            state: None,
            speed: None,
            ratio: None,
            scene: None,
            brightness: None,
        }
    }

    pub fn to_jsonrpc(self: Self) -> Result<Value, QueryError> {
        match serde_json::to_value(self.clone()) {
            Err(_) => Err(QueryError::Serialization(SerializationError::ValueError {
                params: self,
            })),
            Ok(value) => Ok(value),
        }
    }

    fn to_map(self: Self) -> HashMap<String, Value> {
        // TODO Add error handling
        serde_json::from_value(serde_json::to_value(self).unwrap()).unwrap()
    }

    fn from_map(map: HashMap<String, Value>) -> OptionalRequestParams {
        // TODO Add error handling
        serde_json::from_value(serde_json::to_value(map).unwrap()).unwrap()
    }

    pub fn patch(mut self: Self, patch: OptionalRequestParams) {
        self.apply_options(patch)
    }

    //? This function is sus, gotta test it well
    pub fn filter(self: Self, filter: ParamsFilter) -> Result<Self, QueryError> {
        let self_bind = self.clone();
        let filter_bind = filter.clone();
        let mut new_params = Self::new();
        let mut new_params_map: HashMap<String, Value> = HashMap::new();

        let self_map = self.to_map();
        let filter_map = filter.to_map();

        for (filter_param, is_expected) in filter_map {
            if is_expected {
                let value = self_map
                    .get(&filter_param)
                    .ok_or(QueryError::FilterError {
                        params: self_bind.clone(),
                        filter: filter_bind.clone(),
                    })?
                    .clone();
                new_params_map.insert(filter_param, value);
            }
        }

        new_params.apply_options(Self::from_map(new_params_map));

        Ok(new_params)
    }
}
