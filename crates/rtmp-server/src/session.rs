use bytes::Bytes;
use rml_rtmp::sessions::{
    ServerSession, ServerSessionConfig, ServerSessionEvent, ServerSessionResult,
};
use std::io;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tracing::{debug, info, trace};

use crate::flv::{self, AvcDecoderConfig, VideoPacket};

/// Callback for receiving decoded video data from the RTMP session.
pub trait VideoSink: Send + 'static {
    /// Called when an AVC sequence header (SPS/PPS) is received.
    fn on_decoder_config(&mut self, config: AvcDecoderConfig);

    /// Called with AVCC-framed NAL units for a single video frame.
    /// Data is already in AVCC format: [4-byte len][NAL1][4-byte len][NAL2]...
    fn on_video_data(&mut self, data: Bytes, timestamp: u32);
}

/// Manages one RTMP publishing session.
pub struct RtmpSession {
    session: ServerSession,
}

impl RtmpSession {
    /// Create a new RTMP session and send initial protocol messages to the client.
    pub async fn new(stream: &mut TcpStream) -> io::Result<Self> {
        let config = ServerSessionConfig::new();
        let (session, initial_results) = ServerSession::new(config).map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("failed to create ServerSession: {e:?}"),
            )
        })?;

        // Send initial RTMP messages (chunk size, window ack, etc.)
        for result in initial_results {
            if let ServerSessionResult::OutboundResponse(packet) = result {
                stream.write_all(&packet.bytes).await?;
            }
        }
        stream.flush().await?;

        debug!("RTMP session created, initial messages sent");
        Ok(Self { session })
    }

    /// Process incoming RTMP data and dispatch events.
    /// Returns bytes to send back to the client.
    pub async fn handle_input(
        &mut self,
        data: &[u8],
        stream: &mut TcpStream,
        sink: &mut dyn VideoSink,
    ) -> io::Result<()> {
        let results = self.session.handle_input(data).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("session handle_input error: {e:?}"),
            )
        })?;

        for result in results {
            match result {
                ServerSessionResult::OutboundResponse(packet) => {
                    stream.write_all(&packet.bytes).await?;
                }
                ServerSessionResult::RaisedEvent(event) => {
                    self.handle_event(event, stream, sink).await?;
                }
                ServerSessionResult::UnhandleableMessageReceived(msg) => {
                    trace!("unhandled RTMP message: type_id={}", msg.type_id);
                }
            }
        }
        stream.flush().await?;
        Ok(())
    }

    async fn handle_event(
        &mut self,
        event: ServerSessionEvent,
        stream: &mut TcpStream,
        sink: &mut dyn VideoSink,
    ) -> io::Result<()> {
        match event {
            ServerSessionEvent::ConnectionRequested {
                request_id,
                app_name,
            } => {
                info!(app_name, "connection requested, accepting");
                let results = self.accept(request_id)?;
                self.send_results(results, stream).await?;
            }

            ServerSessionEvent::PublishStreamRequested {
                request_id,
                app_name,
                stream_key,
                mode,
            } => {
                info!(app_name, stream_key, ?mode, "publish requested, accepting");
                let results = self.accept(request_id)?;
                self.send_results(results, stream).await?;
            }

            ServerSessionEvent::VideoDataReceived {
                data, timestamp, ..
            } => {
                let ts = timestamp.value as u32;
                match flv::parse_video_data(&data, ts) {
                    VideoPacket::SequenceHeader(config) => {
                        info!("received AVC sequence header");
                        sink.on_decoder_config(config);
                    }
                    VideoPacket::NaluData { avcc_payload, timestamp } => {
                        sink.on_video_data(avcc_payload, timestamp);
                    }
                    VideoPacket::EndOfSequence => {
                        info!("received end of sequence");
                    }
                    VideoPacket::Unsupported => {}
                }
            }

            ServerSessionEvent::StreamMetadataChanged {
                app_name,
                stream_key,
                metadata,
            } => {
                info!(
                    app_name,
                    stream_key,
                    ?metadata,
                    "stream metadata changed"
                );
            }

            ServerSessionEvent::PublishStreamFinished {
                app_name,
                stream_key,
            } => {
                info!(app_name, stream_key, "publish finished");
            }

            ServerSessionEvent::AudioDataReceived { .. } => {
                // We only care about video
                trace!("audio data received (ignored)");
            }

            ServerSessionEvent::ReleaseStreamRequested { request_id, .. } => {
                let results = self.accept(request_id)?;
                self.send_results(results, stream).await?;
            }

            other => {
                debug!(?other, "unhandled RTMP event");
            }
        }

        Ok(())
    }

    fn accept(&mut self, request_id: u32) -> io::Result<Vec<ServerSessionResult>> {
        self.session.accept_request(request_id).map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("accept_request error: {e:?}"),
            )
        })
    }

    async fn send_results(
        &self,
        results: Vec<ServerSessionResult>,
        stream: &mut TcpStream,
    ) -> io::Result<()> {
        for result in results {
            if let ServerSessionResult::OutboundResponse(packet) = result {
                stream.write_all(&packet.bytes).await?;
            }
        }
        stream.flush().await?;
        Ok(())
    }
}
