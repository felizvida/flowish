use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{self, Display, Formatter};

pub use flowjoish_core::CompensationMatrix;
use flowjoish_core::SampleFrame;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FcsHeader {
    pub version: String,
    pub text_start: usize,
    pub text_end: usize,
    pub data_start: usize,
    pub data_end: usize,
    pub analysis_start: Option<usize>,
    pub analysis_end: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FcsChannel {
    pub index: usize,
    pub short_name: String,
    pub long_name: Option<String>,
    pub bits: Option<u32>,
    pub range: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Endianness {
    Little,
    Big,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FcsFile {
    pub header: FcsHeader,
    pub metadata: BTreeMap<String, String>,
    pub channels: Vec<FcsChannel>,
    pub compensation: Option<CompensationMatrix>,
    pub event_count: usize,
    pub parameter_count: usize,
    pub data_type: char,
    pub byte_order: Endianness,
    pub events: Vec<Vec<f64>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FcsError {
    FileTooShort,
    InvalidHeader(String),
    InvalidOffset {
        field: &'static str,
        value: String,
    },
    SegmentOutOfBounds {
        segment: &'static str,
        start: usize,
        end: usize,
        len: usize,
    },
    InvalidText(String),
    InvalidMetadata(&'static str),
    Unsupported(String),
    InvalidCompensation(String),
    Utf8(String),
}

impl Display for FcsError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::FileTooShort => write!(f, "FCS file is shorter than the 58-byte header"),
            Self::InvalidHeader(message) => f.write_str(message),
            Self::InvalidOffset { field, value } => {
                write!(f, "invalid offset for {field}: '{value}'")
            }
            Self::SegmentOutOfBounds {
                segment,
                start,
                end,
                len,
            } => write!(
                f,
                "{} segment {}..={} is outside file length {}",
                segment, start, end, len
            ),
            Self::InvalidText(message) => f.write_str(message),
            Self::InvalidMetadata(name) => write!(f, "missing or invalid metadata field '{name}'"),
            Self::Unsupported(message) => f.write_str(message),
            Self::InvalidCompensation(message) => f.write_str(message),
            Self::Utf8(message) => f.write_str(message),
        }
    }
}

impl Error for FcsError {}

pub fn parse(bytes: &[u8]) -> Result<FcsFile, FcsError> {
    if bytes.len() < 58 {
        return Err(FcsError::FileTooShort);
    }

    let header = parse_header(bytes)?;
    let text_segment = slice_inclusive(bytes, "TEXT", header.text_start, header.text_end)?;
    let metadata = parse_text_segment(text_segment)?;
    let event_count = required_usize(&metadata, "$TOT")?;
    let parameter_count = required_usize(&metadata, "$PAR")?;
    let data_type = required_metadata(&metadata, "$DATATYPE")?
        .chars()
        .next()
        .ok_or(FcsError::InvalidMetadata("$DATATYPE"))?;
    let byte_order = match metadata.get("$BYTEORD").map(String::as_str) {
        Some(value) => parse_byte_order(value)?,
        None if matches!(data_type, 'A' | 'a') => Endianness::Little,
        None => return Err(FcsError::InvalidMetadata("$BYTEORD")),
    };
    let mode = required_metadata(&metadata, "$MODE")?;
    if !mode.eq_ignore_ascii_case("L") {
        return Err(FcsError::Unsupported(format!(
            "unsupported FCS mode '{mode}', only list mode is supported"
        )));
    }

    let channels = parse_channels(&metadata, parameter_count)?;
    let compensation = parse_compensation(&metadata)?;
    let data_start = resolve_offset(&metadata, "$BEGINDATA", header.data_start)?;
    let data_end = resolve_offset(&metadata, "$ENDDATA", header.data_end)?;
    let data_segment = slice_inclusive(bytes, "DATA", data_start, data_end)?;
    let events = decode_events(
        data_segment,
        &channels,
        event_count,
        parameter_count,
        data_type,
        byte_order,
    )?;

    Ok(FcsFile {
        header: FcsHeader {
            data_start,
            data_end,
            ..header
        },
        metadata,
        channels,
        compensation,
        event_count,
        parameter_count,
        data_type,
        byte_order,
        events,
    })
}

impl FcsFile {
    pub fn into_sample_frame(self, sample_id: impl Into<String>) -> Result<SampleFrame, FcsError> {
        let mut used = BTreeSet::new();
        let mut channels = Vec::with_capacity(self.channels.len());
        for channel in self.channels {
            let base = if channel.short_name.trim().is_empty() {
                format!("P{}", channel.index)
            } else {
                channel.short_name
            };
            let mut candidate = base.clone();
            let mut suffix = 2usize;
            while !used.insert(candidate.clone()) {
                candidate = format!("{base}#{suffix}");
                suffix += 1;
            }
            channels.push(candidate);
        }

        SampleFrame::new(sample_id, channels, self.events)
            .map_err(|error| FcsError::InvalidHeader(error.to_string()))
    }
}

fn parse_header(bytes: &[u8]) -> Result<FcsHeader, FcsError> {
    let version = std::str::from_utf8(&bytes[0..6])
        .map_err(|error| FcsError::Utf8(error.to_string()))?
        .trim()
        .to_string();
    if !version.starts_with("FCS") {
        return Err(FcsError::InvalidHeader(format!(
            "expected header to start with FCS, found '{version}'"
        )));
    }

    Ok(FcsHeader {
        version,
        text_start: parse_offset_field(bytes, 10, 18, "TEXT start")?,
        text_end: parse_offset_field(bytes, 18, 26, "TEXT end")?,
        data_start: parse_offset_field(bytes, 26, 34, "DATA start")?,
        data_end: parse_offset_field(bytes, 34, 42, "DATA end")?,
        analysis_start: parse_optional_offset_field(bytes, 42, 50, "ANALYSIS start")?,
        analysis_end: parse_optional_offset_field(bytes, 50, 58, "ANALYSIS end")?,
    })
}

fn parse_offset_field(
    bytes: &[u8],
    start: usize,
    end: usize,
    field: &'static str,
) -> Result<usize, FcsError> {
    let text = std::str::from_utf8(&bytes[start..end])
        .map_err(|error| FcsError::Utf8(error.to_string()))?
        .trim();
    if text.is_empty() {
        return Ok(0);
    }
    text.parse::<usize>().map_err(|_| FcsError::InvalidOffset {
        field,
        value: text.to_string(),
    })
}

fn parse_optional_offset_field(
    bytes: &[u8],
    start: usize,
    end: usize,
    field: &'static str,
) -> Result<Option<usize>, FcsError> {
    let value = parse_offset_field(bytes, start, end, field)?;
    if value == 0 {
        Ok(None)
    } else {
        Ok(Some(value))
    }
}

fn slice_inclusive<'a>(
    bytes: &'a [u8],
    segment: &'static str,
    start: usize,
    end: usize,
) -> Result<&'a [u8], FcsError> {
    if end < start {
        return Err(FcsError::SegmentOutOfBounds {
            segment,
            start,
            end,
            len: bytes.len(),
        });
    }
    if end >= bytes.len() {
        return Err(FcsError::SegmentOutOfBounds {
            segment,
            start,
            end,
            len: bytes.len(),
        });
    }
    Ok(&bytes[start..=end])
}

