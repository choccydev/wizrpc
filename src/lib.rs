pub mod error;
mod model;
mod networking;
#[cfg(test)]
mod tests;

pub use model::{
    MethodNames as Method, RequestParamsFieldType as Param, WizRPCRequest as Request,
    WizRPCResponse as Response,
};
pub use networking::Client;
