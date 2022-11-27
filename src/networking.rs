use crate::error::{QueryError, SerializationError, WizNetError};
use crate::model::{RPCResponse, Target, WizRPCRequest};
use crate::WizRPCResponse;
use dns_lookup::lookup_host;
use lazy_static::lazy_static;
use local_ip_address::local_ip;
use macaddr::MacAddr6;
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use std::io::Read;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::str::{from_utf8, FromStr};
use std::time::Duration;

pub const DEFAULT_BUFFER_SIZE: usize = 1024;
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
    pub names: Vec<String>,
    pub devices: Vec<Target>,
    pub ping_sock: Socket,
    pub data_sock: Socket,
    pub ping_timeout: Duration,
    pub data_timeout: Duration,
    pub buffer_size: usize,
    pub address: SockAddr,
    pub host: IpAddr,
}

impl Client {
    pub async fn default() -> Result<Self, QueryError> {
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

        Ok(Client {
            names: Vec::new(),
            devices: Vec::new(),
            data_sock: data_sock,
            ping_sock: ping_sock,
            ping_timeout: *DEFAULT_PING_TIMEOUT,
            data_timeout: *DEFAULT_DATA_TIMEOUT,
            buffer_size: DEFAULT_BUFFER_SIZE,
            address: DEFAULT_SOCK_DATA_ADDRESS.clone(),
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

        Ok(Client {
            names: Vec::new(),
            devices: Vec::new(),
            data_sock: data_sock,
            ping_sock: ping_sock,
            ping_timeout: ping_timeout,
            data_timeout: data_timeout,
            buffer_size: buffer,
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
            if self.ping(ip).await?.as_str() != "" {
                valid_devices.push(ip);
            }
        }

        for device in valid_devices {
            let host = device;
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

    pub fn get_ip(self: &Self, address: String) -> Result<IpAddr, QueryError> {
        let ip = lookup_host(address.as_str())?;

        Ok(ip[0])
    }

    pub async fn get_mac(self: &mut Self, address: IpAddr) -> Result<MacAddr6, QueryError> {
        let mac = self.ping(address).await?;
        match MacAddr6::from_str(&mac) {
            Ok(macaddr) => Ok(macaddr),
            Err(_) => Err(QueryError::Serialization(
                SerializationError::MacAddressError,
            )),
        }
    }

    pub async fn ping(self: &mut Self, target: IpAddr) -> Result<String, QueryError> {
        let addr = SockAddr::from(SocketAddr::new(target, *DEFAULT_TARGET_PORT));

        let mut buffer = [0; DEFAULT_BUFFER_SIZE];

        self.ping_sock.connect(&addr)?;

        self.ping_sock
            .send("{\"method\":\"getSystemConfig\"}".as_bytes())?;

        //? this is blocking? weirdly? huh?
        let result_length = self.ping_sock.read(&mut buffer)?;

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

    pub async fn scan(self: &mut Self) -> Result<Vec<IpAddr>, QueryError> {
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
                match &self.ping(address).await {
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

    pub async fn raw_send(
        self: &mut Self,
        data: &[u8],
        target: IpAddr,
    ) -> Result<[u8; DEFAULT_BUFFER_SIZE], QueryError> {
        let addr = SockAddr::from(SocketAddr::new(target, *DEFAULT_TARGET_PORT));

        let mut buffer = [0; DEFAULT_BUFFER_SIZE];

        self.data_sock.connect_timeout(&addr, self.data_timeout)?;

        self.data_sock.send(data)?;

        let result_length = self.data_sock.read(&mut buffer)?;

        if result_length > 0 {
            Ok(buffer.clone())
        } else {
            return Err(QueryError::Network(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Ningún dato recibido",
            )));
        }
    }

    pub fn parse_raw(self: &mut Self, data: &[u8]) -> Result<WizRPCResponse, QueryError> {
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

    pub fn trim_slice(self: &mut Self, slice: &[u8]) -> Vec<u8> {
        let mut trimmed = Vec::new();

        for val in slice {
            if val != &0 {
                trimmed.push(*val);
            }
        }

        trimmed
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
    let mut client = Client::default().await.unwrap();

    let addr1 = IpAddr::from_str("192.168.0.88").unwrap();
    let addr2 = IpAddr::from_str("192.168.0.89").unwrap();

    client.ping(addr1).await.unwrap();
    client.ping(addr2).await.unwrap();

    drop(client);
}

#[tokio::test]
async fn test_client_discover() {
    // FIXME for some reason here the ping to valid addresses fails. It's weird.
    let mut client = Client::default().await.unwrap();

    client = client.discover().await.unwrap();

    assert_ne!(client.devices.len(), 0);

    drop(client);
}
