use crate::model::{IDOpts, ParamsFilter, RPCErrorData, RequestParams};

use super::model::RPCError;
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
    pub fn to_rpc_error_data(self: Self) -> RPCErrorData {
        match self {
            Self::Parse { data } => RPCErrorData {
                code: -32700,
                message: "Parse error".to_string(),
                data: data,
            },
            Self::InvalidRequest { data } => RPCErrorData {
                code: -32600,
                message: "Invalid Request".to_string(),
                data: data,
            },
            Self::MethodNotFound { data } => RPCErrorData {
                code: -32601,
                message: "Method not found".to_string(),
                data: data,
            },
            Self::InvalidParameters { data } => RPCErrorData {
                code: -32602,
                message: "Invalid params".to_string(),
                data: data,
            },
            Self::Internal { data } => RPCErrorData {
                code: -32603,
                message: "Internal error".to_string(),
                data: data,
            },
            Self::Server { data, message } => RPCErrorData {
                code: -32000,
                message: message,
                data: data,
            },
            Self::Unknown { message, data } => RPCErrorData {
                code: -30000,
                message: message,
                data: data,
            },
        }
    }

    pub fn to_rpc_error(self: Self, id: IDOpts) -> RPCError {
        RPCError::from_wiz_error(self, id)
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
}
