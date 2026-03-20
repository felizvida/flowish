use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{self, Display, Formatter};

use crate::bitmask::BitMask;
use crate::hash::StableHasher;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point2D {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RectangleGate {
    pub x_min: f64,
    pub x_max: f64,
    pub y_min: f64,
    pub y_max: f64,
}

impl RectangleGate {
    pub fn new(x_min: f64, x_max: f64, y_min: f64, y_max: f64) -> Result<Self, GatingError> {
        let values = [x_min, x_max, y_min, y_max];
        if values.iter().any(|value| !value.is_finite()) {
            return Err(GatingError::InvalidGeometry(
                "rectangle gate values must be finite".to_string(),
            ));
        }
        Ok(Self {
            x_min: x_min.min(x_max),
            x_max: x_min.max(x_max),
            y_min: y_min.min(y_max),
            y_max: y_min.max(y_max),
        })
    }

    fn contains(&self, point: Point2D) -> bool {
        point.x >= self.x_min
            && point.x <= self.x_max
            && point.y >= self.y_min
            && point.y <= self.y_max
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PolygonGate {
    pub vertices: Vec<Point2D>,
}

impl PolygonGate {
    pub fn new(vertices: Vec<Point2D>) -> Result<Self, GatingError> {
        if vertices.len() < 3 {
            return Err(GatingError::InvalidGeometry(
                "polygon gate requires at least three vertices".to_string(),
            ));
        }
        if vertices
            .iter()
            .any(|vertex| !vertex.x.is_finite() || !vertex.y.is_finite())
        {
            return Err(GatingError::InvalidGeometry(
                "polygon vertices must be finite".to_string(),
            ));
        }
        Ok(Self { vertices })
    }

    fn contains(&self, point: Point2D) -> bool {
        let mut inside = false;
        for index in 0..self.vertices.len() {
            let start = self.vertices[index];
            let end = self.vertices[(index + 1) % self.vertices.len()];

            if point_on_segment(point, start, end) {
                return true;
            }

            let intersects = ((start.y > point.y) != (end.y > point.y))
                && (point.x
                    <= (end.x - start.x) * (point.y - start.y) / (end.y - start.y) + start.x);

            if intersects {
                inside = !inside;
            }
        }
        inside
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum GateShape {
    Rectangle(RectangleGate),
    Polygon(PolygonGate),
}

impl GateShape {
    fn contains(&self, point: Point2D) -> bool {
        match self {
            Self::Rectangle(shape) => shape.contains(point),
            Self::Polygon(shape) => shape.contains(point),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GateDefinition {
    pub population_id: String,
    pub parent_population: Option<String>,
    pub x_channel: String,
    pub y_channel: String,
    pub shape: GateShape,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Population {
    pub sample_id: String,
    pub population_id: String,
    pub parent_population: Option<String>,
    pub mask: BitMask,
    pub matched_events: usize,
    pub node_hash: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SampleFrame {
    sample_id: String,
    channels: Vec<String>,
    channel_lookup: BTreeMap<String, usize>,
    events: Vec<Vec<f64>>,
    fingerprint: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SampleError {
    EmptySampleId,
    EmptyChannels,
    DuplicateChannel(String),
    RaggedEvents {
        row: usize,
        expected: usize,
        found: usize,
    },
    NonFiniteValue {
        row: usize,
        column: usize,
    },
}

impl Display for SampleError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptySampleId => write!(f, "sample id cannot be empty"),
            Self::EmptyChannels => write!(f, "sample requires at least one channel"),
            Self::DuplicateChannel(name) => write!(f, "duplicate channel '{name}'"),
            Self::RaggedEvents {
                row,
                expected,
                found,
            } => write!(
                f,
                "row {} has {} values but {} were expected",
                row, found, expected
            ),
            Self::NonFiniteValue { row, column } => {
                write!(
                    f,
                    "row {} column {} contains a non-finite value",
                    row, column
                )
            }
        }
    }
}

impl Error for SampleError {}

impl SampleFrame {
    pub fn new(
        sample_id: impl Into<String>,
        channels: Vec<String>,
        events: Vec<Vec<f64>>,
    ) -> Result<Self, SampleError> {
        let sample_id = sample_id.into();
        if sample_id.trim().is_empty() {
            return Err(SampleError::EmptySampleId);
        }
        if channels.is_empty() {
            return Err(SampleError::EmptyChannels);
        }

        let mut seen = BTreeSet::new();
        for channel in &channels {
            if !seen.insert(channel.clone()) {
                return Err(SampleError::DuplicateChannel(channel.clone()));
            }
        }

        for (row_index, row) in events.iter().enumerate() {
            if row.len() != channels.len() {
                return Err(SampleError::RaggedEvents {
                    row: row_index,
                    expected: channels.len(),
                    found: row.len(),
                });
            }
            for (column_index, value) in row.iter().enumerate() {
                if !value.is_finite() {
                    return Err(SampleError::NonFiniteValue {
                        row: row_index,
                        column: column_index,
                    });
                }
            }
        }

        let channel_lookup = channels
            .iter()
            .enumerate()
            .map(|(index, name)| (name.clone(), index))
            .collect::<BTreeMap<_, _>>();

        let fingerprint = compute_sample_fingerprint(&sample_id, &channels, &events);

        Ok(Self {
            sample_id,
            channels,
            channel_lookup,
            events,
            fingerprint,
        })
    }

    pub fn sample_id(&self) -> &str {
        &self.sample_id
    }

    pub fn channels(&self) -> &[String] {
        &self.channels
    }

    pub fn channel_index(&self, channel: &str) -> Option<usize> {
        self.channel_lookup.get(channel).copied()
    }

    pub fn events(&self) -> &[Vec<f64>] {
        &self.events
    }

    pub fn event_count(&self) -> usize {
        self.events.len()
    }

    pub fn fingerprint(&self) -> u64 {
        self.fingerprint
    }

    fn axis_pair(&self, x_channel: &str, y_channel: &str) -> Result<(usize, usize), GatingError> {
        let x_index = self
            .channel_lookup
            .get(x_channel)
            .copied()
            .ok_or_else(|| GatingError::UnknownChannel(x_channel.to_string()))?;
        let y_index = self
            .channel_lookup
            .get(y_channel)
            .copied()
            .ok_or_else(|| GatingError::UnknownChannel(y_channel.to_string()))?;
        Ok((x_index, y_index))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GatingError {
    UnknownChannel(String),
    InvalidGeometry(String),
    ParentPopulationLengthMismatch { expected: usize, found: usize },
}

impl Display for GatingError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownChannel(name) => write!(f, "unknown channel '{name}'"),
            Self::InvalidGeometry(message) => f.write_str(message),
            Self::ParentPopulationLengthMismatch { expected, found } => write!(
                f,
                "parent population length mismatch: expected {}, found {}",
                expected, found
            ),
        }
    }
}

impl Error for GatingError {}

pub fn apply_gate(
    sample: &SampleFrame,
    gate: &GateDefinition,
    parent_population: Option<&BitMask>,
) -> Result<Population, GatingError> {
    let (x_index, y_index) = sample.axis_pair(&gate.x_channel, &gate.y_channel)?;

    if let Some(parent) = parent_population
        && parent.len() != sample.event_count()
    {
        return Err(GatingError::ParentPopulationLengthMismatch {
            expected: sample.event_count(),
            found: parent.len(),
        });
    }

    let mask = BitMask::from_predicate(sample.event_count(), |event_index| {
        if let Some(parent) = parent_population
            && !parent.contains(event_index)
        {
            return false;
        }

        let row = &sample.events[event_index];
        let point = Point2D {
            x: row[x_index],
            y: row[y_index],
        };
        gate.shape.contains(point)
    });

    Ok(Population {
        sample_id: sample.sample_id.clone(),
        population_id: gate.population_id.clone(),
        parent_population: gate.parent_population.clone(),
        matched_events: mask.count_ones(),
        mask,
        node_hash: 0,
    })
}

fn point_on_segment(point: Point2D, start: Point2D, end: Point2D) -> bool {
    const EPSILON: f64 = 1e-9;

    let cross = (point.y - start.y) * (end.x - start.x) - (point.x - start.x) * (end.y - start.y);
    if cross.abs() > EPSILON {
        return false;
    }

    let min_x = start.x.min(end.x) - EPSILON;
    let max_x = start.x.max(end.x) + EPSILON;
    let min_y = start.y.min(end.y) - EPSILON;
    let max_y = start.y.max(end.y) + EPSILON;

    point.x >= min_x && point.x <= max_x && point.y >= min_y && point.y <= max_y
}

fn compute_sample_fingerprint(sample_id: &str, channels: &[String], events: &[Vec<f64>]) -> u64 {
    let mut hasher = StableHasher::new();
    hasher.update_str(sample_id);
    hasher.update_u64(channels.len() as u64);
    for channel in channels {
        hasher.update_str(channel);
    }
    hasher.update_u64(events.len() as u64);
    for row in events {
        for value in row {
            hasher.update(&value.to_le_bytes());
        }
    }
    hasher.finish_u64()
}

#[cfg(test)]
mod tests {
    use super::{
        GateDefinition, GateShape, Point2D, PolygonGate, RectangleGate, SampleFrame, apply_gate,
    };
    use crate::bitmask::BitMask;

    fn sample() -> SampleFrame {
        SampleFrame::new(
            "sample-a",
            vec!["FSC-A".to_string(), "SSC-A".to_string()],
            vec![
                vec![10.0, 10.0],
                vec![30.0, 30.0],
                vec![50.0, 50.0],
                vec![70.0, 20.0],
            ],
        )
        .expect("valid sample")
    }

    #[test]
    fn rectangle_gates_are_boundary_inclusive() {
        let gate = GateDefinition {
            population_id: "rect".to_string(),
            parent_population: None,
            x_channel: "FSC-A".to_string(),
            y_channel: "SSC-A".to_string(),
            shape: GateShape::Rectangle(
                RectangleGate::new(10.0, 50.0, 10.0, 50.0).expect("rectangle"),
            ),
        };
        let result = apply_gate(&sample(), &gate, None).expect("gate applies");
        assert_eq!(result.matched_events, 3);
        assert_eq!(result.mask.iter_ones().collect::<Vec<_>>(), vec![0, 1, 2]);
    }

    #[test]
    fn polygon_gates_treat_edge_points_as_inside() {
        let gate = GateDefinition {
            population_id: "poly".to_string(),
            parent_population: None,
            x_channel: "FSC-A".to_string(),
            y_channel: "SSC-A".to_string(),
            shape: GateShape::Polygon(
                PolygonGate::new(vec![
                    Point2D { x: 10.0, y: 10.0 },
                    Point2D { x: 55.0, y: 10.0 },
                    Point2D { x: 55.0, y: 55.0 },
                ])
                .expect("polygon"),
            ),
        };
        let result = apply_gate(&sample(), &gate, None).expect("gate applies");
        assert_eq!(result.mask.iter_ones().collect::<Vec<_>>(), vec![0, 1, 2]);
    }

    #[test]
    fn parent_masks_are_respected() {
        let gate = GateDefinition {
            population_id: "child".to_string(),
            parent_population: Some("parent".to_string()),
            x_channel: "FSC-A".to_string(),
            y_channel: "SSC-A".to_string(),
            shape: GateShape::Rectangle(
                RectangleGate::new(0.0, 100.0, 0.0, 100.0).expect("rectangle"),
            ),
        };
        let parent = BitMask::from_predicate(4, |index| index <= 1);
        let result = apply_gate(&sample(), &gate, Some(&parent)).expect("gate applies");
        assert_eq!(result.mask.iter_ones().collect::<Vec<_>>(), vec![0, 1]);
    }
}
