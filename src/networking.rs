use crate::error::{QueryError, WizNetError};
use crate::model::RPCResponse;
use dns_lookup::lookup_host;
use http::Uri;
use lazy_static::lazy_static;
use std::str::from_utf8;
use tokio::net::UdpSocket;

lazy_static! {
    pub static ref TARGET_PORT: &'static str = "38899";
}

pub fn get_ip(address: Uri) -> Result<String, QueryError> {
    // TODO add better error handling
    let host = address.host().unwrap();
    let ip = match lookup_host(host) {
        Ok(ips) => ips,
        Err(err) => return Err(QueryError::Network(err)),
    };

    Ok(ip[0].to_string())
}

pub async fn ping(address: Uri, sock: UdpSocket) -> Result<bool, QueryError> {
    let receive = raw_send(get_ip(address)?, "0".as_bytes(), sock).await?;
    if receive.len() > 0 {
        return Ok(true);
    }
    Err(QueryError::RPC(WizNetError::NotAWizDevice))
}

pub fn parse_raw(data: Vec<u8>) -> Result<RPCResponse, QueryError> {
    let serde_result: Result<RPCResponse, _> = serde_json::from_str(match from_utf8(&data) {
        Ok(text) => text,
        Err(_) => {
            return Err(QueryError::Serialization(
                crate::error::SerializationError::StrFromBytes,
            ))
        }
    });
    match serde_result {
        Err(err) => Err(QueryError::Serialization(
            crate::error::SerializationError::ValueDeserialization,
        )),
        Ok(deserialized) => Ok(deserialized),
    }
}

pub async fn raw_send(ip: String, data: &[u8], sock: UdpSocket) -> Result<Vec<u8>, QueryError> {
    let mut addr = String::new();
    addr.push_str(ip.as_str());
    addr.push(':');
    addr.push_str(&TARGET_PORT);

    let mut buffer = [0; 1024];

    match sock.connect(addr).await {
        Ok(_) => {}
        Err(err) => return Err(QueryError::Network(err)),
    }

    match sock.send(data).await {
        Ok(_) => {}
        Err(err) => return Err(QueryError::Network(err)),
    }

    let resultLength = match sock.recv(&mut buffer).await {
        Ok(len) => len,
        Err(err) => return Err(QueryError::Network(err)),
    };

    if resultLength > 0 {
        Ok(Vec::from(buffer))
    } else {
        return Err(QueryError::Network(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Ningún dato recibido",
        )));
    }
}
