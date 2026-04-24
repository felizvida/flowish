use std::error::Error;
use std::fmt::{self, Display, Formatter};

use crate::bitmask::BitMask;
use crate::gating::SampleFrame;
use crate::workspace::WorkspaceState;

#[derive(Clone, Debug, PartialEq)]
pub struct ChannelStats {
    pub channel: String,
    pub mean: Option<f64>,
    pub median: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PopulationStats {
    pub sample_id: String,
    pub population_id: String,
    pub parent_population: Option<String>,
    pub matched_events: usize,
    pub parent_events: usize,
    pub frequency_of_all: f64,
    pub frequency_of_parent: f64,
    pub channel_stats: Vec<ChannelStats>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StatsError {
    UnknownPopulation(String),
    PopulationSampleMismatch {
        population_id: String,
        expected_sample: String,
        found_sample: String,
    },
}

impl Display for StatsError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownPopulation(population_id) => {
                write!(f, "unknown population '{population_id}'")
            }
            Self::PopulationSampleMismatch {
                population_id,
                expected_sample,
                found_sample,
            } => write!(
                f,
                "population '{}' belongs to sample '{}' but '{}' was requested",
                population_id, found_sample, expected_sample
            ),
        }
    }
}

impl Error for StatsError {}

pub fn compute_population_stats(
    sample: &SampleFrame,
    state: &WorkspaceState,
    population_id: &str,
) -> Result<PopulationStats, StatsError> {
    let total_events = sample.event_count();
    if population_id == "__all__" {
        return Ok(PopulationStats {
            sample_id: sample.sample_id().to_string(),
            population_id: "All Events".to_string(),
            parent_population: None,
            matched_events: total_events,
            parent_events: total_events,
            frequency_of_all: frequency(total_events, total_events),
            frequency_of_parent: frequency(total_events, total_events),
            channel_stats: sample
                .channels()
                .iter()
                .enumerate()
                .map(|(index, channel)| channel_stats(sample, channel, index, None))
                .collect(),
        });
    }

    let population = state
        .populations
        .get(population_id)
        .ok_or_else(|| StatsError::UnknownPopulation(population_id.to_string()))?;
    if population.sample_id != sample.sample_id() {
        return Err(StatsError::PopulationSampleMismatch {
            population_id: population.population_id.clone(),
            expected_sample: sample.sample_id().to_string(),
            found_sample: population.sample_id.clone(),
        });
    }

    let parent_events = match population.parent_population.as_deref() {
        Some(parent_id) => state
            .populations
            .get(parent_id)
            .map(|parent| parent.matched_events)
            .unwrap_or(total_events),
        None => total_events,
    };

    Ok(PopulationStats {
        sample_id: sample.sample_id().to_string(),
        population_id: population.population_id.clone(),
        parent_population: population.parent_population.clone(),
        matched_events: population.matched_events,
        parent_events,
        frequency_of_all: frequency(population.matched_events, total_events),
        frequency_of_parent: frequency(population.matched_events, parent_events),
        channel_stats: sample
            .channels()
            .iter()
            .enumerate()
            .map(|(index, channel)| channel_stats(sample, channel, index, Some(&population.mask)))
            .collect(),
    })
}

pub fn compute_population_stats_table(
    sample: &SampleFrame,
    state: &WorkspaceState,
) -> Result<Vec<PopulationStats>, StatsError> {
    let mut values = Vec::with_capacity(state.populations.len() + 1);
    values.push(compute_population_stats(sample, state, "__all__")?);
    for node in &state.execution_graph {
        values.push(compute_population_stats(
            sample,
            state,
            &node.population_id,
        )?);
    }
    Ok(values)
}

fn channel_stats(
    sample: &SampleFrame,
    channel: &str,
    index: usize,
    mask: Option<&BitMask>,
) -> ChannelStats {
    let mut values = sample
        .events()
        .iter()
        .enumerate()
        .filter(|(event_index, _)| mask.is_none_or(|mask| mask.contains(*event_index)))
        .map(|(_, row)| row[index])
        .collect::<Vec<_>>();
    values.sort_by(f64::total_cmp);

    let mean = mean(&values);
    let median = if values.is_empty() {
        None
    } else if values.len() % 2 == 1 {
        Some(values[values.len() / 2])
    } else {
        let upper = values.len() / 2;
        Some(midpoint(values[upper - 1], values[upper]))
    };

    ChannelStats {
        channel: channel.to_string(),
        mean,
        median,
    }
}

