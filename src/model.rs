use std::collections::HashMap;

use crate::errors::{QueryError, SerializationError};

use super::errors::WizNetError;
use lazy_static::lazy_static;
use optional_struct::OptionalStruct;
use serde::{Deserialize, Serialize};
use serde_json::{value::RawValue, Number, Value};

lazy_static! {
    #[derive(Clone, Debug, Deserialize, Serialize, Copy)]
    pub static ref JSONRPCVERSION: String = "2.0".to_string();
}

#[derive(Clone, Debug, Deserialize)]
pub struct RPCErrorData {
    pub code: i32,
    pub message: String,
    pub data: Option<Box<RawValue>>,
}

#[derive(Clone, Debug, Deserialize)]
pub enum IDOpts {
    Num(Number),
    Str(String),
}

#[derive(Clone, Debug, Deserialize)]
pub struct RPCError {
    pub jsonrpc: JSONRPCVERSION,
    pub error: RPCErrorData,
    pub id: IDOpts,
}

impl RPCError {
    pub fn new(error: RPCErrorData, id: IDOpts) -> Self {
        Self {
            jsonrpc: JSONRPCVERSION,
            error: error,
            id: id,
        }
    }
    pub fn from_wiz_error(error: WizNetError, id: IDOpts) -> Self {
        Self::new(error.to_rpc_error_data(), id)
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct RPCResult {
    pub jsonrpc: String,
    pub result: Box<RawValue>,
    pub id: IDOpts,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RPCRequest {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<Box<RawValue>>,
    pub id: IDOpts,
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
