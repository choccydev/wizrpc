use local_ip_address::local_ip;
use macaddr::MacAddr6;
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use std::{
    collections::HashMap,
    mem::MaybeUninit,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};
use tokio::{
    net::UdpSocket,
    sync::Mutex,
    time::{sleep, timeout},
};
use uuid::Uuid;

use lazy_static::lazy_static;
use retry::delay::{jitter, Fixed};
use retry::retry;
use tokio::time::Instant;

use crate::{
    error::{QueryError, SerializationError, WizNetError},
    model::RPCResponse,
    Request, Response, Target,
};

pub const DEFAULT_BUFFER_SIZE: usize = 1024;
pub const DEFAULT_RETRIES: usize = 10;

lazy_static! {
    pub static ref DEFAULT_TARGET_PORT: u16 = 38899;
    pub static ref DEFAULT_SOURCE_RECEIVE_PORT: u16 = 39900;
    pub static ref DEFAULT_SOURCE_SEND_PORT: u16 = 39901;
    pub static ref DEFAULT_BIND_ADDRESS: IpAddr = local_ip().unwrap();
    pub static ref DEFAULT_MULTICAST_ADDRESS: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 1);
    pub static ref DEFAULT_SOCK_RECEIVE_ADDRESS: String =
        format!("{}:{}", *DEFAULT_BIND_ADDRESS, *DEFAULT_SOURCE_RECEIVE_PORT);
    pub static ref DEFAULT_SOCK_SEND_ADDRESS: String =
        format!("{}:{}", *DEFAULT_BIND_ADDRESS, *DEFAULT_SOURCE_SEND_PORT);
    pub static ref DEFAULT_PING_TIMEOUT: Duration = Duration::from_millis(35);
    pub static ref DEFAULT_DATA_TIMEOUT: Duration = Duration::from_millis(200);
}

#[derive(Debug)]
pub struct WizEvent {
    pub request: Option<Request>,
    pub request_raw: Vec<u8>,
    pub request_time: Instant,
    pub response: Option<Response>,
    pub response_raw: Option<Vec<u8>>,
    pub response_time: Option<Instant>,
    pub target: Option<Target>,
}

#[derive(Debug)]
pub struct Client {
    pub sock: UdpSocket,
    pub devices: Mutex<HashMap<String, Target>>,
    pub retries: usize,
    pub history: Mutex<HashMap<Uuid, WizEvent>>,
}

impl Client {
    pub async fn default() -> Result<Arc<Self>, QueryError> {
        Ok(Client::new(None, None, None, None).await?)
    }

    pub async fn new(
        bind_addr: Option<IpAddr>,
        bind_port: Option<u16>,
        data_timeout_ms: Option<u64>,
        retries_number: Option<usize>,
    ) -> Result<Arc<Self>, QueryError> {
        let addr = SocketAddr::new(
            bind_addr.unwrap_or(*DEFAULT_BIND_ADDRESS),
            bind_port.unwrap_or(DEFAULT_SOURCE_SEND_PORT.clone()),
        );

        let data_timeout = if let Some(data) = data_timeout_ms {
            Duration::from_millis(data)
        } else {
            *DEFAULT_DATA_TIMEOUT
        };

        let sock = UdpSocket::bind(&addr).await?;
        sock.join_multicast_v4(*DEFAULT_MULTICAST_ADDRESS, Ipv4Addr::UNSPECIFIED)?;

        let retries = retries_number.unwrap_or(DEFAULT_RETRIES);
        Ok(Arc::new(Client {
            sock,
            retries,
            devices: Mutex::new(HashMap::new()),
            history: Mutex::new(HashMap::new()),
        }))
    }

    pub async fn register_device(
        self: &Arc<Self>,
        name: String,
        address: String,
    ) -> Result<(), QueryError> {
        // Check if device is responsive
        self.ping_addr(address.clone()).await?;

        // Then request basic info
        let device_data = parse_raw(
            self.send_raw(
                "{\"method\": \"getSystemConfig\"}".to_string().as_bytes(),
                address.parse()?,
            )
            .await?,
        )?
        .result
        .ok_or(QueryError::Serialization(
            SerializationError::ValueDeserialization,
        ))?;

        let mut devices = self.devices.lock().await;
        let mut history = self.history.lock().await;

        let device = Target {
            address: address.parse()?,
            mac: MacAddr6::from(parse_mac_address(device_data.mac.unwrap().as_str())?),
        };

        devices.insert(name, device.clone());

        let id = history.len() + 1;

        history.insert(
            Uuid::now_v1(&counter_to_bytes(id)),
            WizEvent {
                request: None,
                request_raw: Vec::from("REGISTERED_DEVICE".as_bytes()),
                request_time: Instant::now(),
                response: None,
                response_raw: None,
                response_time: None,
                target: Some(device),
            },
        );

        Ok(())
    }

    async fn ping_addr(self: &Arc<Self>, address: String) -> Result<(), QueryError> {
        let payload = [0; 8];

        let (_packet, _duration) = surge_ping::ping(address.parse()?, &payload).await?;

        Ok(())
    }

    async fn get_device(self: &Arc<Self>, name: Option<String>) -> Result<Target, QueryError> {
        let name_some = if let Some(nam) = name {
            nam
        } else {
            return Err(QueryError::NoTarget);
        };
        let devices = self.devices.lock().await;

        match devices.get(&name_some) {
            Some(target) => {
                return Ok(target.clone());
            }
            None => {
                return Err(QueryError::NoTarget);
            }
        }
    }

