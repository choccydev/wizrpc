use crate::error::{QueryError, SerializationError, WizNetError};
use crate::model::{RPCResponse, Target, WizRPCRequest};
use crate::WizRPCResponse;
use dns_lookup::lookup_host;
use lazy_static::lazy_static;
use local_ip_address::local_ip;
use macaddr::MacAddr6;
use retry::delay::{jitter, Fixed};
use retry::retry;
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use std::io::Read;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::str::{from_utf8, FromStr};
use std::sync::{Arc, RwLock};
use std::time::Duration;

pub const DEFAULT_BUFFER_SIZE: usize = 1024;
pub const DEFAULT_RETRIES: usize = 3;
lazy_static! {
    pub static ref DEFAULT_TARGET_PORT: u16 = 38899;
    pub static ref DEFAULT_SOURCE_DATA_PORT: u16 = 39900;
    pub static ref DEFAULT_SOURCE_PING_PORT: u16 = 39901;
    pub static ref DEFAULT_BIND_ADDRESS: IpAddr = local_ip().unwrap();
    pub static ref DEFAULT_SOCK_DATA_ADDRESS: SockAddr = SockAddr::from(SocketAddr::new(
        *DEFAULT_BIND_ADDRESS,
        *DEFAULT_SOURCE_DATA_PORT,
    ));
    pub static ref DEFAULT_SOCK_PING_ADDRESS: SockAddr = SockAddr::from(SocketAddr::new(
        *DEFAULT_BIND_ADDRESS,
        *DEFAULT_SOURCE_PING_PORT,
    ));
    pub static ref DEFAULT_PING_TIMEOUT: Duration = Duration::from_millis(35);
    pub static ref DEFAULT_DATA_TIMEOUT: Duration = Duration::from_millis(100);
}

#[derive(Debug)]
pub struct Client {
    pub address: SockAddr,
    pub host: IpAddr,
    names_lock: RwLock<Vec<String>>,
    devices_lock: RwLock<Vec<Target>>,
    ping_lock: RwLock<Socket>,
    data_lock: RwLock<Socket>,
    ping_timeout: Duration,
    data_timeout: Duration,
    buffer_size: usize,
    retries: usize,
}

impl Client {
    pub async fn default() -> Result<Arc<Self>, QueryError> {
        let data_sock = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        data_sock.set_reuse_address(true)?;
        data_sock.set_reuse_port(true)?;
        data_sock.set_read_timeout(Some(*DEFAULT_DATA_TIMEOUT))?;
        data_sock.set_write_timeout(Some(*DEFAULT_DATA_TIMEOUT))?;
        data_sock.set_send_buffer_size(DEFAULT_BUFFER_SIZE)?;
        data_sock.set_recv_buffer_size(DEFAULT_BUFFER_SIZE)?;
        data_sock.set_reuse_port(true)?;
        data_sock.bind(&DEFAULT_SOCK_DATA_ADDRESS)?;

        // ??? why it needs elevated permissions wtf
        let ping_sock = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        ping_sock.set_reuse_address(true)?;
        ping_sock.set_reuse_port(true)?;
        ping_sock.set_read_timeout(Some(*DEFAULT_PING_TIMEOUT))?;
        ping_sock.set_write_timeout(Some(*DEFAULT_PING_TIMEOUT))?;
        ping_sock.set_send_buffer_size(DEFAULT_BUFFER_SIZE)?;
        ping_sock.set_recv_buffer_size(DEFAULT_BUFFER_SIZE)?;
        ping_sock.set_reuse_port(true)?;

        let host = data_sock.local_addr().unwrap().as_socket().unwrap().ip();

        Ok(Arc::new(Client {
            names_lock: RwLock::new(Vec::new()),
            devices_lock: RwLock::new(Vec::new()),
            data_lock: RwLock::new(data_sock),
            ping_lock: RwLock::new(ping_sock),
            ping_timeout: *DEFAULT_PING_TIMEOUT,
            data_timeout: *DEFAULT_DATA_TIMEOUT,
            buffer_size: DEFAULT_BUFFER_SIZE,
            retries: DEFAULT_RETRIES,
            address: DEFAULT_SOCK_DATA_ADDRESS.clone(),
            host: host,
        }))
    }

