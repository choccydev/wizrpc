pub mod error;
pub mod model;

pub async fn send(
    request: model::WizRPCRequest,
) -> Result<model::WizRPCResponse, error::QueryError> {
    todo!()
}
