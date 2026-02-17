use bytes::Bytes;
use tracing::{debug, trace, warn};

/// Parsed H.264 decoder configuration (SPS + PPS).
#[derive(Debug, Clone)]
pub struct AvcDecoderConfig {
    pub sps: Vec<Vec<u8>>,
    pub pps: Vec<Vec<u8>>,
    pub nalu_length_size: u8,
}

/// Result of parsing an RTMP video data packet.
#[derive(Debug)]
pub enum VideoPacket {
    /// AVC sequence header containing SPS/PPS
    SequenceHeader(AvcDecoderConfig),
    /// AVCC-framed video data: [4-byte len][NAL1][4-byte len][NAL2]...
    NaluData { avcc_payload: Bytes, timestamp: u32 },
    /// End of sequence
    EndOfSequence,
    /// Not H.264 or not AVC — skip
    Unsupported,
}

/// Parse an RTMP video data payload (FLV video tag body).
///
/// FLV video tag format:
///   byte 0: frame type (4 bits) | codec id (4 bits)
///   For AVC (codec id 7):
///     byte 1: AVC packet type (0=seq header, 1=NALU, 2=end of seq)
///     bytes 2-4: composition time offset (signed, 24-bit)
///     bytes 5+: AVC data
pub fn parse_video_data(data: &Bytes, timestamp: u32) -> VideoPacket {
    if data.len() < 2 {
        return VideoPacket::Unsupported;
    }

    let codec_id = data[0] & 0x0F;
    if codec_id != 7 {
        // Not H.264/AVC
        trace!(codec_id, "non-AVC video codec, skipping");
        return VideoPacket::Unsupported;
    }

    let avc_packet_type = data[1];

    match avc_packet_type {
        0 => parse_sequence_header(data),
        1 => parse_nalu_data(data, timestamp),
        2 => VideoPacket::EndOfSequence,
        _ => {
            warn!(avc_packet_type, "unknown AVC packet type");
            VideoPacket::Unsupported
        }
    }
}

/// Parse AVCDecoderConfigurationRecord from sequence header.
///
/// Format (ISO 14496-15):
///   byte 0: version (always 1)
///   byte 1: profile
///   byte 2: profile compat
///   byte 3: level
///   byte 4: 0b111111xx where xx = (nalu_length_size - 1)
///   byte 5: 0b111xxxxx where xxxxx = num_sps
///   For each SPS:
///     2 bytes: sps_length
///     sps_length bytes: SPS data
///   1 byte: num_pps
///   For each PPS:
///     2 bytes: pps_length
///     pps_length bytes: PPS data
fn parse_sequence_header(data: &Bytes) -> VideoPacket {
    // Skip: video tag header (1 byte) + avc packet type (1 byte) + composition time (3 bytes)
    let offset = 5;
    if data.len() < offset + 6 {
        warn!("sequence header too short");
        return VideoPacket::Unsupported;
    }

    let config = &data[offset..];

    let version = config[0];
    if version != 1 {
        warn!(version, "unexpected AVCDecoderConfigurationRecord version");
        return VideoPacket::Unsupported;
    }

    let profile = config[1];
    let level = config[3];
    let nalu_length_size = (config[4] & 0x03) + 1;
    debug!(profile, level, nalu_length_size, "AVC decoder config");

    let num_sps = (config[5] & 0x1F) as usize;
    let mut pos = 6;
    let mut sps = Vec::with_capacity(num_sps);

    for _ in 0..num_sps {
        if pos + 2 > config.len() {
            warn!("truncated SPS length");
            return VideoPacket::Unsupported;
        }
        let sps_len = u16::from_be_bytes([config[pos], config[pos + 1]]) as usize;
        pos += 2;
        if pos + sps_len > config.len() {
            warn!("truncated SPS data");
            return VideoPacket::Unsupported;
        }
        sps.push(config[pos..pos + sps_len].to_vec());
        pos += sps_len;
    }

    if pos >= config.len() {
        warn!("truncated PPS count");
        return VideoPacket::Unsupported;
    }

    let num_pps = config[pos] as usize;
    pos += 1;
    let mut pps = Vec::with_capacity(num_pps);

    for _ in 0..num_pps {
        if pos + 2 > config.len() {
            warn!("truncated PPS length");
            return VideoPacket::Unsupported;
        }
        let pps_len = u16::from_be_bytes([config[pos], config[pos + 1]]) as usize;
        pos += 2;
        if pos + pps_len > config.len() {
            warn!("truncated PPS data");
            return VideoPacket::Unsupported;
        }
        pps.push(config[pos..pos + pps_len].to_vec());
        pos += pps_len;
    }

    debug!(num_sps = sps.len(), num_pps = pps.len(), "parsed AVC decoder config");

    VideoPacket::SequenceHeader(AvcDecoderConfig {
        sps,
        pps,
        nalu_length_size,
    })
}

