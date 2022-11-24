pub mod error;
mod model;
mod networking;
pub use model::{MethodNames, WizRPCRequest, WizRPCResponse};
use tokio::net::UdpSocket;

pub struct Client {
    pub names: Vec<String>,
    pub devices: Vec<model::Target>,
    pub sock: UdpSocket,
}

impl Client {
    pub fn init(sock: UdpSocket) -> Self {
        todo!()
    }

    pub async fn send(
        request: model::WizRPCRequest,
        device: String,
    ) -> Result<model::WizRPCResponse, error::QueryError> {
        todo!()
    }

    pub async fn discover() {
        todo!()
    }
}
