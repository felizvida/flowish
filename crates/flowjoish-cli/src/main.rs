use std::env;
use std::fs;
use std::process::ExitCode;

use flowjoish_core::{Command, CommandLog, JsonValue, Point2D, ReplayEnvironment, SampleFrame};
use flowjoish_fcs::{CompensationMatrix, Endianness, FcsChannel, FcsFile, parse as parse_fcs};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let args = env::args().collect::<Vec<_>>();
    match args.get(1).map(String::as_str) {
        Some("inspect-fcs") => {
            let path = args
                .get(2)
                .ok_or_else(|| "usage: flowjoish-cli inspect-fcs <path>".to_string())?;
            inspect_fcs(path, false)
        }
        Some("inspect-fcs-json") => {
            let path = args
                .get(2)
                .ok_or_else(|| "usage: flowjoish-cli inspect-fcs-json <path>".to_string())?;
            inspect_fcs(path, true)
        }
        Some("demo-replay") => demo_replay(),
        _ => Err("usage: flowjoish-cli <inspect-fcs|inspect-fcs-json|demo-replay> [args]".to_string()),
    }
}

fn inspect_fcs(path: &str, as_json: bool) -> Result<(), String> {
    let bytes = fs::read(path).map_err(|error| format!("failed to read {path}: {error}"))?;
    let parsed = parse_fcs(&bytes).map_err(|error| format!("failed to parse FCS: {error}"))?;

    if as_json {
        println!("{}", fcs_report_json(path, &parsed).stringify_canonical());
    } else {
        println!("version: {}", parsed.header.version);
        println!("events: {}", parsed.event_count);
        println!("parameters: {}", parsed.parameter_count);
        println!("data_type: {}", parsed.data_type);
        println!("byte_order: {:?}", parsed.byte_order);
        println!("channels:");
        for channel in &parsed.channels {
            println!(
                "  - P{} {} {}",
                channel.index,
                channel.short_name,
                channel.long_name.as_deref().unwrap_or("")
            );
        }
        if let Some(compensation) = &parsed.compensation {
            println!(
                "compensation: {}x{} via {}",
                compensation.dimension, compensation.dimension, compensation.source_key
            );
        }
        println!("metadata keys: {}", parsed.metadata.len());
    }
    Ok(())
}

fn fcs_report_json(path: &str, parsed: &FcsFile) -> JsonValue {
    JsonValue::object([
        ("path", JsonValue::String(path.to_string())),
        (
            "version",
            JsonValue::String(parsed.header.version.clone()),
        ),
        (
            "event_count",
            JsonValue::Number(parsed.event_count as f64),
        ),
        (
            "parameter_count",
            JsonValue::Number(parsed.parameter_count as f64),
        ),
        (
            "data_type",
            JsonValue::String(parsed.data_type.to_string()),
        ),
        (
            "byte_order",
            JsonValue::String(format_endianness(parsed.byte_order).to_string()),
        ),
        (
            "metadata_keys",
            JsonValue::Number(parsed.metadata.len() as f64),
        ),
        (
            "channels",
            JsonValue::Array(
                parsed
                    .channels
                    .iter()
                    .map(channel_json)
                    .collect::<Vec<_>>(),
            ),
        ),
        ("compensation", compensation_json(parsed.compensation.as_ref())),
    ])
}

fn channel_json(channel: &FcsChannel) -> JsonValue {
    JsonValue::object([
        ("index", JsonValue::Number(channel.index as f64)),
        (
            "short_name",
            JsonValue::String(channel.short_name.clone()),
        ),
        (
            "long_name",
            match &channel.long_name {
                Some(long_name) => JsonValue::String(long_name.clone()),
                None => JsonValue::Null,
            },
        ),
        (
            "bits",
            match channel.bits {
                Some(bits) => JsonValue::Number(bits as f64),
                None => JsonValue::Null,
            },
        ),
        (
            "range",
            match channel.range {
                Some(range) => JsonValue::Number(range as f64),
                None => JsonValue::Null,
            },
        ),
    ])
}

fn compensation_json(compensation: Option<&CompensationMatrix>) -> JsonValue {
    match compensation {
        Some(compensation) => JsonValue::object([
            (
                "source_key",
                JsonValue::String(compensation.source_key.clone()),
            ),
            (
                "dimension",
                JsonValue::Number(compensation.dimension as f64),
            ),
            (
                "parameter_names",
                JsonValue::Array(
                    compensation
                        .parameter_names
                        .iter()
                        .map(|name| JsonValue::String(name.clone()))
                        .collect::<Vec<_>>(),
                ),
            ),
            (
                "values",
                JsonValue::Array(
                    compensation
                        .values
                        .iter()
                        .copied()
                        .map(JsonValue::Number)
                        .collect::<Vec<_>>(),
                ),
            ),
        ]),
        None => JsonValue::Null,
    }
}

fn format_endianness(endianness: Endianness) -> &'static str {
    match endianness {
        Endianness::Little => "Little",
        Endianness::Big => "Big",
    }
}

fn demo_replay() -> Result<(), String> {
    let sample = SampleFrame::new(
        "demo-sample",
        vec![
            "FSC-A".to_string(),
            "SSC-A".to_string(),
            "CD3".to_string(),
            "CD4".to_string(),
        ],
        vec![
            vec![10.0, 10.0, 1.0, 9.0],
            vec![20.0, 20.0, 5.0, 8.0],
            vec![30.0, 30.0, 9.0, 1.0],
            vec![80.0, 80.0, 4.0, 2.0],
        ],
    )
    .map_err(|error| error.to_string())?;

    let mut environment = ReplayEnvironment::new();
    environment
        .insert_sample(sample)
        .map_err(|error| error.to_string())?;

    let mut log = CommandLog::new();
    log.append(Command::RectangleGate {
        sample_id: "demo-sample".to_string(),
        population_id: "lymphocytes".to_string(),
        parent_population: None,
        x_channel: "FSC-A".to_string(),
        y_channel: "SSC-A".to_string(),
        x_min: 0.0,
        x_max: 35.0,
        y_min: 0.0,
        y_max: 35.0,
    });
    log.append(Command::PolygonGate {
        sample_id: "demo-sample".to_string(),
        population_id: "cd3_cd4".to_string(),
        parent_population: Some("lymphocytes".to_string()),
        x_channel: "CD3".to_string(),
        y_channel: "CD4".to_string(),
        vertices: vec![
            Point2D { x: 0.0, y: 7.0 },
            Point2D { x: 6.0, y: 7.0 },
            Point2D { x: 6.0, y: 10.0 },
            Point2D { x: 0.0, y: 10.0 },
        ],
    });

    let state = log
        .replay(&environment)
        .map_err(|error| error.to_string())?;
    println!("command_log_json={}", log.to_json());
    println!("execution_hash={:016x}", state.execution_hash);
    for node in &state.execution_graph {
        let population = state
            .populations
            .get(&node.population_id)
            .ok_or_else(|| "replay graph and populations diverged".to_string())?;
        println!(
            "{} sample={} matched={} hash={:016x}",
            node.population_id, node.sample_id, population.matched_events, node.hash
        );
    }
    Ok(())
}
