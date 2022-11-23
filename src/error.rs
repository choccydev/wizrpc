use super::model::RPCError;
use crate::model::{ParamsFilter, RPCErrorData, RequestParams};
use serde_json::value::RawValue;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum WizNetError {
    #[error("Invalid JSON was received by the server. An error occurred on the server while parsing the JSON text:  {data:#?}")]
    Parse { data: Option<Box<RawValue>> },
    #[error("The JSON sent is not a valid Request object: {data:#?}")]
    InvalidRequest { data: Option<Box<RawValue>> },
    #[error("Invalid method parameter(s): {data:#?}")]
    InvalidParameters { data: Option<Box<RawValue>> },
    #[error("The method does not exist / is not available: {data:#?}")]
    MethodNotFound { data: Option<Box<RawValue>> },
    #[error("Internal JSON-RPC error: {data:#?}")]
    Internal { data: Option<Box<RawValue>> },
    #[error("{message:?}: {data:#?}")]
    Server {
        message: String,
        data: Option<Box<RawValue>>,
    },
    #[error("Unknown error; {message:?}: {data:#?}")]
    Unknown {
        message: String,
        data: Option<Box<RawValue>>,
    },
}

impl WizNetError {
    pub fn from_rpc_error(rpcerror: RPCError) -> Self {
        let code = rpcerror.error.code;

        match code {
            -32700 => Self::Parse {
                data: rpcerror.error.data,
            },
            -32600 => Self::InvalidRequest {
                data: rpcerror.error.data,
            },
            -32601 => Self::MethodNotFound {
                data: rpcerror.error.data,
            },
            -32602 => Self::InvalidParameters {
                data: rpcerror.error.data,
            },
            -32603 => Self::Internal {
                data: rpcerror.error.data,
            },
            _ => {
                if -32000 <= code && code <= -32099 {
                    Self::Server {
                        message: rpcerror.error.message,
                        data: rpcerror.error.data,
                    }
                } else {
                    Self::Unknown {
                        message: rpcerror.error.message,
                        data: rpcerror.error.data,
                    }
                }
            }
        }
    }
}

#[derive(Error, Debug)]
pub enum QueryError {
    #[error("Not all keys required by the method are present on the parameters given.\n Required Parameters: {filter:#?}\n Given parameters: {params:#?}")]
    FilterError {
        params: RequestParams,
        filter: ParamsFilter,
    },
    #[error(transparent)]
    Serialization(SerializationError),
    #[error(transparent)]
    RPC(WizNetError),
}
#[derive(Error, Debug)]
pub enum SerializationError {
    #[error("Failed serializing object into raw JSON-RPC: {params:#?}")]
    ValueError { params: RequestParams },
    #[error("Failed getting MAC address of device")]
    MacAddressError,
}
