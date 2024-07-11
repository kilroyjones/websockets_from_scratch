use crate::base64::Base64;
use crate::sha1::Sha1;

use std::fmt;
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::str;
use std::time::{Duration, Instant};

pub enum Frame {
    Data(Vec<u8>),
    Ping,
    Pong,
    Close,
}

#[derive(Debug)]
pub enum WebSocketError {
    IoError(io::Error),
    Utf8Error(str::Utf8Error),
    HandshakeError(String),
    NonGetRequest,
    ProtocolError(String),
}

impl fmt::Display for WebSocketError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            WebSocketError::IoError(ref err) => write!(f, "I/O error: {}", err),
            WebSocketError::Utf8Error(ref err) => write!(f, "UTF-8 decoding error: {}", err),
            WebSocketError::HandshakeError(ref msg) => write!(f, "Handshake error: {}", msg),
            WebSocketError::NonGetRequest => write!(f, "Received non-GET request"),
            WebSocketError::ProtocolError(ref msg) => write!(f, "Protocol error: {}", msg),
        }
    }
}

impl From<io::Error> for WebSocketError {
    fn from(err: io::Error) -> WebSocketError {
        WebSocketError::IoError(err)
    }
}

impl From<str::Utf8Error> for WebSocketError {
    fn from(err: str::Utf8Error) -> WebSocketError {
        WebSocketError::Utf8Error(err)
    }
}

pub struct WebSocket {
    stream: TcpStream,
    last_ping: Instant,
    id: String, // We'll get back to this later
}

impl WebSocket {
    pub fn new(stream: TcpStream) -> WebSocket {
        WebSocket {
            stream,
            last_ping: Instant::now(),
            id: String::new(),
        }
    }

    /// Connect the websocket
    ///
    /// Here we use 1024 byte buffer which should be sufficient to contain the incoming
    /// HTTP GET request needed to initialize the socket.
    ///
    /// Returns
    ///
    /// - `<(), String>`: With `()` denoting an appropriate termination of the
    /// connection and `String` providing an error message.
    ///
    pub fn connect(&mut self) -> Result<(), WebSocketError> {
        let mut buffer: [u8; 1024] = [0; 1024];

        // From the stream read in the HTTP request
        let byte_length = self.stream.read(&mut buffer)?;

        // Read only the request from the buffer
        let request = str::from_utf8(&buffer[..byte_length])?;

        // We only want to deal with GET requests for the upgrade
        if request.starts_with("GET") == false {
            return Err(WebSocketError::NonGetRequest);
        }

        // Get the HTTP response header and send it back
        let response = self.handle_websocket_handshake(request)?;
        self.stream.write_all(response.as_bytes())?;
        self.stream.flush()?;
        Ok(())
    }

    /// Validate the websocket upgrade request via the handshake
    ///
    /// # Parameters
    ///
    /// - `request`: A string of the HTTP request header
    ///
    /// # Returns
    ///
    /// - `String`: The approriate HTTP response header
    ///
    fn handle_websocket_handshake(&mut self, request: &str) -> Result<String, WebSocketError> {
        let mut base64 = Base64::new();
        let mut sha1 = Sha1::new();

        let key_header = "Sec-WebSocket-Key: ";

        // Given the request we find the line starting the the `key_header` and then find the
        // key sent from the client.
        println!("{:?}", request.lines());
        let key = request
            .lines()
            .find(|line| line.starts_with(key_header))
            .map(|line| line[key_header.len()..].trim())
            .ok_or_else(|| {
                WebSocketError::HandshakeError(
                    "Could not find Sec-WebSocket-Key in HTTP request header".to_string(),
                )
            })?;

        // Append key with the necessary id as per the WebSocket Protocol specification
        let response_key = format!("{}258EAFA5-E914-47DA-95CA-C5AB0DC85B11", key);

        // First we take the hash of the random key sent by the client
        let hash = sha1.hash(response_key).map_err(|_| {
            WebSocketError::HandshakeError("Failed to hash the response key".to_string())
        })?;

        // Second we encode that hash as Base64
        let key = base64.encode(hash).map_err(|_| {
            WebSocketError::HandshakeError("Failed to encode the hash as Base64".to_string())
        })?;

        // Lastly we attach that key to the our response header
        Ok(format!(
            "HTTP/1.1 101 Switching Protocols\r\n\
        Upgrade: websocket\r\n\
        Connection: Upgrade\r\n\
        Sec-WebSocket-Accept: {}\r\n\r\n",
            key
        ))
    }

