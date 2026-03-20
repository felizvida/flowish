use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

use flowjoish_core::{Command, CommandLog, JsonValue, Point2D, ReplayEnvironment, SampleFrame};

pub fn capabilities_json() -> String {
    let (log, execution_hash) = demo_execution_summary().unwrap_or_else(|message| {
        let fallback = JsonValue::object([
            ("status", JsonValue::String("degraded".to_string())),
            ("message", JsonValue::String(message)),
        ]);
        return (
            fallback.stringify_canonical(),
            "0000000000000000".to_string(),
        );
    });

    JsonValue::object([
        ("status", JsonValue::String("ready".to_string())),
        ("mode", JsonValue::String("local-first".to_string())),
        (
            "shared_engine",
            JsonValue::String("flowjoish-core".to_string()),
        ),
        (
            "desktop_contract",
            JsonValue::String("Qt/QML via Rust FFI bridge".to_string()),
        ),
        (
            "endpoints",
            JsonValue::Array(
                ["/", "/health", "/capabilities"]
                    .into_iter()
                    .map(|path| JsonValue::String(path.to_string()))
                    .collect(),
            ),
        ),
        (
            "reproducibility",
            JsonValue::object([
                ("command_log_hash", JsonValue::String(log)),
                ("execution_hash", JsonValue::String(execution_hash)),
            ]),
        ),
    ])
    .stringify_canonical()
}

pub fn health_json() -> String {
    JsonValue::object([
        ("status", JsonValue::String("ok".to_string())),
        ("service", JsonValue::String("parallax-backend".to_string())),
        ("local_first", JsonValue::Bool(true)),
    ])
    .stringify_canonical()
}

pub fn serve(bind_addr: &str) -> Result<(), String> {
    let listener = TcpListener::bind(bind_addr)
        .map_err(|error| format!("failed to bind {}: {}", bind_addr, error))?;

    eprintln!("parallax-backend listening on http://{bind_addr}");
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                if let Err(error) = handle_client(stream) {
                    eprintln!("connection error: {error}");
                }
            }
            Err(error) => eprintln!("accept error: {error}"),
        }
    }

    Ok(())
}

fn handle_client(mut stream: TcpStream) -> Result<(), String> {
    let mut buffer = [0u8; 4096];
    let bytes_read = stream
        .read(&mut buffer)
        .map_err(|error| format!("failed to read request: {error}"))?;
    if bytes_read == 0 {
        return Ok(());
    }

    let request = String::from_utf8_lossy(&buffer[..bytes_read]);
    let path = request_path(&request);

    let (status, body) = match path {
        "/" => (200, capabilities_json()),
        "/health" => (200, health_json()),
        "/capabilities" => (200, capabilities_json()),
        _ => (
            404,
            JsonValue::object([
                ("status", JsonValue::String("not_found".to_string())),
                ("path", JsonValue::String(path.to_string())),
            ])
            .stringify_canonical(),
        ),
    };

    let response = render_json_response(status, &body);
    stream
        .write_all(response.as_bytes())
        .map_err(|error| format!("failed to write response: {error}"))?;
    stream
        .flush()
        .map_err(|error| format!("failed to flush response: {error}"))?;

    Ok(())
}

fn request_path(request: &str) -> &str {
    request
        .lines()
        .next()
        .and_then(|line| {
            let mut parts = line.split_whitespace();
            let _method = parts.next()?;
            parts.next()
        })
        .unwrap_or("/")
}

fn render_json_response(status_code: u16, body: &str) -> String {
    let status_text = match status_code {
        200 => "OK",
        404 => "Not Found",
        _ => "Internal Server Error",
    };

    format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status_code,
        status_text,
        body.len(),
        body
    )
}

fn demo_execution_summary() -> Result<(String, String), String> {
    let sample = SampleFrame::new(
        "backend-demo",
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
        sample_id: "backend-demo".to_string(),
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
        sample_id: "backend-demo".to_string(),
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
    Ok((
        format!("{:016x}", log.execution_hash()),
        format!("{:016x}", state.execution_hash),
    ))
}

#[cfg(test)]
mod tests {
    use super::{capabilities_json, render_json_response, request_path};
    use flowjoish_core::JsonValue;

    #[test]
    fn parses_request_path_from_http_request() {
        let request = "GET /health HTTP/1.1\r\nHost: localhost\r\n\r\n";
        assert_eq!(request_path(request), "/health");
    }

    #[test]
    fn capabilities_payload_is_valid_json() {
        let payload = capabilities_json();
        let parsed = JsonValue::parse(&payload).expect("valid json");
        assert_eq!(
            parsed.get("shared_engine").and_then(JsonValue::as_str),
            Some("flowjoish-core")
        );
    }

    #[test]
    fn renders_http_response_headers() {
        let response = render_json_response(200, "{\"status\":\"ok\"}");
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.contains("Content-Type: application/json"));
    }
}
