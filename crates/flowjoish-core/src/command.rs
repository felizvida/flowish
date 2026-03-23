use std::error::Error;
use std::fmt::{self, Display, Formatter};

use crate::gating::{GateDefinition, GateShape, Point2D, PolygonGate, RectangleGate};
use crate::hash::StableHasher;
use crate::json::{JsonError, JsonValue};

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    RectangleGate {
        sample_id: String,
        population_id: String,
        parent_population: Option<String>,
        x_channel: String,
        y_channel: String,
        x_min: f64,
        x_max: f64,
        y_min: f64,
        y_max: f64,
    },
    PolygonGate {
        sample_id: String,
        population_id: String,
        parent_population: Option<String>,
        x_channel: String,
        y_channel: String,
        vertices: Vec<Point2D>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct CommandRecord {
    pub sequence: u64,
    pub previous_hash: Option<u64>,
    pub command_hash: u64,
    pub command: Command,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CommandLog {
    records: Vec<CommandRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandError {
    Json(JsonError),
    MissingField(&'static str),
    InvalidField(&'static str),
    UnknownKind(String),
    InvalidGeometry(String),
    HashMismatch {
        sequence: u64,
        expected: u64,
        found: u64,
    },
}

impl Display for CommandError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => Display::fmt(error, f),
            Self::MissingField(name) => write!(f, "missing field '{name}'"),
            Self::InvalidField(name) => write!(f, "invalid field '{name}'"),
            Self::UnknownKind(kind) => write!(f, "unknown command kind '{kind}'"),
            Self::InvalidGeometry(message) => f.write_str(message),
            Self::HashMismatch {
                sequence,
                expected,
                found,
            } => write!(
                f,
                "hash mismatch at sequence {}: expected {:016x}, found {:016x}",
                sequence, expected, found
            ),
        }
    }
}

impl Error for CommandError {}

impl From<JsonError> for CommandError {
    fn from(value: JsonError) -> Self {
        Self::Json(value)
    }
}

impl Command {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::RectangleGate { .. } => "rectangle_gate",
            Self::PolygonGate { .. } => "polygon_gate",
        }
    }

    pub fn sample_id(&self) -> &str {
        match self {
            Self::RectangleGate { sample_id, .. } | Self::PolygonGate { sample_id, .. } => {
                sample_id
            }
        }
    }

    pub fn with_sample_id(&self, sample_id: impl Into<String>) -> Self {
        let sample_id = sample_id.into();
        match self {
            Self::RectangleGate {
                population_id,
                parent_population,
                x_channel,
                y_channel,
                x_min,
                x_max,
                y_min,
                y_max,
                ..
            } => Self::RectangleGate {
                sample_id,
                population_id: population_id.clone(),
                parent_population: parent_population.clone(),
                x_channel: x_channel.clone(),
                y_channel: y_channel.clone(),
                x_min: *x_min,
                x_max: *x_max,
                y_min: *y_min,
                y_max: *y_max,
            },
            Self::PolygonGate {
                population_id,
                parent_population,
                x_channel,
                y_channel,
                vertices,
                ..
            } => Self::PolygonGate {
                sample_id,
                population_id: population_id.clone(),
                parent_population: parent_population.clone(),
                x_channel: x_channel.clone(),
                y_channel: y_channel.clone(),
                vertices: vertices.clone(),
            },
        }
    }

    pub fn population_id(&self) -> &str {
        match self {
            Self::RectangleGate { population_id, .. } | Self::PolygonGate { population_id, .. } => {
                population_id
            }
        }
    }

    pub fn parent_population(&self) -> Option<&str> {
        match self {
            Self::RectangleGate {
                parent_population, ..
            }
            | Self::PolygonGate {
                parent_population, ..
            } => parent_population.as_deref(),
        }
    }

    pub fn to_gate_definition(&self) -> Result<GateDefinition, CommandError> {
        match self {
            Self::RectangleGate {
                population_id,
                parent_population,
                x_channel,
                y_channel,
                x_min,
                x_max,
                y_min,
                y_max,
                ..
            } => Ok(GateDefinition {
                population_id: population_id.clone(),
                parent_population: parent_population.clone(),
                x_channel: x_channel.clone(),
                y_channel: y_channel.clone(),
                shape: GateShape::Rectangle(
                    RectangleGate::new(*x_min, *x_max, *y_min, *y_max)
                        .map_err(|error| CommandError::InvalidGeometry(error.to_string()))?,
                ),
            }),
            Self::PolygonGate {
                population_id,
                parent_population,
                x_channel,
                y_channel,
                vertices,
                ..
            } => Ok(GateDefinition {
                population_id: population_id.clone(),
                parent_population: parent_population.clone(),
                x_channel: x_channel.clone(),
                y_channel: y_channel.clone(),
                shape: GateShape::Polygon(
                    PolygonGate::new(vertices.clone())
                        .map_err(|error| CommandError::InvalidGeometry(error.to_string()))?,
                ),
            }),
        }
    }

    pub fn stable_hash(&self) -> u64 {
        let mut hasher = StableHasher::new();
        hasher.update_str(&self.to_json_value().stringify_canonical());
        hasher.finish_u64()
    }

    pub fn to_json_value(&self) -> JsonValue {
        match self {
            Self::RectangleGate {
                sample_id,
                population_id,
                parent_population,
                x_channel,
                y_channel,
                x_min,
                x_max,
                y_min,
                y_max,
            } => JsonValue::object([
                ("kind", JsonValue::String(self.kind().to_string())),
                ("sample_id", JsonValue::String(sample_id.clone())),
                ("population_id", JsonValue::String(population_id.clone())),
                (
                    "parent_population",
                    option_to_json(parent_population.as_ref()),
                ),
                ("x_channel", JsonValue::String(x_channel.clone())),
                ("y_channel", JsonValue::String(y_channel.clone())),
                ("x_min", JsonValue::Number(*x_min)),
                ("x_max", JsonValue::Number(*x_max)),
                ("y_min", JsonValue::Number(*y_min)),
                ("y_max", JsonValue::Number(*y_max)),
            ]),
            Self::PolygonGate {
                sample_id,
                population_id,
                parent_population,
                x_channel,
                y_channel,
                vertices,
            } => JsonValue::object([
                ("kind", JsonValue::String(self.kind().to_string())),
                ("sample_id", JsonValue::String(sample_id.clone())),
                ("population_id", JsonValue::String(population_id.clone())),
                (
                    "parent_population",
                    option_to_json(parent_population.as_ref()),
                ),
                ("x_channel", JsonValue::String(x_channel.clone())),
                ("y_channel", JsonValue::String(y_channel.clone())),
                (
                    "vertices",
                    JsonValue::Array(
                        vertices
                            .iter()
                            .map(|vertex| {
                                JsonValue::object([
                                    ("x", JsonValue::Number(vertex.x)),
                                    ("y", JsonValue::Number(vertex.y)),
                                ])
                            })
                            .collect(),
                    ),
                ),
            ]),
        }
    }

    pub fn from_json_value(value: &JsonValue) -> Result<Self, CommandError> {
        let kind = required_string(value, "kind")?;
        match kind {
            "rectangle_gate" => Ok(Self::RectangleGate {
                sample_id: required_string(value, "sample_id")?.to_string(),
                population_id: required_string(value, "population_id")?.to_string(),
                parent_population: optional_string(value, "parent_population")?,
                x_channel: required_string(value, "x_channel")?.to_string(),
                y_channel: required_string(value, "y_channel")?.to_string(),
                x_min: required_number(value, "x_min")?,
                x_max: required_number(value, "x_max")?,
                y_min: required_number(value, "y_min")?,
                y_max: required_number(value, "y_max")?,
            }),
            "polygon_gate" => Ok(Self::PolygonGate {
                sample_id: required_string(value, "sample_id")?.to_string(),
                population_id: required_string(value, "population_id")?.to_string(),
                parent_population: optional_string(value, "parent_population")?,
                x_channel: required_string(value, "x_channel")?.to_string(),
                y_channel: required_string(value, "y_channel")?.to_string(),
                vertices: required_vertices(value, "vertices")?,
            }),
            other => Err(CommandError::UnknownKind(other.to_string())),
        }
    }
}