fn parse_text_segment(segment: &[u8]) -> Result<BTreeMap<String, String>, FcsError> {
    if segment.is_empty() {
        return Err(FcsError::InvalidText("TEXT segment is empty".to_string()));
    }
    let delimiter = segment[0];
    let mut tokens = Vec::new();
    let mut current = Vec::new();
    let mut index = 1usize;

    while index < segment.len() {
        let byte = segment[index];
        if byte == delimiter {
            if index + 1 < segment.len() && segment[index + 1] == delimiter {
                current.push(delimiter);
                index += 2;
                continue;
            }
            let text = decode_text_token(&current);
            tokens.push(text);
            current.clear();
            index += 1;
            continue;
        }
        current.push(byte);
        index += 1;
    }

    if !current.is_empty() {
        if current
            .iter()
            .all(|byte| byte.is_ascii_whitespace() || *byte == 0)
        {
            // Some vendors pad the declared TEXT segment after the final delimiter.
        } else if is_printable_text_token(&current) && tokens.len() % 2 == 1 {
            tokens.push(decode_text_token(&current));
        } else if is_likely_metadata_key(&current) && tokens.len() % 2 == 0 {
            // A few exports end with an unterminated vendor-private key after all
            // required key/value pairs. Dropping that dangling key is safer than
            // rejecting otherwise intact event data.
        } else {
            return Err(FcsError::InvalidText(
                "TEXT segment did not terminate cleanly".to_string(),
            ));
        }
    }

    if tokens.len() % 2 != 0 {
        return Err(FcsError::InvalidText(
            "TEXT segment has an odd number of key/value tokens".to_string(),
        ));
    }

    let mut metadata = BTreeMap::new();
    for pair in tokens.chunks(2) {
        metadata.insert(pair[0].clone(), pair[1].clone());
    }
    Ok(metadata)
}

fn decode_text_token(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(value) => value.to_string(),
        Err(_) => bytes.iter().map(|byte| char::from(*byte)).collect(),
    }
}

fn is_printable_text_token(bytes: &[u8]) -> bool {
    bytes
        .iter()
        .all(|byte| *byte >= 0x20 || matches!(*byte, b'\t' | b'\r' | b'\n'))
}

fn is_likely_metadata_key(bytes: &[u8]) -> bool {
    is_printable_text_token(bytes) && matches!(bytes.first(), Some(b'$' | b'&'))
}

fn required_metadata<'a>(
    metadata: &'a BTreeMap<String, String>,
    key: &'static str,
) -> Result<&'a str, FcsError> {
    metadata
        .get(key)
        .map(String::as_str)
        .ok_or(FcsError::InvalidMetadata(key))
}

fn required_usize(
    metadata: &BTreeMap<String, String>,
    key: &'static str,
) -> Result<usize, FcsError> {
    required_metadata(metadata, key)?
        .trim()
        .parse::<usize>()
        .map_err(|_| FcsError::InvalidMetadata(key))
}

fn optional_u32(metadata: &BTreeMap<String, String>, key: &str) -> Result<Option<u32>, FcsError> {
    match metadata.get(key) {
        Some(value) => value
            .trim()
            .parse::<u32>()
            .map(Some)
            .map_err(|_| FcsError::InvalidText(format!("invalid integer metadata '{key}'"))),
        None => Ok(None),
    }
}

