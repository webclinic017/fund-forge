use std::io;
use rustls::ServerConfig;
use std::net::SocketAddr;
use tokio_rustls::TlsAcceptor;
use std::sync::Arc;
use std::sync::atomic::{AtomicU16, AtomicUsize, Ordering};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::server::TlsStream;
use ff_standard_lib::messages::data_server_messaging::{DataServerRequest, DataServerResponse};
use ff_standard_lib::standardized_types::enums::StrategyMode;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use ff_standard_lib::standardized_types::bytes_trait::Bytes;
use crate::request_handlers::manage_async_requests;
use crate::subscribe_server_shutdown;
use socket2::{Socket, Domain, Type, Protocol};
use tokio::sync::Notify;

pub(crate) async fn create_listener(addr: SocketAddr) -> io::Result<TcpListener> {
    let domain = if addr.is_ipv4() { Domain::IPV4 } else { Domain::IPV6 };

    let socket = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))?;

    socket.set_reuse_address(true)?;
    #[cfg(unix)] // Unix-specific option
    socket.set_reuse_port(true)?;

    // For IPv6, we might want to handle both IPv4 and IPv6
    if addr.is_ipv6() {
        socket.set_only_v6(false)?;
    }

    socket.bind(&addr.into())?;
    socket.listen(1024)?;

    // Set non-blocking mode for tokio
    socket.set_nonblocking(true)?;

    TcpListener::from_std(socket.into())
}

use std::collections::HashSet;
use std::sync::Mutex;

pub(crate) async fn async_server(config: ServerConfig, addr: SocketAddr) {
    let acceptor = TlsAcceptor::from(Arc::new(config));

    let listener = match create_listener(addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Server: Failed to create listener on {}: {}", addr, e);
            return;
        }
    };

    println!("Listening on: {}", addr);

    let mut shutdown_receiver = subscribe_server_shutdown();
    let active_connections = Arc::new(AtomicUsize::new(0));
    let listener = Arc::new(listener);
    let shutdown_complete_tx = Arc::new(Notify::new());
    let shutdown_complete_rx = shutdown_complete_tx.clone();

    // Track active connection IDs to ensure uniqueness
    let active_ids = Arc::new(Mutex::new(HashSet::new()));
    let next_id = Arc::new(AtomicU16::new(1));

    loop {
        tokio::select! {
            result = listener.accept() => {
                match result {
                    Ok((stream, _peer_addr)) => {
                        let acceptor = acceptor.clone();
                        let active_connections = active_connections.clone();
                        let shutdown_complete = shutdown_complete_tx.clone();
                        let active_ids = active_ids.clone();
                        let next_id = next_id.clone();

                        active_connections.fetch_add(1, Ordering::SeqCst);

                        // Find next available ID
                        let conn_id = {
                            let mut ids = active_ids.lock().unwrap();
                            let mut candidate = next_id.load(Ordering::SeqCst);
                            // Keep trying until we find an unused ID
                            while ids.contains(&candidate) {
                                candidate = candidate.wrapping_add(1);
                            }
                            ids.insert(candidate);
                            next_id.store(candidate.wrapping_add(1), Ordering::SeqCst);
                            candidate
                        };

                        tokio::spawn(async move {
                            match acceptor.accept(stream).await {
                                Ok(tls_stream) => {
                                    handle_async_connection(tls_stream, conn_id).await;
                                }
                                Err(_e) => {
                                    //eprintln!("Server: Failed to accept TLS connection: {:?}", e);
                                }
                            }
                            // Remove ID from active set when connection closes
                            {
                                let mut ids = active_ids.lock().unwrap();
                                ids.remove(&conn_id);
                            }
                            if active_connections.fetch_sub(1, Ordering::SeqCst) == 1 {
                                shutdown_complete.notify_one();
                            }
                        });
                    }
                    Err(_e) => {
                        //eprintln!("Server: Failed to accept connection: {:?}", e);
                        continue;
                    }
                }
            },

            _ = shutdown_receiver.recv() => {
                println!("Server: Shutdown signal received, stopping accept loop");
                break;
            }
        }
    }

    let timeout = tokio::time::sleep(std::time::Duration::from_secs(10));
    tokio::pin!(timeout);

    tokio::select! {
        _ = shutdown_complete_rx.notified() => {
            println!("Server: All connections completed gracefully");
        }
        _ = &mut timeout => {
            println!("Server: Shutdown timeout reached, forcing close");
        }
    }

    drop(listener);
}

async fn handle_async_connection(mut tls_stream: TlsStream<TcpStream>, stream_name: u16) {
    const LENGTH: usize = 4;
    let mut length_bytes = [0u8; LENGTH];
    let mut mode = StrategyMode::Backtest;
    while let Ok(_) = tls_stream.read_exact(&mut length_bytes).await {
        // Parse the length from the header
        let msg_length = u32::from_be_bytes(length_bytes) as usize;
        let mut message_body = vec![0u8; msg_length];

        // Read the message body based on the length
        match tls_stream.read_exact(&mut message_body).await {
            Ok(_) => {},
            Err(_e) => {
                //eprintln!("Server: Error reading message body: {}", _e);
                return;
            }
        }

        // Parse the request from the message body
        let request = match DataServerRequest::from_bytes(&message_body) {
            Ok(req) => req,
            Err(_e) => {
                //eprintln!("Server: Failed to parse request: {:?}", _e);
                return;
            }
        };
        //println!("{:?}", request);
        // Handle the request and generate a response
        match request {
            DataServerRequest::Register(registered_mode) => {
                mode = registered_mode;
                break;
            },
            _ => eprintln!("Server: Strategy Did not register a Strategy mode")
        }
    }
    //println!("Server: TLS connection established with {:?}", peer_addr);


    // If we are using live stream send the stream response so that the strategy can
    if mode == StrategyMode::Live || mode == StrategyMode::LivePaperTrading {
        let response = DataServerResponse::RegistrationResponse(stream_name.clone());
        // Convert the response to bytes
        let bytes = response.to_bytes();

        // Prepare the message with a 4-byte length header in big-endian format
        let length = (bytes.len() as u64).to_be_bytes();
        let mut prefixed_msg = Vec::with_capacity(8 + bytes.len());
        prefixed_msg.extend_from_slice(&length);
        prefixed_msg.extend_from_slice(&bytes);

        // Write the response to the stream
        if let Err(_e) = tls_stream.write_all(&prefixed_msg).await {
            return;
            // Handle the error (log it or take some other action)
        }
    }

    manage_async_requests(
        mode,
        tls_stream,
        stream_name
    ).await;
}