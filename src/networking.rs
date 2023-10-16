use local_ip_address::local_ip;
use macaddr::MacAddr6;
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use std::{
    collections::HashMap,
    io::ErrorKind,
    mem::MaybeUninit,
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::Duration,
};
use tokio::sync::Mutex;
use uuid::Uuid;

use lazy_static::lazy_static;
use retry::delay::{jitter, Fixed};
use retry::retry;
use tokio::{
    net::UdpSocket,
    sync::{oneshot, RwLock},
    time::Instant,
};

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
    pub static ref DEFAULT_SOCK_RECEIVE_ADDRESS: String =
        format!("{}:{}", *DEFAULT_BIND_ADDRESS, *DEFAULT_SOURCE_RECEIVE_PORT);
    pub static ref DEFAULT_SOCK_SEND_ADDRESS: String =
        format!("{}:{}", *DEFAULT_BIND_ADDRESS, *DEFAULT_SOURCE_SEND_PORT);
    pub static ref DEFAULT_PING_TIMEOUT: Duration = Duration::from_millis(35);
    pub static ref DEFAULT_DATA_TIMEOUT: Duration = Duration::from_millis(200);
}

#[derive(Debug)]
pub struct WizEvent {
    pub request: Request,
    pub request_raw: Vec<u8>,
    pub request_time: Instant,
    pub response: Option<Response>,
    pub response_raw: Option<Vec<u8>>,
    pub response_time: Option<Instant>,
    pub target: Option<Target>,
}

#[derive(Debug)]
pub struct Client {
    pub sock: Socket,
    pub devices: Mutex<HashMap<String, Target>>,
    pub retries: usize,
    pub queue: HashMap<Uuid, u8>,
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
        let addr = SockAddr::from(SocketAddr::new(
            if let Some(addr) = bind_addr {
                addr
            } else {
                *DEFAULT_BIND_ADDRESS
            },
            if let Some(port) = bind_port {
                port
            } else {
                DEFAULT_SOURCE_SEND_PORT.clone()
            },
        ));

        let data_timeout = if let Some(data) = data_timeout_ms {
            Duration::from_millis(data)
        } else {
            *DEFAULT_DATA_TIMEOUT
        };

        let sock = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        sock.set_reuse_address(true)?;
        sock.set_reuse_port(true)?;
        sock.set_read_timeout(Some(data_timeout))?;
        sock.set_write_timeout(Some(data_timeout))?;
        sock.set_send_buffer_size(DEFAULT_BUFFER_SIZE)?;
        sock.set_recv_buffer_size(DEFAULT_BUFFER_SIZE)?;
        sock.bind(&addr)?;

        let host = sock.local_addr().unwrap().as_socket().unwrap().ip();

        let retries = if let Some(number) = retries_number {
            number // TODO return Err if this is larger than a u8
        } else {
            DEFAULT_RETRIES
        };
        Ok(Arc::new(Client {
            sock,
            retries,
            devices: Mutex::new(HashMap::new()),
            queue: HashMap::new(),
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
        let device_data = self
            .parse_raw(
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

        devices.insert(
            name,
            Target {
                address: address.parse()?,
                mac: MacAddr6::from(parse_mac_address(device_data.mac.unwrap().as_str())?),
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
        match self.send_raw(request.to_raw()?.as_slice(), addr).await {
            Err(err) => {
                return Err(err);
            }
            Ok(val) => {
                return Ok(self.parse_raw(val)?);
            }
        };
    }

    pub async fn ping(self: &Arc<Self>, name: String) -> Result<(), QueryError> {
        let device = self.get_device(Some(name)).await?;

        let address = device.address.to_string();

        // Check if device is responsive
        self.ping_addr(address.clone()).await?;

        // Then request basic info
        self.parse_raw(
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

        Ok(())
    }

    async fn send_raw(
        self: &Arc<Self>,
        data: &[u8],
        target: IpAddr,
    ) -> Result<Vec<u8>, QueryError> {
        let addr = SockAddr::from(SocketAddr::new(target, *DEFAULT_TARGET_PORT));

        let move_retries = self.retries.clone();
        let move_data = Vec::from(data).clone();

        let mut buf = Vec::with_capacity(DEFAULT_BUFFER_SIZE); // Buffer to store incoming data
        let mut uninitialized_buf = vec![MaybeUninit::uninit(); DEFAULT_BUFFER_SIZE];

        self.sock.connect(&addr).unwrap();

        retry(
            Fixed::from(*DEFAULT_DATA_TIMEOUT)
                .map(jitter)
                .take(move_retries),
            || self.sock.send(move_data.as_slice()),
        )?;

        match self.sock.recv(&mut uninitialized_buf) {
            Ok(received) => {
                // Safely initialize and set the length of the buffer
                unsafe {
                    buf.set_len(received);
                    std::ptr::copy_nonoverlapping(
                        uninitialized_buf.as_ptr() as *const u8,
                        buf.as_mut_ptr(),
                        received,
                    );
                }
                return Ok(Vec::from(&buf[0..received]));
            }
            Err(_) => {
                return Err(QueryError::Network(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Ningún dato recibido",
                )));
            }
        };
    }

    fn parse_raw(self: &Arc<Self>, data: Vec<u8>) -> Result<Response, QueryError> {
        // TODO add error handling
        let converted_str = String::from_utf8(data).unwrap();

        let serde_result: Result<RPCResponse, _> =
            match serde_json::from_str(converted_str.as_str()) {
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
