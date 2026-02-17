use bytes::Bytes;
use rml_rtmp::handshake::{Handshake, HandshakeProcessResult, PeerType};
use std::io;

/// Drives the RTMP handshake to completion, returning any leftover bytes
/// that belong to the RTMP session (post-handshake data).
pub struct HandshakeState {
    inner: Handshake,
    completed: bool,
}

impl HandshakeState {
    pub fn new() -> Self {
        Self {
            inner: Handshake::new(PeerType::Server),
            completed: false,
        }
    }

    /// Process incoming bytes. Returns (response_bytes_to_send, maybe_remaining).
    /// If `maybe_remaining` is Some, the handshake is complete and the bytes
    /// are leftover RTMP data to feed into ServerSession.
    pub fn process(&mut self, data: &[u8]) -> io::Result<(Bytes, Option<Bytes>)> {
        if self.completed {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "handshake already completed",
            ));
        }

        match self.inner.process_bytes(data) {
            Ok(HandshakeProcessResult::InProgress { response_bytes }) => {
                Ok((Bytes::from(response_bytes), None))
            }
            Ok(HandshakeProcessResult::Completed {
                response_bytes,
                remaining_bytes,
            }) => {
                self.completed = true;
                Ok((Bytes::from(response_bytes), Some(Bytes::from(remaining_bytes))))
            }
            Err(e) => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("handshake error: {e:?}"),
            )),
        }
    }

    pub fn is_completed(&self) -> bool {
        self.completed
    }
}
