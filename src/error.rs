use crate::model::RPCError;
use serde_json::Value;
use std::io::ErrorKind;
use std::net::AddrParseError;
use std::sync::PoisonError;
use surge_ping::SurgeError;
use thiserror::Error;
use tokio::sync::oneshot::error::RecvError;
use tokio::sync::TryLockError;
use tokio::sync::{RwLockReadGuard, RwLockWriteGuard};
use tokio::task::JoinError;
use tokio::time::error::Elapsed;

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
    NotAWizTarget,
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
    #[error("Socket is busy")]
    WouldBlock,
    #[error("No Target device selected")]
    NoTarget,
    #[error("No idea what happened. File a bug report.")]
    Unknown,
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
    #[error("Mutex lock failed")]
    RwLock,
    #[error("Joining task failed")]
    JoinTask,
    #[error("ID missing on history. Consistency error?")]
    BadId,
    #[error("Error receiving data in OneShot channel.")]
    ChannelReceive,
}

impl From<RecvError> for QueryError {
    fn from(_err: RecvError) -> Self {
        QueryError::Sync(SyncError::ChannelReceive)
    }
}

impl From<TryLockError> for QueryError {
    fn from(_: TryLockError) -> Self {
        QueryError::Sync(SyncError::RwLock)
    }
}

impl From<JoinError> for QueryError {
    fn from(_: JoinError) -> Self {
        QueryError::Sync(SyncError::JoinTask)
    }
}

impl From<AddrParseError> for QueryError {
    fn from(_: AddrParseError) -> Self {
        QueryError::Serialization(SerializationError::IPAddrError)
    }
}

impl From<SurgeError> for QueryError {
    fn from(_: SurgeError) -> Self {
        // TODO this is garbage please flesh this out better
        QueryError::Network(ErrorKind::Other.into())
    }
}

impl From<Elapsed> for QueryError {
    fn from(_: Elapsed) -> Self {
        // TODO this is garbage please flesh this out better
        QueryError::Network(ErrorKind::TimedOut.into())
    }
}

impl From<std::io::ErrorKind> for QueryError {
    fn from(err: std::io::ErrorKind) -> Self {
        match err {
            ErrorKind::WouldBlock => QueryError::WouldBlock,
            _ => QueryError::Network(err.into()),
        }
    }
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

impl<T> From<retry::Error<PoisonError<RwLockWriteGuard<'_, T>>>> for QueryError {
    fn from(_: retry::Error<PoisonError<RwLockWriteGuard<'_, T>>>) -> Self {
        QueryError::Sync(SyncError::MutexLock)
    }
}

impl<T> From<PoisonError<RwLockWriteGuard<'_, T>>> for QueryError {
    fn from(_: PoisonError<RwLockWriteGuard<'_, T>>) -> Self {
        QueryError::Sync(SyncError::MutexLock)
    }
}

impl<T> From<retry::Error<PoisonError<RwLockReadGuard<'_, T>>>> for QueryError {
    fn from(_: retry::Error<PoisonError<RwLockReadGuard<'_, T>>>) -> Self {
        QueryError::Sync(SyncError::MutexLock)
    }
}

impl<T> From<PoisonError<RwLockReadGuard<'_, T>>> for QueryError {
    fn from(_: PoisonError<RwLockReadGuard<'_, T>>) -> Self {
        QueryError::Sync(SyncError::MutexLock)
    }
}
