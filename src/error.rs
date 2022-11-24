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
    #[error("Target is not a wiz device or returned bogus data")]
    NotAWizDevice,
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
    #[error(transparent)]
    Serialization(SerializationError),
    #[error(transparent)]
    RPC(WizNetError),
    #[error(transparent)]
    Network(std::io::Error),
}
#[derive(Error, Debug)]
pub enum SerializationError {
    #[error("Failed getting MAC address of device")]
    MacAddressError,
    #[error("Failed deserializating returned Method Name")]
    MethodNameDeserialization,
    #[error("Failed deserializating value from string")]
    ValueDeserialization,
    #[error("Failed converting bytes to str")]
    StrFromBytes,
}
