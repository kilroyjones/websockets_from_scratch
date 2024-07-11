mod base64;
mod sha1;
mod websocket;

use std::net::TcpListener;
use std::thread;

use websocket::{Frame, WebSocket, WebSocketError};

struct WebSocketManager {
    listener: TcpListener,
}

impl WebSocketManager {
    fn new(addr: &str) -> Self {
        let listener = TcpListener::bind(addr).unwrap();
        WebSocketManager { listener }
    }

    fn start<F>(&self, handler: F)
    where
        F: FnMut(Frame) -> Result<(), WebSocketError> + Send + 'static + Clone,
    {
        for stream in self.listener.incoming() {
            match stream {
                Ok(stream) => {
                    let handler_clone = handler.clone();
                    thread::spawn(move || {
                        let mut ws_conn = WebSocket::new(stream);
                        if let Err(e) = ws_conn.connect() {
                            eprintln!("Failed to connect: {}", e);
                            return;
                        }
                        if let Err(e) = ws_conn.handle_connection(handler_clone) {
                            eprintln!("Connection error: {}", e);
                        }
                        println!("Connection ended");
                    });
                }
                Err(e) => {
                    println!("Error: {}", e);
                }
            }
        }
    }
}

fn main() {
    let manager = WebSocketManager::new("127.0.0.1:8080");

    println!("Server listening on port 8080");

    manager.start(|frame| {
        match frame {
            Frame::Data(data) => {
                println!("Received data: {}", String::from_utf8_lossy(&data));
                // Handle data...
            }
            Frame::Ping => {
                println!("Received Ping");
                // Handle ping...
            }
            Frame::Pong => {
                println!("Received Pong");
                // Handle pong...
            }
            Frame::Close => {
                println!("Received Close");
                // Handle close...
                return Ok(());
            }
        }
        Ok(())
    });
}
//