    pub async fn new(
        bind_addr: Option<IpAddr>,
        bind_port: Option<u16>,
        ping_timeout_ms: Option<u64>,
        data_timeout_ms: Option<u64>,
        buffer_size: Option<usize>,
        retries: Option<usize>,
    ) -> Result<Arc<Self>, QueryError> {
        let addr = SockAddr::from(SocketAddr::new(
            if let Some(addr) = bind_addr {
                addr
            } else {
                *DEFAULT_BIND_ADDRESS
            },
            if let Some(port) = bind_port {
                port
            } else {
                *DEFAULT_SOURCE_DATA_PORT
            },
        ));

        let data_timeout = if let Some(data) = data_timeout_ms {
            Duration::from_millis(data)
        } else {
            *DEFAULT_DATA_TIMEOUT
        };

        let buffer = if let Some(size) = buffer_size {
            size
        } else {
            DEFAULT_BUFFER_SIZE
        };

        let data_sock = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        data_sock.set_reuse_address(true)?;
        data_sock.set_reuse_port(true)?;
        data_sock.set_read_timeout(Some(data_timeout))?;
        data_sock.set_write_timeout(Some(data_timeout))?;
        data_sock.set_send_buffer_size(buffer)?;
        data_sock.set_recv_buffer_size(buffer)?;
        data_sock.set_reuse_port(true)?;
        data_sock.bind(&addr)?;

        let ping_timeout = if let Some(ping) = ping_timeout_ms {
            Duration::from_millis(ping)
        } else {
            *DEFAULT_PING_TIMEOUT
        };

        let ping_sock = Socket::new(Domain::IPV4, Type::RAW, Some(Protocol::ICMPV4))?;
        ping_sock.set_reuse_address(true)?;
        ping_sock.set_reuse_port(true)?;
        ping_sock.set_read_timeout(Some(ping_timeout))?;
        ping_sock.set_write_timeout(Some(ping_timeout))?;
        ping_sock.set_send_buffer_size(DEFAULT_BUFFER_SIZE)?;
        ping_sock.set_recv_buffer_size(DEFAULT_BUFFER_SIZE)?;
        ping_sock.set_reuse_port(true)?;

        let host = data_sock.local_addr().unwrap().as_socket().unwrap().ip();

        let retry = if let Some(number) = retries {
            number
        } else {
            DEFAULT_RETRIES
        };

        Ok(Arc::new(Client {
            names_lock: RwLock::new(Vec::new()),
            devices_lock: RwLock::new(Vec::new()),
            data_lock: RwLock::new(data_sock),
            ping_lock: RwLock::new(ping_sock),
            ping_timeout: ping_timeout,
            data_timeout: data_timeout,
            buffer_size: buffer,
            retries: retry,
            address: addr,
            host: host,
        }))
    }

    pub async fn discover(self: &mut Arc<Self>) -> Result<(), QueryError> {
        self.update_discovery().await?;
        self.update_names()?;
        Ok(())
    }

    pub async fn send(
        request: WizRPCRequest,
        device: String,
    ) -> Result<WizRPCResponse, QueryError> {
        todo!()
    }

    async fn update_discovery(self: &mut Arc<Self>) -> Result<(), QueryError> {
        let valid_ips = self.scan().await?;

        let mut valid_devices = Vec::new();

        for ip in valid_ips {
            if self.ping(ip).await?.as_str() != "" {
                valid_devices.push(ip);
            }
        }

        for device in valid_devices {
            let host = device;
            let mac = self.get_mac(host.clone()).await?;

            let mut devices = self.devices_lock.write()?;
            devices.push(Target::new(host, mac, None));
        }

        Ok(())
    }

    fn update_names(self: &Arc<Self>) -> Result<(), QueryError> {
        let devices = self.devices_lock.read()?;
        let mut names = self.names_lock.write()?;
        let mut names_data: Vec<String> = devices
            .iter()
            .map(|target: &Target| target.name.clone())
            .collect();
        names.drain(0..);
        names.append(&mut names_data);
        Ok(())
    }

    fn get_ip(self: &Arc<Self>, address: String) -> Result<IpAddr, QueryError> {
        let ip = lookup_host(address.as_str())?;

        Ok(ip[0])
    }

    async fn get_mac(self: &Arc<Self>, address: IpAddr) -> Result<MacAddr6, QueryError> {
        let mac = self.ping(address).await?;
        match MacAddr6::from_str(&mac) {
            Ok(macaddr) => Ok(macaddr),
            Err(_) => Err(QueryError::Serialization(
                SerializationError::MacAddressError,
            )),
        }
    }

