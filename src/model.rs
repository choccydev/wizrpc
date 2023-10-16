//! owo

use crate::error::{QueryError, SerializationError};
use crate::Param;
use debug_helper::impl_debug_for_enum;
use field_types::FieldType;
use macaddr::MacAddr6;
use optional_struct::{optional_struct, Applyable};
use rand::{thread_rng, Rng};
use serde::Deserialize;
use serde::Serialize;
use serde_json::Number;
use serde_json::Value;
use std::string::ToString;
use std::{fmt::Debug, net::IpAddr, str::FromStr};
use strum_macros::{Display, EnumString};

#[optional_struct]
#[derive(Debug, Clone, Serialize, FieldType)]
pub struct RequestParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ratio: Option<Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brightness: Option<Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temp: Option<Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r: Option<Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub g: Option<Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub b: Option<Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub c: Option<Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub w: Option<Number>,
    #[serde(rename = "sceneId", skip_serializing_if = "Option::is_none")]
    pub scene_id: Option<Number>,
    // TODO possibly missing params
}

impl RequestParams {
    fn empty() -> Self {
        RequestParams {
            state: None,
            speed: None,
            ratio: None,
            brightness: None,
            temp: None,
            r: None,
            g: None,
            b: None,
            c: None,
            w: None,
            scene_id: None,
        }
    }
}

impl OptionalRequestParams {
    fn empty() -> Self {
        OptionalRequestParams {
            state: None,
            speed: None,
            ratio: None,
            brightness: None,
            temp: None,
            r: None,
            g: None,
            b: None,
            c: None,
            w: None,
            scene_id: None,
        }
    }
}

impl Param {
    fn to_option_struct(self: Self) -> OptionalRequestParams {
        let mut params_struct = OptionalRequestParams::empty();
        match self {
            Param::State(state) => {
                params_struct.state = state;
                params_struct
            }
            Param::Speed(speed) => {
                params_struct.speed = speed;
                params_struct
            }
            Param::Ratio(ratio) => {
                params_struct.ratio = ratio;
                params_struct
            }
            Param::Brightness(brightness) => {
                params_struct.brightness = brightness;
                params_struct
            }
            Param::Temp(temp) => {
                params_struct.temp = temp;
                params_struct
            }
            Param::R(r) => {
                params_struct.r = r;
                params_struct
            }
            Param::G(g) => {
                params_struct.g = g;
                params_struct
            }
            Param::B(b) => {
                params_struct.b = b;
                params_struct
            }
            Param::W(w) => {
                params_struct.w = w;
                params_struct
            }
            Param::C(c) => {
                params_struct.c = c;
                params_struct
            }
            Param::SceneId(scene_id) => {
                params_struct.scene_id = scene_id;
                params_struct
            }
        }
    }
}

impl Debug for Param {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        impl_debug_for_enum!(Param::{
            (State(state): (.state)), 
            (Speed(speed): (.speed)), 
            (Ratio(ratio): (.ratio)), 
            (Brightness(brightness): (.brightness)), 
            (Temp(temp): (.temp)),
            (R(r): (.r)),
            (G(g): (.g)), 
            (B(b): (.b)), 
            (W(w): (.w)),
            (C(c): (.c)),
            (SceneId(scene_id): (.scene_id))
        }, f, self);
    }
}

impl Clone for Param {
    fn clone(&self) -> Self {
        match self {
            Param::State(state) => Param::State(*state),
            Param::Speed(speed) => Param::Speed(speed.clone()),
            Param::Ratio(ratio) => Param::Ratio(ratio.clone()),
            Param::Brightness(brightness) => Param::Brightness(brightness.clone()),
            Param::Temp(temp) => Param::Temp(temp.clone()),
            Param::R(r) => Param::R(r.clone()),
            Param::G(g) => Param::G(g.clone()),
            Param::B(b) => Param::B(b.clone()),
            Param::W(w) => Param::W(w.clone()),
            Param::C(c) => Param::C(c.clone()),
            Param::SceneId(scene_id) => Param::SceneId(scene_id.clone()),
        }
    }
}

pub trait ToStruct {
    fn to_struct(self) -> RequestParams;
}

impl ToStruct for Vec<Param> {
    fn to_struct(self) -> RequestParams {
        let mut params_struct = RequestParams::empty();
        for param in self {
            param.to_option_struct().apply_to(&mut params_struct);
            //params_struct.apply_options();
        }
        params_struct
    }
}

/// All methods available, both for request and response
#[derive(Debug, Clone, Copy, Display, EnumString)]
pub enum Method {
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
    #[strum(serialize = "PING")]
    Ping,
    #[strum(serialize = "REGISTERED_DEVICE")]
    RegisteredDevice,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
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

/// Fingerprint with device MAC and return ID of a message
#[derive(Debug, Clone)]
pub struct Fingerprint {
    //pub device: Option<MacAddr6>,
    pub id: Option<u32>,
}

/// Data definition for talking with a WizConnected device
/// TODO add getModelConfig to have capabilities registered
#[derive(Debug, Clone)]
pub struct Target {
    pub address: IpAddr,
    pub mac: MacAddr6,
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
    pub id: Option<u32>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RPCResult {
    pub method: String,
    pub env: String,
    pub result: Option<ResponseParams>,
    pub id: Option<u32>,
}

impl RPCResult {
    pub fn to_wizres(self: Self, _mac: Option<MacAddr6>) -> Result<WizRPCResponse, QueryError> {
        Ok(WizRPCResponse {
            method: if let Ok(method_name) = Method::from_str(self.method.as_str()) {
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
                    //device: mac,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<RequestParams>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<u32>,
}

impl RPCRequest {
    pub fn new(method: Method, params: Option<RequestParams>) -> Self {
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

#[derive(Debug, Clone, Default)]
pub enum RequestKind {
    #[default]
    Send,
    Ping,
    Mac,
    Discovery,
}

#[derive(Debug, Clone)]
/// Serializable data for performing a request to a device
pub struct WizRPCRequest {
    pub device: Option<String>,
    pub method: Method,
    pub params: Option<Vec<Param>>,
    pub kind:  RequestKind
}

impl WizRPCRequest {
    pub fn new_with_kind(device: Option<String>, method: Method, params: Option<Vec<Param>>, kind: RequestKind) -> Self {
        WizRPCRequest {
            device,
            method,
            params,
            kind
        }
    }
    pub fn new(device: Option<String>, method: Method, params: Option<Vec<Param>>) -> Self {
        WizRPCRequest {
            device,
            method,
            params,
            kind: Default::default()
        }
    }
    pub fn to_raw(self: Self) -> Result<Vec<u8>, QueryError> {
        let params = if let Some(params_enum) = self.params {
            Some(params_enum.to_struct())
        } else {
            None
        };

        let request_raw: String;
        match self.method {
            Method::Ping => {
                request_raw = "{}".to_string();
            }
            _ => {
                let request = RPCRequest::new(self.method, params);
                request_raw = serde_json::to_string(&request)?;
            }
        }
        
        Ok(Vec::from(request_raw.as_bytes()))
    }
}

/// Deserialized response from a device
#[derive(Clone, Debug)]
pub struct WizRPCResponse {
    pub method: Method,
    pub result: Option<ResponseParams>,
    pub fingerprint: Option<Fingerprint>,
}