    pub fn handle_connection<F>(&mut self, mut handler: F) -> Result<(), WebSocketError>
    where
        F: FnMut(Frame) -> Result<(), WebSocketError>,
    {
        let mut buffer = [0; 2048];

        loop {
            if self.last_ping.elapsed() > Duration::from_secs(5) {
                self.send_ping()?;
                self.last_ping = Instant::now();
            }

            match self.stream.read(&mut buffer) {
                Ok(n) if n > 0 => match self.parse_frame(&buffer[..n]) {
                    Ok(frame) => {
                        if let Frame::Close = frame {
                            return Ok(());
                        }
                        handler(frame)?;
                    }
                    Err(e) => {
                        return Err(WebSocketError::ProtocolError(format!(
                            "Error parsing frame: {}",
                            e
                        )));
                    }
                },
                Ok(_) => continue,
                Err(e) if e.kind() != io::ErrorKind::WouldBlock => {
                    return Err(WebSocketError::from(e))
                }
                Err(_) => continue,
            }
        }
    }

    fn send_ping(&mut self) -> io::Result<usize> {
        self.stream.write(&[0x89, 0x00])
    }

    fn send_pong(&mut self) -> io::Result<usize> {
        self.stream.write(&[0x8A, 0x00]) // Opcode for pong is 0xA and FIN set
    }

    pub fn send_text(&mut self, data: &str) -> io::Result<()> {
        let mut frame = Vec::new();
        frame.push(0x81); // 0x80 | 0x01, FIN bit and opcode for text frames

        let data_bytes = data.as_bytes();
        let length = data_bytes.len();
        if length <= 125 {
            frame.push(length as u8); // Payload length fits in one byte
        } else if length <= 65535 {
            frame.push(126); // Signal that the next two bytes contain the payload length
            frame.extend_from_slice(&(length as u16).to_be_bytes());
        } else {
            frame.push(127); // Signal that the next eight bytes contain the payload length
            frame.extend_from_slice(&(length as u64).to_be_bytes());
        }

        frame.extend_from_slice(data_bytes);

        self.stream.write_all(&frame)?;
        self.stream.flush()
    }

    fn parse_frame(&mut self, buffer: &[u8]) -> Result<Frame, &'static str> {
        if buffer.len() < 2 {
            return Err("Frame too short");
        }

        let first_byte = buffer[0];
        let fin = (first_byte & 0x80) != 0;
        let opcode = first_byte & 0x0F;
        let second_byte = buffer[1];
        let masked = (second_byte & 0x80) != 0;
        let mut payload_len = (second_byte & 0x7F) as usize;

        if !masked {
            return Err("Frames from client must be masked");
        }

        let mut offset = 2;
        if payload_len == 126 {
            if buffer.len() < 4 {
                return Err("Frame too short for extended payload length");
            }
            payload_len = u16::from_be_bytes([buffer[offset], buffer[offset + 1]]) as usize;
            offset += 2;
        } else if payload_len == 127 {
            return Err("Extended payload length too large");
        }

        if buffer.len() < offset + 4 + payload_len {
            return Err("Frame too short for mask and data");
        }

        let mask = &buffer[offset..offset + 4];
        offset += 4;

        let mut data = Vec::with_capacity(payload_len);
        for i in 0..payload_len {
            data.push(buffer[offset + i] ^ mask[i % 4]);
        }

        Ok(match opcode {
            0x01 => Frame::Data(data), // text frame
            0x02 => Frame::Data(data), // binary frame
            0x08 => Frame::Close,      // close frame
            0x09 => Frame::Ping,       // ping frame
            0x0A => Frame::Pong,       // pong frame
            _ => return Err("Unknown opcode"),
        })
    }
}