impl CommandLog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn records(&self) -> &[CommandRecord] {
        &self.records
    }

    pub fn append(&mut self, command: Command) -> &CommandRecord {
        let sequence = self.records.len() as u64 + 1;
        let previous_hash = self.records.last().map(|record| record.command_hash);
        let command_hash = command.stable_hash();
        self.records.push(CommandRecord {
            sequence,
            previous_hash,
            command_hash,
            command,
        });
        self.records.last().expect("record was just pushed")
    }

    pub fn pop(&mut self) -> Option<CommandRecord> {
        self.records.pop()
    }

    pub fn execution_hash(&self) -> u64 {
        let mut hasher = StableHasher::new();
        for record in &self.records {
            hasher.update_u64(record.sequence);
            hasher.update_u64(record.previous_hash.unwrap_or(0));
            hasher.update_u64(record.command_hash);
        }
        hasher.finish_u64()
    }

    pub fn to_json(&self) -> String {
        JsonValue::Array(
            self.records
                .iter()
                .map(CommandRecord::to_json_value)
                .collect(),
        )
        .stringify_canonical()
    }

    pub fn from_json(input: &str) -> Result<Self, CommandError> {
        let parsed = JsonValue::parse(input)?;
        let records = parsed
            .as_array()
            .ok_or(CommandError::InvalidField("records"))?
            .iter()
            .map(CommandRecord::from_json_value)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { records })
    }
}

