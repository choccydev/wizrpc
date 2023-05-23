use crate::error::{QueryError, SerializationError, SyncError, WizNetError};
use crate::model::{RPCResponse, RequestKind, Target};
use crate::{Method, Request, Response};
use async_recursion::async_recursion;
use dns_lookup::lookup_host;
use lazy_static::lazy_static;
use local_ip_address::local_ip;
use macaddr::MacAddr6;
use retry::delay::{jitter, Fixed};
use retry::retry;
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use std::collections::HashMap;
use std::io::{ErrorKind, Read};
use std::net::{IpAddr, SocketAddr};
//use std::net::{Ipv4Addr};
use std::str::{from_utf8, FromStr};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{oneshot, oneshot::Sender, Mutex, RwLock};
use uuid::Uuid;

pub const DEFAULT_BUFFER_SIZE: usize = 1024;
pub const DEFAULT_RETRIES: usize = 10;
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
    pub static ref DEFAULT_DATA_TIMEOUT: Duration = Duration::from_millis(200);
}

#[derive(Debug)]
pub struct Event {
    pub id: Uuid,
    //pub channel: Channel,
    pub request: Request,
    pub request_raw: Vec<u8>,
    pub request_time: Instant,
    pub response: Option<Response>,
    pub response_raw: Option<Vec<u8>>,
    pub response_time: Option<Instant>,
    pub target: Option<Target>,
}

impl Event {
    pub fn new(id: Uuid, request: Request, target: Option<Target>) -> Result<Self, QueryError> {
        Ok(Self {
            id,
            //channel: None,
            request: request.clone(),
            request_raw: request.to_raw()?,
            request_time: Instant::now(),
            response: None,
            response_raw: None,
            response_time: None,
            target,
        })
    }
}

fn new_socket() -> Result<Socket, QueryError> {
    let addr = SockAddr::from(SocketAddr::new(
        *DEFAULT_BIND_ADDRESS,
        *DEFAULT_SOURCE_DATA_PORT,
    ));

    let sock = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    sock.set_reuse_address(true)?;
    sock.set_reuse_port(true)?;
    sock.set_read_timeout(Some(*DEFAULT_PING_TIMEOUT))?;
    sock.set_write_timeout(Some(*DEFAULT_PING_TIMEOUT))?;
    sock.set_send_buffer_size(DEFAULT_BUFFER_SIZE)?;
    sock.set_recv_buffer_size(DEFAULT_BUFFER_SIZE)?;
    sock.bind(&addr)?;
    Ok(sock)
}

/// UDP Client for talking with WizConnected devices
#[derive(Debug)]
pub struct Client {
    pub address: SockAddr,
    pub host: IpAddr,
    names_lock: RwLock<Vec<String>>,
    devices_lock: RwLock<Vec<Target>>,
    ping_lock: Mutex<Socket>,
    //data_lock: Mutex<Socket>,
    ping_timeout: Duration,
    data_timeout: Duration,
    retries: usize,
    queue: Arc<RwLock<HashMap<Uuid, u8>>>,
    history: Arc<RwLock<HashMap<Uuid, Event>>>,
}

impl Client {
    /// Creates a client with default values.
    ///
    /// It's recommended to use this instead of [`Client::new()`] unless you have special connectivity requirements.
    ///
    /// # Error
    ///
    /// Calling this constructor will error out if the socket cannot be created, the auto-selected interface
    /// cannot be bound, or there is some sort of error from the C network bindings.
    ///
    /// # Example
    ///
    /// ```
    /// use wizrpc::Client;
    ///
    /// tokio_test::block_on(async {
    /// let client = Client::default().await.unwrap();
    /// })
    /// ```
    ///
    /// Note that this returns an [`Arc<Client>`](std::sync::Arc<T>), and all methods operate over this reference counter,
    /// not over the struct itself.
    pub async fn default() -> Result<Arc<Self>, QueryError> {
        Ok(Client::new(None, None, None, None, None, None).await?)
    }

