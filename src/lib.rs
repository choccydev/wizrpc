pub mod error;
mod model;
mod networking;
pub use model::{MethodNames, WizRPCRequest, WizRPCResponse};

pub use networking::Client;
