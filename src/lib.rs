use anyhow::{anyhow, Error};
use byteorder::{ReadBytesExt, LE};
use caf::{chunks::AudioDescription, writing::PacketWriter, FormatType};
use ogg::PacketReader;
use std::io::{Cursor, Read, Seek, Write};

pub fn convert<R: Read + Seek, W: Write>(rdr: R, wtr: W) -> Result<(), Error> {
    // read
    let mut packet_reader = PacketReader::new(rdr);

    // read header packets
    let id_header_packet = packet_reader
        .read_packet()?
        .ok_or(anyhow!("missing identification header packet"))?;
    let _comment_header_packet = packet_reader
        .read_packet()?
        .ok_or(anyhow!("missing comment header packet"))?;

    // parse opus headers into caf audio description
    let opus_head = OpusHead::read(Cursor::new(id_header_packet.data))?;
    let sample_rate = if opus_head.input_sample_rate == 0 {
        48000.0
    } else {
        opus_head.input_sample_rate as f64
    };
    let audio_description = AudioDescription {
        sample_rate,
        format_id: FormatType::Other(u32::from_be_bytes(*b"opus")),
        format_flags: 0,
        bytes_per_packet: 0,
        frames_per_packet: 960, // TODO: we probably can't assume this
        channels_per_frame: opus_head.channel_count as u32,
        bits_per_channel: 0,
    };

    // write
    let mut packet_writer = PacketWriter::new(wtr, &audio_description)?;

    // preskip
    packet_writer.set_priming_frames(opus_head.preskip as i32);

    // read audio data packets from ogg and add them to caf
    while let Some(packet) = packet_reader.read_packet()? {
        packet_writer.add_packet(&packet.data, None)?;
    }
    packet_writer.write_audio_data()?;

    Ok(())
}

pub struct OpusHead {
    channel_count: u8,
    preskip: u16,
    input_sample_rate: u32,
    output_gain: i16,
    channel_mapping_family: u8,
    // channel_mapping_table: Option<>, // not implemented
}

impl OpusHead {
    pub fn read<T: Read>(mut rdr: T) -> Result<Self, Error> {
        let mut magic = [0; 8];
        rdr.read_exact(&mut magic)?;
        if magic != *b"OpusHead" {
            return Err(anyhow!("missing magic signature"));
        }

        let version = rdr.read_u8()?;
        if version != 0x01 {
            return Err(anyhow!("invalid version"));
        }

        let channel_count = rdr.read_u8()?;
        let preskip = rdr.read_u16::<LE>()?;
        let input_sample_rate = rdr.read_u32::<LE>()?;
        let output_gain = rdr.read_i16::<LE>()?;

        let channel_mapping_family = rdr.read_u8()?;
        if channel_mapping_family != 0 {
            let _stream_count = rdr.read_u8()?;
            let _coupled_count = rdr.read_u8()?;

            // skip channel_count * 8 bytes for the channel mappings
            std::io::copy(
                &mut rdr.by_ref().take(8 * channel_count as u64),
                &mut std::io::sink(),
            )?;
        } else {
            // no channel mapping table
        }

        Ok(OpusHead {
            channel_count,
            preskip,
            input_sample_rate,
            output_gain,
            channel_mapping_family,
        })
    }
}

pub struct OpusTags {
    vendor_string: String,
    user_comments: Vec<String>,
}

impl OpusTags {
    pub fn read<T: Read>(mut rdr: T) -> Result<Self, Error> {
        let mut magic = [0; 8];
        rdr.read_exact(&mut magic)?;
        if magic != *b"OpusTags" {
            return Err(anyhow!("missing magic signature"));
        }

        let vendor_string_len = rdr.read_u32::<LE>()?;
        let mut vendor_string_bytes = vec![0; vendor_string_len as usize];
        rdr.read_exact(&mut vendor_string_bytes)?;
        let vendor_string = String::from_utf8(vendor_string_bytes)?;

        let user_comments_count = rdr.read_u32::<LE>()?;
        let mut user_comments = Vec::new();
        for _ in 0..user_comments_count {
            let user_comment_len = rdr.read_u32::<LE>()?;
            let mut user_comment_bytes = vec![0; user_comment_len as usize];
            rdr.read_exact(&mut user_comment_bytes)?;
            let user_comment = String::from_utf8(user_comment_bytes)?;
            user_comments.push(user_comment);
        }

        Ok(Self {
            vendor_string,
            user_comments,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::{OpusHead, OpusTags};
    use std::io::{Cursor, ErrorKind, Read};

    #[test]
    pub fn read_opus_head() {
        let mut rdr = Cursor::new(&[
            0x4f, 0x70, 0x75, 0x73, 0x48, 0x65, 0x61, 0x64, 0x01, 0x02, 0x38, 0x01, 0x80, 0xbb,
            0x00, 0x00, 0x00, 0x00, 0x00,
        ]);
        let opus_head = OpusHead::read(&mut rdr).unwrap();
        assert_eq!(opus_head.channel_count, 2);
        assert_eq!(opus_head.preskip, 312);
        assert_eq!(opus_head.input_sample_rate, 48000);
        assert_eq!(opus_head.output_gain, 0);
        assert_eq!(opus_head.channel_mapping_family, 0);

        let read_err = rdr.read_exact(&mut [0]).expect_err("should be EOF");
        assert_eq!(read_err.kind(), ErrorKind::UnexpectedEof);
    }

    #[test]
    pub fn read_opus_tags() {
        let mut rdr = Cursor::new(&[
            0x4f, 0x70, 0x75, 0x73, 0x54, 0x61, 0x67, 0x73, 0x0d, 0x00, 0x00, 0x00, 0x4c, 0x61,
            0x76, 0x66, 0x35, 0x38, 0x2e, 0x32, 0x39, 0x2e, 0x31, 0x30, 0x30, 0x01, 0x00, 0x00,
            0x00, 0x1d, 0x00, 0x00, 0x00, 0x65, 0x6e, 0x63, 0x6f, 0x64, 0x65, 0x72, 0x3d, 0x4c,
            0x61, 0x76, 0x63, 0x35, 0x38, 0x2e, 0x35, 0x34, 0x2e, 0x31, 0x30, 0x30, 0x20, 0x6c,
            0x69, 0x62, 0x6f, 0x70, 0x75, 0x73,
        ]);
        let opus_tags = OpusTags::read(&mut rdr).unwrap();
        assert_eq!(opus_tags.vendor_string, "Lavf58.29.100");
        assert_eq!(
            opus_tags.user_comments,
            vec!["encoder=Lavc58.54.100 libopus"]
        );

        let read_err = rdr.read_exact(&mut [0]).expect_err("should be EOF");
        assert_eq!(read_err.kind(), ErrorKind::UnexpectedEof);
    }
}