fn mean(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }

    let divisor = values.len() as f64;
    let mut sum = 0.0;
    let mut correction = 0.0;
    for value in values {
        let addend = (*value / divisor) - correction;
        let next = sum + addend;
        correction = (next - sum) - addend;
        sum = next;
    }
    Some(sum)
}

fn midpoint(lower: f64, upper: f64) -> f64 {
    (lower / 2.0) + (upper / 2.0)
}

fn frequency(count: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        count as f64 / total as f64
    }
}

#[cfg(test)]
mod tests {
    use super::{StatsError, compute_population_stats, compute_population_stats_table};
    use crate::command::{Command, CommandLog};
    use crate::workspace::ReplayEnvironment;
    use crate::{Point2D, SampleFrame, WorkspaceState};

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

    fn replay_state() -> crate::WorkspaceState {
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

        log.replay(&environment).expect("replay succeeds")
    }

    #[test]
    fn computes_stats_for_all_events_and_child_populations() {
        let sample = sample();
        let state = replay_state();

        let all_events = compute_population_stats(&sample, &state, "__all__").expect("all stats");
        assert_eq!(all_events.matched_events, 4);
        assert_eq!(all_events.parent_events, 4);
        assert_eq!(all_events.frequency_of_all, 1.0);
        assert_eq!(all_events.frequency_of_parent, 1.0);
        assert_eq!(all_events.channel_stats[2].channel, "CD3");
        assert_eq!(all_events.channel_stats[2].mean, Some(4.75));
        assert_eq!(all_events.channel_stats[2].median, Some(4.5));

        let child = compute_population_stats(&sample, &state, "cd3_cd4").expect("child stats");
        assert_eq!(child.matched_events, 2);
        assert_eq!(child.parent_events, 3);
        assert_eq!(child.frequency_of_all, 0.5);
        assert!((child.frequency_of_parent - (2.0 / 3.0)).abs() < 1e-9);
        assert_eq!(child.channel_stats[3].mean, Some(8.5));
        assert_eq!(child.channel_stats[3].median, Some(8.5));
    }

    #[test]
    fn returns_population_stats_in_execution_order() {
        let table = compute_population_stats_table(&sample(), &replay_state()).expect("table");
        let ids = table
            .into_iter()
            .map(|stats| stats.population_id)
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["All Events", "lymphocytes", "cd3_cd4"]);
    }

    #[test]
    fn rejects_unknown_population_keys() {
        let error = compute_population_stats(&sample(), &replay_state(), "missing")
            .expect_err("missing population should fail");
        assert_eq!(error, StatsError::UnknownPopulation("missing".to_string()));
    }

    #[test]
    fn stats_mean_and_median_avoid_overflow_for_large_finite_values() {
        let empty_state = WorkspaceState {
            populations: Default::default(),
            execution_graph: Vec::new(),
            execution_hash: 0,
        };

        let opposing_extremes = SampleFrame::new(
            "opposing-extremes",
            vec!["signal".to_string()],
            vec![
                vec![-f64::MAX],
                vec![-f64::MAX],
                vec![f64::MAX],
                vec![f64::MAX],
            ],
        )
        .expect("valid sample");
        let stats = compute_population_stats(&opposing_extremes, &empty_state, "__all__")
            .expect("stats succeed");
        assert_eq!(stats.channel_stats[0].mean, Some(0.0));
        assert_eq!(stats.channel_stats[0].median, Some(0.0));

        let same_sign_extremes = SampleFrame::new(
            "same-sign-extremes",
            vec!["signal".to_string()],
            vec![vec![f64::MAX], vec![f64::MAX]],
        )
        .expect("valid sample");
        let stats = compute_population_stats(&same_sign_extremes, &empty_state, "__all__")
            .expect("stats succeed");
        assert_eq!(stats.channel_stats[0].mean, Some(f64::MAX));
        assert_eq!(stats.channel_stats[0].median, Some(f64::MAX));
    }
}
