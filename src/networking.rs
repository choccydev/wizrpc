use crate::error::{QueryError, SerializationError, WizNetError};
use crate::model::{RPCResponse, Target, WizRPCRequest};
use crate::WizRPCResponse;
use dns_lookup::lookup_host;
use lazy_static::lazy_static;
use macaddr::MacAddr6;
use qscan::{QSPrintMode, QScanPingState, QScanResult, QScanType, QScanner};
use serde_json::Value;
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use std::mem::MaybeUninit;
use std::net::{IpAddr, SocketAddr};
use std::str::{from_utf8, FromStr};
use std::time::Duration;
use url::Host;

lazy_static! {
    pub static ref DEFAULT_TARGET_PORT: u16 = 38899;
    pub static ref DEFAULT_SOURCE_PORT: u16 = 39900;
    pub static ref DEFAULT_BIND_ADDRESS: IpAddr = IpAddr::from_str("0.0.0.0").unwrap();
    pub static ref DEFAULT_SOCK_ADDRESS: SockAddr =
        SockAddr::from(SocketAddr::new(*DEFAULT_BIND_ADDRESS, *DEFAULT_SOURCE_PORT,));
    pub static ref DEFAULT_PING_TIMEOUT: Duration = Duration::from_millis(35);
    pub static ref DEFAULT_DATA_TIMEOUT: Duration = Duration::from_millis(100);
    pub static ref DEFAULT_BUFFER_SIZE: usize = 1024;
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

#[derive(Debug)]
pub struct Client {
    pub names: Vec<String>,
    pub devices: Vec<Target>,
    pub sock: Socket,
    pub ping_timeout: Duration,
    pub data_timeout: Duration,
    pub buffer_size: usize,
    pub address: SockAddr,
    pub host: IpAddr,
}

impl Client {
    pub async fn default() -> Result<Self, QueryError> {
        let sock = match Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)) {
            Ok(sock_ok) => sock_ok,
            Err(err) => return Err(QueryError::Network(err)),
        };

        match sock.set_reuse_address(true) {
            Ok(_) => {}
            Err(err) => return Err(QueryError::Network(err)),
        }

        match sock.set_reuse_port(true) {
            Ok(_) => {}
            Err(err) => return Err(QueryError::Network(err)),
        }

        match sock.bind(&DEFAULT_SOCK_ADDRESS) {
            Ok(_) => {}
            Err(err) => return Err(QueryError::Network(err)),
        }

        let host = sock.local_addr().unwrap().as_socket().unwrap().ip();

        Ok(Client {
            names: Vec::new(),
            devices: Vec::new(),
            sock: sock,
            ping_timeout: *DEFAULT_PING_TIMEOUT,
            data_timeout: *DEFAULT_DATA_TIMEOUT,
            buffer_size: *DEFAULT_BUFFER_SIZE,
            address: DEFAULT_SOCK_ADDRESS.clone(),
            host: host,
        })
    }

    pub async fn new(
        bind_addr: Option<IpAddr>,
        bind_port: Option<u16>,
        ping_timeout_ms: Option<u64>,
        data_timeout_ms: Option<u64>,
        buffer_size: Option<usize>,
    ) -> Result<Self, QueryError> {
        let addr = SockAddr::from(SocketAddr::new(
            if let Some(addr) = bind_addr {
                addr
            } else {
                *DEFAULT_BIND_ADDRESS
            },
            if let Some(port) = bind_port {
                port
            } else {
                *DEFAULT_SOURCE_PORT
            },
        ));

        let sock = match Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)) {
            Ok(sock_ok) => sock_ok,
            Err(err) => return Err(QueryError::Network(err)),
        };

        match sock.set_reuse_address(true) {
            Ok(_) => {}
            Err(err) => return Err(QueryError::Network(err)),
        }

        match sock.set_reuse_port(true) {
            Ok(_) => {}
            Err(err) => return Err(QueryError::Network(err)),
        }

        match sock.bind(&addr) {
            Ok(_) => {}
            Err(err) => return Err(QueryError::Network(err)),
        }

        let host = sock.local_addr().unwrap().as_socket().unwrap().ip();

        Ok(Client {
            names: Vec::new(),
            devices: Vec::new(),
            sock: sock,
            ping_timeout: if let Some(ping) = ping_timeout_ms {
                Duration::from_millis(ping)
            } else {
                *DEFAULT_PING_TIMEOUT
            },
            data_timeout: if let Some(data) = data_timeout_ms {
                Duration::from_millis(data)
            } else {
                *DEFAULT_DATA_TIMEOUT
            },
            buffer_size: if let Some(buffer) = buffer_size {
                buffer
            } else {
                *DEFAULT_BUFFER_SIZE
            },
            address: addr,
            host: host,
        })
    }

    pub async fn discover(self: Self) -> Result<Self, QueryError> {
        Ok(self.update_discovery().await?.update_names())
    }

    pub async fn send(
        request: WizRPCRequest,
        device: String,
    ) -> Result<WizRPCResponse, QueryError> {
        todo!()
    }

    async fn update_discovery(mut self: Self) -> Result<Self, QueryError> {
        let valid_ips = self.scan().await?;

        let mut valid_devices = Vec::new();

        for ip in valid_ips {
            if self
                .ping(
                    match Host::parse(&self.address.as_socket().unwrap().to_string()) {
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

    pub fn get_ip(self: &Self, address: Host) -> Result<IpAddr, QueryError> {
        let ip = match lookup_host(address.to_string().as_str()) {
            Ok(ips) => ips,
            Err(err) => return Err(QueryError::Network(err)),
        };

        Ok(ip[0])
    }

    pub async fn get_mac(self: &Self, address: Host) -> Result<MacAddr6, QueryError> {
        let data = parse_raw(
            self.raw_send(
                String::from("{\"method\":\"getSystemConfig\"}").as_bytes(),
                address,
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
        let receive = self.raw_send("0".as_bytes(), address).await?;
        if receive.len() > 0 {
            return Ok(true);
        }
        Err(QueryError::RPC(WizNetError::NotAWizDevice))
    }

    pub async fn scan(self: &Self) -> Result<Vec<IpAddr>, QueryError> {
        if self.host.is_ipv4() {
            let host_string = self.host.to_string();
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
            scanner.set_timeout_ms(self.ping_timeout.as_millis().try_into().unwrap());

            scanner.set_ping_interval_ms((self.ping_timeout.as_millis() / 4).try_into().unwrap());
            scanner.set_ntries(3);
            scanner.set_scan_type(QScanType::Ping);
            scanner.set_print_mode(QSPrintMode::NonRealTime);

            // TODO find a way to communicate this
            let res: &Vec<QScanResult> = scanner.scan_ping().await;

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

    pub async fn raw_send(self: &Self, data: &[u8], target: Host) -> Result<Vec<u8>, QueryError> {
        let addr = SockAddr::from(SocketAddr::new(self.get_ip(target)?, *DEFAULT_TARGET_PORT));

        let mut buffer = [MaybeUninit::uninit()];

        match self.sock.connect_timeout(&addr, self.data_timeout) {
            Ok(_) => {}
            Err(err) => return Err(QueryError::Network(err)),
        }

        match self.sock.send(data) {
            Ok(_) => {}
            Err(err) => return Err(QueryError::Network(err)),
        }

        let result_length = match self.sock.recv(&mut buffer) {
            Ok(len) => len,
            Err(err) => return Err(QueryError::Network(err)),
        };

        if result_length > 0 {
            Ok(buffer
                .iter()
                //? Is this ok? it gives me the jeeves
                .map(|byte| unsafe { byte.assume_init() })
                .collect::<Vec<u8>>())
        } else {
            return Err(QueryError::Network(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Ningún dato recibido",
            )));
        }
    }
}

#[tokio::test]
async fn test_client_new() {
    {
        Client::default().await.unwrap();
    }
}

#[tokio::test]
async fn test_client_discover() {
    let mut client = Client::default().await.unwrap();

    client = client.discover().await.unwrap();

    print!("detected devices: {:#?}", client.devices);

    drop(client);
}
