use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use flowjoish_core::JsonValue;

#[test]
fn inspect_fcs_json_emits_structured_output() {
    let path = unique_temp_path("inspect-fcs-json");
    fs::write(
        &path,
        build_test_fcs(
            vec!["FSC-A", "SSC-A"],
            vec![vec![1.0, 2.0], vec![3.0, 4.0]],
            Some(("SPILLOVER", "2,FSC-A,SSC-A,1,0.1,0.2,1")),
        ),
    )
    .expect("write temp fcs file");

    let output = Command::new(env!("CARGO_BIN_EXE_flowjoish-cli"))
        .args(["inspect-fcs-json", path.to_str().expect("utf8 path")])
        .output()
        .expect("run flowjoish-cli");

    let _ = fs::remove_file(&path);

    assert!(
        output.status.success(),
        "expected success, stderr was {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed = JsonValue::parse(stdout.trim()).expect("valid canonical json");

    assert_eq!(
        parsed.get("version").and_then(JsonValue::as_str),
        Some("FCS3.1")
    );
    assert_eq!(
        parsed.get("event_count").and_then(JsonValue::as_u64),
        Some(2)
    );
    assert_eq!(
        parsed.get("parameter_count").and_then(JsonValue::as_u64),
        Some(2)
    );
    assert_eq!(
        parsed.get("data_type").and_then(JsonValue::as_str),
        Some("F")
    );
    assert_eq!(
        parsed.get("byte_order").and_then(JsonValue::as_str),
        Some("Little")
    );

    let compensation = parsed
        .get("compensation")
        .and_then(JsonValue::as_object)
        .expect("compensation object");
    assert_eq!(
        compensation
            .get("source_key")
            .and_then(JsonValue::as_str),
        Some("SPILLOVER")
    );
    assert_eq!(
        compensation.get("dimension").and_then(JsonValue::as_u64),
        Some(2)
    );
}

fn unique_temp_path(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    env::temp_dir().join(format!(
        "flowjoish-cli-{prefix}-{}-{nanos}.fcs",
        std::process::id()
    ))
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