    /// Creates a client with given values.
    ///
    /// This function creates a new client with user-provided configuration instead of using the default values.
    ///
    /// This is useful in case the network you're going to use this client in has some nonstandard characteristics,
    /// like not being on a LAN, having to move ports around, having abysmally bad latency, or an unreliable connection.
    ///
    /// The possible values to control on constructing a new client are:
    ///
    /// - _Bind Address_: Adress to bind the client to.
    /// - _Bind Port_: Port to bind the client to.
    /// - _Ping Timeout_: Base timeout for pinging devices.
    /// - _Data Timeout_: Base timeout for sending requests to devices.
    /// - _Buffer size_: Max buffer size for a request.
    /// - _Retries_: Amount of retries the system will make when sending a request.
    ///
    /// # Error
    ///
    /// Calling this constructor will error out if the socket cannot be created, the auto-selected interface
    /// cannot be bound, or there is some sort of error from the C network bindings.
    ///
    /// # Example
    ///
    /// ```
    /// use wizrpc::Client;
    /// use std::net::{IpAddr, Ipv4Addr};
    ///
    /// tokio_test::block_on(async {
    /// let client = Client::new(Some(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))), Some(40000), None, None, None, None).await.unwrap();
    /// })
    /// ```
    ///
    /// Attributes marked as `None` will use the default values.
    ///
    /// Note that this returns an [`Arc<Client>`](std::sync::Arc<T>), and all methods operate over this reference counter,
    /// not over the struct itself.
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
        data_sock.bind(&addr)?;

        let ping_timeout = if let Some(ping) = ping_timeout_ms {
            Duration::from_millis(ping)
        } else {
            *DEFAULT_PING_TIMEOUT
        };

        let ping_sock = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        ping_sock.set_reuse_address(true)?;
        ping_sock.set_reuse_port(true)?;
        ping_sock.set_read_timeout(Some(ping_timeout))?;
        ping_sock.set_write_timeout(Some(ping_timeout))?;
        ping_sock.set_send_buffer_size(DEFAULT_BUFFER_SIZE)?;
        ping_sock.set_recv_buffer_size(DEFAULT_BUFFER_SIZE)?;
        ping_sock.bind(&addr)?;

        let host = data_sock.local_addr().unwrap().as_socket().unwrap().ip();

        let retry = if let Some(number) = retries {
            number // TODO return Err if this is larger than a u8
        } else {
            DEFAULT_RETRIES
        };

