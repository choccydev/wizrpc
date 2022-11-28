pub mod error;
mod model;
mod networking;
#[cfg(test)]
mod tests;

pub use model::{MethodNames, WizRPCRequest, WizRPCResponse};
pub use networking::Client;