impl CommandRecord {
    fn to_json_value(&self) -> JsonValue {
        JsonValue::object([
            ("sequence", JsonValue::Number(self.sequence as f64)),
            (
                "previous_hash",
                match self.previous_hash {
                    Some(value) => JsonValue::String(format!("{value:016x}")),
                    None => JsonValue::Null,
                },
            ),
            (
                "command_hash",
                JsonValue::String(format!("{:016x}", self.command_hash)),
            ),
            ("command", self.command.to_json_value()),
        ])
    }

    fn from_json_value(value: &JsonValue) -> Result<Self, CommandError> {
        let sequence = value
            .get("sequence")
            .and_then(JsonValue::as_u64)
            .ok_or(CommandError::MissingField("sequence"))?;
        let previous_hash = match value.get("previous_hash") {
            Some(JsonValue::Null) | None => None,
            Some(JsonValue::String(hex)) => Some(parse_hex_hash(hex, "previous_hash")?),
            _ => return Err(CommandError::InvalidField("previous_hash")),
        };
        let found_hash = value
            .get("command_hash")
            .and_then(JsonValue::as_str)
            .ok_or(CommandError::MissingField("command_hash"))
            .and_then(|hash| parse_hex_hash(hash, "command_hash"))?;
        let command_value = value
            .get("command")
            .ok_or(CommandError::MissingField("command"))?;
        let command = Command::from_json_value(command_value)?;
        let expected_hash = command.stable_hash();
        if expected_hash != found_hash {
            return Err(CommandError::HashMismatch {
                sequence,
                expected: expected_hash,
                found: found_hash,
            });
        }

        Ok(Self {
            sequence,
            previous_hash,
            command_hash: found_hash,
            command,
        })
    }
}

fn option_to_json(value: Option<&String>) -> JsonValue {
    match value {
        Some(value) => JsonValue::String(value.clone()),
        None => JsonValue::Null,
    }
}

fn required_string<'a>(value: &'a JsonValue, field: &'static str) -> Result<&'a str, CommandError> {
    value
        .get(field)
        .and_then(JsonValue::as_str)
        .ok_or(CommandError::MissingField(field))
}