fn optional_u64(metadata: &BTreeMap<String, String>, key: &str) -> Result<Option<u64>, FcsError> {
    match metadata.get(key) {
        Some(value) => parse_numeric_range(value)
            .map(Some)
            .map_err(|_| FcsError::InvalidText(format!("invalid numeric metadata '{key}'"))),
        None => Ok(None),
    }
}

fn parse_numeric_range(value: &str) -> Result<u64, ()> {
    let trimmed = value.trim();
    if let Ok(parsed) = trimmed.parse::<u64>() {
        return Ok(parsed);
    }

    let parsed = trimmed.parse::<f64>().map_err(|_| ())?;
    if !parsed.is_finite() || parsed < 0.0 || parsed > u64::MAX as f64 {
        return Err(());
    }
    Ok(parsed.ceil() as u64)
}

fn resolve_offset(
    metadata: &BTreeMap<String, String>,
    key: &'static str,
    header_value: usize,
) -> Result<usize, FcsError> {
    if header_value != 0 {
        return Ok(header_value);
    }
    required_usize(metadata, key)
}

fn parse_channels(
    metadata: &BTreeMap<String, String>,
    parameter_count: usize,
) -> Result<Vec<FcsChannel>, FcsError> {
    (1..=parameter_count)
        .map(|index| {
            let short_name = metadata
                .get(&format!("$P{index}N"))
                .cloned()
                .ok_or_else(|| FcsError::InvalidMetadata("$PnN"))?;
            let long_name = metadata.get(&format!("$P{index}S")).cloned();
            let bits = optional_u32(metadata, &format!("$P{index}B"))?;
            let range = optional_u64(metadata, &format!("$P{index}R"))?;
            Ok(FcsChannel {
                index,
                short_name,
                long_name,
                bits,
                range,
            })
        })
        .collect()
}

