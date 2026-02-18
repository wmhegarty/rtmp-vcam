use std::io;
use std::net::SocketAddr;

use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, TcpStream};
use tracing::{error, info, warn};

use crate::handshake::HandshakeState;
use crate::session::{RtmpSession, VideoSink};

/// Start the RTMP server on the given address.
/// Calls `sink_factory` for each new connection to get a VideoSink.
/// If `stream_key` is `Some`, only clients publishing with that key are accepted.
pub async fn run<F>(addr: SocketAddr, sink_factory: F, stream_key: Option<String>) -> io::Result<()>
where
    F: Fn() -> Box<dyn VideoSink> + Send + Sync + 'static,
{
    let listener = TcpListener::bind(addr).await?;
    if stream_key.is_some() {
        info!(%addr, "RTMP server listening (stream key required)");
    } else {
        info!(%addr, "RTMP server listening (no stream key — accepting all)");
    }

    loop {
        let (stream, peer_addr) = listener.accept().await?;
        info!(%peer_addr, "new connection");

        let mut sink = sink_factory();
        let key = stream_key.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, peer_addr, &mut *sink, key).await {
                if e.kind() == io::ErrorKind::PermissionDenied {
                    warn!(%peer_addr, "connection rejected: {e}");
                } else {
                    error!(%peer_addr, %e, "connection error");
                }
            }
            info!(%peer_addr, "connection closed");
        });
    }
}

async fn handle_connection(
    mut stream: TcpStream,
    peer_addr: SocketAddr,
    sink: &mut dyn VideoSink,
    stream_key: Option<String>,
) -> io::Result<()> {
    let mut buf = vec![0u8; 4096];

    // Phase 1: RTMP Handshake
    let mut handshake = HandshakeState::new();
    let remaining = loop {
        let n = stream.read(&mut buf).await?;
        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "connection closed during handshake",
            ));
        }

        let (response, maybe_remaining) = handshake.process(&buf[..n])?;
        if !response.is_empty() {
            use tokio::io::AsyncWriteExt;
            stream.write_all(&response).await?;
            stream.flush().await?;
        }

        if let Some(remaining) = maybe_remaining {
            info!(%peer_addr, "handshake complete");
            break remaining;
        }
    };

    // Phase 2: RTMP Session
    let mut session = RtmpSession::new(&mut stream, stream_key).await?;

    // Process any leftover bytes from the handshake
    if !remaining.is_empty() {
        session
            .handle_input(&remaining, &mut stream, sink)
            .await?;
    }

    // Main read loop
    loop {
        let n = stream.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        session.handle_input(&buf[..n], &mut stream, sink).await?;
    }

    Ok(())
}
