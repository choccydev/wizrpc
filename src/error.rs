use crate::model::RPCError;
use crate::model::Target;
use serde_json::Value;
use socket2::Socket;
use std::sync::{PoisonError, RwLockReadGuard, RwLockWriteGuard};
use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum WizNetError {
    #[error("Invalid JSON was received by the server. An error occurred on the server while parsing the JSON text:  {data:#?}")]
    Parse { data: Option<Value> },
    #[error("The JSON sent is not a valid Request object: {data:#?}")]
    InvalidRequest { data: Option<Value> },
    #[error("Invalid method parameter(s): {data:#?}")]
    InvalidParameters { data: Option<Value> },
    #[error("The method does not exist / is not available: {data:#?}")]
    MethodNotFound { data: Option<Value> },
    #[error("Internal JSON-RPC error: {data:#?}")]
    Internal { data: Option<Value> },
    #[error("{message:?}: {data:#?}")]
    Server {
        message: String,
        data: Option<Value>,
    },
    #[error("Unknown error; {message:?}: {data:#?}")]
    Unknown {
        message: String,
        data: Option<Value>,
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
    #[error(transparent)]
    Sync(SyncError),
}
#[derive(Error, Debug, Clone, Copy)]
pub enum SerializationError {
    #[error("Failed getting MAC address of device")]
    MacAddressError,
    #[error("Failed parsing provided address")]
    IPAddrError,
    #[error("Failed deserializating returned Method Name")]
    MethodNameDeserialization,
    #[error("Failed deserializating value from string")]
    ValueDeserialization,
    #[error("Failed converting bytes to str")]
    StrFromBytes,
    #[error("Falied to find a device with a matching name")]
    NameNotFound,
    #[error("Serde error")]
    Serde,
}

#[derive(Error, Debug, Clone, Copy)]
pub enum SyncError {
    #[error("Mutex lock failed")]
    MutexLock,
}

impl From<std::io::Error> for QueryError {
    fn from(err: std::io::Error) -> Self {
        QueryError::Network(err)
    }
}

impl From<serde_json::Error> for QueryError {
    fn from(_: serde_json::Error) -> Self {
        QueryError::Serialization(SerializationError::Serde)
    }
}

impl From<retry::Error<std::io::Error>> for QueryError {
    fn from(err: retry::Error<std::io::Error>) -> Self {
        QueryError::Network(err.error)
    }
}

impl From<retry::Error<PoisonError<RwLockWriteGuard<'_, Socket>>>> for QueryError {
    fn from(_: retry::Error<PoisonError<RwLockWriteGuard<'_, Socket>>>) -> Self {
        QueryError::Sync(SyncError::MutexLock)
    }
}

impl From<PoisonError<RwLockWriteGuard<'_, Vec<Target>>>> for QueryError {
    fn from(_: PoisonError<RwLockWriteGuard<'_, Vec<Target>>>) -> Self {
        QueryError::Sync(SyncError::MutexLock)
    }
}

impl From<PoisonError<RwLockWriteGuard<'_, Vec<String>>>> for QueryError {
    fn from(_: PoisonError<RwLockWriteGuard<'_, Vec<String>>>) -> Self {
        QueryError::Sync(SyncError::MutexLock)
    }
}

impl From<retry::Error<PoisonError<RwLockReadGuard<'_, Socket>>>> for QueryError {
    fn from(_: retry::Error<PoisonError<RwLockReadGuard<'_, Socket>>>) -> Self {
        QueryError::Sync(SyncError::MutexLock)
    }
}

impl From<PoisonError<RwLockReadGuard<'_, Vec<Target>>>> for QueryError {
    fn from(_: PoisonError<RwLockReadGuard<'_, Vec<Target>>>) -> Self {
        QueryError::Sync(SyncError::MutexLock)
    }
}

impl From<PoisonError<RwLockReadGuard<'_, Vec<String>>>> for QueryError {
    fn from(_: PoisonError<RwLockReadGuard<'_, Vec<String>>>) -> Self {
        QueryError::Sync(SyncError::MutexLock)
    }
}