fn parse_compensation(
    metadata: &BTreeMap<String, String>,
) -> Result<Option<CompensationMatrix>, FcsError> {
    let entry = ["$SPILLOVER", "SPILLOVER", "$SPILL", "SPILL"]
        .iter()
        .find_map(|key| {
            metadata
                .get(*key)
                .map(|value| ((*key).to_string(), value.clone()))
        });

    let Some((source_key, raw)) = entry else {
        return Ok(None);
    };

    let tokens = raw
        .split(',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return Err(FcsError::InvalidCompensation(
            "compensation entry is empty".to_string(),
        ));
    }

    let dimension = tokens[0]
        .parse::<usize>()
        .map_err(|_| FcsError::InvalidCompensation("invalid spillover dimension".to_string()))?;
    let expected = 1 + dimension + dimension * dimension;
    if tokens.len() == 1 + dimension {
        return Ok(None);
    }
    if tokens.len() != expected {
        return Err(FcsError::InvalidCompensation(format!(
            "spillover entry expected {expected} tokens but found {}",
            tokens.len()
        )));
    }

    let parameter_names = tokens[1..1 + dimension]
        .iter()
        .map(|name| (*name).to_string())
        .collect::<Vec<_>>();
    let values = tokens[1 + dimension..]
        .iter()
        .map(|value| {
            value.parse::<f64>().map_err(|_| {
                FcsError::InvalidCompensation("spillover matrix contains invalid float".to_string())
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Some(CompensationMatrix {
        source_key,
        dimension,
        parameter_names,
        values,
    }))
}

fn parse_byte_order(value: &str) -> Result<Endianness, FcsError> {
    let normalized = value.replace(' ', "");
    match normalized.as_str() {
        "1,2,3,4" | "1,2" => Ok(Endianness::Little),
        "4,3,2,1" | "2,1" => Ok(Endianness::Big),
        _ => Err(FcsError::Unsupported(format!(
            "unsupported byte order '{value}'"
        ))),
    }
}

fn decode_events(
    bytes: &[u8],
    channels: &[FcsChannel],
    event_count: usize,
    parameter_count: usize,
    data_type: char,
    byte_order: Endianness,
) -> Result<Vec<Vec<f64>>, FcsError> {
    let event_layout = match data_type {
        'I' | 'i' => {
            let bit_widths = channels
                .iter()
                .map(|channel| {
                    let bits = channel.bits.ok_or(FcsError::InvalidMetadata("$PnB"))?;
                    if bits == 0 || bits > 64 {
                        return Err(FcsError::Unsupported(format!(
                            "unsupported integer width {} bits",
                            bits
                        )));
                    }
                    Ok(bits)
                })
                .collect::<Result<Vec<_>, _>>()?;
            if bit_widths.iter().all(|bits| bits % 8 == 0) {
                EventLayout::ByteAligned(
                    bit_widths.iter().map(|bits| (*bits / 8) as usize).collect(),
                )
            } else {
                let bits_per_event = bit_widths
                    .iter()
                    .try_fold(0usize, |total, bits| total.checked_add(*bits as usize))
                    .ok_or_else(|| {
                        FcsError::Unsupported("integer event width overflow".to_string())
                    })?;
                EventLayout::BitPacked {
                    bit_widths,
                    bits_per_event,
                    storage: infer_packed_integer_storage(
                        bytes.len(),
                        bits_per_event,
                        event_count,
                    )?,
                }
            }
        }
        'F' | 'f' => EventLayout::ByteAligned(vec![4; parameter_count]),
        'D' | 'd' => EventLayout::ByteAligned(vec![8; parameter_count]),
        'A' | 'a' => EventLayout::ByteAligned(channel_byte_widths(channels, "ASCII")?),
        other => {
            return Err(FcsError::Unsupported(format!(
                "unsupported data type '{other}'"
            )));
        }
    };

    match event_layout {
        EventLayout::ByteAligned(bytes_per_event) => decode_byte_aligned_events(
            bytes,
            bytes_per_event,
            event_count,
            parameter_count,
            data_type,
            byte_order,
        ),
        EventLayout::BitPacked {
            bit_widths,
            bits_per_event,
            storage,
        } => decode_bit_packed_integer_events(
            bytes,
            &bit_widths,
            bits_per_event,
            event_count,
            storage,
            byte_order,
        ),
    }
}

fn channel_byte_widths(channels: &[FcsChannel], label: &str) -> Result<Vec<usize>, FcsError> {
    channels
        .iter()
        .map(|channel| {
            let bits = channel.bits.ok_or(FcsError::InvalidMetadata("$PnB"))?;
            if bits == 0 || bits % 8 != 0 {
                return Err(FcsError::Unsupported(format!(
                    "unsupported {label} width {} bits",
                    bits
                )));
            }
            Ok((bits / 8) as usize)
        })
        .collect()
}

enum EventLayout {
    ByteAligned(Vec<usize>),
    BitPacked {
        bit_widths: Vec<u32>,
        bits_per_event: usize,
        storage: PackedIntegerStorage,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PackedIntegerStorage {
    GlobalBitStream,
    EventBytePadded,
}

fn infer_packed_integer_storage(
    available_bytes: usize,
    bits_per_event: usize,
    event_count: usize,
) -> Result<PackedIntegerStorage, FcsError> {
    let global_expected = bits_to_bytes(
        bits_per_event
            .checked_mul(event_count)
            .ok_or_else(|| FcsError::Unsupported("integer DATA bit width overflow".to_string()))?,
    );
    let padded_expected = bits_to_bytes(bits_per_event)
        .checked_mul(event_count)
        .ok_or_else(|| FcsError::Unsupported("integer DATA byte width overflow".to_string()))?;

    // Prefer exact global bit-stream matches; otherwise accept record layouts
    // where each event is padded to the next byte boundary.
    if available_bytes == global_expected {
        return Ok(PackedIntegerStorage::GlobalBitStream);
    }
    if available_bytes >= padded_expected {
        return Ok(PackedIntegerStorage::EventBytePadded);
    }
    if available_bytes >= global_expected {
        return Ok(PackedIntegerStorage::GlobalBitStream);
    }

    Err(FcsError::InvalidHeader(format!(
        "DATA segment is {} bytes but at least {} were required",
        available_bytes,
        global_expected.min(padded_expected)
    )))
}

fn bits_to_bytes(bits: usize) -> usize {
    bits.div_ceil(8)
}

fn decode_byte_aligned_events(
    bytes: &[u8],
    bytes_per_event: Vec<usize>,
    event_count: usize,
    parameter_count: usize,
    data_type: char,
    byte_order: Endianness,
) -> Result<Vec<Vec<f64>>, FcsError> {
    let expected_bytes = bytes_per_event.iter().sum::<usize>() * event_count;
    if bytes.len() < expected_bytes {
        return Err(FcsError::InvalidHeader(format!(
            "DATA segment is {} bytes but at least {} were required",
            bytes.len(),
            expected_bytes
        )));
    }

    let mut cursor = 0usize;
    let mut events = Vec::with_capacity(event_count);
    for _event_index in 0..event_count {
        let mut row = Vec::with_capacity(parameter_count);
        for width in &bytes_per_event {
            let end = cursor + *width;
            let cell = &bytes[cursor..end];
            let value = match data_type {
                'I' | 'i' => read_integer(cell, byte_order) as f64,
                'F' | 'f' => read_f32(cell, byte_order)? as f64,
                'D' | 'd' => read_f64(cell, byte_order)?,
                'A' | 'a' => read_ascii_number(cell)?,
                _ => unreachable!(),
            };
            row.push(value);
            cursor = end;
        }
        events.push(row);
    }
    Ok(events)
}

fn decode_bit_packed_integer_events(
    bytes: &[u8],
    bit_widths: &[u32],
    bits_per_event: usize,
    event_count: usize,
    storage: PackedIntegerStorage,
    byte_order: Endianness,
) -> Result<Vec<Vec<f64>>, FcsError> {
    let required_bytes = match storage {
        PackedIntegerStorage::GlobalBitStream => {
            bits_to_bytes(bits_per_event.checked_mul(event_count).ok_or_else(|| {
                FcsError::Unsupported("integer DATA bit width overflow".to_string())
            })?)
        }
        PackedIntegerStorage::EventBytePadded => bits_to_bytes(bits_per_event)
            .checked_mul(event_count)
            .ok_or_else(|| FcsError::Unsupported("integer DATA byte width overflow".to_string()))?,
    };
    if bytes.len() < required_bytes {
        return Err(FcsError::InvalidHeader(format!(
            "DATA segment is {} bytes but at least {} were required",
            bytes.len(),
            required_bytes
        )));
    }

    let mut events = Vec::with_capacity(event_count);
    let event_stride_bits = match storage {
        PackedIntegerStorage::GlobalBitStream => bits_per_event,
        PackedIntegerStorage::EventBytePadded => bits_to_bytes(bits_per_event) * 8,
    };
    for event_index in 0..event_count {
        let mut bit_cursor = event_index * event_stride_bits;
        let mut row = Vec::with_capacity(bit_widths.len());
        for width in bit_widths {
            row.push(read_packed_integer(bytes, bit_cursor, *width, byte_order)? as f64);
            bit_cursor += *width as usize;
        }
        events.push(row);
    }
    Ok(events)
}

fn read_integer(bytes: &[u8], endianness: Endianness) -> u64 {
    match endianness {
        Endianness::Little => bytes.iter().enumerate().fold(0u64, |acc, (index, byte)| {
            acc | (u64::from(*byte) << (index * 8))
        }),
        Endianness::Big => bytes
            .iter()
            .fold(0u64, |acc, byte| (acc << 8) | u64::from(*byte)),
    }
}

fn read_packed_integer(
    bytes: &[u8],
    start_bit: usize,
    width: u32,
    endianness: Endianness,
) -> Result<u64, FcsError> {
    if width == 0 || width > 64 {
        return Err(FcsError::Unsupported(format!(
            "unsupported integer width {} bits",
            width
        )));
    }

    let mut value = 0u64;
    for bit_index in 0..width as usize {
        let bit = read_packed_bit(bytes, start_bit + bit_index, endianness)?;
        match endianness {
            Endianness::Little => value |= u64::from(bit) << bit_index,
            Endianness::Big => value = (value << 1) | u64::from(bit),
        }
    }
    Ok(value)
}

fn read_packed_bit(bytes: &[u8], bit_index: usize, endianness: Endianness) -> Result<u8, FcsError> {
    let byte = bytes
        .get(bit_index / 8)
        .ok_or_else(|| FcsError::InvalidHeader("packed integer DATA ended early".to_string()))?;
    let bit_in_byte = bit_index % 8;
    let shift = match endianness {
        Endianness::Little => bit_in_byte,
        Endianness::Big => 7 - bit_in_byte,
    };
    Ok((byte >> shift) & 1)
}

fn read_ascii_number(bytes: &[u8]) -> Result<f64, FcsError> {
    let text = std::str::from_utf8(bytes).map_err(|error| FcsError::Utf8(error.to_string()))?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(FcsError::InvalidText(
            "ASCII event value is empty".to_string(),
        ));
    }
    trimmed
        .parse::<f64>()
        .map_err(|_| FcsError::InvalidText(format!("invalid ASCII event value '{trimmed}'")))
}

fn read_f32(bytes: &[u8], endianness: Endianness) -> Result<f32, FcsError> {
    if bytes.len() != 4 {
        return Err(FcsError::Unsupported(format!(
            "expected 4 bytes for f32, found {}",
            bytes.len()
        )));
    }
    let mut buffer = [0u8; 4];
    buffer.copy_from_slice(bytes);
    Ok(match endianness {
        Endianness::Little => f32::from_le_bytes(buffer),
        Endianness::Big => f32::from_be_bytes(buffer),
    })
}

fn read_f64(bytes: &[u8], endianness: Endianness) -> Result<f64, FcsError> {
    if bytes.len() != 8 {
        return Err(FcsError::Unsupported(format!(
            "expected 8 bytes for f64, found {}",
            bytes.len()
        )));
    }
    let mut buffer = [0u8; 8];
    buffer.copy_from_slice(bytes);
    Ok(match endianness {
        Endianness::Little => f64::from_le_bytes(buffer),
        Endianness::Big => f64::from_be_bytes(buffer),
    })
}

#[cfg(test)]
mod tests {
    use super::{CompensationMatrix, Endianness, FcsError, parse};

    #[test]
    fn parses_channels_metadata_and_compensation() {
        let bytes = build_test_fcs(
            vec!["FSC-A", "SSC-A"],
            vec![vec![1.0, 2.0], vec![3.0, 4.0]],
            Some(("SPILLOVER", "2,FSC-A,SSC-A,1,0.1,0.2,1")),
        );

        let parsed = parse(&bytes).expect("valid fcs");
        assert_eq!(parsed.event_count, 2);
        assert_eq!(parsed.parameter_count, 2);
        assert_eq!(parsed.byte_order, Endianness::Little);
        assert_eq!(parsed.channels[0].short_name, "FSC-A");
        assert_eq!(
            parsed.compensation,
            Some(CompensationMatrix {
                source_key: "SPILLOVER".to_string(),
                dimension: 2,
                parameter_names: vec!["FSC-A".to_string(), "SSC-A".to_string()],
                values: vec![1.0, 0.1, 0.2, 1.0],
            })
        );
        assert_eq!(parsed.events, vec![vec![1.0, 2.0], vec![3.0, 4.0]]);
    }

    #[test]
    fn ignores_incomplete_spillover_entries_without_matrix_values() {
        let bytes = build_test_fcs(
            vec!["FITC-A", "PE-A"],
            vec![vec![1.0, 2.0]],
            Some(("$SPILLOVER", "2,FITC-A,PE-A")),
        );

        let parsed = parse(&bytes).expect("incomplete spillover metadata should not block events");
        assert_eq!(parsed.compensation, None);
        assert_eq!(parsed.events, vec![vec![1.0, 2.0]]);
    }

    #[test]
    fn converts_to_core_sample_frame() {
        let bytes = build_test_fcs(vec!["CD3", "CD3"], vec![vec![5.0, 6.0]], None);
        let parsed = parse(&bytes).expect("valid fcs");
        let sample = parsed.into_sample_frame("sample-a").expect("sample frame");
        assert_eq!(sample.channels(), &["CD3".to_string(), "CD3#2".to_string()]);
    }

    #[test]
    fn rejects_out_of_bounds_segments_cleanly() {
        let mut bytes = build_test_fcs(vec!["FSC-A"], vec![vec![1.0]], None);
        bytes[26..34].copy_from_slice(format!("{:>8}", 9_999_999).as_bytes());
        let error = parse(&bytes).expect_err("invalid data offset");
        assert!(matches!(
            error,
            FcsError::SegmentOutOfBounds {
                segment: "DATA",
                ..
            }
        ));
    }

    #[test]
    fn parses_text_segments_with_trailing_padding() {
        let metadata = super::parse_text_segment(b"/$TOT/1/$PAR/2/        ")
            .expect("trailing padding after the final delimiter is recoverable");

        assert_eq!(metadata.get("$TOT"), Some(&"1".to_string()));
        assert_eq!(metadata.get("$PAR"), Some(&"2".to_string()));
    }

    #[test]
    fn parses_text_segments_with_unclosed_final_values() {
        let metadata = super::parse_text_segment(b"/$TOT/1/$SRC/final value")
            .expect("a printable final value can be recovered without a closing delimiter");

        assert_eq!(metadata.get("$SRC"), Some(&"final value".to_string()));
    }

    #[test]
    fn ignores_trailing_dangling_private_metadata_keys() {
        let metadata = super::parse_text_segment(b"/$TOT/1/&13Analysis Doc.\\")
            .expect("dangling vendor-private keys after complete pairs are recoverable");

        assert_eq!(metadata.get("$TOT"), Some(&"1".to_string()));
        assert_eq!(metadata.get("&13Analysis Doc.\\"), None);
    }

    #[test]
    fn decodes_non_utf8_text_tokens_as_latin1() {
        let metadata = super::parse_text_segment(b"/$SRC/caf\xe9/")
            .expect("vendor Latin-1 metadata is recoverable");

        assert_eq!(metadata.get("$SRC"), Some(&"café".to_string()));
    }

    #[test]
    fn parses_decimal_channel_ranges_lossily_for_summary_metadata() {
        assert_eq!(super::parse_numeric_range("23.4094"), Ok(24));
    }

    #[test]
    fn parses_event_byte_padded_non_byte_aligned_integer_payloads() {
        let bytes = build_integer_test_fcs(
            vec![("FL1-A", 10, 1024), ("FL2-A", 10, 1024)],
            vec![vec![0x155, 0x2aa], vec![0x3ff, 0x001]],
            Endianness::Little,
            PackedTestStorage::EventBytePadded,
        );

        let parsed = parse(&bytes).expect("byte-padded packed integer FCS");
        assert_eq!(
            parsed.events,
            vec![
                vec![0x155 as f64, 0x2aa as f64],
                vec![0x3ff as f64, 0x001 as f64]
            ]
        );
    }

    #[test]
    fn parses_global_bit_stream_non_byte_aligned_integer_payloads() {
        let bytes = build_integer_test_fcs(
            vec![("FL1-A", 10, 1024), ("FL2-A", 10, 1024)],
            vec![vec![0x155, 0x2aa], vec![0x3ff, 0x001]],
            Endianness::Big,
            PackedTestStorage::GlobalBitStream,
        );

        let parsed = parse(&bytes).expect("globally packed integer FCS");
        assert_eq!(
            parsed.events,
            vec![
                vec![0x155 as f64, 0x2aa as f64],
                vec![0x3ff as f64, 0x001 as f64]
            ]
        );
    }

    #[test]
    fn parses_fixed_width_ascii_event_payloads() {
        let bytes = build_ascii_test_fcs(
            vec![("FSC-A", 12, 100_000), ("SSC-A", 12, 100_000)],
            vec![vec![12.5, -3.0], vec![1000.0, 0.0125]],
            true,
        );

        let parsed = parse(&bytes).expect("fixed-width ASCII FCS");
        assert_eq!(parsed.data_type, 'A');
        assert_eq!(parsed.events, vec![vec![12.5, -3.0], vec![1000.0, 0.0125]]);
    }

    #[test]
    fn parses_ascii_payloads_without_byte_order_metadata() {
        let bytes = build_ascii_test_fcs(
            vec![("FL1-A", 10, 100_000), ("FL2-A", 10, 100_000)],
            vec![vec![1.0, 2.5]],
            false,
        );

        let parsed = parse(&bytes).expect("ASCII FCS without byte order");
        assert_eq!(parsed.byte_order, Endianness::Little);
        assert_eq!(parsed.events, vec![vec![1.0, 2.5]]);
    }

    fn build_test_fcs(
        channels: Vec<&str>,
        rows: Vec<Vec<f64>>,
        extra_metadata: Option<(&str, &str)>,
    ) -> Vec<u8> {
        let delimiter = '/';
        let event_count = rows.len();
        let parameter_count = channels.len();
        let mut metadata = vec![
            ("$TOT".to_string(), event_count.to_string()),
            ("$PAR".to_string(), parameter_count.to_string()),
            ("$DATATYPE".to_string(), "F".to_string()),
            ("$BYTEORD".to_string(), "1,2,3,4".to_string()),
            ("$MODE".to_string(), "L".to_string()),
            ("$NEXTDATA".to_string(), "0".to_string()),
        ];

        for (index, name) in channels.iter().enumerate() {
            let channel_index = index + 1;
            metadata.push((format!("$P{channel_index}N"), (*name).to_string()));
            metadata.push((format!("$P{channel_index}B"), "32".to_string()));
            metadata.push((format!("$P{channel_index}R"), "262144".to_string()));
        }

        if let Some((key, value)) = extra_metadata {
            metadata.push((key.to_string(), value.to_string()));
        }

        let data_bytes = rows
            .iter()
            .flat_map(|row| {
                row.iter()
                    .flat_map(|value| (*value as f32).to_le_bytes())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        let text_start = 58usize;
        let mut text = build_text_segment(delimiter, &metadata);
        let mut data_start = text_start + text.len();
        let mut data_end = data_start + data_bytes.len().saturating_sub(1);

        metadata.push(("$BEGINDATA".to_string(), data_start.to_string()));
        metadata.push(("$ENDDATA".to_string(), data_end.to_string()));
        text = build_text_segment(delimiter, &metadata);
        data_start = text_start + text.len();
        data_end = data_start + data_bytes.len().saturating_sub(1);

        let mut header = vec![b' '; 58];
        header[0..6].copy_from_slice(b"FCS3.1");
        write_field(&mut header, 10, 18, text_start);
        write_field(&mut header, 18, 26, text_start + text.len() - 1);
        write_field(&mut header, 26, 34, data_start);
        write_field(&mut header, 34, 42, data_end);
        write_field(&mut header, 42, 50, 0);
        write_field(&mut header, 50, 58, 0);

        let mut bytes = header;
        bytes.extend_from_slice(&text);
        bytes.extend_from_slice(&data_bytes);
        bytes
    }

    fn build_ascii_test_fcs(
        channels: Vec<(&str, usize, u64)>,
        rows: Vec<Vec<f64>>,
        include_byte_order: bool,
    ) -> Vec<u8> {
        let delimiter = '/';
        let event_count = rows.len();
        let parameter_count = channels.len();
        let mut metadata = vec![
            ("$TOT".to_string(), event_count.to_string()),
            ("$PAR".to_string(), parameter_count.to_string()),
            ("$DATATYPE".to_string(), "A".to_string()),
            ("$MODE".to_string(), "L".to_string()),
            ("$NEXTDATA".to_string(), "0".to_string()),
        ];
        if include_byte_order {
            metadata.push(("$BYTEORD".to_string(), "1,2,3,4".to_string()));
        }

        for (index, (name, width, range)) in channels.iter().enumerate() {
            let channel_index = index + 1;
            metadata.push((format!("$P{channel_index}N"), (*name).to_string()));
            metadata.push((format!("$P{channel_index}B"), (width * 8).to_string()));
            metadata.push((format!("$P{channel_index}R"), range.to_string()));
        }

        let data_bytes = rows
            .iter()
            .flat_map(|row| {
                assert_eq!(row.len(), channels.len());
                row.iter()
                    .zip(&channels)
                    .flat_map(|(value, (_, width, _))| {
                        let text = format!("{value:>width$.4e}");
                        assert_eq!(text.len(), *width);
                        text.into_bytes()
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        let text_start = 58usize;
        let mut text = build_text_segment(delimiter, &metadata);
        let mut data_start = text_start + text.len();
        let mut data_end = data_start + data_bytes.len().saturating_sub(1);

        metadata.push(("$BEGINDATA".to_string(), data_start.to_string()));
        metadata.push(("$ENDDATA".to_string(), data_end.to_string()));
        text = build_text_segment(delimiter, &metadata);
        data_start = text_start + text.len();
        data_end = data_start + data_bytes.len().saturating_sub(1);

        let mut header = vec![b' '; 58];
        header[0..6].copy_from_slice(b"FCS3.1");
        write_field(&mut header, 10, 18, text_start);
        write_field(&mut header, 18, 26, text_start + text.len() - 1);
        write_field(&mut header, 26, 34, data_start);
        write_field(&mut header, 34, 42, data_end);
        write_field(&mut header, 42, 50, 0);
        write_field(&mut header, 50, 58, 0);

        let mut bytes = header;
        bytes.extend_from_slice(&text);
        bytes.extend_from_slice(&data_bytes);
        bytes
    }

    #[derive(Clone, Copy)]
    enum PackedTestStorage {
        GlobalBitStream,
        EventBytePadded,
    }

    fn build_integer_test_fcs(
        channels: Vec<(&str, u32, u64)>,
        rows: Vec<Vec<u64>>,
        byte_order: Endianness,
        storage: PackedTestStorage,
    ) -> Vec<u8> {
        let delimiter = '/';
        let event_count = rows.len();
        let parameter_count = channels.len();
        let mut metadata = vec![
            ("$TOT".to_string(), event_count.to_string()),
            ("$PAR".to_string(), parameter_count.to_string()),
            ("$DATATYPE".to_string(), "I".to_string()),
            (
                "$BYTEORD".to_string(),
                match byte_order {
                    Endianness::Little => "1,2,3,4".to_string(),
                    Endianness::Big => "4,3,2,1".to_string(),
                },
            ),
            ("$MODE".to_string(), "L".to_string()),
            ("$NEXTDATA".to_string(), "0".to_string()),
        ];

        for (index, (name, bits, range)) in channels.iter().enumerate() {
            let channel_index = index + 1;
            metadata.push((format!("$P{channel_index}N"), (*name).to_string()));
            metadata.push((format!("$P{channel_index}B"), bits.to_string()));
            metadata.push((format!("$P{channel_index}R"), range.to_string()));
        }

        let bit_widths = channels
            .iter()
            .map(|(_, bits, _)| *bits)
            .collect::<Vec<_>>();
        let data_bytes = pack_integer_rows(&rows, &bit_widths, byte_order, storage);

        let text_start = 58usize;
        let mut text = build_text_segment(delimiter, &metadata);
        let mut data_start = text_start + text.len();
        let mut data_end = data_start + data_bytes.len().saturating_sub(1);

        metadata.push(("$BEGINDATA".to_string(), data_start.to_string()));
        metadata.push(("$ENDDATA".to_string(), data_end.to_string()));
        text = build_text_segment(delimiter, &metadata);
        data_start = text_start + text.len();
        data_end = data_start + data_bytes.len().saturating_sub(1);

        let mut header = vec![b' '; 58];
        header[0..6].copy_from_slice(b"FCS3.1");
        write_field(&mut header, 10, 18, text_start);
        write_field(&mut header, 18, 26, text_start + text.len() - 1);
        write_field(&mut header, 26, 34, data_start);
        write_field(&mut header, 34, 42, data_end);
        write_field(&mut header, 42, 50, 0);
        write_field(&mut header, 50, 58, 0);

        let mut bytes = header;
        bytes.extend_from_slice(&text);
        bytes.extend_from_slice(&data_bytes);
        bytes
    }

    fn pack_integer_rows(
        rows: &[Vec<u64>],
        bit_widths: &[u32],
        byte_order: Endianness,
        storage: PackedTestStorage,
    ) -> Vec<u8> {
        let bits_per_event = bit_widths.iter().map(|bits| *bits as usize).sum::<usize>();
        let event_stride_bits = match storage {
            PackedTestStorage::GlobalBitStream => bits_per_event,
            PackedTestStorage::EventBytePadded => bits_per_event.div_ceil(8) * 8,
        };
        let total_bits = match storage {
            PackedTestStorage::GlobalBitStream => bits_per_event * rows.len(),
            PackedTestStorage::EventBytePadded => event_stride_bits * rows.len(),
        };
        let mut bytes = vec![0u8; total_bits.div_ceil(8)];

        for (event_index, row) in rows.iter().enumerate() {
            assert_eq!(row.len(), bit_widths.len());
            let mut bit_cursor = event_index * event_stride_bits;
            for (value, width) in row.iter().zip(bit_widths) {
                write_packed_integer(&mut bytes, bit_cursor, *width, *value, byte_order);
                bit_cursor += *width as usize;
            }
        }
        bytes
    }

    fn write_packed_integer(
        bytes: &mut [u8],
        start_bit: usize,
        width: u32,
        value: u64,
        byte_order: Endianness,
    ) {
        assert!(width > 0 && width <= 64);
        if width < 64 {
            assert!(value < (1u64 << width));
        }

        for bit_index in 0..width as usize {
            let bit = match byte_order {
                Endianness::Little => (value >> bit_index) & 1,
                Endianness::Big => (value >> (width as usize - 1 - bit_index)) & 1,
            };
            if bit == 0 {
                continue;
            }
            let output_bit = start_bit + bit_index;
            let shift = match byte_order {
                Endianness::Little => output_bit % 8,
                Endianness::Big => 7 - (output_bit % 8),
            };
            bytes[output_bit / 8] |= 1u8 << shift;
        }
    }

    fn build_text_segment(delimiter: char, metadata: &[(String, String)]) -> Vec<u8> {
        let delimiter = delimiter as u8;
        let mut bytes = vec![delimiter];
        for (key, value) in metadata {
            bytes.extend_from_slice(&escape_value(key, delimiter));
            bytes.push(delimiter);
            bytes.extend_from_slice(&escape_value(value, delimiter));
            bytes.push(delimiter);
        }
        bytes
    }

    fn escape_value(value: &str, delimiter: u8) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(value.len());
        for byte in value.as_bytes() {
            if *byte == delimiter {
                bytes.push(delimiter);
            }
            bytes.push(*byte);
        }
        bytes
    }

    fn write_field(header: &mut [u8], start: usize, end: usize, value: usize) {
        let width = end - start;
        let text = format!("{value:>width$}");
        header[start..end].copy_from_slice(text.as_bytes());
    }
}