        Ok(Arc::new(Client {
            names_lock: RwLock::new(Vec::new()),
            devices_lock: RwLock::new(Vec::new()),
            //data_lock: Mutex::new(data_sock),
            ping_lock: Mutex::new(ping_sock),
            ping_timeout: ping_timeout,
            data_timeout: data_timeout,
            retries: retry,
            address: addr,
            host: host,
            queue: Arc::new(RwLock::new(HashMap::new())),
            history: Arc::new(RwLock::new(HashMap::new())),
        }))
    }

    pub async fn register_device(
        self: &Arc<Self>,
        name: String,
        address: String,
    ) -> Result<(), QueryError> {
        let ip = self.get_ip(address)?;
        let is_device = self.raw_ping(ip).await?; // TODO replace with queued version

        if is_device {
            let mac = self.get_mac(ip).await?;
            {
                let mut devices = self.devices_lock.write().await;
                devices.push(Target::new(ip, mac, Some(name)));
            }
            self.update_names().await?;
            Ok(())
        } else {
            Err(QueryError::Network(std::io::Error::new(
                ErrorKind::Other,
                "Ping responded, but with unexpected data",
            )))
        }
    }

    pub async fn register_event(self: &Arc<Self>, request: Request) -> Result<Uuid, QueryError> {
        let device = match &request.device {
            None => None,
            Some(device) => Some(self.get_device(device).await?),
        };
        let id = Uuid::new_v4();

        {
            self.history
                .write()
                .await
                .insert(id.clone(), Event::new(id, request, device)?);
        }

        Ok(id)
    }
    /// Sends a request
    ///
    /// The [request struct](crate::Request) already includes the registered name of the device to target.
    ///
    /// This method will serialize the request, non-blockingly send it to the target device, and return a parsed request
    /// (or error) as corresponds.
    ///
    /// # Error
    ///
    /// This method will more commonly error out if the timeout ends (the device didn't respond), if there is some
    /// oddity with the parsing, or if there is an unrecoverable network issue.
    ///
    /// # Example
    ///
    ///   
    /// ```
    /// use wizrpc::{Client, Request, Method};
    ///
    /// tokio_test::block_on(async {
    ///
    ///     let client = Client::default().await.unwrap();
    ///
    ///     client
    ///         .register_device("Office".to_string(), "192.168.0.100".to_string())
    ///         .await
    ///         .unwrap();
    ///     let request = Request::new(Some("Office".to_string()), Method::GetSystemConfig, None);
    ///
    ///     let module = client
    ///         .send(request)
    ///         .await
    ///         .unwrap()
    ///         .result
    ///         .unwrap()
    ///         .module_name
    ///         .unwrap();
    ///     assert_eq!(module.as_str(), "ESP01_SHRGB1C_31");
    /// })
    /// ```
    /// For more information on sending data, check the [request struct](crate::Request), and both the
    /// available [methods](crate::Method) and [parameters](crate::Param).
    pub async fn send(self: &Arc<Self>, request: Request) -> Result<Response, QueryError> {
        let (sender, receiver) = oneshot::channel();
        let id = self.register_event(request).await?;
        self.queue
            .write()
            .await
            .insert(id, self.retries.clone().try_into().unwrap());
        self.run_entry(id, sender, None).await?;

        let response = receiver.await?; // TODO add type
        Ok(response)
    }

    // pub async fn discover(self: &Arc<Self>) -> Result<(), QueryError> {
    //     todo!();
    //     self.update_discovery().await?;
    //     self.update_names().await?;
    //     Ok(())
    // }

    async fn get_device(self: &Arc<Self>, device: &String) -> Result<Target, QueryError> {
        let devices = self.devices().await?;
        let names = self.names().await?;

        for (index, name) in names.iter().enumerate() {
            if name == device {
                return Ok(devices[index].clone());
            }
        }

        Err(QueryError::Serialization(SerializationError::NameNotFound))
    }

    // // TODO use queue
    // async fn update_discovery(self: &Arc<Self>) -> Result<(), QueryError> {
    //     todo!();
    //     let valid_ips = self.scan().await?;

    //     let mut valid_devices = Vec::new();

    //     for ip in valid_ips {
    //         if self.raw_ping(ip).await? {
    //             valid_devices.push(ip);
    //         }
    //     }

    //     for device in valid_devices {
    //         let host = device;
    //         let mac = self.get_mac(host.clone()).await?;

    //         let mut devices = self.devices().await?;
    //         devices.push(Target::new(host, mac, None));
    //         drop(devices);
    //     }

    //     Ok(())
    // }

    async fn update_names(self: &Arc<Self>) -> Result<(), QueryError> {
        let devices = self.devices().await?;
        {
            let mut names = self.names_lock.write().await;
            let mut names_data: Vec<String> = devices
                .iter()
                .map(|target: &Target| target.name.clone())
                .collect();
            names.drain(0..);
            names.append(&mut names_data);
        }
        Ok(())
    }

    fn get_ip(self: &Arc<Self>, address: String) -> Result<IpAddr, QueryError> {
        let ip = lookup_host(address.as_str())?;
        Ok(ip[0])
    }

    async fn get_mac(self: &Arc<Self>, address: IpAddr) -> Result<MacAddr6, QueryError> {
        let (sender, receiver) = oneshot::channel();
        let id = self
            .register_event(Request::new_with_kind(
                None,
                Method::GetSystemConfig,
                None,
                RequestKind::Mac,
            ))
            .await?;

        {
            self.queue
                .write()
                .await
                .insert(id, self.retries.clone().try_into().unwrap());
        }

        self.run_entry(id, sender, Some(address)).await?;

        let response = receiver.await?;

        let params = response.result.ok_or(QueryError::Serialization(
            SerializationError::ValueDeserialization,
        ))?;

        let mac = params.mac.ok_or(QueryError::Serialization(
            SerializationError::ValueDeserialization,
        ))?;

        match MacAddr6::from_str(&mac) {
            Ok(macaddr) => Ok(macaddr),
            Err(_) => Err(QueryError::Serialization(
                SerializationError::MacAddressError,
            )),
        }
    }

    pub async fn ping(self: &Arc<Self>, device: String) -> Result<bool, QueryError> {
        let target = self.get_device(&device).await?;
        self.raw_ping(target.address).await
    }

    async fn raw_ping(self: &Arc<Self>, target: IpAddr) -> Result<bool, QueryError> {
        // TODO thread requests
        let id = self
            .register_event(Request::new_with_kind(
                None,
                Method::Ping,
                None,
                RequestKind::Ping,
            ))
            .await?;

        {
            self.queue
                .write()
                .await
                .insert(id, self.retries.clone().try_into().unwrap());
        }

        let addr = SockAddr::from(SocketAddr::new(target, *DEFAULT_TARGET_PORT));

        let mut buffer = [0; DEFAULT_BUFFER_SIZE];

        let mut sock = self.ping_lock.lock().await;

        sock.connect(&addr)?;

        retry(
            Fixed::from(self.ping_timeout)
                .map(jitter)
                .take(self.retries),
            || sock.send("{}".as_bytes()),
        )?;

        let result_length = match sock.read(&mut buffer) {
            Ok(len) => len,
            Err(err) => {
                if err.kind() == ErrorKind::Interrupted {
                    sock.read(&mut buffer)?
                } else {
                    Err(err)?
                }
            }
        };

        drop(sock);

        if result_length > 0 {
            let good_response = match self.parse_raw(&buffer, None) {
                Ok(_) => false,
                Err(err) => match err {
                    QueryError::RPC(WizNetError::MethodNotFound { .. }) => true,
                    _ => Err(err)?,
                },
            };
            {
                let mut history_write = self.history.write().await;
                let mut entry = history_write
                    .get_mut(&id)
                    .ok_or(QueryError::Sync(SyncError::BadId))?;

                entry.response_time = Some(Instant::now());
                entry.response_raw = Some(buffer.to_vec());
                let response = None;
                entry.response = response.clone();
                self.queue.write().await.remove(&id);
            }

            return Ok(good_response);
        } else {
            return Err(QueryError::Network(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Ningún dato recibido",
            )));
        }
    }

    // // TODO use queue
    // async fn scan(self: &Arc<Self>) -> Result<Vec<IpAddr>, QueryError> {
    //     if self.host.is_ipv4() {
    //         let mut host_parts: Vec<u8> = Vec::new();
    //         let host_string = self.host.to_string();
    //         let mut host_split = host_string.split('.');
    //         let deserialization_error = crate::error::SerializationError::IPAddrError.clone();

    //         host_parts.push(
    //             match host_split
    //                 .next()
    //                 .ok_or(QueryError::Serialization(deserialization_error))?
    //                 .parse()
    //             {
    //                 Ok(int) => int,
    //                 Err(_) => return Err(QueryError::Serialization(deserialization_error)),
    //             },
    //         );

    //         host_parts.push(
    //             match host_split
    //                 .next()
    //                 .ok_or(QueryError::Serialization(deserialization_error))?
    //                 .parse()
    //             {
    //                 Ok(int) => int,
    //                 Err(_) => return Err(QueryError::Serialization(deserialization_error)),
    //             },
    //         );

    //         host_parts.push(
    //             match host_split
    //                 .next()
    //                 .ok_or(QueryError::Serialization(deserialization_error))?
    //                 .parse()
    //             {
    //                 Ok(int) => int,
    //                 Err(_) => return Err(QueryError::Serialization(deserialization_error)),
    //             },
    //         );

    //         let addr_range = 0..255;

    //         let mut addresses: Vec<IpAddr> = Vec::new();

    //         for target in addr_range {
    //             addresses.push(IpAddr::V4(Ipv4Addr::new(
    //                 host_parts[0],
    //                 host_parts[1],
    //                 host_parts[2],
    //                 target,
    //             )))
    //         }

    //         let mut valid_ips = Vec::new();
    //         let mut remainders = Vec::new();

    //         for address in addresses {
    //             match self.raw_ping(address).await {
    //                 Ok(res) => {
    //                     if res {
    //                         valid_ips.push(address)
    //                     }
    //                 }
    //                 Err(err) => match err {
    //                     QueryError::Network(net_err) => {
    //                         if net_err.kind() == ErrorKind::WouldBlock {
    //                             remainders.push(address);
    //                         }
    //                     }
    //                     _ => {}
    //                 },
    //             };
    //         }

    //         if remainders.len() > 0 {
    //             for address in remainders {
    //                 match self.raw_ping(address).await {
    //                     Ok(res) => {
    //                         if res {
    //                             valid_ips.push(address)
    //                         }
    //                     }
    //                     Err(_) => {}
    //                 };
    //             }
    //         }

    //         Ok(valid_ips)
    //     } else {
    //         Err(QueryError::Network(std::io::Error::new(
    //             std::io::ErrorKind::Unsupported,
    //             "Doesn't support IPv6 yet",
    //         )))
    //     }
    // }

    async fn run_entry(
        self: &Arc<Self>,
        id: Uuid,
        sender: Sender<Response>,
        address: Option<IpAddr>,
    ) -> Result<(), QueryError> {
        let (sender_local, receiver_local) =
            oneshot::channel::<Result<[u8; DEFAULT_BUFFER_SIZE], QueryError>>();

        let addr: IpAddr;
        let mac: Option<MacAddr6>;
        let result_raw: [u8; DEFAULT_BUFFER_SIZE];

        {
            let history_read = self.history.read().await;
            let data = history_read
                .get(&id)
                .ok_or(QueryError::Sync(SyncError::BadId))?
                .clone();

            addr = match &data.target {
                Some(target) => target.address,
                None => {
                    let addr = if let Some(addr) = address {
                        addr
                    } else {
                        Err(QueryError::NoTarget)?
                    };
                    addr
                }
            };

            mac = match &data.target {
                Some(target) => Some(target.mac),
                None => None,
            };

            match self.send_raw(id, &data.request_raw, addr).await {
                Err(err) => {
                    sender_local.send(Err(err)).unwrap();
                    ()
                }
                Ok(val) => {
                    sender_local.send(Ok(val)).unwrap();
                    ()
                }
            };

            result_raw = receiver_local.await??;
        }

        let mut history_write = self.history.write().await;
        let mut entry = history_write
            .get_mut(&id)
            .ok_or(QueryError::Sync(SyncError::BadId))?;

        entry.response_time = Some(Instant::now());
        entry.response_raw = Some(result_raw.to_vec());
        let response = Some(self.parse_raw(&result_raw, mac)?);
        entry.response = response.clone();
        self.queue.write().await.remove(&id);
        sender.send(response.unwrap()).unwrap();
        Ok(())
    }

    #[async_recursion]
    async fn send_raw(
        self: &Arc<Self>,
        id: Uuid,
        data: &[u8],
        target: IpAddr,
    ) -> Result<[u8; DEFAULT_BUFFER_SIZE], QueryError> {
        // TODO thread requests
        let addr = SockAddr::from(SocketAddr::new(target, *DEFAULT_TARGET_PORT));
        let (sender, receiver) = oneshot::channel();

        let move_timeout = self.data_timeout.clone();
        let move_retries = self.retries.clone();
        let move_data = Vec::from(data).clone();

        match tokio::spawn(
            async move {
                let mut buffer = [0; DEFAULT_BUFFER_SIZE];

                let mut sock = new_socket()?;
                sock.connect(&addr)?;

                retry(
                    Fixed::from(move_timeout).map(jitter).take(move_retries),
                    || sock.send(move_data.as_slice()),
                )?;

                let result_length = match sock.read(&mut buffer) {
                    Ok(len) => len,
                    Err(err) => Err(err)?,
                };

                sender.send((buffer, result_length)).unwrap();
                Ok(())
            }
            .outputting::<Result<(), QueryError>>(),
        )
        .await?
        {
            Ok(_) => {}
            Err(err) => {
                match err {
                    QueryError::Network(error) => {
                        match error.kind() {
                            ErrorKind::WouldBlock | ErrorKind::Interrupted => {
                                let mut queue = self.queue.write().await;
                                let queue_count = queue.get_mut(&id).unwrap().clone();

                                if queue_count > 0 {
                                    {
                                        queue.insert(id, queue_count - 1);
                                        drop(queue_count);
                                        drop(queue);
                                    } // todo add error handling

                                    // FIXME This stuff is broken for some reason and threads just continue and doesn't retry
                                    self.send_raw(id, data, target);
                                }

                                Err(ErrorKind::ConnectionRefused)?
                            }
                            _ => Err(error)?,
                        }
                    }
                    QueryError::WouldBlock => {
                        let mut queue = self.queue.write().await;
                        let queue_count = queue.get_mut(&id).unwrap().clone();

                        if queue_count > 0 {
                            {
                                queue.insert(id, queue_count - 1);
                                drop(queue_count);
                                drop(queue);
                            } // todo add error handling
                            self.send_raw(id, data, target);
                        }

                        Err(ErrorKind::ConnectionRefused)?
                    }
                    _ => Err(err)?,
                }
            }
        };

        let data = receiver.await?;

        if data.1 > 0 {
            Ok(data.0)
        } else {
            return Err(QueryError::Network(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Ningún dato recibido",
            )));
        }
    }

    fn parse_raw(
        self: &Arc<Self>,
        data: &[u8],
        mac: Option<MacAddr6>,
    ) -> Result<Response, QueryError> {
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
                RPCResponse::Ok(res) => res.to_wizres(mac),
            },
        }
    }

    // async fn send_raw_parsed(
    //     self: &Arc<Self>,
    //     id: Uuid,
    //     data: &[u8],
    //     target: IpAddr,
    //     mac: Option<MacAddr6>,
    // ) -> Result<Response, QueryError> {
    //     Ok(self.parse_raw(&self.send_raw(id, data, target).await?, mac)?)
    // }

    fn trim_slice(self: &Arc<Self>, slice: &[u8]) -> Vec<u8> {
        let mut trimmed = Vec::new();

        for val in slice {
            if val != &0 {
                trimmed.push(*val);
            }
        }

        trimmed
    }

    pub async fn devices(self: &Arc<Self>) -> Result<Vec<Target>, QueryError> {
        let devices = self.devices_lock.read().await;
        Ok(devices.clone())
    }

    pub async fn names(self: &Arc<Self>) -> Result<Vec<String>, QueryError> {
        let names = self.names_lock.read().await;
        Ok(names.clone())
    }
}

use std::future::Future;
trait Outputting: Sized {
    fn outputting<O>(self) -> Self
    where
        Self: Future<Output = O>,
    {
        self
    }
}
impl<T: Future> Outputting for T {}