    pub async fn send(self: &Arc<Self>, request: Request) -> Result<Response, QueryError> {
        let device = self.get_device(request.clone().device).await?;

        let addr = device.address;
        let req_time = Instant::now();
        match self
            .send_raw(request.clone().to_raw()?.as_slice(), addr)
            .await
        {
            Err(err) => {
                return Err(err);
            }
            Ok(val) => {
                let res_time = Instant::now();

                let parsed = parse_raw(val.clone())?;

                let mut history = self.history.lock().await;
                let id = history.len() + 1;

                history.insert(
                    Uuid::now_v1(&counter_to_bytes(id)),
                    WizEvent {
                        request: Some(request.clone()),
                        request_raw: request.to_raw()?,
                        request_time: req_time,
                        response: Some(parsed.clone()),
                        response_raw: Some(val),
                        response_time: Some(res_time),
                        target: Some(device),
                    },
                );

                return Ok(parsed);
            }
        };
    }

    pub async fn ping(self: &Arc<Self>, name: String) -> Result<(), QueryError> {
        let device = self.get_device(Some(name)).await?;

        let address = device.address.to_string();

        // Check if device is responsive
        self.ping_addr(address.clone()).await?;

        // Then request basic info
        parse_raw(
            self.send_raw(
                "{\"method\": \"getSystemConfig\"}".to_string().as_bytes(),
                address.parse()?,
            )
            .await?,
        )?
        .result
        .ok_or(QueryError::Serialization(
            SerializationError::ValueDeserialization,
        ))?;

        let mut history = self.history.lock().await;
        let id = history.len() + 1;

        history.insert(
            Uuid::now_v1(&counter_to_bytes(id)),
            WizEvent {
                request: None,
                request_raw: Vec::from("PING".as_bytes()),
                request_time: Instant::now(),
                response: None,
                response_raw: None,
                response_time: None,
                target: Some(device),
            },
        );

        Ok(())
    }

    pub async fn discover(self: &Arc<Self>, wait: u8) -> Result<Vec<Target>, QueryError> {
        let mut buf = vec![0u8; DEFAULT_BUFFER_SIZE]; // Buffer to store incoming data
        let mut entries: Vec<Target> = [].to_vec();

        // You might want to initialize this dynamically based on some condition
        let max_attempts = 50;
        let mut attempts = 0;

        // Timeout for sending data
        let send_timeout = Duration::from_secs(wait.into());
        let send_data_future = self.sock.send_to(
            "{\"method\": \"getSystemConfig\"}".as_bytes(),
            SocketAddr::new(
                DEFAULT_MULTICAST_ADDRESS
                    .to_string()
                    .parse::<IpAddr>()
                    .unwrap(),
                *DEFAULT_TARGET_PORT,
            ),
        );

        match timeout(send_timeout, send_data_future).await {
            Ok(_) => {
                while attempts < max_attempts {
                    let recv_timeout = Duration::from_millis(150);
                    let result = timeout(recv_timeout, self.sock.recv_from(&mut buf)).await;

                    match result {
                        Ok(Ok((size, addr))) => {
                            if size > 0 {
                                let device_data =
                                    parse_raw(Vec::from(&buf[0..size]))?.result.unwrap();

                                entries.push(Target {
                                    address: get_ip_from_sockaddr(addr),
                                    mac: MacAddr6::from(parse_mac_address(
                                        device_data.mac.unwrap().as_str(),
                                    )?),
                                });
                            }
                        }
                        Ok(Err(err)) => {
                            return Err(err.into());
                        }
                        Err(_) => {
                            attempts += 1;
                        }
                    }
                }
                Ok(entries)
            }
            Err(_) => Ok(entries),
        }
    }

    async fn send_raw(
        self: &Arc<Self>,
        data: &[u8],
        target: IpAddr,
    ) -> Result<Vec<u8>, QueryError> {
        let addr = SocketAddr::new(target, *DEFAULT_TARGET_PORT);

        let mut buf = vec![0u8; DEFAULT_BUFFER_SIZE]; // Buffer to store incoming data

        timeout(Duration::from_secs(1), self.sock.send_to(data, &addr)).await??;

        let (received, _) =
            timeout(Duration::from_secs(1), self.sock.recv_from(&mut buf)).await??;

        Ok(buf[0..received].to_vec())
    }
}

fn parse_mac_address(mac_address_str: &str) -> Result<[u8; 6], QueryError> {
    if mac_address_str.len() != 12 {
        return Err(QueryError::Serialization(
            SerializationError::MacAddressError,
        ));
    }

    let mut bytes = [0u8; 6];
    for i in 0..6 {
        let start_index = i * 2;
        let end_index = start_index + 2;
        let byte_str = &mac_address_str[start_index..end_index];
        bytes[i] = u8::from_str_radix(byte_str, 16).unwrap();
    }

    Ok(bytes)
}

fn parse_raw(data: Vec<u8>) -> Result<Response, QueryError> {
    // TODO add error handling
    let converted_str = String::from_utf8(data).unwrap();

    let serde_result: Result<RPCResponse, _> = match serde_json::from_str(converted_str.as_str()) {
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

fn counter_to_bytes(counter: usize) -> [u8; 6] {
    assert!(counter <= (u64::MAX >> 16).try_into().unwrap());

    let mut bytes = [0u8; 6];
    let counter_bytes = (counter as u64).to_be_bytes(); // Convert to big-endian bytes
    bytes.copy_from_slice(&counter_bytes[2..]); // Copy the last 6 bytes
    bytes
}

fn get_ip_from_sockaddr(sock_addr: SocketAddr) -> IpAddr {
    match sock_addr {
        SocketAddr::V4(v4_addr) => IpAddr::V4(*v4_addr.ip()),
        SocketAddr::V6(v6_addr) => IpAddr::V6(*v6_addr.ip()),
    }
}
