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
    let byte_order = parse_byte_order(required_metadata(&metadata, "$BYTEORD")?)?;
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
            let text = String::from_utf8(current.clone())
                .map_err(|error| FcsError::Utf8(error.to_string()))?;
            tokens.push(text);
            current.clear();
            index += 1;
            continue;
        }
        current.push(byte);
        index += 1;
    }

    if !current.is_empty() {
        return Err(FcsError::InvalidText(
            "TEXT segment did not terminate cleanly".to_string(),
        ));
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
        Some(value) => value
            .trim()
            .parse::<u64>()
            .map(Some)
            .map_err(|_| FcsError::InvalidText(format!("invalid integer metadata '{key}'"))),
        None => Ok(None),
    }
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
    let bytes_per_event = match data_type {
        'I' | 'i' => channels
            .iter()
            .map(|channel| {
                channel
                    .bits
                    .ok_or_else(|| FcsError::InvalidMetadata("$PnB"))
            })
            .collect::<Result<Vec<_>, _>>()?
            .iter()
            .map(|bits| {
                if bits % 8 != 0 || *bits == 0 {
                    Err(FcsError::Unsupported(format!(
                        "unsupported integer width {} bits",
                        bits
                    )))
                } else {
                    Ok((*bits / 8) as usize)
                }
            })
            .collect::<Result<Vec<_>, _>>()?,
        'F' | 'f' => vec![4; parameter_count],
        'D' | 'd' => vec![8; parameter_count],
        other => {
            return Err(FcsError::Unsupported(format!(
                "unsupported data type '{other}'"
            )));
        }
    };

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
                _ => unreachable!(),
            };
            row.push(value);
            cursor = end;
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
