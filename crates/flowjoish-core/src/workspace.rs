use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{self, Display, Formatter};

use crate::command::{CommandError, CommandLog};
use crate::gating::{Population, SampleFrame, apply_gate};
use crate::hash::StableHasher;

#[derive(Clone, Debug, Default)]
pub struct ReplayEnvironment {
    samples: BTreeMap<String, SampleFrame>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionGraphNode {
    pub sequence: u64,
    pub sample_id: String,
    pub population_id: String,
    pub parent_population: Option<String>,
    pub hash: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceState {
    pub populations: BTreeMap<String, Population>,
    pub execution_graph: Vec<ExecutionGraphNode>,
    pub execution_hash: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReplayError {
    DuplicateSample(String),
    MissingSample(String),
    DuplicatePopulation(String),
    UnknownParentPopulation(String),
    ParentPopulationSampleMismatch {
        parent_population: String,
        expected_sample: String,
        found_sample: String,
    },
    InvalidCommand(CommandError),
    InvalidGate(String),
}

impl Display for ReplayError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateSample(sample) => write!(f, "sample '{sample}' already exists"),
            Self::MissingSample(sample) => write!(f, "missing sample '{sample}'"),
            Self::DuplicatePopulation(population) => {
                write!(f, "population '{population}' already exists")
            }
            Self::UnknownParentPopulation(population) => {
                write!(f, "unknown parent population '{population}'")
            }
            Self::ParentPopulationSampleMismatch {
                parent_population,
                expected_sample,
                found_sample,
            } => write!(
                f,
                "parent population '{}' belongs to sample '{}' but '{}' was requested",
                parent_population, found_sample, expected_sample
            ),
            Self::InvalidCommand(error) => Display::fmt(error, f),
            Self::InvalidGate(message) => f.write_str(message),
        }
    }
}

impl Error for ReplayError {}

impl From<CommandError> for ReplayError {
    fn from(value: CommandError) -> Self {
        Self::InvalidCommand(value)
    }
}

impl ReplayEnvironment {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_sample(&mut self, sample: SampleFrame) -> Result<(), ReplayError> {
        let sample_id = sample.sample_id().to_string();
        if self.samples.insert(sample_id.clone(), sample).is_some() {
            return Err(ReplayError::DuplicateSample(sample_id));
        }
        Ok(())
    }

    pub fn sample(&self, sample_id: &str) -> Option<&SampleFrame> {
        self.samples.get(sample_id)
    }
}

impl CommandLog {
    pub fn replay(&self, environment: &ReplayEnvironment) -> Result<WorkspaceState, ReplayError> {
        let mut populations: BTreeMap<String, Population> = BTreeMap::new();
        let mut graph = Vec::with_capacity(self.records().len());
        let mut execution_hasher = StableHasher::new();

        for record in self.records() {
            let sample = environment
                .sample(record.command.sample_id())
                .ok_or_else(|| {
                    ReplayError::MissingSample(record.command.sample_id().to_string())
                })?;

            if populations.contains_key(record.command.population_id()) {
                return Err(ReplayError::DuplicatePopulation(
                    record.command.population_id().to_string(),
                ));
            }

            let parent_population = match record.command.parent_population() {
                Some(parent_id) => {
                    let parent = populations.get(parent_id).ok_or_else(|| {
                        ReplayError::UnknownParentPopulation(parent_id.to_string())
                    })?;
                    if parent.sample_id != sample.sample_id() {
                        return Err(ReplayError::ParentPopulationSampleMismatch {
                            parent_population: parent_id.to_string(),
                            expected_sample: sample.sample_id().to_string(),
                            found_sample: parent.sample_id.clone(),
                        });
                    }
                    Some(parent.clone())
                }
                None => None,
            };

            let gate = record.command.to_gate_definition()?;
            let mut population = apply_gate(
                sample,
                &gate,
                parent_population.as_ref().map(|parent| &parent.mask),
            )
            .map_err(|error| ReplayError::InvalidGate(error.to_string()))?;

            let mut node_hasher = StableHasher::new();
            node_hasher.update_u64(sample.fingerprint());
            node_hasher.update_u64(record.command_hash);
            node_hasher.update_u64(
                parent_population
                    .as_ref()
                    .map(|parent| parent.node_hash)
                    .unwrap_or(0),
            );
            node_hasher.update(&population.mask.to_bytes());
            let node_hash = node_hasher.finish_u64();
            population.node_hash = node_hash;

            execution_hasher.update_u64(record.sequence);
            execution_hasher.update_u64(node_hash);

            graph.push(ExecutionGraphNode {
                sequence: record.sequence,
                sample_id: sample.sample_id().to_string(),
                population_id: population.population_id.clone(),
                parent_population: population.parent_population.clone(),
                hash: node_hash,
            });
            populations.insert(population.population_id.clone(), population);
        }

        Ok(WorkspaceState {
            populations,
            execution_graph: graph,
            execution_hash: execution_hasher.finish_u64(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{ReplayEnvironment, ReplayError};
    use crate::command::{Command, CommandLog};
    use crate::gating::{Point2D, SampleFrame};

    fn sample() -> SampleFrame {
        SampleFrame::new(
            "sample-a",
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
        .expect("valid sample")
    }

    #[test]
    fn replay_is_deterministic() {
        let mut environment = ReplayEnvironment::new();
        environment.insert_sample(sample()).expect("sample insert");

        let mut log = CommandLog::new();
        log.append(Command::RectangleGate {
            sample_id: "sample-a".to_string(),
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
            sample_id: "sample-a".to_string(),
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

        let first = log.replay(&environment).expect("replay succeeds");
        let second = CommandLog::from_json(&log.to_json())
            .expect("restore")
            .replay(&environment)
            .expect("replay succeeds");

        assert_eq!(first, second);
        assert_eq!(
            first
                .populations
                .get("cd3_cd4")
                .expect("population")
                .matched_events,
            2
        );
    }

    #[test]
    fn rejects_unknown_parent_population() {
        let mut environment = ReplayEnvironment::new();
        environment.insert_sample(sample()).expect("sample insert");

        let mut log = CommandLog::new();
        log.append(Command::RectangleGate {
            sample_id: "sample-a".to_string(),
            population_id: "child".to_string(),
            parent_population: Some("missing".to_string()),
            x_channel: "FSC-A".to_string(),
            y_channel: "SSC-A".to_string(),
            x_min: 0.0,
            x_max: 10.0,
            y_min: 0.0,
            y_max: 10.0,
        });

        let error = log.replay(&environment).expect_err("missing parent");
        assert_eq!(
            error,
            ReplayError::UnknownParentPopulation("missing".to_string())
        );
    }
}
