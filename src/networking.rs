use crate::error::{QueryError, SerializationError, WizNetError};
use crate::model::{RPCResponse, Target, WizRPCRequest};
use crate::WizRPCResponse;
use dns_lookup::lookup_host;
use lazy_static::lazy_static;
use macaddr::MacAddr6;
use qscan::{QSPrintMode, QScanPingState, QScanResult, QScanType, QScanner};
use serde_json::Value;
use std::net::{IpAddr, SocketAddr};
use std::str::{from_utf8, FromStr};
use tokio::net::UdpSocket;
use tokio::runtime::Runtime;
use url::Host;

lazy_static! {
    pub static ref TARGET_PORT: &'static str = "38899";
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

pub async fn raw_send(ip: String, data: &[u8], sock: &UdpSocket) -> Result<Vec<u8>, QueryError> {
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

pub async fn scan(host: IpAddr, timeout: Option<u16>) -> Result<Vec<IpAddr>, QueryError> {
    if host.is_ipv4() {
        let host_string = host.to_string();
        let mut space = String::new();
        let mut host_parts = host_string.split('.');
        space.push_str(host_parts.next().ok_or(QueryError::Serialization(
            crate::error::SerializationError::IPAddrError,
        ))?);
        space.push('.');
        space.push_str(host_parts.next().ok_or(QueryError::Serialization(
            crate::error::SerializationError::IPAddrError,
        ))?);
        space.push('.');
        space.push_str(host_parts.next().ok_or(QueryError::Serialization(
            crate::error::SerializationError::IPAddrError,
        ))?);
        space.push('.');

        let addr_range = 0..255;

        let mut addresses = String::new();

        for target in addr_range {
            if target != 0 {
                addresses.push(',');
            }
            let mut this_addr = space.clone();
            this_addr.push_str(target.to_string().as_str());
            addresses.push_str(this_addr.as_str());
        }

        let mut scanner = QScanner::new(addresses.as_str(), "");
        scanner.set_batch(1000);
        scanner.set_timeout_ms(if let Some(timeout) = timeout {
            timeout.into()
        } else {
            50
        });

        scanner.set_ping_interval_ms(if let Some(timeout) = timeout {
            (timeout / 4).into()
        } else {
            10
        });
        scanner.set_ntries(3);
        scanner.set_scan_type(QScanType::Ping);
        scanner.set_print_mode(QSPrintMode::NonRealTime);

        // TODO find a way to communicate this
        let res: &Vec<QScanResult> = Runtime::new().unwrap().block_on(scanner.scan_ping());

        let mut ips_up: Vec<IpAddr> = Vec::new();

        for r in res {
            if let QScanResult::Ping(pr) = r {
                match pr.state {
                    QScanPingState::Up => {
                        ips_up.push(pr.target);
                    }
                    _ => {}
                }
            }
        }

        Ok(ips_up)
    } else {
        Err(QueryError::Network(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Doesn't support IPv6 yet",
        )))
    }
}

#[derive(Debug)]
pub struct Client {
    pub names: Vec<String>,
    pub devices: Vec<Target>,
    pub sock: UdpSocket,
    pub timeout: Option<u16>,
    pub address: SocketAddr,
}

impl Client {
    pub async fn new(bind_addr: SocketAddr, timeout: Option<u16>) -> Result<Self, QueryError> {
        Ok(Client {
            names: Vec::new(),
            devices: Vec::new(),
            sock: match UdpSocket::bind(bind_addr).await {
                Ok(sock) => sock,
                Err(err) => return Err(QueryError::Network(err)),
            },
            timeout: timeout,
            address: bind_addr,
        })
    }
    pub async fn init(self: Self) -> Result<Self, QueryError> {
        Ok(self.discover().await?.update_names())
    }

    pub async fn send(
        request: WizRPCRequest,
        device: String,
    ) -> Result<WizRPCResponse, QueryError> {
        todo!()
    }

    pub async fn discover(mut self: Self) -> Result<Self, QueryError> {
        let valid_ips = scan(self.address.ip(), self.timeout).await?;

        let mut valid_devices = Vec::new();

        for ip in valid_ips {
            if self
                .ping(
                    match Host::parse(&self.sock.local_addr().unwrap().ip().to_string()) {
                        Ok(host) => host,
                        Err(err) => {
                            return Err(QueryError::Serialization(SerializationError::IPAddrError))
                        }
                    },
                )
                .await?
            {
                valid_devices.push(ip);
            }
        }

        for device in valid_devices {
            let host = match Host::parse(device.to_string().as_str()) {
                Ok(host) => host,
                Err(_) => return Err(QueryError::Serialization(SerializationError::IPAddrError)),
            };
            let mac = self.get_mac(host.clone()).await?;
            self.devices.push(Target::new(host, mac, None));
        }

        Ok(self)
    }

    fn update_names(mut self: Self) -> Self {
        self.names = self
            .devices
            .iter()
            .map(|target: &Target| target.name.clone())
            .collect();
        self
    }

    pub fn get_ip(self: &Self, address: Host) -> Result<String, QueryError> {
        let ip = match lookup_host(address.to_string().as_str()) {
            Ok(ips) => ips,
            Err(err) => return Err(QueryError::Network(err)),
        };

        Ok(ip[0].to_string())
    }

    pub async fn get_mac(self: &Self, address: Host) -> Result<MacAddr6, QueryError> {
        let data = parse_raw(
            raw_send(
                self.get_ip(address)?,
                String::from("{\"method\":\"getSystemConfig\"}").as_bytes(),
                &self.sock,
            )
            .await?,
        )?;

        match data {
            RPCResponse::Ok(res) => {
                let result = if let Some(res_data) = res.result {
                    match Value::from_str(res_data.to_string().as_str()) {
                        Ok(val) => val,
                        Err(_) => {
                            return Err(QueryError::Serialization(
                                SerializationError::ValueDeserialization,
                            ))
                        }
                    }
                } else {
                    return Err(QueryError::Network(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "Ningún dato recibido",
                    )));
                };

                let mac = match result.as_object().unwrap().get("mac") {
                    Some(macaddr) => macaddr.as_str().ok_or(QueryError::Serialization(
                        SerializationError::ValueDeserialization,
                    ))?,
                    None => {
                        return Err(QueryError::Network(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "Ningún dato recibido",
                        )))
                    }
                };

                match MacAddr6::from_str(mac) {
                    Ok(macaddr) => Ok(macaddr),
                    Err(_) => Err(QueryError::Serialization(
                        SerializationError::MacAddressError,
                    )),
                }
            }
            RPCResponse::Err(error) => {
                return Err(QueryError::RPC(WizNetError::from_rpc_error(error)))
            }
        }
    }

    pub async fn ping(self: &Self, address: Host) -> Result<bool, QueryError> {
        let receive = raw_send(self.get_ip(address)?, "0".as_bytes(), &self.sock).await?;
        if receive.len() > 0 {
            return Ok(true);
        }
        Err(QueryError::RPC(WizNetError::NotAWizDevice))
    }
}