    async fn ping(self: &Arc<Self>, target: IpAddr) -> Result<String, QueryError> {
        let addr = SockAddr::from(SocketAddr::new(target, *DEFAULT_TARGET_PORT));

        let mut buffer = [0; DEFAULT_BUFFER_SIZE];

        let mut sock = retry(
            Fixed::from(self.ping_timeout)
                .map(jitter)
                .take(self.retries),
            || self.ping_lock.write(),
        )?;

        sock.connect(&addr)?;

        sock.send("{\"method\":\"getSystemConfig\"}".as_bytes())?;

        let result_length = sock.read(&mut buffer)?;

        drop(sock);

        if result_length > 0 {
            let response = self.parse_raw(&buffer)?;
            let result = response.result.ok_or(QueryError::Serialization(
                SerializationError::ValueDeserialization,
            ))?;

            let mac = result
                .get("mac")
                .ok_or(QueryError::Serialization(
                    SerializationError::ValueDeserialization,
                ))?
                .as_str()
                .ok_or(QueryError::Serialization(
                    SerializationError::ValueDeserialization,
                ))?
                .clone();
            return Ok(String::from(mac));
        } else {
            return Err(QueryError::Network(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Ningún dato recibido",
            )));
        }
    }

    async fn scan(self: &mut Arc<Self>) -> Result<Vec<IpAddr>, QueryError> {
        if self.host.is_ipv4() {
            let mut host_parts: Vec<u8> = Vec::new();
            let host_string = self.host.to_string();
            let mut host_split = host_string.split('.');
            let deserialization_error = crate::error::SerializationError::IPAddrError.clone();

            host_parts.push(
                match host_split
                    .next()
                    .ok_or(QueryError::Serialization(deserialization_error))?
                    .parse()
                {
                    Ok(int) => int,
                    Err(_) => return Err(QueryError::Serialization(deserialization_error)),
                },
            );

            host_parts.push(
                match host_split
                    .next()
                    .ok_or(QueryError::Serialization(deserialization_error))?
                    .parse()
                {
                    Ok(int) => int,
                    Err(_) => return Err(QueryError::Serialization(deserialization_error)),
                },
            );

            host_parts.push(
                match host_split
                    .next()
                    .ok_or(QueryError::Serialization(deserialization_error))?
                    .parse()
                {
                    Ok(int) => int,
                    Err(_) => return Err(QueryError::Serialization(deserialization_error)),
                },
            );

            let addr_range = 0..255;

            let mut addresses: Vec<IpAddr> = Vec::new();

            for target in addr_range {
                addresses.push(IpAddr::V4(Ipv4Addr::new(
                    host_parts[0],
                    host_parts[1],
                    host_parts[2],
                    target,
                )))
            }

            let mut valid_ips = Vec::new();

            // TODO Add parallelization here (spawn tasks, collect handles, and iterate over them joining them?)
            for address in addresses {
                match self.ping(address).await {
                    Ok(res) => {
                        if res.as_str() != "" {
                            valid_ips.push(address)
                        }
                    }
                    Err(_) => {}
                };
            }

            Ok(valid_ips)
        } else {
            Err(QueryError::Network(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "Doesn't support IPv6 yet",
            )))
        }
    }

    async fn raw_send(
        self: &mut Arc<Self>,
        data: &[u8],
        target: IpAddr,
    ) -> Result<[u8; DEFAULT_BUFFER_SIZE], QueryError> {
        let addr = SockAddr::from(SocketAddr::new(target, *DEFAULT_TARGET_PORT));

        let mut buffer = [0; DEFAULT_BUFFER_SIZE];

        let mut sock = retry(
            Fixed::from(self.ping_timeout)
                .map(jitter)
                .take(self.retries),
            || self.data_lock.write(),
        )?;

        sock.connect_timeout(&addr, self.data_timeout)?;

        sock.send(data)?;

        let result_length = sock.read(&mut buffer)?;

        drop(sock);

        if result_length > 0 {
            Ok(buffer.clone())
        } else {
            return Err(QueryError::Network(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Ningún dato recibido",
            )));
        }
    }

    fn parse_raw(self: &Arc<Self>, data: &[u8]) -> Result<WizRPCResponse, QueryError> {
        let trimmed = self.trim_slice(data);
        // TODO add error handling
        let trimmed_str = from_utf8(trimmed.as_slice()).unwrap();

        let serde_result: Result<RPCResponse, _> = match serde_json::from_str(trimmed_str) {
            Ok(val) => Ok(val),
            Err(_) => Err(QueryError::Serialization(
                SerializationError::ValueDeserialization,
            )),
        };
        match serde_result {
            Err(_) => Err(QueryError::Serialization(
                crate::error::SerializationError::ValueDeserialization,
            )),
            Ok(deserialized) => match deserialized {
                RPCResponse::Err(err) => Err(QueryError::RPC(WizNetError::from_rpc_error(err))),
                RPCResponse::Ok(res) => res.to_wizres(None),
            },
        }
    }

    fn trim_slice(self: &Arc<Self>, slice: &[u8]) -> Vec<u8> {
        let mut trimmed = Vec::new();

        for val in slice {
            if val != &0 {
                trimmed.push(*val);
            }
        }

        trimmed
    }

    pub fn devices(self: Arc<Self>) -> Result<Vec<Target>, QueryError> {
        let devices = self.devices_lock.read()?;
        Ok(devices.clone())
    }

    pub fn names(self: Arc<Self>) -> Result<Vec<String>, QueryError> {
        let names = self.names_lock.read()?;
        Ok(names.clone())
    }
}

#[tokio::test]
async fn test_client_new() {
    {
        Client::default().await.unwrap();
    }
}

#[tokio::test]
async fn test_ping() {
    let client = Client::default().await.unwrap();

    let addr1 = IpAddr::from_str("192.168.0.88").unwrap();
    let addr2 = IpAddr::from_str("192.168.0.89").unwrap();

    client.ping(addr1).await.unwrap();
    client.ping(addr2).await.unwrap();
}

#[tokio::test]
async fn test_client_discover() {
    // FIXME for some reason here the ping to valid addresses fails. It's weird.
    let mut client = Client::default().await.unwrap();

    client.discover().await.unwrap();

    assert_ne!(client.devices().unwrap().len(), 0);
}
