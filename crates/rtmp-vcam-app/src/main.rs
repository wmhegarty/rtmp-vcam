mod ipc;

use std::net::SocketAddr;
use std::sync::Arc;

use bytes::Bytes;
use tracing::{error, info};

use rtmp_server::{AvcDecoderConfig, VideoSink};
use video_pipeline::H264Decoder;

use crate::ipc::SharedFrameBuffer;

/// VideoSink implementation that decodes H.264 and copies pixel data to shared memory.
struct DecoderSink {
    decoder: Option<H264Decoder>,
    shm: Arc<SharedFrameBuffer>,
}

impl DecoderSink {
    fn new(shm: Arc<SharedFrameBuffer>) -> Self {
        Self {
            decoder: None,
            shm,
        }
    }
}

impl VideoSink for DecoderSink {
    fn on_decoder_config(&mut self, config: AvcDecoderConfig) {
        info!(
            sps_count = config.sps.len(),
            pps_count = config.pps.len(),
            nalu_length_size = config.nalu_length_size,
            "received decoder configuration, creating VT decoder"
        );

        match H264Decoder::new(
            &config.sps,
            &config.pps,
            config.nalu_length_size,
            self.shm.ptr(),
        ) {
            Ok(decoder) => {
                self.decoder = Some(decoder);
                info!("H264 decoder created successfully");
            }
            Err(e) => {
                error!(%e, "failed to create H264 decoder");
            }
        }
    }

    fn on_video_data(&mut self, data: Bytes, timestamp: u32) {
        if let Some(decoder) = &mut self.decoder {
            if let Err(e) = decoder.decode_avcc(&data, timestamp) {
                // Don't log every bad data error (common for B-frames before IDR)
                if !e.contains("-12909") {
                    tracing::warn!(%e, "decode error");
                }
            }
        }
    }
}

fn parse_args() -> (SocketAddr, bool) {
    let mut port: u16 = 1935;
    let mut verbose = false;

    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--port" | "-p" => {
                if i + 1 < args.len() {
                    port = args[i + 1].parse().unwrap_or(1935);
                    i += 1;
                }
            }
            "--verbose" | "-v" => {
                verbose = true;
            }
            "--help" | "-h" => {
                println!("rtmp-vcam — RTMP Virtual Camera for macOS");
                println!();
                println!("Usage: rtmp-vcam-app [OPTIONS]");
                println!();
                println!("Options:");
                println!("  -p, --port <PORT>    RTMP listen port (default: 1935)");
                println!("  -v, --verbose        Enable debug logging");
                println!("  -h, --help           Show this help");
                std::process::exit(0);
            }
            _ => {}
        }
        i += 1;
    }

    let addr: SocketAddr = format!("0.0.0.0:{port}").parse().unwrap();
    (addr, verbose)
}

#[tokio::main]
async fn main() {
    let (addr, verbose) = parse_args();

    // Initialize tracing
    let filter = if verbose {
        "rtmp_server=debug,video_pipeline=debug,rtmp_vcam_app=debug"
    } else {
        "rtmp_server=info,video_pipeline=info,rtmp_vcam_app=info"
    };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| filter.into()),
        )
        .init();

    info!("rtmp-vcam starting");

    // Create shared memory for IPC with the Camera Extension
    let shm = match SharedFrameBuffer::create() {
        Ok(shm) => Arc::new(shm),
        Err(e) => {
            error!(%e, "failed to create shared memory");
            std::process::exit(1);
        }
    };

    // Set up Ctrl+C handler to clean up shared memory
    let shm_for_ctrlc = Arc::clone(&shm);
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        info!("shutting down...");
        drop(shm_for_ctrlc);
        std::process::exit(0);
    });

    info!(%addr, "starting RTMP server");

    let shm_clone = Arc::clone(&shm);

    // Start the RTMP server
    if let Err(e) = rtmp_server::server::run(addr, move || {
        Box::new(DecoderSink::new(Arc::clone(&shm_clone)))
    })
    .await
    {
        error!(%e, "RTMP server error");
        std::process::exit(1);
    }
}