fn optional_string(value: &JsonValue, field: &'static str) -> Result<Option<String>, CommandError> {
    match value.get(field) {
        Some(JsonValue::Null) | None => Ok(None),
        Some(JsonValue::String(text)) => Ok(Some(text.clone())),
        _ => Err(CommandError::InvalidField(field)),
    }
}

fn required_number(value: &JsonValue, field: &'static str) -> Result<f64, CommandError> {
    value
        .get(field)
        .and_then(JsonValue::as_f64)
        .ok_or(CommandError::MissingField(field))
}

fn required_vertices(value: &JsonValue, field: &'static str) -> Result<Vec<Point2D>, CommandError> {
    let vertices = value
        .get(field)
        .and_then(JsonValue::as_array)
        .ok_or(CommandError::MissingField(field))?;
    vertices
        .iter()
        .map(|vertex| {
            let x = vertex
                .get("x")
                .and_then(JsonValue::as_f64)
                .ok_or(CommandError::InvalidField("vertices.x"))?;
            let y = vertex
                .get("y")
                .and_then(JsonValue::as_f64)
                .ok_or(CommandError::InvalidField("vertices.y"))?;
            Ok(Point2D { x, y })
        })
        .collect()
}

fn parse_hex_hash(value: &str, field: &'static str) -> Result<u64, CommandError> {
    u64::from_str_radix(value, 16).map_err(|_| CommandError::InvalidField(field))
}

#[cfg(test)]
mod tests {
    use super::{Command, CommandLog};
    use crate::gating::Point2D;

    #[test]
    fn round_trips_command_logs_through_json() {
        let mut log = CommandLog::new();
        log.append(Command::RectangleGate {
            sample_id: "sample-a".to_string(),
            population_id: "lymphocytes".to_string(),
            parent_population: None,
            x_channel: "FSC-A".to_string(),
            y_channel: "SSC-A".to_string(),
            x_min: 0.0,
            x_max: 10.0,
            y_min: 1.0,
            y_max: 11.0,
        });
        log.append(Command::PolygonGate {
            sample_id: "sample-a".to_string(),
            population_id: "cd3".to_string(),
            parent_population: Some("lymphocytes".to_string()),
            x_channel: "CD3".to_string(),
            y_channel: "CD4".to_string(),
            vertices: vec![
                Point2D { x: 0.0, y: 0.0 },
                Point2D { x: 10.0, y: 0.0 },
                Point2D { x: 10.0, y: 10.0 },
            ],
        });

        let json = log.to_json();
        let restored = CommandLog::from_json(&json).expect("log is valid json");
        assert_eq!(restored, log);
        assert_eq!(restored.execution_hash(), log.execution_hash());
    }

    #[test]
    fn pop_returns_last_record_and_updates_execution_hash() {
        let mut log = CommandLog::new();
        log.append(Command::RectangleGate {
            sample_id: "sample-a".to_string(),
            population_id: "root".to_string(),
            parent_population: None,
            x_channel: "FSC-A".to_string(),
            y_channel: "SSC-A".to_string(),
            x_min: 0.0,
            x_max: 10.0,
            y_min: 0.0,
            y_max: 10.0,
        });
        log.append(Command::PolygonGate {
            sample_id: "sample-a".to_string(),
            population_id: "child".to_string(),
            parent_population: Some("root".to_string()),
            x_channel: "CD3".to_string(),
            y_channel: "CD4".to_string(),
            vertices: vec![
                Point2D { x: 0.0, y: 0.0 },
                Point2D { x: 10.0, y: 0.0 },
                Point2D { x: 10.0, y: 10.0 },
            ],
        });

        let hash_before_pop = log.execution_hash();
        let popped = log.pop().expect("record exists");
        assert_eq!(popped.sequence, 2);
        assert_eq!(popped.command.population_id(), "child");
        assert_eq!(log.len(), 1);
        assert_ne!(log.execution_hash(), hash_before_pop);
        assert!(log.pop().is_some());
        assert!(log.pop().is_none());
    }
}