/// Extract AVCC-formatted payload from a video data packet.
///
/// Returns the raw AVCC payload (length-prefixed NAL units) for direct
/// submission to VideoToolbox as a single CMSampleBuffer.
fn parse_nalu_data(data: &Bytes, timestamp: u32) -> VideoPacket {
    // Skip: video tag header (1 byte) + avc packet type (1 byte) + composition time (3 bytes)
    let offset = 5;
    if data.len() <= offset {
        return VideoPacket::Unsupported;
    }

    let avcc_payload = data.slice(offset..);
    trace!(len = avcc_payload.len(), timestamp, "AVCC payload");
    VideoPacket::NaluData { avcc_payload, timestamp }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_non_avc() {
        let data = Bytes::from_static(&[0x22, 0x00]); // codec_id = 2 (Sorenson H.263)
        assert!(matches!(parse_video_data(&data, 0), VideoPacket::Unsupported));
    }

    #[test]
    fn test_parse_end_of_sequence() {
        // frame_type=1 (keyframe) | codec_id=7 (AVC), packet_type=2 (end of seq)
        let data = Bytes::from_static(&[0x17, 0x02]);
        assert!(matches!(parse_video_data(&data, 0), VideoPacket::EndOfSequence));
    }

    #[test]
    fn test_parse_sequence_header() {
        // Minimal AVCDecoderConfigurationRecord
        let mut buf = vec![
            0x17, // keyframe + AVC
            0x00, // sequence header
            0x00, 0x00, 0x00, // composition time
            // AVCDecoderConfigurationRecord:
            0x01, // version
            0x64, // profile (High)
            0x00, // profile compat
            0x1F, // level 3.1
            0xFF, // nalu_length_size = 4
            0xE1, // num_sps = 1
        ];
        // SPS: 4 bytes
        buf.extend_from_slice(&[0x00, 0x04]); // sps_length = 4
        buf.extend_from_slice(&[0x67, 0x64, 0x00, 0x1F]); // SPS data
        // PPS
        buf.push(0x01); // num_pps = 1
        buf.extend_from_slice(&[0x00, 0x03]); // pps_length = 3
        buf.extend_from_slice(&[0x68, 0xEB, 0xE3]); // PPS data

        let data = Bytes::from(buf);
        match parse_video_data(&data, 0) {
            VideoPacket::SequenceHeader(config) => {
                assert_eq!(config.sps.len(), 1);
                assert_eq!(config.pps.len(), 1);
                assert_eq!(config.nalu_length_size, 4);
                assert_eq!(config.sps[0], &[0x67, 0x64, 0x00, 0x1F]);
                assert_eq!(config.pps[0], &[0x68, 0xEB, 0xE3]);
            }
            other => panic!("expected SequenceHeader, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_nalu_data() {
        let mut buf = vec![
            0x27, // inter frame + AVC
            0x01, // NALU
            0x00, 0x00, 0x00, // composition time
        ];
        // First NAL unit: 5 bytes
        buf.extend_from_slice(&[0x00, 0x00, 0x00, 0x05]);
        buf.extend_from_slice(&[0x65, 0x88, 0x80, 0x40, 0x00]);
        // Second NAL unit: 3 bytes
        buf.extend_from_slice(&[0x00, 0x00, 0x00, 0x03]);
        buf.extend_from_slice(&[0x06, 0x05, 0x00]);

        let data = Bytes::from(buf);
        match parse_video_data(&data, 100) {
            VideoPacket::NaluData { avcc_payload, timestamp } => {
                assert_eq!(timestamp, 100);
                // AVCC payload should contain both NAL units with length prefixes
                let expected: &[u8] = &[
                    0x00, 0x00, 0x00, 0x05, 0x65, 0x88, 0x80, 0x40, 0x00,
                    0x00, 0x00, 0x00, 0x03, 0x06, 0x05, 0x00,
                ];
                assert_eq!(&avcc_payload[..], expected);
            }
            other => panic!("expected NaluData, got {:?}", other),
        }
    }
}
