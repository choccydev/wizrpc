//! Wiz RPC
//! - Todo -

pub use model::Fingerprint;
pub use model::Method;
pub use model::RequestParamsFieldType as Param;
pub use model::Target;
pub use model::WizRPCRequest as Request;
pub use model::WizRPCResponse as Response;
pub use networking::Client;

/// Errors
/// - Todo -
pub mod error;

mod model;
mod networking;
#[cfg(test)]
mod tests;
