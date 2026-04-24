use std::collections::BTreeMap;
use std::ffi::{CStr, CString};
use std::fmt::Write as _;
use std::fs;
use std::os::raw::c_char;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::ptr;

use flowjoish_core::{
    BitMask, ChannelTransform, Command, CommandLog, CompensationMatrix, JsonValue, PopulationStats,
    ReplayEnvironment, SampleAnalysisProfile, SampleFrame, StableHasher, WorkspaceState,
    apply_sample_analysis, compute_population_stats_table,
};
use flowjoish_fcs::parse as parse_fcs;

#[derive(Clone, Debug)]
struct DesktopSampleInfo {
    display_name: String,
    source_path: Option<String>,
    group_label: String,
}

#[derive(Clone, Debug)]
struct DesktopSampleArtifact {
    raw_sample: SampleFrame,
    compensation: Option<CompensationMatrix>,
}

#[derive(Clone, Debug, PartialEq)]
struct PopulationComparisonRow {
    sample_id: String,
    display_name: String,
    source_path: Option<String>,
    group_label: String,
    is_active_sample: bool,
    status: String,
    matched_events: Option<usize>,
    parent_events: Option<usize>,
    frequency_of_all: Option<f64>,
    frequency_of_parent: Option<f64>,
    delta_frequency_of_all: Option<f64>,
    delta_frequency_of_parent: Option<f64>,
    derived_metric_status: String,
    derived_metric_value: Option<f64>,
    derived_metric_delta_value: Option<f64>,
    derived_metric_message: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
struct PopulationGroupSummaryRow {
    group_label: String,
    is_active_group: bool,
    sample_count: usize,
    available_sample_count: usize,
    missing_sample_count: usize,
    derived_metric_available_sample_count: usize,
    derived_metric_unavailable_sample_count: usize,
    total_matched_events: usize,
    total_parent_events: usize,
    mean_frequency_of_all: Option<f64>,
    mean_frequency_of_parent: Option<f64>,
    delta_mean_frequency_of_all: Option<f64>,
    delta_mean_frequency_of_parent: Option<f64>,
    mean_derived_metric_value: Option<f64>,
    delta_mean_derived_metric_value: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
enum DerivedMetricDefinition {
    PositiveFraction {
        channel: String,
        threshold: f64,
    },
    MeanRatio {
        numerator_channel: String,
        denominator_channel: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
struct DerivedMetricEvaluation {
    status: String,
    value: Option<f64>,
    message: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
enum AnalysisAction {
    SetCompensationEnabled {
        sample_id: String,
        enabled: bool,
    },
    SetChannelTransform {
        sample_id: String,
        channel: String,
        transform: ChannelTransform,
    },
}

#[derive(Clone, Debug, PartialEq)]
struct AnalysisActionRecord {
    sequence: u64,
    previous_hash: Option<u64>,
    action_hash: u64,
    action: AnalysisAction,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct AnalysisActionLog {
    records: Vec<AnalysisActionRecord>,
}

#[derive(Clone, Debug, PartialEq)]
enum ViewAction {
    ResetPlotView {
        sample_id: String,
        plot_id: String,
    },
    FocusPlotPopulation {
        sample_id: String,
        plot_id: String,
        population_id: String,
        padding_fraction: f64,
    },
    ScalePlotView {
        sample_id: String,
        plot_id: String,
        factor: f64,
    },
}

#[derive(Clone, Debug, PartialEq)]
struct ViewActionRecord {
    sequence: u64,
    previous_hash: Option<u64>,
    action_hash: u64,
    action: ViewAction,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct ViewActionLog {
    records: Vec<ViewActionRecord>,
}

#[derive(Clone, Debug, PartialEq)]
struct PlotRangeState {
    x_min: f64,
    x_max: f64,
    y_min: f64,
    y_max: f64,
    summary: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlotKind {
    Scatter,
    Histogram,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PlotSpec {
    id: String,
    title: String,
    kind: PlotKind,
    x_channel: String,
    y_channel: Option<String>,
}

impl DerivedMetricDefinition {
    fn label(&self) -> String {
        match self {
            Self::PositiveFraction { channel, threshold } => {
                format!("Positive fraction: {channel} >= {threshold:.2}")
            }
            Self::MeanRatio {
                numerator_channel,
                denominator_channel,
            } => format!("Mean ratio: {numerator_channel} / {denominator_channel}"),
        }
    }

    fn kind_name(&self) -> &'static str {
        match self {
            Self::PositiveFraction { .. } => "positive_fraction",
            Self::MeanRatio { .. } => "mean_ratio",
        }
    }

    fn stable_hash(&self) -> u64 {
        let mut hasher = StableHasher::new();
        hasher.update_str(&self.to_json_value().stringify_canonical());
        hasher.finish_u64()
    }

    fn to_json_value(&self) -> JsonValue {
        match self {
            Self::PositiveFraction { channel, threshold } => JsonValue::object([
                ("kind", JsonValue::String(self.kind_name().to_string())),
                ("label", JsonValue::String(self.label())),
                ("channel", JsonValue::String(channel.clone())),
                ("threshold", JsonValue::Number(*threshold)),
            ]),
            Self::MeanRatio {
                numerator_channel,
                denominator_channel,
            } => JsonValue::object([
                ("kind", JsonValue::String(self.kind_name().to_string())),
                ("label", JsonValue::String(self.label())),
                (
                    "numerator_channel",
                    JsonValue::String(numerator_channel.clone()),
                ),
                (
                    "denominator_channel",
                    JsonValue::String(denominator_channel.clone()),
                ),
            ]),
        }
    }

    fn from_json_value(value: &JsonValue) -> Result<Self, String> {
        let kind = value
            .get("kind")
            .and_then(JsonValue::as_str)
            .ok_or_else(|| "derived metric is missing kind".to_string())?;
        match kind {
            "positive_fraction" => {
                let channel = value
                    .get("channel")
                    .and_then(JsonValue::as_str)
                    .ok_or_else(|| {
                        "positive_fraction derived metric is missing channel".to_string()
                    })?
                    .to_string();
                let threshold = value
                    .get("threshold")
                    .and_then(JsonValue::as_f64)
                    .ok_or_else(|| {
                        "positive_fraction derived metric is missing threshold".to_string()
                    })?;
                if !threshold.is_finite() {
                    return Err("positive_fraction threshold must be finite".to_string());
                }
                Ok(Self::PositiveFraction { channel, threshold })
            }
            "mean_ratio" => {
                let numerator_channel = value
                    .get("numerator_channel")
                    .and_then(JsonValue::as_str)
                    .ok_or_else(|| {
                        "mean_ratio derived metric is missing numerator_channel".to_string()
                    })?
                    .to_string();
                let denominator_channel = value
                    .get("denominator_channel")
                    .and_then(JsonValue::as_str)
                    .ok_or_else(|| {
                        "mean_ratio derived metric is missing denominator_channel".to_string()
                    })?
                    .to_string();
                Ok(Self::MeanRatio {
                    numerator_channel,
                    denominator_channel,
                })
            }
            other => Err(format!("unknown derived metric kind '{other}'")),
        }
    }
}

impl AnalysisAction {
    fn kind(&self) -> &'static str {
        match self {
            Self::SetCompensationEnabled { .. } => "set_compensation_enabled",
            Self::SetChannelTransform { .. } => "set_channel_transform",
        }
    }

    fn sample_id(&self) -> &str {
        match self {
            Self::SetCompensationEnabled { sample_id, .. }
            | Self::SetChannelTransform { sample_id, .. } => sample_id,
        }
    }

    fn stable_hash(&self) -> u64 {
        let mut hasher = StableHasher::new();
        hasher.update_str(&self.to_json_value().stringify_canonical());
        hasher.finish_u64()
    }

    fn to_json_value(&self) -> JsonValue {
        match self {
            Self::SetCompensationEnabled { sample_id, enabled } => JsonValue::object([
                ("kind", JsonValue::String(self.kind().to_string())),
                ("sample_id", JsonValue::String(sample_id.clone())),
                ("enabled", JsonValue::Bool(*enabled)),
            ]),
            Self::SetChannelTransform {
                sample_id,
                channel,
                transform,
            } => JsonValue::object([
                ("kind", JsonValue::String(self.kind().to_string())),
                ("sample_id", JsonValue::String(sample_id.clone())),
                ("channel", JsonValue::String(channel.clone())),
                ("transform", transform_json(transform)),
            ]),
        }
    }

    fn from_json_value(value: &JsonValue) -> Result<Self, String> {
        let kind = required_json_string(value, "kind")?;
        match kind {
            "set_compensation_enabled" => Ok(Self::SetCompensationEnabled {
                sample_id: required_json_string(value, "sample_id")?.to_string(),
                enabled: required_json_bool(value, "enabled")?,
            }),
            "set_channel_transform" => Ok(Self::SetChannelTransform {
                sample_id: required_json_string(value, "sample_id")?.to_string(),
                channel: required_json_string(value, "channel")?.to_string(),
                transform: parse_transform_json(
                    value
                        .get("transform")
                        .ok_or_else(|| "missing field 'transform'".to_string())?,
                )?,
            }),
            other => Err(format!("unknown analysis action kind '{other}'")),
        }
    }
}

impl AnalysisActionLog {
    fn new() -> Self {
        Self::default()
    }

    fn len(&self) -> usize {
        self.records.len()
    }

    fn records(&self) -> &[AnalysisActionRecord] {
        &self.records
    }

    fn append(&mut self, action: AnalysisAction) -> &AnalysisActionRecord {
        let sequence = self.records.len() as u64 + 1;
        let previous_hash = self.records.last().map(|record| record.action_hash);
        let action_hash = action.stable_hash();
        self.records.push(AnalysisActionRecord {
            sequence,
            previous_hash,
            action_hash,
            action,
        });
        self.records.last().expect("record was just pushed")
    }

    fn execution_hash(&self) -> u64 {
        let mut hasher = StableHasher::new();
        for record in &self.records {
            hasher.update_u64(record.sequence);
            hasher.update_u64(record.previous_hash.unwrap_or(0));
            hasher.update_u64(record.action_hash);
        }
        hasher.finish_u64()
    }

    fn to_json_value(&self) -> JsonValue {
        JsonValue::Array(
            self.records
                .iter()
                .map(AnalysisActionRecord::to_json_value)
                .collect::<Vec<_>>(),
        )
    }

    fn from_json_value(value: &JsonValue) -> Result<Self, String> {
        let records = value
            .as_array()
            .ok_or_else(|| "analysis log must be an array".to_string())?
            .iter()
            .map(AnalysisActionRecord::from_json_value)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { records })
    }

    fn replay_profile(
        &self,
        sample_id: &str,
        raw_sample: &SampleFrame,
        compensation: Option<&CompensationMatrix>,
    ) -> Result<SampleAnalysisProfile, String> {
        let mut profile = SampleAnalysisProfile::default();
        for record in &self.records {
            if record.action.sample_id() != sample_id {
                return Err(format!(
                    "analysis action at sequence {} belongs to sample '{}' instead of '{}'",
                    record.sequence,
                    record.action.sample_id(),
                    sample_id
                ));
            }

            match &record.action {
                AnalysisAction::SetCompensationEnabled { enabled, .. } => {
                    profile.compensation_enabled = *enabled;
                }
                AnalysisAction::SetChannelTransform {
                    channel, transform, ..
                } => {
                    profile
                        .transforms
                        .insert(channel.clone(), transform.clone());
                }
            }

            apply_sample_analysis(raw_sample, compensation, &profile)
                .map_err(|error| error.to_string())?;
        }

        Ok(profile)
    }
}

impl ViewAction {
    fn kind(&self) -> &'static str {
        match self {
            Self::ResetPlotView { .. } => "reset_plot_view",
            Self::FocusPlotPopulation { .. } => "focus_plot_population",
            Self::ScalePlotView { .. } => "scale_plot_view",
        }
    }

    fn sample_id(&self) -> &str {
        match self {
            Self::ResetPlotView { sample_id, .. }
            | Self::FocusPlotPopulation { sample_id, .. }
            | Self::ScalePlotView { sample_id, .. } => sample_id,
        }
    }

    fn plot_id(&self) -> &str {
        match self {
            Self::ResetPlotView { plot_id, .. }
            | Self::FocusPlotPopulation { plot_id, .. }
            | Self::ScalePlotView { plot_id, .. } => plot_id,
        }
    }

    fn stable_hash(&self) -> u64 {
        let mut hasher = StableHasher::new();
        hasher.update_str(&self.to_json_value().stringify_canonical());
        hasher.finish_u64()
    }

    fn to_json_value(&self) -> JsonValue {
        match self {
            Self::ResetPlotView { sample_id, plot_id } => JsonValue::object([
                ("kind", JsonValue::String(self.kind().to_string())),
                ("sample_id", JsonValue::String(sample_id.clone())),
                ("plot_id", JsonValue::String(plot_id.clone())),
            ]),
            Self::FocusPlotPopulation {
                sample_id,
                plot_id,
                population_id,
                padding_fraction,
            } => JsonValue::object([
                ("kind", JsonValue::String(self.kind().to_string())),
                ("sample_id", JsonValue::String(sample_id.clone())),
                ("plot_id", JsonValue::String(plot_id.clone())),
                ("population_id", JsonValue::String(population_id.clone())),
                ("padding_fraction", JsonValue::Number(*padding_fraction)),
            ]),
            Self::ScalePlotView {
                sample_id,
                plot_id,
                factor,
            } => JsonValue::object([
                ("kind", JsonValue::String(self.kind().to_string())),
                ("sample_id", JsonValue::String(sample_id.clone())),
                ("plot_id", JsonValue::String(plot_id.clone())),
                ("factor", JsonValue::Number(*factor)),
            ]),
        }
    }

    fn from_json_value(value: &JsonValue) -> Result<Self, String> {
        let kind = required_json_string(value, "kind")?;
        match kind {
            "reset_plot_view" => Ok(Self::ResetPlotView {
                sample_id: required_json_string(value, "sample_id")?.to_string(),
                plot_id: required_json_string(value, "plot_id")?.to_string(),
            }),
            "focus_plot_population" => Ok(Self::FocusPlotPopulation {
                sample_id: required_json_string(value, "sample_id")?.to_string(),
                plot_id: required_json_string(value, "plot_id")?.to_string(),
                population_id: required_json_string(value, "population_id")?.to_string(),
                padding_fraction: value
                    .get("padding_fraction")
                    .and_then(JsonValue::as_f64)
                    .unwrap_or(0.08),
            }),
            "scale_plot_view" => Ok(Self::ScalePlotView {
                sample_id: required_json_string(value, "sample_id")?.to_string(),
                plot_id: required_json_string(value, "plot_id")?.to_string(),
                factor: value
                    .get("factor")
                    .and_then(JsonValue::as_f64)
                    .ok_or_else(|| "missing field 'factor'".to_string())?,
            }),
            other => Err(format!("unknown view action kind '{other}'")),
        }
    }
}

impl ViewActionLog {
    fn new() -> Self {
        Self::default()
    }

    fn records(&self) -> &[ViewActionRecord] {
        &self.records
    }

    fn append(&mut self, action: ViewAction) -> &ViewActionRecord {
        let sequence = self.records.len() as u64 + 1;
        let previous_hash = self.records.last().map(|record| record.action_hash);
        let action_hash = action.stable_hash();
        self.records.push(ViewActionRecord {
            sequence,
            previous_hash,
            action_hash,
            action,
        });
        self.records.last().expect("record was just pushed")
    }

    fn execution_hash(&self) -> u64 {
        let mut hasher = StableHasher::new();
        for record in &self.records {
            hasher.update_u64(record.sequence);
            hasher.update_u64(record.previous_hash.unwrap_or(0));
            hasher.update_u64(record.action_hash);
        }
        hasher.finish_u64()
    }

    fn to_json_value(&self) -> JsonValue {
        JsonValue::Array(
            self.records
                .iter()
                .map(ViewActionRecord::to_json_value)
                .collect::<Vec<_>>(),
        )
    }

    fn from_json_value(value: &JsonValue) -> Result<Self, String> {
        let records = value
            .as_array()
            .ok_or_else(|| "view log must be an array".to_string())?
            .iter()
            .map(ViewActionRecord::from_json_value)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { records })
    }

    fn replay_ranges(
        &self,
        sample_id: &str,
        sample: &SampleFrame,
        state: &WorkspaceState,
        plots: &[PlotSpec],
    ) -> Result<BTreeMap<String, PlotRangeState>, String> {
        let mut ranges = BTreeMap::new();
        for plot in plots {
            ranges.insert(plot.id.clone(), auto_plot_range(sample, plot)?);
        }

        for record in &self.records {
            if record.action.sample_id() != sample_id {
                return Err(format!(
                    "view action at sequence {} belongs to sample '{}' instead of '{}'",
                    record.sequence,
                    record.action.sample_id(),
                    sample_id
                ));
            }

            let plot = plots
                .iter()
                .find(|candidate| candidate.id == record.action.plot_id())
                .ok_or_else(|| {
                    format!(
                        "view action at sequence {} references unknown plot '{}'",
                        record.sequence,
                        record.action.plot_id()
                    )
                })?;

            let next_range = match &record.action {
                ViewAction::ResetPlotView { .. } => auto_plot_range(sample, plot)?,
                ViewAction::FocusPlotPopulation {
                    population_id,
                    padding_fraction,
                    ..
                } => focus_plot_range(sample, state, plot, population_id, *padding_fraction)?,
                ViewAction::ScalePlotView { factor, .. } => {
                    let current =
                        ranges
                            .get(record.action.plot_id())
                            .cloned()
                            .ok_or_else(|| {
                                format!(
                                    "missing active range for plot '{}'",
                                    record.action.plot_id()
                                )
                            })?;
                    scale_plot_range(&current, *factor)?
                }
            };
            ranges.insert(record.action.plot_id().to_string(), next_range);
        }

        Ok(ranges)
    }
}

impl AnalysisActionRecord {
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
                "action_hash",
                JsonValue::String(format!("{:016x}", self.action_hash)),
            ),
            ("action", self.action.to_json_value()),
        ])
    }

    fn from_json_value(value: &JsonValue) -> Result<Self, String> {
        let sequence = value
            .get("sequence")
            .and_then(JsonValue::as_u64)
            .ok_or_else(|| "missing field 'sequence'".to_string())?;
        let previous_hash = match value.get("previous_hash") {
            Some(JsonValue::Null) | None => None,
            Some(JsonValue::String(hex)) => Some(parse_hex_u64(hex, "previous_hash")?),
            _ => return Err("invalid field 'previous_hash'".to_string()),
        };
        let action_hash = value
            .get("action_hash")
            .and_then(JsonValue::as_str)
            .ok_or_else(|| "missing field 'action_hash'".to_string())
            .and_then(|value| parse_hex_u64(value, "action_hash"))?;
        let action = AnalysisAction::from_json_value(
            value
                .get("action")
                .ok_or_else(|| "missing field 'action'".to_string())?,
        )?;
        let expected_hash = action.stable_hash();
        if expected_hash != action_hash {
            return Err(format!(
                "analysis action hash mismatch at sequence {}: expected {:016x}, found {:016x}",
                sequence, expected_hash, action_hash
            ));
        }
        Ok(Self {
            sequence,
            previous_hash,
            action_hash,
            action,
        })
    }
}

impl ViewActionRecord {
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
                "action_hash",
                JsonValue::String(format!("{:016x}", self.action_hash)),
            ),
            ("action", self.action.to_json_value()),
        ])
    }

    fn from_json_value(value: &JsonValue) -> Result<Self, String> {
        let sequence = value
            .get("sequence")
            .and_then(JsonValue::as_u64)
            .ok_or_else(|| "missing field 'sequence'".to_string())?;
        let previous_hash = match value.get("previous_hash") {
            Some(JsonValue::Null) | None => None,
            Some(JsonValue::String(hex)) => Some(parse_hex_u64(hex, "previous_hash")?),
            _ => return Err("invalid field 'previous_hash'".to_string()),
        };
        let action_hash = value
            .get("action_hash")
            .and_then(JsonValue::as_str)
            .ok_or_else(|| "missing field 'action_hash'".to_string())
            .and_then(|value| parse_hex_u64(value, "action_hash"))?;
        let action = ViewAction::from_json_value(
            value
                .get("action")
                .ok_or_else(|| "missing field 'action'".to_string())?,
        )?;
        let expected_hash = action.stable_hash();
        if expected_hash != action_hash {
            return Err(format!(
                "view action hash mismatch at sequence {}: expected {:016x}, found {:016x}",
                sequence, expected_hash, action_hash
            ));
        }
        Ok(Self {
            sequence,
            previous_hash,
            action_hash,
            action,
        })
    }
}

struct WorkspaceSampleSpec {
    id: String,
    display_name: String,
    group_label: String,
    source: WorkspaceSampleSource,
}

enum WorkspaceSampleSource {
    EmbeddedDemo,
    FcsFile(String),
}

struct WorkspaceDocument {
    active_sample_id: String,
    sample_specs: Vec<WorkspaceSampleSpec>,
    derived_metric: DerivedMetricDefinition,
    command_logs: BTreeMap<String, CommandLog>,
    analysis_logs: BTreeMap<String, AnalysisActionLog>,
    view_logs: BTreeMap<String, ViewActionLog>,
    redo_stacks: BTreeMap<String, Vec<Command>>,
}

pub struct DesktopSession {
    environment: ReplayEnvironment,
    sample_artifacts: BTreeMap<String, DesktopSampleArtifact>,
    sample_id: String,
    sample_order: Vec<String>,
    sample_info: BTreeMap<String, DesktopSampleInfo>,
    derived_metric: DerivedMetricDefinition,
    command_logs: BTreeMap<String, CommandLog>,
    analysis_logs: BTreeMap<String, AnalysisActionLog>,
    view_logs: BTreeMap<String, ViewActionLog>,
    redo_stacks: BTreeMap<String, Vec<Command>>,
}

impl DesktopSession {
    fn new() -> Result<Self, String> {
        let sample_id = "desktop-demo".to_string();
        let sample = demo_sample(sample_id.clone())?;

        let mut environment = ReplayEnvironment::new();
        environment
            .insert_sample(sample.clone())
            .map_err(|error| error.to_string())?;
        let mut sample_artifacts = BTreeMap::new();
        sample_artifacts.insert(
            sample_id.clone(),
            DesktopSampleArtifact {
                raw_sample: sample,
                compensation: None,
            },
        );

        let mut sample_info = BTreeMap::new();
        sample_info.insert(
            sample_id.clone(),
            DesktopSampleInfo {
                display_name: "Demo Sample".to_string(),
                source_path: None,
                group_label: default_group_label(),
            },
        );
        let mut command_logs = BTreeMap::new();
        command_logs.insert(sample_id.clone(), CommandLog::new());
        let mut analysis_logs = BTreeMap::new();
        analysis_logs.insert(sample_id.clone(), AnalysisActionLog::new());
        let mut view_logs = BTreeMap::new();
        view_logs.insert(sample_id.clone(), ViewActionLog::new());
        let mut redo_stacks = BTreeMap::new();
        redo_stacks.insert(sample_id.clone(), Vec::new());
        let derived_metric = default_derived_metric_for_sample(
            sample_artifacts
                .get(&sample_id)
                .map(|artifact| &artifact.raw_sample),
        );

        Ok(Self {
            environment,
            sample_artifacts,
            sample_id,
            sample_order: vec!["desktop-demo".to_string()],
            sample_info,
            derived_metric,
            command_logs,
            analysis_logs,
            view_logs,
            redo_stacks,
        })
    }

    fn reset(&mut self) -> JsonValue {
        for log in self.command_logs.values_mut() {
            *log = CommandLog::new();
        }
        for log in self.analysis_logs.values_mut() {
            *log = AnalysisActionLog::new();
        }
        for log in self.view_logs.values_mut() {
            *log = ViewActionLog::new();
        }
        for redo_stack in self.redo_stacks.values_mut() {
            redo_stack.clear();
        }
        self.snapshot_value()
            .unwrap_or_else(|message| error_json_value(message))
    }

    fn snapshot_value(&self) -> Result<JsonValue, String> {
        let artifact = self.active_sample_artifact()?;
        let sample = &artifact.raw_sample;
        let sample_info = self
            .sample_info
            .get(&self.sample_id)
            .ok_or_else(|| format!("missing sample info '{}'", self.sample_id))?;
        let command_log = self
            .command_logs
            .get(&self.sample_id)
            .ok_or_else(|| format!("missing command log for sample '{}'", self.sample_id))?;
        let analysis_log = self
            .analysis_logs
            .get(&self.sample_id)
            .ok_or_else(|| format!("missing analysis log for sample '{}'", self.sample_id))?;
        let view_log = self
            .view_logs
            .get(&self.sample_id)
            .ok_or_else(|| format!("missing view log for sample '{}'", self.sample_id))?;
        let redo_stack = self
            .redo_stacks
            .get(&self.sample_id)
            .ok_or_else(|| format!("missing redo state for sample '{}'", self.sample_id))?;
        let (processed_sample, analysis_profile, state, execution_hash) =
            self.active_replay_state()?;
        let plot_specs = default_plot_specs(&processed_sample);
        let plot_ranges = view_log.replay_ranges(
            processed_sample.sample_id(),
            &processed_sample,
            &state,
            &plot_specs,
        )?;
        let mut session_hasher = StableHasher::new();
        session_hasher.update_u64(execution_hash);
        session_hasher.update_u64(view_log.execution_hash());
        let execution_hash = session_hasher.finish_u64();

        Ok(JsonValue::object([
            ("status", JsonValue::String("ready".to_string())),
            ("application", desktop_application_json()),
            ("ground_rules", ground_rules_json()),
            (
                "stack",
                JsonValue::object([
                    ("engine", JsonValue::String("Rust".to_string())),
                    ("desktop", JsonValue::String("Qt/QML".to_string())),
                    ("backend", JsonValue::String("Rust".to_string())),
                ]),
            ),
            (
                "samples",
                samples_json(&self.environment, &self.sample_order, &self.sample_info),
            ),
            (
                "sample",
                sample_json(
                    sample,
                    sample_info,
                    artifact.compensation.as_ref(),
                    &analysis_profile,
                ),
            ),
            (
                "command_count",
                JsonValue::Number(command_log.records().len() as f64),
            ),
            (
                "analysis_action_count",
                JsonValue::Number(analysis_log.len() as f64),
            ),
            (
                "view_action_count",
                JsonValue::Number(view_log.records().len() as f64),
            ),
            ("can_undo", JsonValue::Bool(!command_log.is_empty())),
            ("can_redo", JsonValue::Bool(!redo_stack.is_empty())),
            (
                "command_log_hash",
                JsonValue::String(format!("{:016x}", command_log.execution_hash())),
            ),
            (
                "execution_hash",
                JsonValue::String(format!("{:016x}", execution_hash)),
            ),
            (
                "comparison_state_hash",
                JsonValue::String(format!("{:016x}", self.comparison_state_hash()?)),
            ),
            ("commands", commands_json(command_log)),
            ("analysis_actions", analysis_actions_json(analysis_log)),
            ("populations", populations_json(sample, &state)),
            (
                "population_stats",
                population_stats_json(&processed_sample, &state)?,
            ),
            ("derived_metric", self.derived_metric.to_json_value()),
            (
                "plots",
                plots_json(&processed_sample, &state, &plot_specs, &plot_ranges)?,
            ),
            (
                "phase0_focus",
                JsonValue::Array(
                    [
                        "FCS ingestion robustness",
                        "Deterministic gating kernel",
                        "Replayable command log",
                        "Local and service parity",
                    ]
                    .into_iter()
                    .map(|value| JsonValue::String(value.to_string()))
                    .collect(),
                ),
            ),
            (
                "desktop_surface",
                JsonValue::Array(
                    [
                        "Stateful command dispatch from QML",
                        "Shared-engine replay on every action",
                        "Drag-authored gates become explicit commands",
                        "Undo and redo stay in the command log",
                    ]
                    .into_iter()
                    .map(|value| JsonValue::String(value.to_string()))
                    .collect(),
                ),
            ),
        ]))
    }

    fn dispatch_json(&mut self, command_json: &str) -> JsonValue {
        let value = match JsonValue::parse(command_json) {
            Ok(value) => value,
            Err(error) => return error_json_value(error.to_string()),
        };

        match value.get("kind").and_then(JsonValue::as_str) {
            Some("set_compensation_enabled") | Some("set_channel_transform") => {
                return self.dispatch_analysis_json(&value);
            }
            Some("reset_plot_view") | Some("focus_plot_population") | Some("scale_plot_view") => {
                return self.dispatch_view_json(&value);
            }
            _ => {}
        }

        let command = match Command::from_json_value(&value) {
            Ok(command) => command,
            Err(error) => return error_json_value(error.to_string()),
        };

        if command.sample_id() != self.sample_id {
            return error_json_value(format!(
                "command sample '{}' does not match desktop sample '{}'",
                command.sample_id(),
                self.sample_id
            ));
        }

        let mut next_log = match self.active_command_log() {
            Ok(log) => log.clone(),
            Err(message) => return error_json_value(message),
        };
        next_log.append(command);
        let (_, _, _, replay_environment) = match self.active_processed_environment() {
            Ok(state) => state,
            Err(message) => return error_json_value(message),
        };
        if let Err(error) = next_log.replay(&replay_environment) {
            return error_json_value(error.to_string());
        }

        if let Some(log) = self.command_logs.get_mut(&self.sample_id) {
            *log = next_log;
        }
        if let Some(redo_stack) = self.redo_stacks.get_mut(&self.sample_id) {
            redo_stack.clear();
        }
        self.snapshot_value()
            .unwrap_or_else(|message| error_json_value(message))
    }

    fn dispatch_analysis_json(&mut self, value: &JsonValue) -> JsonValue {
        let action = match AnalysisAction::from_json_value(value) {
            Ok(action) => action,
            Err(message) => return error_json_value(message),
        };

        if action.sample_id() != self.sample_id {
            return error_json_value(format!(
                "analysis action sample '{}' does not match desktop sample '{}'",
                action.sample_id(),
                self.sample_id
            ));
        }

        let mut next_log = match self.active_analysis_log() {
            Ok(log) => log.clone(),
            Err(message) => return error_json_value(message),
        };
        next_log.append(action);

        let artifact = match self.active_sample_artifact() {
            Ok(artifact) => artifact,
            Err(message) => return error_json_value(message),
        };
        let profile = match next_log.replay_profile(
            artifact.raw_sample.sample_id(),
            &artifact.raw_sample,
            artifact.compensation.as_ref(),
        ) {
            Ok(profile) => profile,
            Err(message) => return error_json_value(message),
        };
        let processed_sample = match apply_sample_analysis(
            &artifact.raw_sample,
            artifact.compensation.as_ref(),
            &profile,
        ) {
            Ok(sample) => sample,
            Err(error) => return error_json_value(error.to_string()),
        };
        let mut replay_environment = ReplayEnvironment::new();
        if let Err(error) = replay_environment.insert_sample(processed_sample) {
            return error_json_value(error.to_string());
        }
        if let Err(error) = self.active_command_log().and_then(|command_log| {
            command_log
                .replay(&replay_environment)
                .map_err(|error| error.to_string())
        }) {
            return error_json_value(error);
        }

        if let Some(log) = self.analysis_logs.get_mut(&self.sample_id) {
            *log = next_log;
        }
        self.snapshot_value()
            .unwrap_or_else(|message| error_json_value(message))
    }

    fn dispatch_view_json(&mut self, value: &JsonValue) -> JsonValue {
        let action = match ViewAction::from_json_value(value) {
            Ok(action) => action,
            Err(message) => return error_json_value(message),
        };

        if action.sample_id() != self.sample_id {
            return error_json_value(format!(
                "view action sample '{}' does not match desktop sample '{}'",
                action.sample_id(),
                self.sample_id
            ));
        }

        let mut next_log = match self.active_view_log() {
            Ok(log) => log.clone(),
            Err(message) => return error_json_value(message),
        };
        next_log.append(action);

        let (processed_sample, _, state, _) = match self.active_replay_state() {
            Ok(state) => state,
            Err(message) => return error_json_value(message),
        };
        let plot_specs = default_plot_specs(&processed_sample);
        if let Err(message) = next_log.replay_ranges(
            processed_sample.sample_id(),
            &processed_sample,
            &state,
            &plot_specs,
        ) {
            return error_json_value(message);
        }

        if let Some(log) = self.view_logs.get_mut(&self.sample_id) {
            *log = next_log;
        }
        self.snapshot_value()
            .unwrap_or_else(|message| error_json_value(message))
    }

    fn undo(&mut self) -> JsonValue {
        let sample_id = self.sample_id.clone();
        let popped = match self.command_logs.get_mut(&sample_id) {
            Some(command_log) => command_log.pop(),
            None => {
                return error_json_value(format!("missing command log for sample '{sample_id}'"));
            }
        };

        match popped {
            Some(record) => {
                if let Some(redo_stack) = self.redo_stacks.get_mut(&sample_id) {
                    redo_stack.push(record.command);
                }
                self.snapshot_value()
                    .unwrap_or_else(|message| error_json_value(message))
            }
            None => error_json_value("there is no command to undo"),
        }
    }

    fn redo(&mut self) -> JsonValue {
        let sample_id = self.sample_id.clone();
        let next_command = match self.redo_stacks.get_mut(&sample_id) {
            Some(redo_stack) => redo_stack.pop(),
            None => {
                return error_json_value(format!("missing redo state for sample '{sample_id}'"));
            }
        };

        match next_command {
            Some(command) => {
                let mut next_log = match self.active_command_log() {
                    Ok(log) => log.clone(),
                    Err(message) => return error_json_value(message),
                };
                next_log.append(command);
                let (_, _, _, replay_environment) = match self.active_processed_environment() {
                    Ok(state) => state,
                    Err(message) => return error_json_value(message),
                };
                if let Err(error) = next_log.replay(&replay_environment) {
                    return error_json_value(error.to_string());
                }
                if let Some(log) = self.command_logs.get_mut(&sample_id) {
                    *log = next_log;
                }
                self.snapshot_value()
                    .unwrap_or_else(|message| error_json_value(message))
            }
            None => error_json_value("there is no command to redo"),
        }
    }

    fn import_fcs_json(&mut self, file_paths_json: &str) -> JsonValue {
        let paths = match parse_import_paths(file_paths_json) {
            Ok(paths) => paths,
            Err(message) => return error_json_value(message),
        };

        let imported = match ImportedSession::from_paths(&paths) {
            Ok(imported) => imported,
            Err(message) => return error_json_value(message),
        };

        self.environment = imported.environment;
        self.sample_artifacts = imported.sample_artifacts;
        self.sample_id = imported.active_sample_id;
        self.sample_order = imported.sample_order;
        self.sample_info = imported.sample_info;
        self.derived_metric = imported.derived_metric;
        self.command_logs = imported.command_logs;
        self.analysis_logs = imported.analysis_logs;
        self.view_logs = imported.view_logs;
        self.redo_stacks = imported.redo_stacks;

        self.snapshot_value()
            .unwrap_or_else(|message| error_json_value(message))
    }

    fn select_sample(&mut self, sample_id: &str) -> JsonValue {
        if !self.sample_info.contains_key(sample_id) {
            return error_json_value(format!("unknown sample '{sample_id}'"));
        }

        self.sample_id = sample_id.to_string();
        self.snapshot_value()
            .unwrap_or_else(|message| error_json_value(message))
    }

    fn save_workspace(&self, workspace_path: &str) -> JsonValue {
        if workspace_path.trim().is_empty() {
            return error_json_value("workspace path cannot be empty");
        }

        let document = match self.workspace_document_json() {
            Ok(value) => value.stringify_canonical(),
            Err(message) => return error_json_value(message),
        };

        if let Err(error) = fs::write(workspace_path, document) {
            return error_json_value(format!(
                "failed to write workspace '{}': {error}",
                workspace_path
            ));
        }

        self.snapshot_value()
            .unwrap_or_else(|message| error_json_value(message))
    }

    fn load_workspace(&mut self, workspace_path: &str) -> JsonValue {
        if workspace_path.trim().is_empty() {
            return error_json_value("workspace path cannot be empty");
        }

        let document = match fs::read_to_string(workspace_path) {
            Ok(document) => document,
            Err(error) => {
                return error_json_value(format!(
                    "failed to read workspace '{}': {error}",
                    workspace_path
                ));
            }
        };

        let workspace = match WorkspaceDocument::from_json_str(&document) {
            Ok(workspace) => workspace,
            Err(message) => return error_json_value(message),
        };

        let imported = match ImportedSession::from_workspace_document(workspace) {
            Ok(imported) => imported,
            Err(message) => return error_json_value(message),
        };

        self.environment = imported.environment;
        self.sample_artifacts = imported.sample_artifacts;
        self.sample_id = imported.active_sample_id;
        self.sample_order = imported.sample_order;
        self.sample_info = imported.sample_info;
        self.derived_metric = imported.derived_metric;
        self.command_logs = imported.command_logs;
        self.analysis_logs = imported.analysis_logs;
        self.view_logs = imported.view_logs;
        self.redo_stacks = imported.redo_stacks;

        self.snapshot_value()
            .unwrap_or_else(|message| error_json_value(message))
    }

    fn export_population_stats_csv(&self, export_path: &str) -> JsonValue {
        if export_path.trim().is_empty() {
            return error_json_value("stats export path cannot be empty");
        }

        let (processed_sample, _, state, _) = match self.active_replay_state() {
            Ok(values) => values,
            Err(message) => return error_json_value(message),
        };
        let stats = match compute_population_stats_table(&processed_sample, &state) {
            Ok(stats) => stats,
            Err(error) => return error_json_value(error.to_string()),
        };
        let csv = population_stats_csv(&stats);

        if let Err(error) = fs::write(export_path, csv) {
            return error_json_value(format!(
                "failed to write stats export '{}': {error}",
                export_path
            ));
        }

        self.snapshot_value()
            .unwrap_or_else(|message| error_json_value(message))
    }

    fn apply_active_template_to_other_samples(&mut self) -> JsonValue {
        if self.sample_order.len() < 2 {
            return error_json_value("template application requires at least two loaded samples");
        }

        let source_sample_id = self.sample_id.clone();
        let source_log = match self.command_log_for_sample(&source_sample_id) {
            Ok(log) => log,
            Err(message) => return error_json_value(message),
        };
        if source_log.is_empty() {
            return error_json_value("active sample has no gate commands to apply as a template");
        }

        let mut candidate_logs = Vec::new();
        for target_sample_id in self
            .sample_order
            .iter()
            .filter(|sample_id| sample_id.as_str() != source_sample_id)
        {
            let next_log = match self.template_log_for_target(&source_sample_id, target_sample_id) {
                Ok(log) => log,
                Err(message) => return error_json_value(message),
            };
            let (_, _, _, replay_environment) =
                match self.processed_environment_for_sample(target_sample_id) {
                    Ok(values) => values,
                    Err(message) => return error_json_value(message),
                };
            if let Err(error) = next_log.replay(&replay_environment) {
                return error_json_value(format!(
                    "template is incompatible with sample '{}': {}",
                    target_sample_id, error
                ));
            }
            candidate_logs.push((target_sample_id.clone(), next_log));
        }

        for (sample_id, next_log) in candidate_logs {
            if let Some(log) = self.command_logs.get_mut(&sample_id) {
                *log = next_log;
            }
            if let Some(redo_stack) = self.redo_stacks.get_mut(&sample_id) {
                redo_stack.clear();
            }
            if let Some(view_log) = self.view_logs.get_mut(&sample_id) {
                *view_log = ViewActionLog::new();
            }
        }

        self.snapshot_value()
            .unwrap_or_else(|message| error_json_value(message))
    }

    fn export_batch_population_stats_csv(&self, export_path: &str) -> JsonValue {
        if export_path.trim().is_empty() {
            return error_json_value("batch stats export path cannot be empty");
        }

        let mut stats = Vec::new();
        for sample_id in &self.sample_order {
            let (processed_sample, _, state, _) = match self.replay_state_for_sample(sample_id) {
                Ok(values) => values,
                Err(message) => return error_json_value(message),
            };
            let sample_stats = match compute_population_stats_table(&processed_sample, &state) {
                Ok(stats) => stats,
                Err(error) => return error_json_value(error.to_string()),
            };
            stats.extend(sample_stats);
        }

        let csv = population_stats_csv(&stats);
        if let Err(error) = fs::write(export_path, csv) {
            return error_json_value(format!(
                "failed to write batch stats export '{}': {error}",
                export_path
            ));
        }

        self.snapshot_value()
            .unwrap_or_else(|message| error_json_value(message))
    }

    fn population_comparison(&self, population_key: &str) -> JsonValue {
        if population_key.trim().is_empty() {
            return error_json_value("population comparison key cannot be empty");
        }

        let (population_id, rows) = match self.population_comparison_rows(population_key) {
            Ok(values) => values,
            Err(message) => return error_json_value(message),
        };
        let active_group_label = self
            .active_sample_group_label()
            .unwrap_or_else(default_group_label);
        let group_summaries = population_group_summaries(&rows, &active_group_label);
        let available_sample_count = rows.iter().filter(|row| row.status == "available").count();
        let missing_sample_count = rows.len().saturating_sub(available_sample_count);

        JsonValue::object([
            ("status", JsonValue::String("ready".to_string())),
            (
                "population_comparison",
                JsonValue::object([
                    ("key", JsonValue::String(population_key.to_string())),
                    ("population_id", JsonValue::String(population_id)),
                    (
                        "active_sample_id",
                        JsonValue::String(self.sample_id.clone()),
                    ),
                    (
                        "available_sample_count",
                        JsonValue::Number(available_sample_count as f64),
                    ),
                    (
                        "missing_sample_count",
                        JsonValue::Number(missing_sample_count as f64),
                    ),
                    ("active_group_label", JsonValue::String(active_group_label)),
                    ("derived_metric", self.derived_metric.to_json_value()),
                    (
                        "samples",
                        JsonValue::Array(
                            rows.into_iter()
                                .map(population_comparison_row_json)
                                .collect::<Vec<_>>(),
                        ),
                    ),
                    (
                        "group_summaries",
                        JsonValue::Array(
                            group_summaries
                                .into_iter()
                                .map(population_group_summary_row_json)
                                .collect::<Vec<_>>(),
                        ),
                    ),
                ]),
            ),
        ])
    }

    fn export_population_comparison_csv(
        &self,
        population_key: &str,
        export_path: &str,
    ) -> JsonValue {
        if population_key.trim().is_empty() {
            return error_json_value("population comparison key cannot be empty");
        }
        if export_path.trim().is_empty() {
            return error_json_value("population comparison export path cannot be empty");
        }

        let (population_id, rows) = match self.population_comparison_rows(population_key) {
            Ok(values) => values,
            Err(message) => return error_json_value(message),
        };
        let active_group_label = self
            .active_sample_group_label()
            .unwrap_or_else(default_group_label);
        let csv = population_comparison_csv(
            population_key,
            &population_id,
            &self.sample_id,
            &active_group_label,
            &rows,
        );
        if let Err(error) = fs::write(export_path, csv) {
            return error_json_value(format!(
                "failed to write population comparison export '{}': {error}",
                export_path
            ));
        }

        self.snapshot_value()
            .unwrap_or_else(|message| error_json_value(message))
    }

    fn export_population_group_summary_csv(
        &self,
        population_key: &str,
        export_path: &str,
    ) -> JsonValue {
        if population_key.trim().is_empty() {
            return error_json_value("population group summary key cannot be empty");
        }
        if export_path.trim().is_empty() {
            return error_json_value("population group summary export path cannot be empty");
        }

        let (population_id, rows) = match self.population_comparison_rows(population_key) {
            Ok(values) => values,
            Err(message) => return error_json_value(message),
        };
        let active_group_label = self
            .active_sample_group_label()
            .unwrap_or_else(default_group_label);
        let group_summaries = population_group_summaries(&rows, &active_group_label);
        let csv = population_group_summary_csv(
            population_key,
            &population_id,
            &self.sample_id,
            &active_group_label,
            &group_summaries,
        );
        if let Err(error) = fs::write(export_path, csv) {
            return error_json_value(format!(
                "failed to write population group summary export '{}': {error}",
                export_path
            ));
        }

        self.snapshot_value()
            .unwrap_or_else(|message| error_json_value(message))
    }

    fn set_derived_metric_from_json(&mut self, metric_json: &str) -> JsonValue {
        let value = match JsonValue::parse(metric_json) {
            Ok(value) => value,
            Err(error) => return error_json_value(error.to_string()),
        };
        let metric = match DerivedMetricDefinition::from_json_value(&value) {
            Ok(metric) => metric,
            Err(message) => return error_json_value(message),
        };

        let active_sample = match self.active_sample_artifact() {
            Ok(artifact) => &artifact.raw_sample,
            Err(message) => return error_json_value(message),
        };
        if let Err(message) = validate_derived_metric_for_sample(active_sample, &metric) {
            return error_json_value(message);
        }

        self.derived_metric = metric;
        self.snapshot_value()
            .unwrap_or_else(|message| error_json_value(message))
    }

    fn export_population_derived_metric_csv(
        &self,
        population_key: &str,
        export_path: &str,
    ) -> JsonValue {
        if population_key.trim().is_empty() {
            return error_json_value("population derived metric key cannot be empty");
        }
        if export_path.trim().is_empty() {
            return error_json_value("population derived metric export path cannot be empty");
        }

        let (population_id, rows) = match self.population_comparison_rows(population_key) {
            Ok(values) => values,
            Err(message) => return error_json_value(message),
        };
        let csv = population_derived_metric_csv(
            population_key,
            &population_id,
            &self.sample_id,
            &self.derived_metric,
            &rows,
        );
        if let Err(error) = fs::write(export_path, csv) {
            return error_json_value(format!(
                "failed to write population derived metric export '{}': {error}",
                export_path
            ));
        }

        self.snapshot_value()
            .unwrap_or_else(|message| error_json_value(message))
    }

    fn set_sample_group_label(&mut self, sample_id: &str, group_label: &str) -> JsonValue {
        let info = match self.sample_info.get_mut(sample_id) {
            Some(info) => info,
            None => return error_json_value(format!("unknown sample '{sample_id}'")),
        };
        info.group_label = normalize_group_label(group_label);
        self.snapshot_value()
            .unwrap_or_else(|message| error_json_value(message))
    }

    fn active_command_log(&self) -> Result<&CommandLog, String> {
        self.command_log_for_sample(&self.sample_id)
    }

    fn active_analysis_log(&self) -> Result<&AnalysisActionLog, String> {
        self.analysis_log_for_sample(&self.sample_id)
    }

    fn active_view_log(&self) -> Result<&ViewActionLog, String> {
        self.view_log_for_sample(&self.sample_id)
    }

    fn active_sample_artifact(&self) -> Result<&DesktopSampleArtifact, String> {
        self.sample_artifact_for(&self.sample_id)
    }

    fn active_sample_group_label(&self) -> Option<String> {
        self.sample_info
            .get(&self.sample_id)
            .map(|info| info.group_label.clone())
    }

    fn active_processed_environment(
        &self,
    ) -> Result<(SampleFrame, SampleAnalysisProfile, u64, ReplayEnvironment), String> {
        let (processed_sample, analysis_profile, _, execution_hash) = self.active_replay_state()?;
        let mut environment = ReplayEnvironment::new();
        environment
            .insert_sample(processed_sample.clone())
            .map_err(|error| error.to_string())?;
        Ok((
            processed_sample,
            analysis_profile,
            execution_hash,
            environment,
        ))
    }

    fn active_replay_state(
        &self,
    ) -> Result<(SampleFrame, SampleAnalysisProfile, WorkspaceState, u64), String> {
        self.replay_state_for_sample(&self.sample_id)
    }

    fn command_log_for_sample(&self, sample_id: &str) -> Result<&CommandLog, String> {
        self.command_logs
            .get(sample_id)
            .ok_or_else(|| format!("missing command log for sample '{sample_id}'"))
    }

    fn analysis_log_for_sample(&self, sample_id: &str) -> Result<&AnalysisActionLog, String> {
        self.analysis_logs
            .get(sample_id)
            .ok_or_else(|| format!("missing analysis log for sample '{sample_id}'"))
    }

    fn view_log_for_sample(&self, sample_id: &str) -> Result<&ViewActionLog, String> {
        self.view_logs
            .get(sample_id)
            .ok_or_else(|| format!("missing view log for sample '{sample_id}'"))
    }

    fn sample_artifact_for(&self, sample_id: &str) -> Result<&DesktopSampleArtifact, String> {
        self.sample_artifacts
            .get(sample_id)
            .ok_or_else(|| format!("missing sample artifact for sample '{sample_id}'"))
    }

    fn processed_environment_for_sample(
        &self,
        sample_id: &str,
    ) -> Result<(SampleFrame, SampleAnalysisProfile, u64, ReplayEnvironment), String> {
        let (processed_sample, analysis_profile, _, execution_hash) =
            self.replay_state_for_sample(sample_id)?;
        let mut environment = ReplayEnvironment::new();
        environment
            .insert_sample(processed_sample.clone())
            .map_err(|error| error.to_string())?;
        Ok((
            processed_sample,
            analysis_profile,
            execution_hash,
            environment,
        ))
    }

    fn replay_state_for_sample(
        &self,
        sample_id: &str,
    ) -> Result<(SampleFrame, SampleAnalysisProfile, WorkspaceState, u64), String> {
        let artifact = self.sample_artifact_for(sample_id)?;
        let analysis_log = self.analysis_log_for_sample(sample_id)?;
        let profile = analysis_log.replay_profile(
            artifact.raw_sample.sample_id(),
            &artifact.raw_sample,
            artifact.compensation.as_ref(),
        )?;
        let processed_sample = apply_sample_analysis(
            &artifact.raw_sample,
            artifact.compensation.as_ref(),
            &profile,
        )
        .map_err(|error| error.to_string())?;
        let mut environment = ReplayEnvironment::new();
        environment
            .insert_sample(processed_sample.clone())
            .map_err(|error| error.to_string())?;
        let state = self
            .command_log_for_sample(sample_id)?
            .replay(&environment)
            .map_err(|error| error.to_string())?;

        let mut hasher = StableHasher::new();
        hasher.update_u64(analysis_log.execution_hash());
        hasher.update_u64(state.execution_hash);

        Ok((processed_sample, profile, state, hasher.finish_u64()))
    }

    fn comparison_state_hash(&self) -> Result<u64, String> {
        let mut hasher = StableHasher::new();
        hasher.update_str(&self.sample_id);
        hasher.update_u64(self.derived_metric.stable_hash());
        hasher.update_u64(self.sample_order.len() as u64);

        for sample_id in &self.sample_order {
            hasher.update_str(sample_id);
            let artifact = self.sample_artifact_for(sample_id)?;
            let sample = &artifact.raw_sample;
            let info = self
                .sample_info
                .get(sample_id)
                .ok_or_else(|| format!("missing sample info '{}'", sample_id))?;
            hasher.update_u64(sample.fingerprint());
            hasher.update_u64(compensation_matrix_hash(artifact.compensation.as_ref()));
            hasher.update_u64(sample.event_count() as u64);
            hasher.update_u64(sample.channels().len() as u64);
            for channel in sample.channels() {
                hasher.update_str(channel);
            }
            hasher.update_str(&info.display_name);
            hasher.update_str(info.source_path.as_deref().unwrap_or(""));
            hasher.update_str(&info.group_label);
            hasher.update_u64(self.command_log_for_sample(sample_id)?.execution_hash());
            hasher.update_u64(self.analysis_log_for_sample(sample_id)?.execution_hash());
        }

        Ok(hasher.finish_u64())
    }

    fn population_comparison_rows(
        &self,
        population_key: &str,
    ) -> Result<(String, Vec<PopulationComparisonRow>), String> {
        let (active_sample, _, active_state, _) = self.active_replay_state()?;
        let active_stats =
            find_population_stats_for_key(&active_sample, &active_state, population_key)?
                .ok_or_else(|| {
                    format!(
                        "active sample '{}' does not contain population '{}'",
                        self.sample_id, population_key
                    )
                })?;
        let population_id = active_stats.population_id.clone();
        let baseline_frequency_of_all = active_stats.frequency_of_all;
        let baseline_frequency_of_parent = active_stats.frequency_of_parent;
        let baseline_metric = evaluate_derived_metric(
            &active_sample,
            &active_state,
            population_key,
            &self.derived_metric,
        );
        let baseline_metric_value = baseline_metric.value;

        let mut rows = Vec::with_capacity(self.sample_order.len());
        for sample_id in &self.sample_order {
            let sample_info = self
                .sample_info
                .get(sample_id)
                .ok_or_else(|| format!("missing sample info '{}'", sample_id))?;
            let (processed_sample, _, state, _) = self.replay_state_for_sample(sample_id)?;
            let stats = find_population_stats_for_key(&processed_sample, &state, population_key)?;
            let metric = evaluate_derived_metric(
                &processed_sample,
                &state,
                population_key,
                &self.derived_metric,
            );
            match stats {
                Some(stats) => rows.push(PopulationComparisonRow {
                    sample_id: sample_id.clone(),
                    display_name: sample_info.display_name.clone(),
                    source_path: sample_info.source_path.clone(),
                    group_label: sample_info.group_label.clone(),
                    is_active_sample: sample_id == &self.sample_id,
                    status: "available".to_string(),
                    matched_events: Some(stats.matched_events),
                    parent_events: Some(stats.parent_events),
                    frequency_of_all: Some(stats.frequency_of_all),
                    frequency_of_parent: Some(stats.frequency_of_parent),
                    delta_frequency_of_all: Some(
                        stats.frequency_of_all - baseline_frequency_of_all,
                    ),
                    delta_frequency_of_parent: Some(
                        stats.frequency_of_parent - baseline_frequency_of_parent,
                    ),
                    derived_metric_status: metric.status,
                    derived_metric_value: metric.value,
                    derived_metric_delta_value: match (metric.value, baseline_metric_value) {
                        (Some(value), Some(baseline)) => Some(value - baseline),
                        _ => None,
                    },
                    derived_metric_message: metric.message,
                }),
                None => rows.push(PopulationComparisonRow {
                    sample_id: sample_id.clone(),
                    display_name: sample_info.display_name.clone(),
                    source_path: sample_info.source_path.clone(),
                    group_label: sample_info.group_label.clone(),
                    is_active_sample: sample_id == &self.sample_id,
                    status: "missing".to_string(),
                    matched_events: None,
                    parent_events: None,
                    frequency_of_all: None,
                    frequency_of_parent: None,
                    delta_frequency_of_all: None,
                    delta_frequency_of_parent: None,
                    derived_metric_status: "missing_population".to_string(),
                    derived_metric_value: None,
                    derived_metric_delta_value: None,
                    derived_metric_message: Some(
                        "This population is not present in the current gate history for this sample."
                            .to_string(),
                    ),
                }),
            }
        }

        Ok((population_id, rows))
    }

    fn template_log_for_target(
        &self,
        source_sample_id: &str,
        target_sample_id: &str,
    ) -> Result<CommandLog, String> {
        let source_log = self.command_log_for_sample(source_sample_id)?;
        let mut next_log = CommandLog::new();
        for record in source_log.records() {
            next_log.append(record.command.with_sample_id(target_sample_id.to_string()));
        }
        Ok(next_log)
    }

    fn workspace_document_json(&self) -> Result<JsonValue, String> {
        let mut command_logs = BTreeMap::new();
        let mut analysis_logs = BTreeMap::new();
        let mut view_logs = BTreeMap::new();
        let mut redo_stacks = BTreeMap::new();
        let mut samples = Vec::with_capacity(self.sample_order.len());

        for sample_id in &self.sample_order {
            let info = self
                .sample_info
                .get(sample_id)
                .ok_or_else(|| format!("missing sample info for '{sample_id}'"))?;
            let command_log = self
                .command_logs
                .get(sample_id)
                .ok_or_else(|| format!("missing command log for '{sample_id}'"))?;
            let analysis_log = self
                .analysis_logs
                .get(sample_id)
                .ok_or_else(|| format!("missing analysis log for '{sample_id}'"))?;
            let view_log = self
                .view_logs
                .get(sample_id)
                .ok_or_else(|| format!("missing view log for '{sample_id}'"))?;
            let redo_stack = self
                .redo_stacks
                .get(sample_id)
                .ok_or_else(|| format!("missing redo stack for '{sample_id}'"))?;

            samples.push(workspace_sample_json(sample_id, info));
            command_logs.insert(
                sample_id.clone(),
                JsonValue::parse(&command_log.to_json()).map_err(|error| error.to_string())?,
            );
            analysis_logs.insert(sample_id.clone(), analysis_log.to_json_value());
            view_logs.insert(sample_id.clone(), view_log.to_json_value());
            redo_stacks.insert(
                sample_id.clone(),
                JsonValue::Array(
                    redo_stack
                        .iter()
                        .map(Command::to_json_value)
                        .collect::<Vec<_>>(),
                ),
            );
        }

        Ok(JsonValue::object([
            ("kind", JsonValue::String("parallax_workspace".to_string())),
            ("version", JsonValue::Number(1.0)),
            (
                "active_sample_id",
                JsonValue::String(self.sample_id.clone()),
            ),
            ("samples", JsonValue::Array(samples)),
            ("derived_metric", self.derived_metric.to_json_value()),
            ("command_logs", JsonValue::Object(command_logs)),
            ("analysis_logs", JsonValue::Object(analysis_logs)),
            ("view_logs", JsonValue::Object(view_logs)),
            ("redo_stacks", JsonValue::Object(redo_stacks)),
        ]))
    }
}

struct ImportedSession {
    environment: ReplayEnvironment,
    sample_artifacts: BTreeMap<String, DesktopSampleArtifact>,
    active_sample_id: String,
    sample_order: Vec<String>,
    sample_info: BTreeMap<String, DesktopSampleInfo>,
    derived_metric: DerivedMetricDefinition,
    command_logs: BTreeMap<String, CommandLog>,
    analysis_logs: BTreeMap<String, AnalysisActionLog>,
    view_logs: BTreeMap<String, ViewActionLog>,
    redo_stacks: BTreeMap<String, Vec<Command>>,
}

impl ImportedSession {
    fn from_paths(paths: &[String]) -> Result<Self, String> {
        if paths.is_empty() {
            return Err("import requires at least one .fcs file".to_string());
        }

        let mut environment = ReplayEnvironment::new();
        let mut sample_artifacts = BTreeMap::new();
        let mut sample_order = Vec::new();
        let mut sample_info = BTreeMap::new();
        let mut command_logs = BTreeMap::new();
        let mut analysis_logs = BTreeMap::new();
        let mut view_logs = BTreeMap::new();
        let mut redo_stacks = BTreeMap::new();

        for path in paths {
            let sample_id = next_sample_id(path, &sample_info);
            let display_name = sample_display_name(path);
            let bytes = fs::read(path)
                .map_err(|error| format!("failed to read FCS file '{}': {error}", path))?;
            let parsed = parse_fcs(&bytes)
                .map_err(|error| format!("failed to parse FCS file '{}': {error}", path))?;
            let compensation = parsed.compensation.clone();
            let sample = parsed
                .into_sample_frame(sample_id.clone())
                .map_err(|error| format!("failed to convert FCS file '{}': {error}", path))?;

            environment
                .insert_sample(sample.clone())
                .map_err(|error| format!("failed to import sample '{}': {error}", path))?;
            sample_artifacts.insert(
                sample_id.clone(),
                DesktopSampleArtifact {
                    raw_sample: sample,
                    compensation,
                },
            );
            sample_order.push(sample_id.clone());
            sample_info.insert(
                sample_id.clone(),
                DesktopSampleInfo {
                    display_name,
                    source_path: Some(path.clone()),
                    group_label: default_group_label(),
                },
            );
            command_logs.insert(sample_id.clone(), CommandLog::new());
            analysis_logs.insert(sample_id.clone(), AnalysisActionLog::new());
            view_logs.insert(sample_id.clone(), ViewActionLog::new());
            redo_stacks.insert(sample_id, Vec::new());
        }

        let active_sample_id = sample_order
            .first()
            .cloned()
            .ok_or_else(|| "import did not produce any samples".to_string())?;
        let derived_metric = default_derived_metric_for_sample(
            sample_artifacts
                .get(&active_sample_id)
                .map(|artifact| &artifact.raw_sample),
        );

        Ok(Self {
            environment,
            sample_artifacts,
            active_sample_id,
            sample_order,
            sample_info,
            derived_metric,
            command_logs,
            analysis_logs,
            view_logs,
            redo_stacks,
        })
    }

    fn from_workspace_document(workspace: WorkspaceDocument) -> Result<Self, String> {
        let mut environment = ReplayEnvironment::new();
        let mut sample_artifacts = BTreeMap::new();
        let mut sample_order = Vec::with_capacity(workspace.sample_specs.len());
        let mut sample_info = BTreeMap::new();

        for spec in &workspace.sample_specs {
            let artifact = load_sample_artifact_from_source(&spec.source, &spec.id)?;
            environment
                .insert_sample(artifact.raw_sample.clone())
                .map_err(|error| {
                    format!("failed to load workspace sample '{}': {error}", spec.id)
                })?;
            sample_artifacts.insert(spec.id.clone(), artifact);
            sample_order.push(spec.id.clone());
            sample_info.insert(
                spec.id.clone(),
                DesktopSampleInfo {
                    display_name: spec.display_name.clone(),
                    source_path: spec.source.path().map(str::to_string),
                    group_label: normalize_group_label(&spec.group_label),
                },
            );
        }

        for sample_id in &sample_order {
            let artifact = sample_artifacts
                .get(sample_id)
                .ok_or_else(|| format!("missing sample artifact for '{sample_id}'"))?;
            let analysis_log = workspace
                .analysis_logs
                .get(sample_id)
                .ok_or_else(|| format!("missing analysis log for sample '{sample_id}'"))?;
            let view_log = workspace
                .view_logs
                .get(sample_id)
                .ok_or_else(|| format!("missing view log for sample '{sample_id}'"))?;
            let profile = analysis_log.replay_profile(
                sample_id,
                &artifact.raw_sample,
                artifact.compensation.as_ref(),
            )?;
            let processed_sample = apply_sample_analysis(
                &artifact.raw_sample,
                artifact.compensation.as_ref(),
                &profile,
            )
            .map_err(|error| error.to_string())?;
            let mut replay_environment = ReplayEnvironment::new();
            replay_environment
                .insert_sample(processed_sample.clone())
                .map_err(|error| error.to_string())?;
            let log = workspace
                .command_logs
                .get(sample_id)
                .ok_or_else(|| format!("missing command log for sample '{sample_id}'"))?;
            let state = log.replay(&replay_environment).map_err(|error| {
                format!("failed to replay command log for sample '{sample_id}': {error}")
            })?;
            let plot_specs = default_plot_specs(&processed_sample);
            view_log.replay_ranges(sample_id, &processed_sample, &state, &plot_specs)?;
        }

        Ok(Self {
            environment,
            sample_artifacts,
            active_sample_id: workspace.active_sample_id,
            sample_order,
            sample_info,
            derived_metric: workspace.derived_metric,
            command_logs: workspace.command_logs,
            analysis_logs: workspace.analysis_logs,
            view_logs: workspace.view_logs,
            redo_stacks: workspace.redo_stacks,
        })
    }
}

impl WorkspaceDocument {
    fn from_json_str(input: &str) -> Result<Self, String> {
        let value = JsonValue::parse(input).map_err(|error| error.to_string())?;
        Self::from_json_value(&value)
    }

    fn from_json_value(value: &JsonValue) -> Result<Self, String> {
        if value.get("kind").and_then(JsonValue::as_str) != Some("parallax_workspace") {
            return Err("workspace document kind must be 'parallax_workspace'".to_string());
        }

        if value.get("version").and_then(JsonValue::as_u64) != Some(1) {
            return Err("workspace document version must be 1".to_string());
        }

        let samples_value = value
            .get("samples")
            .and_then(JsonValue::as_array)
            .ok_or_else(|| "workspace document must contain a samples array".to_string())?;
        if samples_value.is_empty() {
            return Err("workspace document must contain at least one sample".to_string());
        }

        let mut sample_specs = Vec::with_capacity(samples_value.len());
        for sample_value in samples_value {
            let spec = WorkspaceSampleSpec::from_json_value(sample_value)?;
            if sample_specs
                .iter()
                .any(|existing: &WorkspaceSampleSpec| existing.id == spec.id)
            {
                return Err(format!(
                    "workspace contains duplicate sample id '{}'",
                    spec.id
                ));
            }
            sample_specs.push(spec);
        }

        let command_logs_object = value
            .get("command_logs")
            .and_then(JsonValue::as_object)
            .ok_or_else(|| "workspace document must contain a command_logs object".to_string())?;
        let derived_metric = match value.get("derived_metric") {
            Some(value) => DerivedMetricDefinition::from_json_value(value)?,
            None => default_derived_metric_for_sample(None),
        };
        let analysis_logs_object = value.get("analysis_logs").and_then(JsonValue::as_object);
        let view_logs_object = value.get("view_logs").and_then(JsonValue::as_object);
        let redo_stacks_object = value
            .get("redo_stacks")
            .and_then(JsonValue::as_object)
            .ok_or_else(|| "workspace document must contain a redo_stacks object".to_string())?;

        for sample_id in command_logs_object.keys() {
            if !sample_specs.iter().any(|spec| spec.id == *sample_id) {
                return Err(format!(
                    "workspace command_logs contains unknown sample '{}'",
                    sample_id
                ));
            }
        }
        if let Some(analysis_logs_object) = analysis_logs_object {
            for sample_id in analysis_logs_object.keys() {
                if !sample_specs.iter().any(|spec| spec.id == *sample_id) {
                    return Err(format!(
                        "workspace analysis_logs contains unknown sample '{}'",
                        sample_id
                    ));
                }
            }
        }
        if let Some(view_logs_object) = view_logs_object {
            for sample_id in view_logs_object.keys() {
                if !sample_specs.iter().any(|spec| spec.id == *sample_id) {
                    return Err(format!(
                        "workspace view_logs contains unknown sample '{}'",
                        sample_id
                    ));
                }
            }
        }
        for sample_id in redo_stacks_object.keys() {
            if !sample_specs.iter().any(|spec| spec.id == *sample_id) {
                return Err(format!(
                    "workspace redo_stacks contains unknown sample '{}'",
                    sample_id
                ));
            }
        }

        let mut command_logs = BTreeMap::new();
        let mut analysis_logs = BTreeMap::new();
        let mut view_logs = BTreeMap::new();
        let mut redo_stacks = BTreeMap::new();
        for spec in &sample_specs {
            let log_json = command_logs_object
                .get(&spec.id)
                .cloned()
                .unwrap_or_else(|| JsonValue::Array(Vec::new()))
                .stringify_canonical();
            let command_log = CommandLog::from_json(&log_json).map_err(|error| {
                format!("invalid command log for sample '{}': {error}", spec.id)
            })?;
            for record in command_log.records() {
                if record.command.sample_id() != spec.id {
                    return Err(format!(
                        "command log for sample '{}' contains command for '{}'",
                        spec.id,
                        record.command.sample_id()
                    ));
                }
            }
            command_logs.insert(spec.id.clone(), command_log);

            let analysis_log = match analysis_logs_object.and_then(|object| object.get(&spec.id)) {
                Some(value) => AnalysisActionLog::from_json_value(value).map_err(|error| {
                    format!("invalid analysis log for sample '{}': {error}", spec.id)
                })?,
                None => AnalysisActionLog::new(),
            };
            for record in analysis_log.records() {
                if record.action.sample_id() != spec.id {
                    return Err(format!(
                        "analysis log for sample '{}' contains action for '{}'",
                        spec.id,
                        record.action.sample_id()
                    ));
                }
            }
            analysis_logs.insert(spec.id.clone(), analysis_log);

            let view_log = match view_logs_object.and_then(|object| object.get(&spec.id)) {
                Some(value) => ViewActionLog::from_json_value(value).map_err(|error| {
                    format!("invalid view log for sample '{}': {error}", spec.id)
                })?,
                None => ViewActionLog::new(),
            };
            for record in view_log.records() {
                if record.action.sample_id() != spec.id {
                    return Err(format!(
                        "view log for sample '{}' contains action for '{}'",
                        spec.id,
                        record.action.sample_id()
                    ));
                }
            }
            view_logs.insert(spec.id.clone(), view_log);

            let redo_values = match redo_stacks_object.get(&spec.id) {
                Some(value) => value.as_array().ok_or_else(|| {
                    format!("redo stack for sample '{}' must be an array", spec.id)
                })?,
                None => &[],
            };
            let mut redo_stack = Vec::with_capacity(redo_values.len());
            for command_value in redo_values {
                let command = Command::from_json_value(command_value).map_err(|error| {
                    format!("invalid redo command for sample '{}': {error}", spec.id)
                })?;
                if command.sample_id() != spec.id {
                    return Err(format!(
                        "redo stack for sample '{}' contains command for '{}'",
                        spec.id,
                        command.sample_id()
                    ));
                }
                redo_stack.push(command);
            }
            redo_stacks.insert(spec.id.clone(), redo_stack);
        }

        let active_sample_id = value
            .get("active_sample_id")
            .and_then(JsonValue::as_str)
            .ok_or_else(|| "workspace document is missing active_sample_id".to_string())?
            .to_string();
        if !sample_specs.iter().any(|spec| spec.id == active_sample_id) {
            return Err(format!(
                "workspace active sample '{}' is not defined in samples",
                active_sample_id
            ));
        }

        Ok(Self {
            active_sample_id,
            sample_specs,
            derived_metric,
            command_logs,
            analysis_logs,
            view_logs,
            redo_stacks,
        })
    }
}

impl WorkspaceSampleSpec {
    fn from_json_value(value: &JsonValue) -> Result<Self, String> {
        let id = value
            .get("id")
            .and_then(JsonValue::as_str)
            .ok_or_else(|| "workspace sample is missing id".to_string())?
            .to_string();
        let display_name = value
            .get("display_name")
            .and_then(JsonValue::as_str)
            .unwrap_or(&id)
            .to_string();
        let group_label = value
            .get("group_label")
            .and_then(JsonValue::as_str)
            .map(normalize_group_label)
            .unwrap_or_else(default_group_label);
        let source_kind = value
            .get("source_kind")
            .and_then(JsonValue::as_str)
            .ok_or_else(|| format!("workspace sample '{}' is missing source_kind", id))?;
        let source_path = match value.get("source_path") {
            Some(JsonValue::Null) | None => None,
            Some(JsonValue::String(path)) => Some(path.clone()),
            _ => {
                return Err(format!(
                    "workspace sample '{}' has an invalid source_path",
                    id
                ));
            }
        };

        let source = match source_kind {
            "embedded_demo" => WorkspaceSampleSource::EmbeddedDemo,
            "fcs_file" => WorkspaceSampleSource::FcsFile(
                source_path
                    .ok_or_else(|| format!("workspace sample '{}' is missing source_path", id))?,
            ),
            other => {
                return Err(format!(
                    "workspace sample '{}' has unknown source_kind '{}'",
                    id, other
                ));
            }
        };

        Ok(Self {
            id,
            display_name,
            group_label,
            source,
        })
    }
}

impl WorkspaceSampleSource {
    fn kind_name(&self) -> &'static str {
        match self {
            Self::EmbeddedDemo => "embedded_demo",
            Self::FcsFile(_) => "fcs_file",
        }
    }

    fn path(&self) -> Option<&str> {
        match self {
            Self::EmbeddedDemo => None,
            Self::FcsFile(path) => Some(path),
        }
    }
}

pub fn bootstrap_json_string() -> String {
    match DesktopSession::new().and_then(|session| session.snapshot_value()) {
        Ok(value) => value.stringify_canonical(),
        Err(message) => error_json_value(message).stringify_canonical(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn flowjoish_desktop_bootstrap_json() -> *mut c_char {
    payload_to_ptr(bootstrap_json_string())
}

#[unsafe(no_mangle)]
pub extern "C" fn flowjoish_desktop_session_new() -> *mut DesktopSession {
    match catch_unwind(AssertUnwindSafe(DesktopSession::new)) {
        Ok(Ok(session)) => Box::into_raw(Box::new(session)),
        Ok(Err(_)) | Err(_) => ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn flowjoish_desktop_session_snapshot_json(
    session: *mut DesktopSession,
) -> *mut c_char {
    with_session_payload(session, |session| session.snapshot_value())
}

#[unsafe(no_mangle)]
pub extern "C" fn flowjoish_desktop_session_dispatch_json(
    session: *mut DesktopSession,
    command_json: *const c_char,
) -> *mut c_char {
    if command_json.is_null() {
        return payload_to_ptr(
            error_json_value("command json pointer was null").stringify_canonical(),
        );
    }

    let command_json = unsafe { CStr::from_ptr(command_json) };
    match command_json.to_str() {
        Ok(command_json) => {
            with_session_payload(session, |session| Ok(session.dispatch_json(command_json)))
        }
        Err(error) => payload_to_ptr(error_json_value(error.to_string()).stringify_canonical()),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn flowjoish_desktop_session_reset(session: *mut DesktopSession) -> *mut c_char {
    with_session_payload(session, |session| Ok(session.reset()))
}

#[unsafe(no_mangle)]
pub extern "C" fn flowjoish_desktop_session_undo(session: *mut DesktopSession) -> *mut c_char {
    with_session_payload(session, |session| Ok(session.undo()))
}

#[unsafe(no_mangle)]
pub extern "C" fn flowjoish_desktop_session_redo(session: *mut DesktopSession) -> *mut c_char {
    with_session_payload(session, |session| Ok(session.redo()))
}

#[unsafe(no_mangle)]
pub extern "C" fn flowjoish_desktop_session_import_fcs_json(
    session: *mut DesktopSession,
    file_paths_json: *const c_char,
) -> *mut c_char {
    if file_paths_json.is_null() {
        return payload_to_ptr(
            error_json_value("file path json pointer was null").stringify_canonical(),
        );
    }

    let file_paths_json = unsafe { CStr::from_ptr(file_paths_json) };
    match file_paths_json.to_str() {
        Ok(file_paths_json) => with_session_payload(session, |session| {
            Ok(session.import_fcs_json(file_paths_json))
        }),
        Err(error) => payload_to_ptr(error_json_value(error.to_string()).stringify_canonical()),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn flowjoish_desktop_session_select_sample(
    session: *mut DesktopSession,
    sample_id: *const c_char,
) -> *mut c_char {
    if sample_id.is_null() {
        return payload_to_ptr(
            error_json_value("sample id pointer was null").stringify_canonical(),
        );
    }

    let sample_id = unsafe { CStr::from_ptr(sample_id) };
    match sample_id.to_str() {
        Ok(sample_id) => {
            with_session_payload(session, |session| Ok(session.select_sample(sample_id)))
        }
        Err(error) => payload_to_ptr(error_json_value(error.to_string()).stringify_canonical()),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn flowjoish_desktop_session_save_workspace(
    session: *mut DesktopSession,
    workspace_path: *const c_char,
) -> *mut c_char {
    if workspace_path.is_null() {
        return payload_to_ptr(
            error_json_value("workspace path pointer was null").stringify_canonical(),
        );
    }

    let workspace_path = unsafe { CStr::from_ptr(workspace_path) };
    match workspace_path.to_str() {
        Ok(workspace_path) => {
            with_session_payload(
                session,
                |session| Ok(session.save_workspace(workspace_path)),
            )
        }
        Err(error) => payload_to_ptr(error_json_value(error.to_string()).stringify_canonical()),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn flowjoish_desktop_session_load_workspace(
    session: *mut DesktopSession,
    workspace_path: *const c_char,
) -> *mut c_char {
    if workspace_path.is_null() {
        return payload_to_ptr(
            error_json_value("workspace path pointer was null").stringify_canonical(),
        );
    }

    let workspace_path = unsafe { CStr::from_ptr(workspace_path) };
    match workspace_path.to_str() {
        Ok(workspace_path) => {
            with_session_payload(
                session,
                |session| Ok(session.load_workspace(workspace_path)),
            )
        }
        Err(error) => payload_to_ptr(error_json_value(error.to_string()).stringify_canonical()),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn flowjoish_desktop_session_export_stats_csv(
    session: *mut DesktopSession,
    export_path: *const c_char,
) -> *mut c_char {
    if export_path.is_null() {
        return payload_to_ptr(
            error_json_value("stats export path pointer was null").stringify_canonical(),
        );
    }

    let export_path = unsafe { CStr::from_ptr(export_path) };
    match export_path.to_str() {
        Ok(export_path) => with_session_payload(session, |session| {
            Ok(session.export_population_stats_csv(export_path))
        }),
        Err(error) => payload_to_ptr(error_json_value(error.to_string()).stringify_canonical()),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn flowjoish_desktop_session_apply_active_template_to_other_samples(
    session: *mut DesktopSession,
) -> *mut c_char {
    with_session_payload(session, |session| {
        Ok(session.apply_active_template_to_other_samples())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn flowjoish_desktop_session_export_batch_stats_csv(
    session: *mut DesktopSession,
    export_path: *const c_char,
) -> *mut c_char {
    if export_path.is_null() {
        return payload_to_ptr(
            error_json_value("batch stats export path pointer was null").stringify_canonical(),
        );
    }

    let export_path = unsafe { CStr::from_ptr(export_path) };
    match export_path.to_str() {
        Ok(export_path) => with_session_payload(session, |session| {
            Ok(session.export_batch_population_stats_csv(export_path))
        }),
        Err(error) => payload_to_ptr(error_json_value(error.to_string()).stringify_canonical()),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn flowjoish_desktop_session_population_comparison_json(
    session: *mut DesktopSession,
    population_key: *const c_char,
) -> *mut c_char {
    if population_key.is_null() {
        return payload_to_ptr(
            error_json_value("population comparison key pointer was null").stringify_canonical(),
        );
    }

    let population_key = unsafe { CStr::from_ptr(population_key) };
    match population_key.to_str() {
        Ok(population_key) => with_session_payload(session, |session| {
            Ok(session.population_comparison(population_key))
        }),
        Err(error) => payload_to_ptr(error_json_value(error.to_string()).stringify_canonical()),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn flowjoish_desktop_session_export_population_comparison_csv(
    session: *mut DesktopSession,
    population_key: *const c_char,
    export_path: *const c_char,
) -> *mut c_char {
    if population_key.is_null() {
        return payload_to_ptr(
            error_json_value("population comparison key pointer was null").stringify_canonical(),
        );
    }
    if export_path.is_null() {
        return payload_to_ptr(
            error_json_value("population comparison export path pointer was null")
                .stringify_canonical(),
        );
    }

    let population_key = unsafe { CStr::from_ptr(population_key) };
    let export_path = unsafe { CStr::from_ptr(export_path) };
    match (population_key.to_str(), export_path.to_str()) {
        (Ok(population_key), Ok(export_path)) => with_session_payload(session, |session| {
            Ok(session.export_population_comparison_csv(population_key, export_path))
        }),
        (Err(error), _) | (_, Err(error)) => {
            payload_to_ptr(error_json_value(error.to_string()).stringify_canonical())
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn flowjoish_desktop_session_export_population_group_summary_csv(
    session: *mut DesktopSession,
    population_key: *const c_char,
    export_path: *const c_char,
) -> *mut c_char {
    if population_key.is_null() {
        return payload_to_ptr(
            error_json_value("population group summary key pointer was null").stringify_canonical(),
        );
    }
    if export_path.is_null() {
        return payload_to_ptr(
            error_json_value("population group summary export path pointer was null")
                .stringify_canonical(),
        );
    }

    let population_key = unsafe { CStr::from_ptr(population_key) };
    let export_path = unsafe { CStr::from_ptr(export_path) };
    match (population_key.to_str(), export_path.to_str()) {
        (Ok(population_key), Ok(export_path)) => with_session_payload(session, |session| {
            Ok(session.export_population_group_summary_csv(population_key, export_path))
        }),
        (Err(error), _) | (_, Err(error)) => {
            payload_to_ptr(error_json_value(error.to_string()).stringify_canonical())
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn flowjoish_desktop_session_set_derived_metric_json(
    session: *mut DesktopSession,
    metric_json: *const c_char,
) -> *mut c_char {
    if metric_json.is_null() {
        return payload_to_ptr(
            error_json_value("derived metric json pointer was null").stringify_canonical(),
        );
    }

    let metric_json = unsafe { CStr::from_ptr(metric_json) };
    match metric_json.to_str() {
        Ok(metric_json) => with_session_payload(session, |session| {
            Ok(session.set_derived_metric_from_json(metric_json))
        }),
        Err(error) => payload_to_ptr(error_json_value(error.to_string()).stringify_canonical()),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn flowjoish_desktop_session_export_population_derived_metric_csv(
    session: *mut DesktopSession,
    population_key: *const c_char,
    export_path: *const c_char,
) -> *mut c_char {
    if population_key.is_null() {
        return payload_to_ptr(
            error_json_value("population derived metric key pointer was null")
                .stringify_canonical(),
        );
    }
    if export_path.is_null() {
        return payload_to_ptr(
            error_json_value("population derived metric export path pointer was null")
                .stringify_canonical(),
        );
    }

    let population_key = unsafe { CStr::from_ptr(population_key) };
    let export_path = unsafe { CStr::from_ptr(export_path) };
    match (population_key.to_str(), export_path.to_str()) {
        (Ok(population_key), Ok(export_path)) => with_session_payload(session, |session| {
            Ok(session.export_population_derived_metric_csv(population_key, export_path))
        }),
        (Err(error), _) | (_, Err(error)) => {
            payload_to_ptr(error_json_value(error.to_string()).stringify_canonical())
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn flowjoish_desktop_session_set_sample_group_label(
    session: *mut DesktopSession,
    sample_id: *const c_char,
    group_label: *const c_char,
) -> *mut c_char {
    if sample_id.is_null() {
        return payload_to_ptr(
            error_json_value("sample group label sample_id pointer was null").stringify_canonical(),
        );
    }
    if group_label.is_null() {
        return payload_to_ptr(
            error_json_value("sample group label pointer was null").stringify_canonical(),
        );
    }

    let sample_id = unsafe { CStr::from_ptr(sample_id) };
    let group_label = unsafe { CStr::from_ptr(group_label) };
    match (sample_id.to_str(), group_label.to_str()) {
        (Ok(sample_id), Ok(group_label)) => with_session_payload(session, |session| {
            Ok(session.set_sample_group_label(sample_id, group_label))
        }),
        (Err(error), _) | (_, Err(error)) => {
            payload_to_ptr(error_json_value(error.to_string()).stringify_canonical())
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn flowjoish_desktop_session_free(ptr: *mut DesktopSession) {
    if ptr.is_null() {
        return;
    }

    unsafe {
        drop(Box::from_raw(ptr));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn flowjoish_string_free(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }

    unsafe {
        drop(CString::from_raw(ptr));
    }
}

fn with_session_payload(
    session: *mut DesktopSession,
    f: impl FnOnce(&mut DesktopSession) -> Result<JsonValue, String>,
) -> *mut c_char {
    if session.is_null() {
        return payload_to_ptr(
            error_json_value("desktop session pointer was null").stringify_canonical(),
        );
    }

    let payload = match catch_unwind(AssertUnwindSafe(|| unsafe {
        let session = &mut *session;
        match f(session) {
            Ok(value) => value.stringify_canonical(),
            Err(message) => error_json_value(message).stringify_canonical(),
        }
    })) {
        Ok(payload) => payload,
        Err(_) => error_json_value("desktop bridge panicked").stringify_canonical(),
    };

    payload_to_ptr(payload)
}

fn payload_to_ptr(payload: String) -> *mut c_char {
    CString::new(payload)
        .expect("desktop bridge json should not contain NUL bytes")
        .into_raw()
}

fn error_json_value(message: impl Into<String>) -> JsonValue {
    JsonValue::object([
        ("status", JsonValue::String("error".to_string())),
        ("message", JsonValue::String(message.into())),
    ])
}

fn desktop_application_json() -> JsonValue {
    JsonValue::object([
        ("name", JsonValue::String("Parallax".to_string())),
        (
            "tagline",
            JsonValue::String("Fast, trustworthy, reproducible cytometry analysis".to_string()),
        ),
        (
            "desktop_host",
            JsonValue::String("Qt/QML desktop with a shared Rust engine".to_string()),
        ),
    ])
}

fn demo_sample(sample_id: String) -> Result<SampleFrame, String> {
    SampleFrame::new(
        sample_id,
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
    .map_err(|error| error.to_string())
}

fn ground_rules_json() -> JsonValue {
    JsonValue::Array(
        [
            "One engine, everywhere",
            "Local-first always works",
            "Everything is replayable",
            "No silent AI actions",
            "Performance is a feature",
        ]
        .into_iter()
        .map(|value| JsonValue::String(value.to_string()))
        .collect(),
    )
}

fn sample_json(
    sample: &SampleFrame,
    sample_info: &DesktopSampleInfo,
    compensation: Option<&CompensationMatrix>,
    analysis_profile: &SampleAnalysisProfile,
) -> JsonValue {
    JsonValue::object([
        ("id", JsonValue::String(sample.sample_id().to_string())),
        (
            "display_name",
            JsonValue::String(sample_info.display_name.clone()),
        ),
        (
            "group_label",
            JsonValue::String(sample_info.group_label.clone()),
        ),
        (
            "source_path",
            match &sample_info.source_path {
                Some(path) => JsonValue::String(path.clone()),
                None => JsonValue::Null,
            },
        ),
        (
            "event_count",
            JsonValue::Number(sample.event_count() as f64),
        ),
        (
            "channel_count",
            JsonValue::Number(sample.channels().len() as f64),
        ),
        (
            "channels",
            JsonValue::Array(
                sample
                    .channels()
                    .iter()
                    .cloned()
                    .map(JsonValue::String)
                    .collect(),
            ),
        ),
        (
            "compensation_available",
            JsonValue::Bool(compensation.is_some()),
        ),
        (
            "compensation_enabled",
            JsonValue::Bool(analysis_profile.compensation_enabled),
        ),
        (
            "compensation_source_key",
            match compensation {
                Some(compensation) => JsonValue::String(compensation.source_key.clone()),
                None => JsonValue::Null,
            },
        ),
        (
            "channel_transforms",
            JsonValue::Array(
                sample
                    .channels()
                    .iter()
                    .map(|channel| {
                        let transform = analysis_profile.transform_for(channel);
                        JsonValue::object([
                            ("channel", JsonValue::String(channel.clone())),
                            ("kind", JsonValue::String(transform.kind_name().to_string())),
                            (
                                "cofactor",
                                match transform {
                                    ChannelTransform::Asinh { cofactor } => {
                                        JsonValue::Number(cofactor)
                                    }
                                    _ => JsonValue::Null,
                                },
                            ),
                            (
                                "decades",
                                match transform {
                                    ChannelTransform::Biexponential {
                                        positive_decades, ..
                                    }
                                    | ChannelTransform::Logicle {
                                        decades: positive_decades,
                                        ..
                                    } => JsonValue::Number(positive_decades),
                                    _ => JsonValue::Null,
                                },
                            ),
                            (
                                "negative_decades",
                                match transform {
                                    ChannelTransform::Biexponential {
                                        negative_decades, ..
                                    } => JsonValue::Number(negative_decades),
                                    _ => JsonValue::Null,
                                },
                            ),
                            (
                                "linear_width",
                                match transform {
                                    ChannelTransform::Logicle { linear_width, .. } => {
                                        JsonValue::Number(linear_width)
                                    }
                                    _ => JsonValue::Null,
                                },
                            ),
                            (
                                "width_basis",
                                match transform {
                                    ChannelTransform::Biexponential { width_basis, .. } => {
                                        JsonValue::Number(width_basis)
                                    }
                                    _ => JsonValue::Null,
                                },
                            ),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
}

fn workspace_sample_json(sample_id: &str, sample_info: &DesktopSampleInfo) -> JsonValue {
    let source = match &sample_info.source_path {
        Some(path) => WorkspaceSampleSource::FcsFile(path.clone()),
        None => WorkspaceSampleSource::EmbeddedDemo,
    };

    JsonValue::object([
        ("id", JsonValue::String(sample_id.to_string())),
        (
            "display_name",
            JsonValue::String(sample_info.display_name.clone()),
        ),
        (
            "group_label",
            JsonValue::String(sample_info.group_label.clone()),
        ),
        (
            "source_kind",
            JsonValue::String(source.kind_name().to_string()),
        ),
        (
            "source_path",
            match source.path() {
                Some(path) => JsonValue::String(path.to_string()),
                None => JsonValue::Null,
            },
        ),
    ])
}

fn samples_json(
    environment: &ReplayEnvironment,
    sample_order: &[String],
    sample_info: &BTreeMap<String, DesktopSampleInfo>,
) -> JsonValue {
    JsonValue::Array(
        sample_order
            .iter()
            .filter_map(|sample_id| {
                let sample = environment.sample(sample_id)?;
                let info = sample_info.get(sample_id)?;
                Some(JsonValue::object([
                    ("id", JsonValue::String(sample_id.clone())),
                    ("display_name", JsonValue::String(info.display_name.clone())),
                    ("group_label", JsonValue::String(info.group_label.clone())),
                    (
                        "source_path",
                        match &info.source_path {
                            Some(path) => JsonValue::String(path.clone()),
                            None => JsonValue::Null,
                        },
                    ),
                    (
                        "event_count",
                        JsonValue::Number(sample.event_count() as f64),
                    ),
                    (
                        "channel_count",
                        JsonValue::Number(sample.channels().len() as f64),
                    ),
                ]))
            })
            .collect(),
    )
}

fn load_sample_artifact_from_source(
    source: &WorkspaceSampleSource,
    sample_id: &str,
) -> Result<DesktopSampleArtifact, String> {
    match source {
        WorkspaceSampleSource::EmbeddedDemo => Ok(DesktopSampleArtifact {
            raw_sample: demo_sample(sample_id.to_string())?,
            compensation: None,
        }),
        WorkspaceSampleSource::FcsFile(path) => {
            let bytes = fs::read(path)
                .map_err(|error| format!("failed to read FCS file '{}': {error}", path))?;
            let parsed = parse_fcs(&bytes)
                .map_err(|error| format!("failed to parse FCS file '{}': {error}", path))?;
            let compensation = parsed.compensation.clone();
            let raw_sample = parsed
                .into_sample_frame(sample_id.to_string())
                .map_err(|error| format!("failed to convert FCS file '{}': {error}", path))?;
            Ok(DesktopSampleArtifact {
                raw_sample,
                compensation,
            })
        }
    }
}

fn commands_json(log: &CommandLog) -> JsonValue {
    JsonValue::Array(
        log.records()
            .iter()
            .map(|record| {
                JsonValue::object([
                    ("sequence", JsonValue::Number(record.sequence as f64)),
                    ("kind", JsonValue::String(record.command.kind().to_string())),
                    (
                        "sample_id",
                        JsonValue::String(record.command.sample_id().to_string()),
                    ),
                    (
                        "population_id",
                        JsonValue::String(record.command.population_id().to_string()),
                    ),
                    (
                        "parent_population",
                        match record.command.parent_population() {
                            Some(parent) => JsonValue::String(parent.to_string()),
                            None => JsonValue::Null,
                        },
                    ),
                ])
            })
            .collect(),
    )
}

fn population_stats_csv(stats: &[PopulationStats]) -> String {
    let mut output = String::from(
        "sample_id,population_key,population_id,parent_population,matched_events,parent_events,frequency_of_all,frequency_of_parent,channel,mean,median\n",
    );
    for population in stats {
        let key = stats_key(&population.population_id);
        let parent_population = population.parent_population.as_deref().unwrap_or("");
        for channel in &population.channel_stats {
            let _ = writeln!(
                output,
                "{},{},{},{},{},{},{:.6},{:.6},{},{},{}",
                csv_field(&population.sample_id),
                csv_field(&key),
                csv_field(&population.population_id),
                csv_field(parent_population),
                population.matched_events,
                population.parent_events,
                population.frequency_of_all,
                population.frequency_of_parent,
                csv_field(&channel.channel),
                csv_number(channel.mean),
                csv_number(channel.median),
            );
        }
    }
    output
}

fn population_comparison_row_json(row: PopulationComparisonRow) -> JsonValue {
    JsonValue::object([
        ("sample_id", JsonValue::String(row.sample_id)),
        ("display_name", JsonValue::String(row.display_name)),
        ("group_label", JsonValue::String(row.group_label)),
        (
            "source_path",
            match row.source_path {
                Some(path) => JsonValue::String(path),
                None => JsonValue::Null,
            },
        ),
        ("is_active_sample", JsonValue::Bool(row.is_active_sample)),
        ("status", JsonValue::String(row.status)),
        ("matched_events", optional_usize_json(row.matched_events)),
        ("parent_events", optional_usize_json(row.parent_events)),
        (
            "frequency_of_all",
            optional_number_json(row.frequency_of_all),
        ),
        (
            "frequency_of_parent",
            optional_number_json(row.frequency_of_parent),
        ),
        (
            "delta_frequency_of_all",
            optional_number_json(row.delta_frequency_of_all),
        ),
        (
            "delta_frequency_of_parent",
            optional_number_json(row.delta_frequency_of_parent),
        ),
        (
            "derived_metric_status",
            JsonValue::String(row.derived_metric_status),
        ),
        (
            "derived_metric_value",
            optional_number_json(row.derived_metric_value),
        ),
        (
            "derived_metric_delta_value",
            optional_number_json(row.derived_metric_delta_value),
        ),
        (
            "derived_metric_message",
            match row.derived_metric_message {
                Some(message) => JsonValue::String(message),
                None => JsonValue::Null,
            },
        ),
    ])
}

fn population_group_summary_row_json(row: PopulationGroupSummaryRow) -> JsonValue {
    JsonValue::object([
        ("group_label", JsonValue::String(row.group_label)),
        ("is_active_group", JsonValue::Bool(row.is_active_group)),
        ("sample_count", JsonValue::Number(row.sample_count as f64)),
        (
            "available_sample_count",
            JsonValue::Number(row.available_sample_count as f64),
        ),
        (
            "missing_sample_count",
            JsonValue::Number(row.missing_sample_count as f64),
        ),
        (
            "derived_metric_available_sample_count",
            JsonValue::Number(row.derived_metric_available_sample_count as f64),
        ),
        (
            "derived_metric_unavailable_sample_count",
            JsonValue::Number(row.derived_metric_unavailable_sample_count as f64),
        ),
        (
            "total_matched_events",
            JsonValue::Number(row.total_matched_events as f64),
        ),
        (
            "total_parent_events",
            JsonValue::Number(row.total_parent_events as f64),
        ),
        (
            "mean_frequency_of_all",
            optional_number_json(row.mean_frequency_of_all),
        ),
        (
            "mean_frequency_of_parent",
            optional_number_json(row.mean_frequency_of_parent),
        ),
        (
            "delta_mean_frequency_of_all",
            optional_number_json(row.delta_mean_frequency_of_all),
        ),
        (
            "delta_mean_frequency_of_parent",
            optional_number_json(row.delta_mean_frequency_of_parent),
        ),
        (
            "mean_derived_metric_value",
            optional_number_json(row.mean_derived_metric_value),
        ),
        (
            "delta_mean_derived_metric_value",
            optional_number_json(row.delta_mean_derived_metric_value),
        ),
    ])
}

fn population_comparison_csv(
    population_key: &str,
    population_id: &str,
    active_sample_id: &str,
    active_group_label: &str,
    rows: &[PopulationComparisonRow],
) -> String {
    let mut output = String::from(
        "population_key,population_id,active_sample_id,active_group_label,sample_id,display_name,group_label,source_path,status,is_active_sample,matched_events,parent_events,frequency_of_all,frequency_of_parent,delta_frequency_of_all,delta_frequency_of_parent\n",
    );
    for row in rows {
        let _ = writeln!(
            output,
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            csv_field(population_key),
            csv_field(population_id),
            csv_field(active_sample_id),
            csv_field(active_group_label),
            csv_field(&row.sample_id),
            csv_field(&row.display_name),
            csv_field(&row.group_label),
            csv_field(row.source_path.as_deref().unwrap_or("")),
            csv_field(&row.status),
            row.is_active_sample,
            csv_optional_usize(row.matched_events),
            csv_optional_usize(row.parent_events),
            csv_number(row.frequency_of_all),
            csv_number(row.frequency_of_parent),
            csv_number(row.delta_frequency_of_all),
            csv_number(row.delta_frequency_of_parent),
        );
    }
    output
}

fn population_group_summary_csv(
    population_key: &str,
    population_id: &str,
    active_sample_id: &str,
    active_group_label: &str,
    rows: &[PopulationGroupSummaryRow],
) -> String {
    let mut output = String::from(
        "population_key,population_id,active_sample_id,active_group_label,group_label,is_active_group,sample_count,available_sample_count,missing_sample_count,derived_metric_available_sample_count,derived_metric_unavailable_sample_count,total_matched_events,total_parent_events,mean_frequency_of_all,mean_frequency_of_parent,delta_mean_frequency_of_all,delta_mean_frequency_of_parent,mean_derived_metric_value,delta_mean_derived_metric_value\n",
    );
    for row in rows {
        let _ = writeln!(
            output,
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            csv_field(population_key),
            csv_field(population_id),
            csv_field(active_sample_id),
            csv_field(active_group_label),
            csv_field(&row.group_label),
            row.is_active_group,
            row.sample_count,
            row.available_sample_count,
            row.missing_sample_count,
            row.derived_metric_available_sample_count,
            row.derived_metric_unavailable_sample_count,
            row.total_matched_events,
            row.total_parent_events,
            csv_number(row.mean_frequency_of_all),
            csv_number(row.mean_frequency_of_parent),
            csv_number(row.delta_mean_frequency_of_all),
            csv_number(row.delta_mean_frequency_of_parent),
            csv_number(row.mean_derived_metric_value),
            csv_number(row.delta_mean_derived_metric_value),
        );
    }
    output
}

fn population_derived_metric_csv(
    population_key: &str,
    population_id: &str,
    active_sample_id: &str,
    metric: &DerivedMetricDefinition,
    rows: &[PopulationComparisonRow],
) -> String {
    let mut output = String::from(
        "population_key,population_id,active_sample_id,metric_kind,metric_label,sample_id,display_name,group_label,status,is_active_sample,value,delta_value,message\n",
    );
    for row in rows {
        let _ = writeln!(
            output,
            "{},{},{},{},{},{},{},{},{},{},{},{},{}",
            csv_field(population_key),
            csv_field(population_id),
            csv_field(active_sample_id),
            csv_field(metric.kind_name()),
            csv_field(&metric.label()),
            csv_field(&row.sample_id),
            csv_field(&row.display_name),
            csv_field(&row.group_label),
            csv_field(&row.derived_metric_status),
            row.is_active_sample,
            csv_number(row.derived_metric_value),
            csv_number(row.derived_metric_delta_value),
            csv_field(row.derived_metric_message.as_deref().unwrap_or("")),
        );
    }
    output
}

fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn csv_number(value: Option<f64>) -> String {
    value.map(|value| format!("{value:.6}")).unwrap_or_default()
}

fn csv_optional_usize(value: Option<usize>) -> String {
    value.map(|value| value.to_string()).unwrap_or_default()
}

fn optional_number_json(value: Option<f64>) -> JsonValue {
    match value {
        Some(value) => JsonValue::Number(value),
        None => JsonValue::Null,
    }
}

fn optional_usize_json(value: Option<usize>) -> JsonValue {
    match value {
        Some(value) => JsonValue::Number(value as f64),
        None => JsonValue::Null,
    }
}

fn default_group_label() -> String {
    "Ungrouped".to_string()
}

fn normalize_group_label(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        default_group_label()
    } else {
        trimmed.to_string()
    }
}

fn stats_key(population_id: &str) -> String {
    if population_id == "All Events" {
        "__all__".to_string()
    } else {
        population_id.to_string()
    }
}

fn find_population_stats_for_key(
    sample: &SampleFrame,
    state: &WorkspaceState,
    population_key: &str,
) -> Result<Option<PopulationStats>, String> {
    let stats = compute_population_stats_table(sample, state).map_err(|error| error.to_string())?;
    Ok(stats
        .into_iter()
        .find(|entry| stats_key(&entry.population_id) == population_key))
}

fn population_group_summaries(
    rows: &[PopulationComparisonRow],
    active_group_label: &str,
) -> Vec<PopulationGroupSummaryRow> {
    let mut grouped = BTreeMap::<String, Vec<&PopulationComparisonRow>>::new();
    for row in rows {
        grouped
            .entry(row.group_label.clone())
            .or_default()
            .push(row);
    }

    let mut summaries = grouped
        .into_iter()
        .map(|(group_label, rows)| {
            let sample_count = rows.len();
            let available_rows = rows
                .iter()
                .filter(|row| row.status == "available")
                .copied()
                .collect::<Vec<_>>();
            let metric_available_rows = available_rows
                .iter()
                .filter(|row| row.derived_metric_value.is_some())
                .copied()
                .collect::<Vec<_>>();
            let available_sample_count = available_rows.len();
            let missing_sample_count = sample_count.saturating_sub(available_sample_count);
            let derived_metric_available_sample_count = metric_available_rows.len();
            let derived_metric_unavailable_sample_count =
                available_sample_count.saturating_sub(derived_metric_available_sample_count);
            let total_matched_events = available_rows
                .iter()
                .filter_map(|row| row.matched_events)
                .sum::<usize>();
            let total_parent_events = available_rows
                .iter()
                .filter_map(|row| row.parent_events)
                .sum::<usize>();
            let mean_frequency_of_all = average_option(
                &available_rows
                    .iter()
                    .filter_map(|row| row.frequency_of_all)
                    .collect::<Vec<_>>(),
            );
            let mean_frequency_of_parent = average_option(
                &available_rows
                    .iter()
                    .filter_map(|row| row.frequency_of_parent)
                    .collect::<Vec<_>>(),
            );
            let mean_derived_metric_value = average_option(
                &metric_available_rows
                    .iter()
                    .filter_map(|row| row.derived_metric_value)
                    .collect::<Vec<_>>(),
            );

            PopulationGroupSummaryRow {
                is_active_group: group_label == active_group_label,
                group_label,
                sample_count,
                available_sample_count,
                missing_sample_count,
                derived_metric_available_sample_count,
                derived_metric_unavailable_sample_count,
                total_matched_events,
                total_parent_events,
                mean_frequency_of_all,
                mean_frequency_of_parent,
                delta_mean_frequency_of_all: None,
                delta_mean_frequency_of_parent: None,
                mean_derived_metric_value,
                delta_mean_derived_metric_value: None,
            }
        })
        .collect::<Vec<_>>();

    let active_baseline = summaries
        .iter()
        .find(|row| row.group_label == active_group_label)
        .map(|row| {
            (
                row.mean_frequency_of_all,
                row.mean_frequency_of_parent,
                row.mean_derived_metric_value,
            )
        });

    if let Some((baseline_all, baseline_parent, baseline_metric)) = active_baseline {
        for row in &mut summaries {
            row.delta_mean_frequency_of_all = match (row.mean_frequency_of_all, baseline_all) {
                (Some(value), Some(baseline)) => Some(value - baseline),
                _ => None,
            };
            row.delta_mean_frequency_of_parent =
                match (row.mean_frequency_of_parent, baseline_parent) {
                    (Some(value), Some(baseline)) => Some(value - baseline),
                    _ => None,
                };
            row.delta_mean_derived_metric_value =
                match (row.mean_derived_metric_value, baseline_metric) {
                    (Some(value), Some(baseline)) => Some(value - baseline),
                    _ => None,
                };
        }
    }

    summaries.sort_by(|left, right| {
        right
            .is_active_group
            .cmp(&left.is_active_group)
            .then_with(|| left.group_label.cmp(&right.group_label))
    });
    summaries
}

fn average_option(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        None
    } else {
        Some(values.iter().sum::<f64>() / values.len() as f64)
    }
}

fn default_derived_metric_for_sample(sample: Option<&SampleFrame>) -> DerivedMetricDefinition {
    let channel = sample
        .and_then(default_metric_channel_for_sample)
        .unwrap_or_else(|| "FSC-A".to_string());
    DerivedMetricDefinition::PositiveFraction {
        channel,
        threshold: 1.0,
    }
}

fn default_metric_channel_for_sample(sample: &SampleFrame) -> Option<String> {
    sample
        .channels()
        .iter()
        .find(|channel| !is_time_channel(channel) && !is_structural_channel(channel))
        .cloned()
        .or_else(|| sample.channels().first().cloned())
}

fn validate_derived_metric_for_sample(
    sample: &SampleFrame,
    metric: &DerivedMetricDefinition,
) -> Result<(), String> {
    match metric {
        DerivedMetricDefinition::PositiveFraction { channel, threshold } => {
            if sample.channel_index(channel).is_none() {
                return Err(format!(
                    "derived metric channel '{}' is not present in the active sample",
                    channel
                ));
            }
            if !threshold.is_finite() {
                return Err("derived metric threshold must be finite".to_string());
            }
            Ok(())
        }
        DerivedMetricDefinition::MeanRatio {
            numerator_channel,
            denominator_channel,
        } => {
            if sample.channel_index(numerator_channel).is_none() {
                return Err(format!(
                    "derived metric numerator channel '{}' is not present in the active sample",
                    numerator_channel
                ));
            }
            if sample.channel_index(denominator_channel).is_none() {
                return Err(format!(
                    "derived metric denominator channel '{}' is not present in the active sample",
                    denominator_channel
                ));
            }
            Ok(())
        }
    }
}

fn evaluate_derived_metric(
    sample: &SampleFrame,
    state: &WorkspaceState,
    population_key: &str,
    metric: &DerivedMetricDefinition,
) -> DerivedMetricEvaluation {
    let mask = if population_key == "__all__" {
        None
    } else {
        match state.populations.get(population_key) {
            Some(population) => Some(&population.mask),
            None => {
                return DerivedMetricEvaluation {
                    status: "missing_population".to_string(),
                    value: None,
                    message: Some(
                        "This population is not present in the current gate history for this sample."
                            .to_string(),
                    ),
                };
            }
        }
    };

    match metric {
        DerivedMetricDefinition::PositiveFraction { channel, threshold } => {
            let Some(channel_index) = sample.channel_index(channel) else {
                return DerivedMetricEvaluation {
                    status: "missing_channel".to_string(),
                    value: None,
                    message: Some(format!(
                        "Channel '{}' is not present in this sample.",
                        channel
                    )),
                };
            };

            let mut matched = 0usize;
            let mut positives = 0usize;
            for (event_index, row) in sample.events().iter().enumerate() {
                if mask.is_some_and(|mask| !mask.contains(event_index)) {
                    continue;
                }
                matched += 1;
                if row[channel_index] >= *threshold {
                    positives += 1;
                }
            }

            if matched == 0 {
                DerivedMetricEvaluation {
                    status: "empty_population".to_string(),
                    value: None,
                    message: Some("The selected population has zero matched events.".to_string()),
                }
            } else {
                DerivedMetricEvaluation {
                    status: "available".to_string(),
                    value: Some(positives as f64 / matched as f64),
                    message: None,
                }
            }
        }
        DerivedMetricDefinition::MeanRatio {
            numerator_channel,
            denominator_channel,
        } => {
            let Some(numerator_index) = sample.channel_index(numerator_channel) else {
                return DerivedMetricEvaluation {
                    status: "missing_channel".to_string(),
                    value: None,
                    message: Some(format!(
                        "Channel '{}' is not present in this sample.",
                        numerator_channel
                    )),
                };
            };
            let Some(denominator_index) = sample.channel_index(denominator_channel) else {
                return DerivedMetricEvaluation {
                    status: "missing_channel".to_string(),
                    value: None,
                    message: Some(format!(
                        "Channel '{}' is not present in this sample.",
                        denominator_channel
                    )),
                };
            };

            let mut matched = 0usize;
            let mut numerator_total = 0.0;
            let mut denominator_total = 0.0;
            for (event_index, row) in sample.events().iter().enumerate() {
                if mask.is_some_and(|mask| !mask.contains(event_index)) {
                    continue;
                }
                matched += 1;
                numerator_total += row[numerator_index];
                denominator_total += row[denominator_index];
            }

            if matched == 0 {
                DerivedMetricEvaluation {
                    status: "empty_population".to_string(),
                    value: None,
                    message: Some("The selected population has zero matched events.".to_string()),
                }
            } else {
                let numerator_mean = numerator_total / matched as f64;
                let denominator_mean = denominator_total / matched as f64;
                if denominator_mean.abs() <= 1e-12 {
                    DerivedMetricEvaluation {
                        status: "undefined".to_string(),
                        value: None,
                        message: Some("The denominator mean is zero for this sample.".to_string()),
                    }
                } else {
                    DerivedMetricEvaluation {
                        status: "available".to_string(),
                        value: Some(numerator_mean / denominator_mean),
                        message: None,
                    }
                }
            }
        }
    }
}

fn analysis_actions_json(log: &AnalysisActionLog) -> JsonValue {
    JsonValue::Array(
        log.records()
            .iter()
            .map(|record| match &record.action {
                AnalysisAction::SetCompensationEnabled { enabled, .. } => JsonValue::object([
                    ("sequence", JsonValue::Number(record.sequence as f64)),
                    ("kind", JsonValue::String(record.action.kind().to_string())),
                    (
                        "summary",
                        JsonValue::String(if *enabled {
                            "Apply FCS compensation".to_string()
                        } else {
                            "Disable FCS compensation".to_string()
                        }),
                    ),
                ]),
                AnalysisAction::SetChannelTransform {
                    channel, transform, ..
                } => JsonValue::object([
                    ("sequence", JsonValue::Number(record.sequence as f64)),
                    ("kind", JsonValue::String(record.action.kind().to_string())),
                    (
                        "summary",
                        JsonValue::String(format!(
                            "{channel} -> {}",
                            transform_display_name(transform)
                        )),
                    ),
                ]),
            })
            .collect(),
    )
}

fn transform_json(transform: &ChannelTransform) -> JsonValue {
    match transform {
        ChannelTransform::Linear => {
            JsonValue::object([("kind", JsonValue::String("linear".to_string()))])
        }
        ChannelTransform::SignedLog10 => {
            JsonValue::object([("kind", JsonValue::String("signed_log10".to_string()))])
        }
        ChannelTransform::Asinh { cofactor } => JsonValue::object([
            ("kind", JsonValue::String("asinh".to_string())),
            ("cofactor", JsonValue::Number(*cofactor)),
        ]),
        ChannelTransform::Biexponential {
            width_basis,
            positive_decades,
            negative_decades,
        } => JsonValue::object([
            ("kind", JsonValue::String("biexponential".to_string())),
            ("width_basis", JsonValue::Number(*width_basis)),
            ("positive_decades", JsonValue::Number(*positive_decades)),
            ("negative_decades", JsonValue::Number(*negative_decades)),
        ]),
        ChannelTransform::Logicle {
            decades,
            linear_width,
        } => JsonValue::object([
            ("kind", JsonValue::String("logicle".to_string())),
            ("decades", JsonValue::Number(*decades)),
            ("linear_width", JsonValue::Number(*linear_width)),
        ]),
    }
}

fn parse_transform_json(value: &JsonValue) -> Result<ChannelTransform, String> {
    let kind = required_json_string(value, "kind")?;
    match kind {
        "linear" => Ok(ChannelTransform::Linear),
        "signed_log10" => Ok(ChannelTransform::SignedLog10),
        "asinh" => Ok(ChannelTransform::Asinh {
            cofactor: value
                .get("cofactor")
                .and_then(JsonValue::as_f64)
                .unwrap_or(150.0),
        }),
        "biexponential" => Ok(ChannelTransform::Biexponential {
            width_basis: value
                .get("width_basis")
                .and_then(JsonValue::as_f64)
                .unwrap_or(120.0),
            positive_decades: value
                .get("positive_decades")
                .and_then(JsonValue::as_f64)
                .unwrap_or(4.5),
            negative_decades: value
                .get("negative_decades")
                .and_then(JsonValue::as_f64)
                .unwrap_or(1.0),
        }),
        "logicle" => Ok(ChannelTransform::Logicle {
            decades: value
                .get("decades")
                .and_then(JsonValue::as_f64)
                .unwrap_or(4.5),
            linear_width: value
                .get("linear_width")
                .and_then(JsonValue::as_f64)
                .unwrap_or(12.0),
        }),
        other => Err(format!("unknown transform kind '{other}'")),
    }
}

fn transform_display_name(transform: &ChannelTransform) -> String {
    match transform {
        ChannelTransform::Linear => "linear".to_string(),
        ChannelTransform::SignedLog10 => "signed_log10".to_string(),
        ChannelTransform::Asinh { cofactor } => format!("asinh({cofactor:.0})"),
        ChannelTransform::Biexponential {
            positive_decades,
            negative_decades,
            ..
        } => format!("biexponential(+{positive_decades:.1}/-{negative_decades:.1})"),
        ChannelTransform::Logicle {
            decades,
            linear_width,
        } => format!("logicle(M={decades:.1},W={linear_width:.1})"),
    }
}

fn required_json_string<'a>(value: &'a JsonValue, field: &str) -> Result<&'a str, String> {
    value
        .get(field)
        .and_then(JsonValue::as_str)
        .ok_or_else(|| format!("missing field '{field}'"))
}

fn required_json_bool(value: &JsonValue, field: &str) -> Result<bool, String> {
    value
        .get(field)
        .and_then(JsonValue::as_bool)
        .ok_or_else(|| format!("missing field '{field}'"))
}

fn parse_hex_u64(value: &str, field: &str) -> Result<u64, String> {
    u64::from_str_radix(value, 16).map_err(|_| format!("invalid field '{field}'"))
}

fn populations_json(sample: &SampleFrame, state: &WorkspaceState) -> JsonValue {
    let mut values = Vec::with_capacity(state.populations.len() + 1);
    values.push(JsonValue::object([
        ("key", JsonValue::String("__all__".to_string())),
        ("population_id", JsonValue::String("All Events".to_string())),
        ("parent_population", JsonValue::Null),
        (
            "matched_events",
            JsonValue::Number(sample.event_count() as f64),
        ),
        ("node_hash", JsonValue::Null),
    ]));

    values.extend(state.populations.values().map(|population| {
        JsonValue::object([
            ("key", JsonValue::String(population.population_id.clone())),
            (
                "population_id",
                JsonValue::String(population.population_id.clone()),
            ),
            (
                "parent_population",
                match &population.parent_population {
                    Some(parent) => JsonValue::String(parent.clone()),
                    None => JsonValue::Null,
                },
            ),
            (
                "matched_events",
                JsonValue::Number(population.matched_events as f64),
            ),
            (
                "node_hash",
                JsonValue::String(format!("{:016x}", population.node_hash)),
            ),
        ])
    }));

    JsonValue::Array(values)
}

fn population_stats_json(
    sample: &SampleFrame,
    state: &WorkspaceState,
) -> Result<JsonValue, String> {
    let stats = compute_population_stats_table(sample, state).map_err(|error| error.to_string())?;
    Ok(JsonValue::Object(
        stats
            .into_iter()
            .map(|stats| {
                let key = stats_key(&stats.population_id);
                (key, population_stats_entry_json(stats))
            })
            .collect::<BTreeMap<_, _>>(),
    ))
}

fn population_stats_entry_json(stats: PopulationStats) -> JsonValue {
    JsonValue::object([
        ("key", JsonValue::String(stats_key(&stats.population_id))),
        ("population_id", JsonValue::String(stats.population_id)),
        (
            "parent_population",
            match stats.parent_population {
                Some(parent) => JsonValue::String(parent),
                None => JsonValue::Null,
            },
        ),
        (
            "matched_events",
            JsonValue::Number(stats.matched_events as f64),
        ),
        (
            "parent_events",
            JsonValue::Number(stats.parent_events as f64),
        ),
        (
            "frequency_of_all",
            JsonValue::Number(stats.frequency_of_all),
        ),
        (
            "frequency_of_parent",
            JsonValue::Number(stats.frequency_of_parent),
        ),
        (
            "channel_stats",
            JsonValue::Array(
                stats
                    .channel_stats
                    .into_iter()
                    .map(|channel| {
                        JsonValue::object([
                            ("channel", JsonValue::String(channel.channel)),
                            (
                                "mean",
                                match channel.mean {
                                    Some(value) => JsonValue::Number(value),
                                    None => JsonValue::Null,
                                },
                            ),
                            (
                                "median",
                                match channel.median {
                                    Some(value) => JsonValue::Number(value),
                                    None => JsonValue::Null,
                                },
                            ),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
}

fn plots_json(
    sample: &SampleFrame,
    state: &WorkspaceState,
    plot_specs: &[PlotSpec],
    plot_ranges: &BTreeMap<String, PlotRangeState>,
) -> Result<JsonValue, String> {
    let plots = plot_specs
        .into_iter()
        .map(|plot| plot_json(sample, state, plot, plot_ranges.get(&plot.id)))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(JsonValue::Array(plots))
}

const HISTOGRAM_BIN_COUNT: usize = 32;

fn default_plot_specs(sample: &SampleFrame) -> Vec<PlotSpec> {
    let mut plots = Vec::new();

    if let Some((x_channel, y_channel)) = preferred_scatter_pair(sample) {
        plots.push(PlotSpec {
            id: plot_id_for_channels(x_channel, y_channel),
            title: format!("{x_channel} vs {y_channel}"),
            kind: PlotKind::Scatter,
            x_channel: x_channel.to_string(),
            y_channel: Some(y_channel.to_string()),
        });
    }

    if let Some((x_channel, y_channel)) = secondary_pair(sample, &plots) {
        plots.push(PlotSpec {
            id: plot_id_for_channels(&x_channel, &y_channel),
            title: format!("{x_channel} vs {y_channel}"),
            kind: PlotKind::Scatter,
            x_channel,
            y_channel: Some(y_channel),
        });
    }

    if let Some(channel) = histogram_channel(sample) {
        plots.push(PlotSpec {
            id: plot_id_for_histogram(&channel),
            title: format!("{channel} histogram"),
            kind: PlotKind::Histogram,
            x_channel: channel,
            y_channel: None,
        });
    }

    if plots.is_empty() {
        let channels = sample.channels();
        if channels.len() >= 2 {
            plots.push(PlotSpec {
                id: plot_id_for_channels(&channels[0], &channels[1]),
                title: format!("{} vs {}", channels[0], channels[1]),
                kind: PlotKind::Scatter,
                x_channel: channels[0].clone(),
                y_channel: Some(channels[1].clone()),
            });
        } else if let Some(channel) = channels.first() {
            plots.push(PlotSpec {
                id: plot_id_for_histogram(channel),
                title: format!("{channel} histogram"),
                kind: PlotKind::Histogram,
                x_channel: channel.clone(),
                y_channel: None,
            });
        }
    }

    plots
}

fn plot_json(
    sample: &SampleFrame,
    state: &WorkspaceState,
    plot: &PlotSpec,
    view_range: Option<&PlotRangeState>,
) -> Result<JsonValue, String> {
    let auto_range = auto_plot_range(sample, plot)?;
    let range = view_range.unwrap_or(&auto_range);

    match plot.kind {
        PlotKind::Scatter => scatter_plot_json(sample, state, plot, range),
        PlotKind::Histogram => histogram_plot_json(sample, state, plot, range),
    }
}

fn scatter_plot_json(
    sample: &SampleFrame,
    state: &WorkspaceState,
    plot: &PlotSpec,
    range: &PlotRangeState,
) -> Result<JsonValue, String> {
    let y_channel = plot
        .y_channel
        .as_deref()
        .ok_or_else(|| format!("scatter plot '{}' is missing y_channel", plot.id))?;
    let x_index = sample
        .channel_index(&plot.x_channel)
        .ok_or_else(|| format!("missing channel '{}'", plot.x_channel))?;
    let y_index = sample
        .channel_index(y_channel)
        .ok_or_else(|| format!("missing channel '{}'", y_channel))?;

    let sampled_indices = sampled_event_indices(sample.event_count(), SCATTER_POINT_LIMIT);
    let point_columns = point_columns_json(sample, x_index, y_index, &sampled_indices);
    let mut population_event_indices = BTreeMap::new();
    for population in state.populations.values() {
        population_event_indices.insert(
            population.population_id.clone(),
            JsonValue::Array(sampled_population_indices_json(
                &sampled_indices,
                &population.mask,
            )),
        );
    }

    Ok(JsonValue::object([
        ("kind", JsonValue::String("scatter".to_string())),
        ("id", JsonValue::String(plot.id.clone())),
        ("title", JsonValue::String(plot.title.clone())),
        ("x_channel", JsonValue::String(plot.x_channel.clone())),
        ("y_channel", JsonValue::String(y_channel.to_string())),
        ("view_summary", JsonValue::String(range.summary.clone())),
        (
            "total_event_count",
            JsonValue::Number(sample.event_count() as f64),
        ),
        (
            "rendered_event_count",
            JsonValue::Number(sampled_indices.len() as f64),
        ),
        ("point_limit", JsonValue::Number(SCATTER_POINT_LIMIT as f64)),
        (
            "decimated",
            JsonValue::Bool(sample.event_count() > sampled_indices.len()),
        ),
        ("point_columns", point_columns),
        (
            "population_event_indices",
            JsonValue::Object(population_event_indices),
        ),
        (
            "x_range",
            JsonValue::object([
                ("min", JsonValue::Number(range.x_min)),
                ("max", JsonValue::Number(range.x_max)),
            ]),
        ),
        (
            "y_range",
            JsonValue::object([
                ("min", JsonValue::Number(range.y_min)),
                ("max", JsonValue::Number(range.y_max)),
            ]),
        ),
    ]))
}

fn histogram_plot_json(
    sample: &SampleFrame,
    state: &WorkspaceState,
    plot: &PlotSpec,
    range: &PlotRangeState,
) -> Result<JsonValue, String> {
    let channel_index = sample
        .channel_index(&plot.x_channel)
        .ok_or_else(|| format!("missing channel '{}'", plot.x_channel))?;
    let all_bins = histogram_bins(
        sample,
        channel_index,
        range.x_min,
        range.x_max,
        HISTOGRAM_BIN_COUNT,
        None,
    );
    let mut population_bins = BTreeMap::new();
    population_bins.insert("__all__".to_string(), JsonValue::Array(all_bins.clone()));
    for population in state.populations.values() {
        population_bins.insert(
            population.population_id.clone(),
            JsonValue::Array(histogram_bins(
                sample,
                channel_index,
                range.x_min,
                range.x_max,
                HISTOGRAM_BIN_COUNT,
                Some(&population.mask),
            )),
        );
    }

    Ok(JsonValue::object([
        ("kind", JsonValue::String("histogram".to_string())),
        ("id", JsonValue::String(plot.id.clone())),
        ("title", JsonValue::String(plot.title.clone())),
        ("x_channel", JsonValue::String(plot.x_channel.clone())),
        ("y_channel", JsonValue::String("Count".to_string())),
        ("view_summary", JsonValue::String(range.summary.clone())),
        ("all_bins", JsonValue::Array(all_bins)),
        ("population_bins", JsonValue::Object(population_bins)),
        (
            "x_range",
            JsonValue::object([
                ("min", JsonValue::Number(range.x_min)),
                ("max", JsonValue::Number(range.x_max)),
            ]),
        ),
        (
            "y_range",
            JsonValue::object([
                ("min", JsonValue::Number(range.y_min)),
                ("max", JsonValue::Number(range.y_max)),
            ]),
        ),
    ]))
}

fn auto_plot_range(sample: &SampleFrame, plot: &PlotSpec) -> Result<PlotRangeState, String> {
    match plot.kind {
        PlotKind::Scatter => {
            let y_channel = plot
                .y_channel
                .as_deref()
                .ok_or_else(|| format!("scatter plot '{}' is missing y_channel", plot.id))?;
            let x_index = sample
                .channel_index(&plot.x_channel)
                .ok_or_else(|| format!("missing channel '{}'", plot.x_channel))?;
            let y_index = sample
                .channel_index(y_channel)
                .ok_or_else(|| format!("missing channel '{}'", y_channel))?;
            let (x_min, x_max) = axis_bounds(sample, x_index);
            let (y_min, y_max) = axis_bounds(sample, y_index);
            Ok(PlotRangeState {
                x_min,
                x_max,
                y_min,
                y_max,
                summary: "Auto extents".to_string(),
            })
        }
        PlotKind::Histogram => {
            histogram_range_for_mask(sample, plot, None, 0.08, "Auto extents".to_string())
        }
    }
}

fn histogram_range_for_mask(
    sample: &SampleFrame,
    plot: &PlotSpec,
    mask: Option<&BitMask>,
    padding_fraction: f64,
    summary: String,
) -> Result<PlotRangeState, String> {
    let channel_index = sample
        .channel_index(&plot.x_channel)
        .ok_or_else(|| format!("missing channel '{}'", plot.x_channel))?;
    let Some((value_min, value_max)) = channel_bounds(sample, channel_index, mask) else {
        return Ok(PlotRangeState {
            x_min: 0.0,
            x_max: 1.0,
            y_min: 0.0,
            y_max: 1.0,
            summary,
        });
    };
    let (x_min, x_max) = padded_bounds(value_min, value_max, padding_fraction);
    let bins = histogram_bins(
        sample,
        channel_index,
        x_min,
        x_max,
        HISTOGRAM_BIN_COUNT,
        mask,
    );
    let max_count = bins
        .iter()
        .filter_map(|bin| bin.get("count").and_then(JsonValue::as_f64))
        .fold(0.0, f64::max);
    Ok(PlotRangeState {
        x_min,
        x_max,
        y_min: 0.0,
        y_max: (max_count * 1.08).max(1.0),
        summary,
    })
}

fn focus_plot_range(
    sample: &SampleFrame,
    state: &WorkspaceState,
    plot: &PlotSpec,
    population_id: &str,
    padding_fraction: f64,
) -> Result<PlotRangeState, String> {
    if !padding_fraction.is_finite() || padding_fraction <= 0.0 {
        return Err("plot focus padding_fraction must be a positive finite number".to_string());
    }

    let focus_mask = if population_id == "__all__" {
        None
    } else {
        match state.populations.get(population_id) {
            Some(population) => Some(&population.mask),
            None => {
                let mut fallback = auto_plot_range(sample, plot)?;
                fallback.summary = format!("Auto extents ({population_id} unavailable)");
                return Ok(fallback);
            }
        }
    };
    let summary = if population_id == "__all__" {
        "Focused on All Events".to_string()
    } else {
        format!("Focused on {population_id}")
    };

    match plot.kind {
        PlotKind::Scatter => {
            let y_channel = plot
                .y_channel
                .as_deref()
                .ok_or_else(|| format!("scatter plot '{}' is missing y_channel", plot.id))?;
            let x_index = sample
                .channel_index(&plot.x_channel)
                .ok_or_else(|| format!("missing channel '{}'", plot.x_channel))?;
            let y_index = sample
                .channel_index(y_channel)
                .ok_or_else(|| format!("missing channel '{}'", y_channel))?;

            let Some((x_min, x_max, y_min, y_max)) =
                plot_bounds(sample, x_index, y_index, focus_mask)
            else {
                let mut fallback = auto_plot_range(sample, plot)?;
                fallback.summary = if population_id == "__all__" {
                    "Auto extents".to_string()
                } else {
                    format!("Auto extents ({population_id} unavailable)")
                };
                return Ok(fallback);
            };

            let (x_min, x_max) = padded_bounds(x_min, x_max, padding_fraction);
            let (y_min, y_max) = padded_bounds(y_min, y_max, padding_fraction);
            Ok(PlotRangeState {
                x_min,
                x_max,
                y_min,
                y_max,
                summary,
            })
        }
        PlotKind::Histogram => {
            histogram_range_for_mask(sample, plot, focus_mask, padding_fraction, summary)
        }
    }
}

fn scale_plot_range(range: &PlotRangeState, factor: f64) -> Result<PlotRangeState, String> {
    if !factor.is_finite() || factor <= 0.0 {
        return Err("plot scale factor must be a positive finite number".to_string());
    }

    let x_center = (range.x_min + range.x_max) / 2.0;
    let y_center = (range.y_min + range.y_max) / 2.0;
    let x_half = ((range.x_max - range.x_min) / 2.0).max(1e-6) * factor;
    let y_half = ((range.y_max - range.y_min) / 2.0).max(1e-6) * factor;
    Ok(PlotRangeState {
        x_min: x_center - x_half,
        x_max: x_center + x_half,
        y_min: y_center - y_half,
        y_max: y_center + y_half,
        summary: if factor < 1.0 {
            format!("Zoomed in ({factor:.2}x)")
        } else {
            format!("Zoomed out ({factor:.2}x)")
        },
    })
}

fn plot_bounds(
    sample: &SampleFrame,
    x_index: usize,
    y_index: usize,
    mask: Option<&BitMask>,
) -> Option<(f64, f64, f64, f64)> {
    let mut x_min = f64::INFINITY;
    let mut x_max = f64::NEG_INFINITY;
    let mut y_min = f64::INFINITY;
    let mut y_max = f64::NEG_INFINITY;
    let mut any = false;

    for (event_index, row) in sample.events().iter().enumerate() {
        if mask.is_some_and(|mask| !mask.contains(event_index)) {
            continue;
        }
        any = true;
        x_min = x_min.min(row[x_index]);
        x_max = x_max.max(row[x_index]);
        y_min = y_min.min(row[y_index]);
        y_max = y_max.max(row[y_index]);
    }

    any.then_some((x_min, x_max, y_min, y_max))
}

fn padded_bounds(min: f64, max: f64, padding_fraction: f64) -> (f64, f64) {
    if min == max {
        return (min - 1.0, max + 1.0);
    }

    let padding = (max - min) * padding_fraction;
    (min - padding, max + padding)
}

fn preferred_scatter_pair(sample: &SampleFrame) -> Option<(&str, &str)> {
    [
        ("FSC-A", "SSC-A"),
        ("FSC", "SSC"),
        ("FSC-H", "SSC-H"),
        ("FSC-A", "SSC-H"),
        ("FSC-H", "SSC-A"),
    ]
    .into_iter()
    .find(|(x_channel, y_channel)| {
        sample.channel_index(x_channel).is_some() && sample.channel_index(y_channel).is_some()
    })
}

fn secondary_pair(sample: &SampleFrame, plots: &[PlotSpec]) -> Option<(String, String)> {
    let primary_channels = plots.first().and_then(|plot| {
        plot.y_channel
            .as_deref()
            .map(|y_channel| (plot.x_channel.as_str(), y_channel))
    });

    let fluorescence_channels = sample
        .channels()
        .iter()
        .filter(|channel| !is_time_channel(channel) && !is_structural_channel(channel))
        .filter(|channel| match primary_channels {
            Some((x_channel, y_channel)) => {
                channel.as_str() != x_channel && channel.as_str() != y_channel
            }
            None => true,
        })
        .cloned()
        .collect::<Vec<_>>();
    if fluorescence_channels.len() >= 2 {
        return Some((
            fluorescence_channels[0].clone(),
            fluorescence_channels[1].clone(),
        ));
    }

    let fallback_channels = sample
        .channels()
        .iter()
        .filter(|channel| !is_time_channel(channel))
        .filter(|channel| match primary_channels {
            Some((x_channel, y_channel)) => {
                channel.as_str() != x_channel && channel.as_str() != y_channel
            }
            None => true,
        })
        .cloned()
        .collect::<Vec<_>>();
    if fallback_channels.len() >= 2 {
        return Some((fallback_channels[0].clone(), fallback_channels[1].clone()));
    }

    None
}

fn histogram_channel(sample: &SampleFrame) -> Option<String> {
    sample
        .channels()
        .iter()
        .find(|channel| !is_time_channel(channel) && !is_structural_channel(channel))
        .cloned()
        .or_else(|| {
            sample
                .channels()
                .iter()
                .find(|channel| !is_time_channel(channel))
                .cloned()
        })
}

fn plot_id_for_channels(x_channel: &str, y_channel: &str) -> String {
    format!(
        "plot_{}_{}",
        sanitize_plot_segment(x_channel),
        sanitize_plot_segment(y_channel)
    )
}

fn plot_id_for_histogram(channel: &str) -> String {
    format!("hist_{}", sanitize_plot_segment(channel))
}

fn sanitize_plot_segment(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut previous_was_underscore = false;
    for ch in value.chars() {
        let lowered = ch.to_ascii_lowercase();
        if lowered.is_ascii_alphanumeric() {
            output.push(lowered);
            previous_was_underscore = false;
            continue;
        }

        if !previous_was_underscore && !output.is_empty() {
            output.push('_');
            previous_was_underscore = true;
        }
    }

    while output.ends_with('_') {
        output.pop();
    }

    if output.is_empty() {
        "plot".to_string()
    } else {
        output
    }
}

fn is_time_channel(channel: &str) -> bool {
    channel.eq_ignore_ascii_case("time") || channel.to_ascii_lowercase().contains("time")
}

fn is_structural_channel(channel: &str) -> bool {
    let lowered = channel.to_ascii_lowercase();
    lowered.starts_with("fsc") || lowered.starts_with("ssc")
}

fn parse_import_paths(file_paths_json: &str) -> Result<Vec<String>, String> {
    let value = JsonValue::parse(file_paths_json).map_err(|error| error.to_string())?;
    let values = value
        .as_array()
        .ok_or_else(|| "file import payload must be a JSON array".to_string())?;

    let mut paths = Vec::with_capacity(values.len());
    for value in values {
        let path = value
            .as_str()
            .ok_or_else(|| "file import payload must contain only strings".to_string())?;
        if !path.trim().is_empty() {
            paths.push(path.to_string());
        }
    }

    if paths.is_empty() {
        return Err("import requires at least one file path".to_string());
    }

    Ok(paths)
}

fn next_sample_id(path: &str, sample_info: &BTreeMap<String, DesktopSampleInfo>) -> String {
    let path = Path::new(path);
    let base = path
        .file_stem()
        .and_then(|value| value.to_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("sample");

    let mut candidate = base.to_string();
    let mut suffix = 2usize;
    while sample_info.contains_key(&candidate) {
        candidate = format!("{base}-{suffix}");
        suffix += 1;
    }
    candidate
}

fn sample_display_name(path: &str) -> String {
    let path = Path::new(path);
    path.file_name()
        .and_then(|value| value.to_str())
        .map(str::to_string)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

fn compensation_matrix_hash(compensation: Option<&CompensationMatrix>) -> u64 {
    let mut hasher = StableHasher::new();
    hasher.update_bool(compensation.is_some());
    if let Some(matrix) = compensation {
        hasher.update_str(&matrix.source_key);
        hasher.update_u64(matrix.dimension as u64);
        hasher.update_u64(matrix.parameter_names.len() as u64);
        for parameter_name in &matrix.parameter_names {
            hasher.update_str(parameter_name);
        }
        hasher.update_u64(matrix.values.len() as u64);
        for value in &matrix.values {
            hasher.update(&value.to_le_bytes());
        }
    }
    hasher.finish_u64()
}

const SCATTER_POINT_LIMIT: usize = 20_000;

fn point_columns_json(
    sample: &SampleFrame,
    x_index: usize,
    y_index: usize,
    event_indices: &[usize],
) -> JsonValue {
    let mut rendered_event_indices = Vec::with_capacity(event_indices.len());
    let mut x_values = Vec::with_capacity(event_indices.len());
    let mut y_values = Vec::with_capacity(event_indices.len());

    for event_index in event_indices {
        let Some(row) = sample.events().get(*event_index) else {
            continue;
        };
        rendered_event_indices.push(JsonValue::Number(*event_index as f64));
        x_values.push(JsonValue::Number(row[x_index]));
        y_values.push(JsonValue::Number(row[y_index]));
    }

    JsonValue::object([
        ("event_indices", JsonValue::Array(rendered_event_indices)),
        ("x_values", JsonValue::Array(x_values)),
        ("y_values", JsonValue::Array(y_values)),
    ])
}

fn sampled_event_indices(event_count: usize, limit: usize) -> Vec<usize> {
    if event_count == 0 {
        return Vec::new();
    }
    if limit == 0 {
        return Vec::new();
    }
    if event_count <= limit {
        return (0..event_count).collect();
    }
    if limit == 1 {
        return vec![0];
    }

    (0..limit)
        .map(|rank| rank * (event_count - 1) / (limit - 1))
        .collect()
}

fn sampled_population_indices_json(sampled_indices: &[usize], mask: &BitMask) -> Vec<JsonValue> {
    sampled_indices
        .iter()
        .copied()
        .filter(|event_index| mask.contains(*event_index))
        .map(|event_index| JsonValue::Number(event_index as f64))
        .collect()
}

fn histogram_bins(
    sample: &SampleFrame,
    channel_index: usize,
    x_min: f64,
    x_max: f64,
    bin_count: usize,
    mask: Option<&BitMask>,
) -> Vec<JsonValue> {
    let width = ((x_max - x_min) / bin_count.max(1) as f64).max(1e-9);
    let mut counts = vec![0u64; bin_count.max(1)];

    for (event_index, row) in sample.events().iter().enumerate() {
        if mask.is_some_and(|mask| !mask.contains(event_index)) {
            continue;
        }
        let value = row[channel_index];
        if value < x_min || value > x_max {
            continue;
        }
        let mut index = if value >= x_max {
            counts.len() - 1
        } else {
            ((value - x_min) / width).floor() as usize
        };
        if index >= counts.len() {
            index = counts.len() - 1;
        }
        counts[index] += 1;
    }

    counts
        .into_iter()
        .enumerate()
        .map(|(index, count)| {
            let start = x_min + (index as f64 * width);
            let end = if index + 1 == bin_count.max(1) {
                x_max
            } else {
                start + width
            };
            JsonValue::object([
                ("x0", JsonValue::Number(start)),
                ("x1", JsonValue::Number(end)),
                ("count", JsonValue::Number(count as f64)),
            ])
        })
        .collect()
}

fn channel_bounds(
    sample: &SampleFrame,
    index: usize,
    mask: Option<&BitMask>,
) -> Option<(f64, f64)> {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    let mut any = false;

    for (event_index, row) in sample.events().iter().enumerate() {
        if mask.is_some_and(|mask| !mask.contains(event_index)) {
            continue;
        }
        any = true;
        min = min.min(row[index]);
        max = max.max(row[index]);
    }

    any.then_some((min, max))
}

fn axis_bounds(sample: &SampleFrame, index: usize) -> (f64, f64) {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for row in sample.events() {
        min = min.min(row[index]);
        max = max.max(row[index]);
    }
    if min == max {
        (min - 1.0, max + 1.0)
    } else {
        let padding = (max - min) * 0.08;
        (min - padding, max + padding)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DesktopSession, SCATTER_POINT_LIMIT, bootstrap_json_string,
        flowjoish_desktop_session_apply_active_template_to_other_samples,
        flowjoish_desktop_session_dispatch_json, flowjoish_desktop_session_export_batch_stats_csv,
        flowjoish_desktop_session_export_population_comparison_csv,
        flowjoish_desktop_session_export_population_derived_metric_csv,
        flowjoish_desktop_session_export_population_group_summary_csv,
        flowjoish_desktop_session_export_stats_csv, flowjoish_desktop_session_free,
        flowjoish_desktop_session_load_workspace, flowjoish_desktop_session_new,
        flowjoish_desktop_session_redo, flowjoish_desktop_session_save_workspace,
        flowjoish_desktop_session_select_sample, flowjoish_desktop_session_set_derived_metric_json,
        flowjoish_desktop_session_set_sample_group_label, flowjoish_desktop_session_undo,
        flowjoish_string_free,
    };
    use std::ffi::{CStr, CString};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use flowjoish_core::JsonValue;

    #[test]
    fn bootstrap_json_is_valid_and_contains_stack() {
        let payload = bootstrap_json_string();
        let parsed = JsonValue::parse(&payload).expect("valid bootstrap json");

        assert_eq!(
            parsed.get("status").and_then(JsonValue::as_str),
            Some("ready")
        );
        assert_eq!(
            parsed
                .get("stack")
                .and_then(|stack| stack.get("desktop"))
                .and_then(JsonValue::as_str),
            Some("Qt/QML")
        );
    }

    #[test]
    fn session_dispatches_commands_and_updates_snapshot() {
        let mut session = DesktopSession::new().expect("session");
        let command = JsonValue::object([
            ("kind", JsonValue::String("rectangle_gate".to_string())),
            ("sample_id", JsonValue::String("desktop-demo".to_string())),
            (
                "population_id",
                JsonValue::String("lymphocytes".to_string()),
            ),
            ("parent_population", JsonValue::Null),
            ("x_channel", JsonValue::String("FSC-A".to_string())),
            ("y_channel", JsonValue::String("SSC-A".to_string())),
            ("x_min", JsonValue::Number(0.0)),
            ("x_max", JsonValue::Number(35.0)),
            ("y_min", JsonValue::Number(0.0)),
            ("y_max", JsonValue::Number(35.0)),
        ])
        .stringify_canonical();

        let snapshot = session.dispatch_json(&command);
        assert_eq!(
            snapshot.get("command_count").and_then(JsonValue::as_u64),
            Some(1)
        );
        assert_eq!(
            snapshot.get("can_undo").and_then(JsonValue::as_bool),
            Some(true)
        );
        assert_eq!(
            snapshot.get("can_redo").and_then(JsonValue::as_bool),
            Some(false)
        );
    }

    #[test]
    fn session_undo_and_redo_update_command_state() {
        let mut session = DesktopSession::new().expect("session");
        let command = JsonValue::object([
            ("kind", JsonValue::String("rectangle_gate".to_string())),
            ("sample_id", JsonValue::String("desktop-demo".to_string())),
            (
                "population_id",
                JsonValue::String("lymphocytes".to_string()),
            ),
            ("parent_population", JsonValue::Null),
            ("x_channel", JsonValue::String("FSC-A".to_string())),
            ("y_channel", JsonValue::String("SSC-A".to_string())),
            ("x_min", JsonValue::Number(0.0)),
            ("x_max", JsonValue::Number(35.0)),
            ("y_min", JsonValue::Number(0.0)),
            ("y_max", JsonValue::Number(35.0)),
        ])
        .stringify_canonical();

        session.dispatch_json(&command);
        let undone = session.undo();
        assert_eq!(
            undone.get("command_count").and_then(JsonValue::as_u64),
            Some(0)
        );
        assert_eq!(
            undone.get("can_undo").and_then(JsonValue::as_bool),
            Some(false)
        );
        assert_eq!(
            undone.get("can_redo").and_then(JsonValue::as_bool),
            Some(true)
        );

        let redone = session.redo();
        assert_eq!(
            redone.get("command_count").and_then(JsonValue::as_u64),
            Some(1)
        );
        assert_eq!(
            redone.get("can_undo").and_then(JsonValue::as_bool),
            Some(true)
        );
        assert_eq!(
            redone.get("can_redo").and_then(JsonValue::as_bool),
            Some(false)
        );
    }

    #[test]
    fn ffi_session_dispatch_round_trips() {
        let session = flowjoish_desktop_session_new();
        assert!(!session.is_null());

        let command = CString::new(
            JsonValue::object([
                ("kind", JsonValue::String("rectangle_gate".to_string())),
                ("sample_id", JsonValue::String("desktop-demo".to_string())),
                (
                    "population_id",
                    JsonValue::String("lymphocytes".to_string()),
                ),
                ("parent_population", JsonValue::Null),
                ("x_channel", JsonValue::String("FSC-A".to_string())),
                ("y_channel", JsonValue::String("SSC-A".to_string())),
                ("x_min", JsonValue::Number(0.0)),
                ("x_max", JsonValue::Number(35.0)),
                ("y_min", JsonValue::Number(0.0)),
                ("y_max", JsonValue::Number(35.0)),
            ])
            .stringify_canonical(),
        )
        .expect("command json");

        let payload = flowjoish_desktop_session_dispatch_json(session, command.as_ptr());
        assert!(!payload.is_null());
        let text = unsafe { CStr::from_ptr(payload) }
            .to_str()
            .expect("utf8 payload")
            .to_string();
        unsafe { flowjoish_string_free(payload) };
        unsafe { flowjoish_desktop_session_free(session) };

        let parsed = JsonValue::parse(&text).expect("json payload");
        assert_eq!(
            parsed.get("command_count").and_then(JsonValue::as_u64),
            Some(1)
        );
    }

    #[test]
    fn ffi_undo_and_redo_round_trip() {
        let session = flowjoish_desktop_session_new();
        assert!(!session.is_null());

        let command = CString::new(
            JsonValue::object([
                ("kind", JsonValue::String("rectangle_gate".to_string())),
                ("sample_id", JsonValue::String("desktop-demo".to_string())),
                (
                    "population_id",
                    JsonValue::String("lymphocytes".to_string()),
                ),
                ("parent_population", JsonValue::Null),
                ("x_channel", JsonValue::String("FSC-A".to_string())),
                ("y_channel", JsonValue::String("SSC-A".to_string())),
                ("x_min", JsonValue::Number(0.0)),
                ("x_max", JsonValue::Number(35.0)),
                ("y_min", JsonValue::Number(0.0)),
                ("y_max", JsonValue::Number(35.0)),
            ])
            .stringify_canonical(),
        )
        .expect("command json");

        let dispatch_payload = flowjoish_desktop_session_dispatch_json(session, command.as_ptr());
        unsafe { flowjoish_string_free(dispatch_payload) };

        let undo_payload = flowjoish_desktop_session_undo(session);
        assert!(!undo_payload.is_null());
        let undo_text = unsafe { CStr::from_ptr(undo_payload) }
            .to_str()
            .expect("undo payload")
            .to_string();
        unsafe { flowjoish_string_free(undo_payload) };

        let redo_payload = flowjoish_desktop_session_redo(session);
        assert!(!redo_payload.is_null());
        let redo_text = unsafe { CStr::from_ptr(redo_payload) }
            .to_str()
            .expect("redo payload")
            .to_string();
        unsafe { flowjoish_string_free(redo_payload) };
        unsafe { flowjoish_desktop_session_free(session) };

        let undone = JsonValue::parse(&undo_text).expect("undo json");
        assert_eq!(
            undone.get("command_count").and_then(JsonValue::as_u64),
            Some(0)
        );
        let redone = JsonValue::parse(&redo_text).expect("redo json");
        assert_eq!(
            redone.get("command_count").and_then(JsonValue::as_u64),
            Some(1)
        );
    }

    #[test]
    fn session_imports_multiple_fcs_files_and_tracks_sample_specific_history() {
        let alpha_path = write_temp_test_fcs(
            "alpha",
            build_test_fcs(
                vec!["FSC-A", "SSC-A", "CD3", "CD4"],
                vec![vec![10.0, 10.0, 1.0, 9.0], vec![25.0, 20.0, 5.0, 8.0]],
                None,
            ),
        );
        let beta_path = write_temp_test_fcs(
            "beta",
            build_test_fcs(
                vec!["TIME", "FSC", "SSC", "FL1", "FL2"],
                vec![
                    vec![1.0, 10.0, 11.0, 100.0, 150.0],
                    vec![2.0, 20.0, 21.0, 200.0, 250.0],
                ],
                None,
            ),
        );

        let mut session = DesktopSession::new().expect("session");
        let import_payload = JsonValue::Array(vec![
            JsonValue::String(alpha_path.to_string_lossy().to_string()),
            JsonValue::String(beta_path.to_string_lossy().to_string()),
        ])
        .stringify_canonical();
        let imported = session.import_fcs_json(&import_payload);
        assert_eq!(
            imported.get("status").and_then(JsonValue::as_str),
            Some("ready")
        );
        assert_eq!(
            imported
                .get("sample")
                .and_then(|sample| sample.get("id"))
                .and_then(JsonValue::as_str),
            Some("alpha")
        );
        assert_eq!(
            imported
                .get("samples")
                .and_then(JsonValue::as_array)
                .map(|samples| samples.len()),
            Some(2)
        );

        let command = JsonValue::object([
            ("kind", JsonValue::String("rectangle_gate".to_string())),
            ("sample_id", JsonValue::String("alpha".to_string())),
            (
                "population_id",
                JsonValue::String("lymphocytes".to_string()),
            ),
            ("parent_population", JsonValue::Null),
            ("x_channel", JsonValue::String("FSC-A".to_string())),
            ("y_channel", JsonValue::String("SSC-A".to_string())),
            ("x_min", JsonValue::Number(0.0)),
            ("x_max", JsonValue::Number(30.0)),
            ("y_min", JsonValue::Number(0.0)),
            ("y_max", JsonValue::Number(30.0)),
        ])
        .stringify_canonical();
        let gated = session.dispatch_json(&command);
        assert_eq!(
            gated.get("command_count").and_then(JsonValue::as_u64),
            Some(1)
        );

        let beta_snapshot = session.select_sample("beta");
        assert_eq!(
            beta_snapshot
                .get("sample")
                .and_then(|sample| sample.get("id"))
                .and_then(JsonValue::as_str),
            Some("beta")
        );
        assert_eq!(
            beta_snapshot
                .get("command_count")
                .and_then(JsonValue::as_u64),
            Some(0)
        );
        let beta_plots = beta_snapshot
            .get("plots")
            .and_then(JsonValue::as_array)
            .expect("beta plots");
        assert_eq!(
            beta_plots
                .get(0)
                .and_then(|plot| plot.get("title"))
                .and_then(JsonValue::as_str),
            Some("FSC vs SSC")
        );
        assert_eq!(
            beta_plots
                .get(1)
                .and_then(|plot| plot.get("title"))
                .and_then(JsonValue::as_str),
            Some("FL1 vs FL2")
        );
        assert_eq!(beta_plots.len(), 3);
        assert_eq!(
            beta_plots
                .get(2)
                .and_then(|plot| plot.get("kind"))
                .and_then(JsonValue::as_str),
            Some("histogram")
        );
        assert_eq!(
            beta_plots
                .get(2)
                .and_then(|plot| plot.get("title"))
                .and_then(JsonValue::as_str),
            Some("FL1 histogram")
        );

        let alpha_snapshot = session.select_sample("alpha");
        assert_eq!(
            alpha_snapshot
                .get("command_count")
                .and_then(JsonValue::as_u64),
            Some(1)
        );

        let _ = fs::remove_file(alpha_path);
        let _ = fs::remove_file(beta_path);
    }

    #[test]
    fn ffi_can_select_imported_sample() {
        let alpha_path = write_temp_test_fcs(
            "ffi-alpha",
            build_test_fcs(
                vec!["FSC-A", "SSC-A"],
                vec![vec![10.0, 10.0], vec![20.0, 20.0]],
                None,
            ),
        );
        let beta_path = write_temp_test_fcs(
            "ffi-beta",
            build_test_fcs(
                vec!["FSC", "SSC"],
                vec![vec![1.0, 2.0], vec![3.0, 4.0]],
                None,
            ),
        );

        let session = flowjoish_desktop_session_new();
        assert!(!session.is_null());

        let import_payload = CString::new(
            JsonValue::Array(vec![
                JsonValue::String(alpha_path.to_string_lossy().to_string()),
                JsonValue::String(beta_path.to_string_lossy().to_string()),
            ])
            .stringify_canonical(),
        )
        .expect("import payload");

        let import_result =
            super::flowjoish_desktop_session_import_fcs_json(session, import_payload.as_ptr());
        unsafe { flowjoish_string_free(import_result) };

        let sample_id = CString::new("ffi-beta").expect("sample id");
        let payload = flowjoish_desktop_session_select_sample(session, sample_id.as_ptr());
        assert!(!payload.is_null());
        let text = unsafe { CStr::from_ptr(payload) }
            .to_str()
            .expect("utf8 payload")
            .to_string();
        unsafe { flowjoish_string_free(payload) };
        unsafe { flowjoish_desktop_session_free(session) };

        let _ = fs::remove_file(alpha_path);
        let _ = fs::remove_file(beta_path);

        let parsed = JsonValue::parse(&text).expect("json payload");
        assert_eq!(
            parsed
                .get("sample")
                .and_then(|sample| sample.get("id"))
                .and_then(JsonValue::as_str),
            Some("ffi-beta")
        );
    }

    #[test]
    fn session_applies_compensation_and_transforms_as_explicit_analysis_actions() {
        let sample_path = write_temp_test_fcs(
            "analysis-sample",
            build_test_fcs(
                vec!["FL1", "FL2", "SSC-A"],
                vec![vec![110.0, 70.0, 10.0], vec![220.0, 140.0, 20.0]],
                Some(("$SPILLOVER", "2,FL1,FL2,1,0.2,0,1")),
            ),
        );

        let mut session = DesktopSession::new().expect("session");
        let import_payload = JsonValue::Array(vec![JsonValue::String(
            sample_path.to_string_lossy().to_string(),
        )])
        .stringify_canonical();
        let imported = session.import_fcs_json(&import_payload);
        assert_eq!(
            imported.get("status").and_then(JsonValue::as_str),
            Some("ready")
        );

        let compensation_payload = JsonValue::object([
            (
                "kind",
                JsonValue::String("set_compensation_enabled".to_string()),
            ),
            (
                "sample_id",
                JsonValue::String("analysis-sample".to_string()),
            ),
            ("enabled", JsonValue::Bool(true)),
        ])
        .stringify_canonical();
        let compensated = session.dispatch_json(&compensation_payload);
        assert_eq!(
            compensated
                .get("sample")
                .and_then(|sample| sample.get("compensation_enabled"))
                .and_then(JsonValue::as_bool),
            Some(true)
        );
        assert_eq!(
            compensated
                .get("analysis_action_count")
                .and_then(JsonValue::as_u64),
            Some(1)
        );

        let transform_payload = JsonValue::object([
            (
                "kind",
                JsonValue::String("set_channel_transform".to_string()),
            ),
            (
                "sample_id",
                JsonValue::String("analysis-sample".to_string()),
            ),
            ("channel", JsonValue::String("FL1".to_string())),
            (
                "transform",
                JsonValue::object([("kind", JsonValue::String("signed_log10".to_string()))]),
            ),
        ])
        .stringify_canonical();
        let transformed = session.dispatch_json(&transform_payload);
        let channel_transforms = transformed
            .get("sample")
            .and_then(|sample| sample.get("channel_transforms"))
            .and_then(JsonValue::as_array)
            .expect("channel transforms");
        assert_eq!(
            channel_transforms
                .iter()
                .find(|entry| entry.get("channel").and_then(JsonValue::as_str) == Some("FL1"))
                .and_then(|entry| entry.get("kind"))
                .and_then(JsonValue::as_str),
            Some("signed_log10")
        );
        assert_eq!(
            transformed
                .get("analysis_action_count")
                .and_then(JsonValue::as_u64),
            Some(2)
        );

        let _ = fs::remove_file(sample_path);
    }

    #[test]
    fn session_replays_plot_view_actions_and_falls_back_when_population_disappears() {
        let sample_path = write_temp_test_fcs(
            "view-sample",
            build_test_fcs(
                vec!["FSC-A", "SSC-A", "CD3", "CD4"],
                vec![
                    vec![10.0, 10.0, 1.0, 9.0],
                    vec![25.0, 20.0, 5.0, 8.0],
                    vec![100.0, 100.0, 2.0, 3.0],
                ],
                None,
            ),
        );

        let mut session = DesktopSession::new().expect("session");
        let import_payload = JsonValue::Array(vec![JsonValue::String(
            sample_path.to_string_lossy().to_string(),
        )])
        .stringify_canonical();
        let imported = session.import_fcs_json(&import_payload);
        assert_eq!(
            imported.get("status").and_then(JsonValue::as_str),
            Some("ready")
        );
        let auto_x_max = imported
            .get("plots")
            .and_then(JsonValue::as_array)
            .and_then(|plots| plots.first())
            .and_then(|plot| plot.get("x_range"))
            .and_then(|range| range.get("max"))
            .and_then(JsonValue::as_f64)
            .expect("auto x max");

        let gate = JsonValue::object([
            ("kind", JsonValue::String("rectangle_gate".to_string())),
            ("sample_id", JsonValue::String("view-sample".to_string())),
            (
                "population_id",
                JsonValue::String("lymphocytes".to_string()),
            ),
            ("parent_population", JsonValue::Null),
            ("x_channel", JsonValue::String("FSC-A".to_string())),
            ("y_channel", JsonValue::String("SSC-A".to_string())),
            ("x_min", JsonValue::Number(0.0)),
            ("x_max", JsonValue::Number(40.0)),
            ("y_min", JsonValue::Number(0.0)),
            ("y_max", JsonValue::Number(40.0)),
        ])
        .stringify_canonical();
        let gated = session.dispatch_json(&gate);
        let comparison_hash_after_gate = gated
            .get("comparison_state_hash")
            .and_then(JsonValue::as_str)
            .expect("comparison hash after gate")
            .to_string();

        let focus_payload = JsonValue::object([
            (
                "kind",
                JsonValue::String("focus_plot_population".to_string()),
            ),
            ("sample_id", JsonValue::String("view-sample".to_string())),
            ("plot_id", JsonValue::String("plot_fsc_a_ssc_a".to_string())),
            (
                "population_id",
                JsonValue::String("lymphocytes".to_string()),
            ),
            ("padding_fraction", JsonValue::Number(0.08)),
        ])
        .stringify_canonical();
        let focused = session.dispatch_json(&focus_payload);
        assert_eq!(
            focused
                .get("comparison_state_hash")
                .and_then(JsonValue::as_str),
            Some(comparison_hash_after_gate.as_str())
        );
        assert_eq!(
            focused
                .get("plots")
                .and_then(JsonValue::as_array)
                .and_then(|plots| plots.first())
                .and_then(|plot| plot.get("view_summary"))
                .and_then(JsonValue::as_str),
            Some("Focused on lymphocytes")
        );
        let focused_x_max = focused
            .get("plots")
            .and_then(JsonValue::as_array)
            .and_then(|plots| plots.first())
            .and_then(|plot| plot.get("x_range"))
            .and_then(|range| range.get("max"))
            .and_then(JsonValue::as_f64)
            .expect("focused x max");
        assert!(focused_x_max < auto_x_max);

        let after_undo = session.undo();
        assert_eq!(
            after_undo.get("status").and_then(JsonValue::as_str),
            Some("ready")
        );
        assert_eq!(
            after_undo
                .get("plots")
                .and_then(JsonValue::as_array)
                .and_then(|plots| plots.first())
                .and_then(|plot| plot.get("view_summary"))
                .and_then(JsonValue::as_str),
            Some("Auto extents (lymphocytes unavailable)")
        );

        let zoom_payload = JsonValue::object([
            ("kind", JsonValue::String("scale_plot_view".to_string())),
            ("sample_id", JsonValue::String("view-sample".to_string())),
            ("plot_id", JsonValue::String("plot_fsc_a_ssc_a".to_string())),
            ("factor", JsonValue::Number(1.4)),
        ])
        .stringify_canonical();
        let zoomed = session.dispatch_json(&zoom_payload);
        assert_eq!(
            zoomed
                .get("plots")
                .and_then(JsonValue::as_array)
                .and_then(|plots| plots.first())
                .and_then(|plot| plot.get("view_summary"))
                .and_then(JsonValue::as_str),
            Some("Zoomed out (1.40x)")
        );

        let _ = fs::remove_file(sample_path);
    }

    #[test]
    fn comparison_state_hash_tracks_raw_data_and_compensation() {
        let sample_path = write_temp_test_fcs(
            "identity-sample",
            build_test_fcs(
                vec!["FSC-A", "SSC-A", "FL1", "FL2"],
                vec![vec![10.0, 10.0, 100.0, 150.0]],
                None,
            ),
        );
        let import_payload = JsonValue::Array(vec![JsonValue::String(
            sample_path.to_string_lossy().to_string(),
        )])
        .stringify_canonical();

        let mut first_session = DesktopSession::new().expect("session");
        let first = first_session.import_fcs_json(&import_payload);
        let first_hash = first
            .get("comparison_state_hash")
            .and_then(JsonValue::as_str)
            .expect("first comparison hash")
            .to_string();

        fs::write(
            &sample_path,
            build_test_fcs(
                vec!["FSC-A", "SSC-A", "FL1", "FL2"],
                vec![vec![20.0, 10.0, 100.0, 150.0]],
                None,
            ),
        )
        .expect("rewrite sample data");
        let mut data_changed_session = DesktopSession::new().expect("session");
        let data_changed = data_changed_session.import_fcs_json(&import_payload);
        let data_changed_hash = data_changed
            .get("comparison_state_hash")
            .and_then(JsonValue::as_str)
            .expect("data-changed comparison hash");
        assert_ne!(first_hash, data_changed_hash);

        fs::write(
            &sample_path,
            build_test_fcs(
                vec!["FSC-A", "SSC-A", "FL1", "FL2"],
                vec![vec![10.0, 10.0, 100.0, 150.0]],
                Some(("$SPILLOVER", "2,FL1,FL2,1,0.2,0,1")),
            ),
        )
        .expect("rewrite compensation metadata");
        let mut compensation_changed_session = DesktopSession::new().expect("session");
        let compensation_changed = compensation_changed_session.import_fcs_json(&import_payload);
        let compensation_changed_hash = compensation_changed
            .get("comparison_state_hash")
            .and_then(JsonValue::as_str)
            .expect("compensation-changed comparison hash");
        assert_ne!(first_hash, compensation_changed_hash);

        let _ = fs::remove_file(sample_path);
    }

    #[test]
    fn scatter_snapshots_are_bounded_and_use_population_index_sets() {
        let rows = (0..(SCATTER_POINT_LIMIT + 10))
            .map(|index| vec![index as f64, (index % 100) as f64])
            .collect::<Vec<_>>();
        let sample_path = write_temp_test_fcs(
            "bounded-scatter",
            build_test_fcs(vec!["FSC-A", "SSC-A"], rows, None),
        );
        let import_payload = JsonValue::Array(vec![JsonValue::String(
            sample_path.to_string_lossy().to_string(),
        )])
        .stringify_canonical();

        let mut session = DesktopSession::new().expect("session");
        let imported = session.import_fcs_json(&import_payload);
        let plot = imported
            .get("plots")
            .and_then(JsonValue::as_array)
            .and_then(|plots| plots.first())
            .expect("scatter plot");
        assert_eq!(
            plot.get("total_event_count").and_then(JsonValue::as_u64),
            Some((SCATTER_POINT_LIMIT + 10) as u64)
        );
        assert!(
            plot.get("rendered_event_count")
                .and_then(JsonValue::as_u64)
                .is_some_and(|count| count == SCATTER_POINT_LIMIT as u64)
        );
        assert_eq!(
            plot.get("decimated").and_then(JsonValue::as_bool),
            Some(true)
        );
        assert_eq!(
            plot.get("point_columns")
                .and_then(|columns| columns.get("event_indices"))
                .and_then(JsonValue::as_array)
                .and_then(|indices| indices.first())
                .and_then(JsonValue::as_u64),
            Some(0)
        );

        let gate = JsonValue::object([
            ("kind", JsonValue::String("rectangle_gate".to_string())),
            (
                "sample_id",
                JsonValue::String("bounded-scatter".to_string()),
            ),
            (
                "population_id",
                JsonValue::String("early-events".to_string()),
            ),
            ("parent_population", JsonValue::Null),
            ("x_channel", JsonValue::String("FSC-A".to_string())),
            ("y_channel", JsonValue::String("SSC-A".to_string())),
            ("x_min", JsonValue::Number(0.0)),
            ("x_max", JsonValue::Number(200.0)),
            ("y_min", JsonValue::Number(0.0)),
            ("y_max", JsonValue::Number(100.0)),
        ])
        .stringify_canonical();
        let gated = session.dispatch_json(&gate);
        let gated_plot = gated
            .get("plots")
            .and_then(JsonValue::as_array)
            .and_then(|plots| plots.first())
            .expect("gated scatter plot");
        assert!(gated_plot.get("all_points").is_none());
        assert!(gated_plot.get("population_points").is_none());
        let sampled_population_indices = gated_plot
            .get("population_event_indices")
            .and_then(|value| value.get("early-events"))
            .and_then(JsonValue::as_array)
            .expect("sampled population indices");
        assert!(!sampled_population_indices.is_empty());

        let _ = fs::remove_file(sample_path);
    }

    #[test]
    fn session_save_and_load_workspace_round_trips_sample_state() {
        let alpha_path = write_temp_test_fcs(
            "save-alpha",
            build_test_fcs(
                vec!["FSC-A", "SSC-A", "CD3", "CD4"],
                vec![vec![10.0, 10.0, 1.0, 9.0], vec![25.0, 20.0, 5.0, 8.0]],
                None,
            ),
        );
        let beta_path = write_temp_test_fcs(
            "save-beta",
            build_test_fcs(
                vec!["FSC", "SSC", "FL1", "FL2"],
                vec![
                    vec![11.0, 12.0, 100.0, 150.0],
                    vec![21.0, 22.0, 200.0, 250.0],
                ],
                Some(("$SPILLOVER", "2,FL1,FL2,1,0.2,0,1")),
            ),
        );
        let workspace_path = temp_workspace_path("round-trip");

        let mut session = DesktopSession::new().expect("session");
        let import_payload = JsonValue::Array(vec![
            JsonValue::String(alpha_path.to_string_lossy().to_string()),
            JsonValue::String(beta_path.to_string_lossy().to_string()),
        ])
        .stringify_canonical();
        let imported = session.import_fcs_json(&import_payload);
        assert_eq!(
            imported.get("status").and_then(JsonValue::as_str),
            Some("ready")
        );

        let alpha_gate = JsonValue::object([
            ("kind", JsonValue::String("rectangle_gate".to_string())),
            ("sample_id", JsonValue::String("save-alpha".to_string())),
            (
                "population_id",
                JsonValue::String("lymphocytes".to_string()),
            ),
            ("parent_population", JsonValue::Null),
            ("x_channel", JsonValue::String("FSC-A".to_string())),
            ("y_channel", JsonValue::String("SSC-A".to_string())),
            ("x_min", JsonValue::Number(0.0)),
            ("x_max", JsonValue::Number(30.0)),
            ("y_min", JsonValue::Number(0.0)),
            ("y_max", JsonValue::Number(30.0)),
        ])
        .stringify_canonical();
        let _ = session.dispatch_json(&alpha_gate);

        let _ = session.select_sample("save-beta");
        let beta_gate = JsonValue::object([
            ("kind", JsonValue::String("rectangle_gate".to_string())),
            ("sample_id", JsonValue::String("save-beta".to_string())),
            ("population_id", JsonValue::String("beta_gate".to_string())),
            ("parent_population", JsonValue::Null),
            ("x_channel", JsonValue::String("FSC".to_string())),
            ("y_channel", JsonValue::String("SSC".to_string())),
            ("x_min", JsonValue::Number(0.0)),
            ("x_max", JsonValue::Number(40.0)),
            ("y_min", JsonValue::Number(0.0)),
            ("y_max", JsonValue::Number(40.0)),
        ])
        .stringify_canonical();
        let _ = session.dispatch_json(&beta_gate);
        let compensation_payload = JsonValue::object([
            (
                "kind",
                JsonValue::String("set_compensation_enabled".to_string()),
            ),
            ("sample_id", JsonValue::String("save-beta".to_string())),
            ("enabled", JsonValue::Bool(true)),
        ])
        .stringify_canonical();
        let _ = session.dispatch_json(&compensation_payload);
        let transform_payload = JsonValue::object([
            (
                "kind",
                JsonValue::String("set_channel_transform".to_string()),
            ),
            ("sample_id", JsonValue::String("save-beta".to_string())),
            ("channel", JsonValue::String("FL1".to_string())),
            (
                "transform",
                JsonValue::object([("kind", JsonValue::String("signed_log10".to_string()))]),
            ),
        ])
        .stringify_canonical();
        let _ = session.dispatch_json(&transform_payload);
        let view_payload = JsonValue::object([
            ("kind", JsonValue::String("scale_plot_view".to_string())),
            ("sample_id", JsonValue::String("save-beta".to_string())),
            ("plot_id", JsonValue::String("plot_fsc_ssc".to_string())),
            ("factor", JsonValue::Number(0.7)),
        ])
        .stringify_canonical();
        let viewed = session.dispatch_json(&view_payload);
        let _ = session.set_sample_group_label("save-alpha", "Control");
        let _ = session.set_sample_group_label("save-beta", "Treated");
        let derived_metric_payload = JsonValue::object([
            ("kind", JsonValue::String("mean_ratio".to_string())),
            ("numerator_channel", JsonValue::String("FL2".to_string())),
            ("denominator_channel", JsonValue::String("FL1".to_string())),
        ])
        .stringify_canonical();
        let derived_metric = session.set_derived_metric_from_json(&derived_metric_payload);
        assert_eq!(
            derived_metric.get("status").and_then(JsonValue::as_str),
            Some("ready")
        );
        assert_eq!(
            viewed
                .get("plots")
                .and_then(JsonValue::as_array)
                .and_then(|plots| plots.first())
                .and_then(|plot| plot.get("view_summary"))
                .and_then(JsonValue::as_str),
            Some("Zoomed in (0.70x)")
        );
        let beta_after_undo = session.undo();
        assert_eq!(
            beta_after_undo.get("can_redo").and_then(JsonValue::as_bool),
            Some(true)
        );

        let saved = session.save_workspace(workspace_path.to_string_lossy().as_ref());
        assert_eq!(
            saved.get("status").and_then(JsonValue::as_str),
            Some("ready")
        );

        let mut reopened = DesktopSession::new().expect("session");
        let loaded = reopened.load_workspace(workspace_path.to_string_lossy().as_ref());
        assert_eq!(
            loaded.get("status").and_then(JsonValue::as_str),
            Some("ready")
        );
        assert_eq!(
            loaded
                .get("sample")
                .and_then(|sample| sample.get("id"))
                .and_then(JsonValue::as_str),
            Some("save-beta")
        );
        assert_eq!(
            loaded
                .get("sample")
                .and_then(|sample| sample.get("group_label"))
                .and_then(JsonValue::as_str),
            Some("Treated")
        );
        assert_eq!(
            loaded
                .get("sample")
                .and_then(|sample| sample.get("compensation_enabled"))
                .and_then(JsonValue::as_bool),
            Some(true)
        );
        assert_eq!(
            loaded
                .get("derived_metric")
                .and_then(|metric| metric.get("kind"))
                .and_then(JsonValue::as_str),
            Some("mean_ratio")
        );
        assert_eq!(
            loaded
                .get("derived_metric")
                .and_then(|metric| metric.get("numerator_channel"))
                .and_then(JsonValue::as_str),
            Some("FL2")
        );
        assert_eq!(
            loaded
                .get("derived_metric")
                .and_then(|metric| metric.get("denominator_channel"))
                .and_then(JsonValue::as_str),
            Some("FL1")
        );
        assert_eq!(
            loaded
                .get("plots")
                .and_then(JsonValue::as_array)
                .and_then(|plots| plots.first())
                .and_then(|plot| plot.get("view_summary"))
                .and_then(JsonValue::as_str),
            Some("Zoomed in (0.70x)")
        );
        assert_eq!(
            loaded.get("command_count").and_then(JsonValue::as_u64),
            Some(0)
        );
        assert_eq!(
            loaded.get("can_redo").and_then(JsonValue::as_bool),
            Some(true)
        );

        let alpha_snapshot = reopened.select_sample("save-alpha");
        assert_eq!(
            alpha_snapshot
                .get("command_count")
                .and_then(JsonValue::as_u64),
            Some(1)
        );
        assert_eq!(
            alpha_snapshot
                .get("sample")
                .and_then(|sample| sample.get("group_label"))
                .and_then(JsonValue::as_str),
            Some("Control")
        );
        let beta_snapshot = reopened.select_sample("save-beta");
        assert_eq!(
            beta_snapshot.get("can_redo").and_then(JsonValue::as_bool),
            Some(true)
        );
        let redone = reopened.redo();
        assert_eq!(
            redone.get("command_count").and_then(JsonValue::as_u64),
            Some(1)
        );

        let _ = fs::remove_file(alpha_path);
        let _ = fs::remove_file(beta_path);
        let _ = fs::remove_file(workspace_path);
    }

    #[test]
    fn ffi_can_save_and_load_workspace() {
        let workspace_path = temp_workspace_path("ffi-workspace");
        let session = flowjoish_desktop_session_new();
        assert!(!session.is_null());

        let save_path =
            CString::new(workspace_path.to_string_lossy().to_string()).expect("workspace path");
        let save_payload = flowjoish_desktop_session_save_workspace(session, save_path.as_ptr());
        unsafe { flowjoish_string_free(save_payload) };

        let load_payload = flowjoish_desktop_session_load_workspace(session, save_path.as_ptr());
        assert!(!load_payload.is_null());
        let text = unsafe { CStr::from_ptr(load_payload) }
            .to_str()
            .expect("utf8 payload")
            .to_string();
        unsafe { flowjoish_string_free(load_payload) };
        unsafe { flowjoish_desktop_session_free(session) };

        let _ = fs::remove_file(workspace_path);

        let parsed = JsonValue::parse(&text).expect("json payload");
        assert_eq!(
            parsed.get("status").and_then(JsonValue::as_str),
            Some("ready")
        );
        assert_eq!(
            parsed
                .get("sample")
                .and_then(|sample| sample.get("id"))
                .and_then(JsonValue::as_str),
            Some("desktop-demo")
        );
    }

    #[test]
    fn snapshot_includes_population_stats_for_selected_sample() {
        let mut session = DesktopSession::new().expect("session");
        let command = JsonValue::object([
            ("kind", JsonValue::String("rectangle_gate".to_string())),
            ("sample_id", JsonValue::String("desktop-demo".to_string())),
            (
                "population_id",
                JsonValue::String("lymphocytes".to_string()),
            ),
            ("parent_population", JsonValue::Null),
            ("x_channel", JsonValue::String("FSC-A".to_string())),
            ("y_channel", JsonValue::String("SSC-A".to_string())),
            ("x_min", JsonValue::Number(0.0)),
            ("x_max", JsonValue::Number(35.0)),
            ("y_min", JsonValue::Number(0.0)),
            ("y_max", JsonValue::Number(35.0)),
        ])
        .stringify_canonical();

        let snapshot = session.dispatch_json(&command);
        let stats = snapshot
            .get("population_stats")
            .and_then(|value| value.get("lymphocytes"))
            .expect("population stats");
        assert_eq!(
            stats.get("matched_events").and_then(JsonValue::as_u64),
            Some(3)
        );
        assert_eq!(
            stats.get("parent_events").and_then(JsonValue::as_u64),
            Some(4)
        );
        assert_eq!(
            stats
                .get("channel_stats")
                .and_then(JsonValue::as_array)
                .map(|values| values.len()),
            Some(4)
        );
    }

    #[test]
    fn ffi_can_export_population_stats_csv() {
        let export_path = temp_stats_export_path("ffi-stats");
        let session = flowjoish_desktop_session_new();
        assert!(!session.is_null());

        let export_path_c =
            CString::new(export_path.to_string_lossy().to_string()).expect("export path");
        let export_payload =
            flowjoish_desktop_session_export_stats_csv(session, export_path_c.as_ptr());
        assert!(!export_payload.is_null());
        unsafe { flowjoish_string_free(export_payload) };
        unsafe { flowjoish_desktop_session_free(session) };

        let exported = fs::read_to_string(&export_path).expect("read stats export");
        assert!(exported.starts_with(
            "sample_id,population_key,population_id,parent_population,matched_events,parent_events,frequency_of_all,frequency_of_parent,channel,mean,median\n"
        ));
        assert!(exported.contains("desktop-demo,__all__,All Events,"));
        assert!(exported.contains(",FSC-A,"));

        let _ = fs::remove_file(export_path);
    }

    #[test]
    fn session_applies_active_template_to_other_samples() {
        let alpha_path = write_temp_test_fcs(
            "batch-alpha",
            build_test_fcs(
                vec!["FSC-A", "SSC-A", "CD3", "CD4"],
                vec![vec![10.0, 10.0, 1.0, 9.0], vec![25.0, 20.0, 5.0, 8.0]],
                None,
            ),
        );
        let beta_path = write_temp_test_fcs(
            "batch-beta",
            build_test_fcs(
                vec!["FSC-A", "SSC-A", "CD3", "CD4"],
                vec![vec![12.0, 11.0, 2.0, 8.5], vec![24.0, 19.0, 4.5, 7.5]],
                None,
            ),
        );

        let mut session = DesktopSession::new().expect("session");
        let import_payload = JsonValue::Array(vec![
            JsonValue::String(alpha_path.to_string_lossy().to_string()),
            JsonValue::String(beta_path.to_string_lossy().to_string()),
        ])
        .stringify_canonical();
        let imported = session.import_fcs_json(&import_payload);
        assert_eq!(
            imported.get("status").and_then(JsonValue::as_str),
            Some("ready")
        );

        let gate = JsonValue::object([
            ("kind", JsonValue::String("rectangle_gate".to_string())),
            ("sample_id", JsonValue::String("batch-alpha".to_string())),
            (
                "population_id",
                JsonValue::String("lymphocytes".to_string()),
            ),
            ("parent_population", JsonValue::Null),
            ("x_channel", JsonValue::String("FSC-A".to_string())),
            ("y_channel", JsonValue::String("SSC-A".to_string())),
            ("x_min", JsonValue::Number(0.0)),
            ("x_max", JsonValue::Number(30.0)),
            ("y_min", JsonValue::Number(0.0)),
            ("y_max", JsonValue::Number(30.0)),
        ])
        .stringify_canonical();
        let _ = session.dispatch_json(&gate);

        let applied = session.apply_active_template_to_other_samples();
        assert_eq!(
            applied.get("status").and_then(JsonValue::as_str),
            Some("ready")
        );

        let beta_snapshot = session.select_sample("batch-beta");
        assert_eq!(
            beta_snapshot
                .get("command_count")
                .and_then(JsonValue::as_u64),
            Some(1)
        );
        assert!(
            beta_snapshot
                .get("population_stats")
                .and_then(|value| value.get("lymphocytes"))
                .is_some()
        );

        let _ = fs::remove_file(alpha_path);
        let _ = fs::remove_file(beta_path);
    }

    #[test]
    fn population_comparison_reports_available_and_missing_samples() {
        let alpha_path = write_temp_test_fcs(
            "compare-alpha",
            build_test_fcs(
                vec!["FSC-A", "SSC-A", "CD3", "CD4"],
                vec![vec![10.0, 10.0, 1.0, 9.0], vec![25.0, 20.0, 5.0, 8.0]],
                None,
            ),
        );
        let beta_path = write_temp_test_fcs(
            "compare-beta",
            build_test_fcs(
                vec!["FSC-A", "SSC-A", "CD3", "CD4"],
                vec![vec![12.0, 11.0, 2.0, 8.5], vec![24.0, 19.0, 4.5, 7.5]],
                None,
            ),
        );

        let mut session = DesktopSession::new().expect("session");
        let import_payload = JsonValue::Array(vec![
            JsonValue::String(alpha_path.to_string_lossy().to_string()),
            JsonValue::String(beta_path.to_string_lossy().to_string()),
        ])
        .stringify_canonical();
        let imported = session.import_fcs_json(&import_payload);
        assert_eq!(
            imported.get("status").and_then(JsonValue::as_str),
            Some("ready")
        );

        let gate = JsonValue::object([
            ("kind", JsonValue::String("rectangle_gate".to_string())),
            ("sample_id", JsonValue::String("compare-alpha".to_string())),
            (
                "population_id",
                JsonValue::String("lymphocytes".to_string()),
            ),
            ("parent_population", JsonValue::Null),
            ("x_channel", JsonValue::String("FSC-A".to_string())),
            ("y_channel", JsonValue::String("SSC-A".to_string())),
            ("x_min", JsonValue::Number(0.0)),
            ("x_max", JsonValue::Number(30.0)),
            ("y_min", JsonValue::Number(0.0)),
            ("y_max", JsonValue::Number(30.0)),
        ])
        .stringify_canonical();
        let _ = session.dispatch_json(&gate);
        let _ = session.set_sample_group_label("compare-alpha", "Control");
        let _ = session.set_sample_group_label("compare-beta", "Treated");
        let derived_metric_payload = JsonValue::object([
            ("kind", JsonValue::String("positive_fraction".to_string())),
            ("channel", JsonValue::String("CD3".to_string())),
            ("threshold", JsonValue::Number(1.5)),
        ])
        .stringify_canonical();
        let _ = session.set_derived_metric_from_json(&derived_metric_payload);

        let comparison = session.population_comparison("lymphocytes");
        assert_eq!(
            comparison.get("status").and_then(JsonValue::as_str),
            Some("ready")
        );
        assert_eq!(
            comparison
                .get("population_comparison")
                .and_then(|value| value.get("available_sample_count"))
                .and_then(JsonValue::as_u64),
            Some(1)
        );
        assert_eq!(
            comparison
                .get("population_comparison")
                .and_then(|value| value.get("missing_sample_count"))
                .and_then(JsonValue::as_u64),
            Some(1)
        );
        let rows = comparison
            .get("population_comparison")
            .and_then(|value| value.get("samples"))
            .and_then(JsonValue::as_array)
            .expect("comparison rows");
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[0].get("status").and_then(JsonValue::as_str),
            Some("available")
        );
        assert_eq!(
            rows[0]
                .get("derived_metric_status")
                .and_then(JsonValue::as_str),
            Some("available")
        );
        assert_eq!(
            rows[0]
                .get("derived_metric_value")
                .and_then(JsonValue::as_f64),
            Some(0.5)
        );
        assert_eq!(
            rows[1].get("status").and_then(JsonValue::as_str),
            Some("missing")
        );
        assert_eq!(
            rows[1]
                .get("derived_metric_status")
                .and_then(JsonValue::as_str),
            Some("missing_population")
        );
        let group_summaries = comparison
            .get("population_comparison")
            .and_then(|value| value.get("group_summaries"))
            .and_then(JsonValue::as_array)
            .expect("group summaries");
        assert_eq!(group_summaries.len(), 2);
        assert_eq!(
            group_summaries[0]
                .get("group_label")
                .and_then(JsonValue::as_str),
            Some("Control")
        );
        assert_eq!(
            group_summaries[0]
                .get("is_active_group")
                .and_then(JsonValue::as_bool),
            Some(true)
        );
        assert_eq!(
            group_summaries[0]
                .get("derived_metric_available_sample_count")
                .and_then(JsonValue::as_u64),
            Some(1)
        );
        assert_eq!(
            group_summaries[0]
                .get("derived_metric_unavailable_sample_count")
                .and_then(JsonValue::as_u64),
            Some(0)
        );

        let _ = session.apply_active_template_to_other_samples();
        let applied_comparison = session.population_comparison("lymphocytes");
        assert_eq!(
            applied_comparison
                .get("population_comparison")
                .and_then(|value| value.get("available_sample_count"))
                .and_then(JsonValue::as_u64),
            Some(2)
        );
        assert_eq!(
            applied_comparison
                .get("population_comparison")
                .and_then(|value| value.get("group_summaries"))
                .and_then(JsonValue::as_array)
                .and_then(|rows| rows.get(1))
                .and_then(|row| row.get("mean_frequency_of_all"))
                .and_then(JsonValue::as_f64),
            Some(1.0)
        );
        assert_eq!(
            applied_comparison
                .get("population_comparison")
                .and_then(|value| value.get("group_summaries"))
                .and_then(JsonValue::as_array)
                .and_then(|rows| rows.get(1))
                .and_then(|row| row.get("mean_derived_metric_value"))
                .and_then(JsonValue::as_f64),
            Some(1.0)
        );
        assert_eq!(
            applied_comparison
                .get("population_comparison")
                .and_then(|value| value.get("group_summaries"))
                .and_then(JsonValue::as_array)
                .and_then(|rows| rows.get(1))
                .and_then(|row| row.get("delta_mean_derived_metric_value"))
                .and_then(JsonValue::as_f64),
            Some(0.5)
        );

        let _ = fs::remove_file(alpha_path);
        let _ = fs::remove_file(beta_path);
    }

    #[test]
    fn cohort_summary_reports_derived_metric_coverage_separately() {
        let alpha_path = write_temp_test_fcs(
            "metric-coverage-alpha",
            build_test_fcs(
                vec!["FSC-A", "SSC-A", "CD3"],
                vec![vec![10.0, 10.0, 1.0], vec![20.0, 20.0, 5.0]],
                None,
            ),
        );
        let beta_path = write_temp_test_fcs(
            "metric-coverage-beta",
            build_test_fcs(
                vec!["FSC-A", "SSC-A", "CD4"],
                vec![vec![12.0, 11.0, 8.0], vec![24.0, 19.0, 7.5]],
                None,
            ),
        );

        let mut session = DesktopSession::new().expect("session");
        let import_payload = JsonValue::Array(vec![
            JsonValue::String(alpha_path.to_string_lossy().to_string()),
            JsonValue::String(beta_path.to_string_lossy().to_string()),
        ])
        .stringify_canonical();
        let imported = session.import_fcs_json(&import_payload);
        assert_eq!(
            imported.get("status").and_then(JsonValue::as_str),
            Some("ready")
        );

        let gate = JsonValue::object([
            ("kind", JsonValue::String("rectangle_gate".to_string())),
            (
                "sample_id",
                JsonValue::String("metric-coverage-alpha".to_string()),
            ),
            (
                "population_id",
                JsonValue::String("lymphocytes".to_string()),
            ),
            ("parent_population", JsonValue::Null),
            ("x_channel", JsonValue::String("FSC-A".to_string())),
            ("y_channel", JsonValue::String("SSC-A".to_string())),
            ("x_min", JsonValue::Number(0.0)),
            ("x_max", JsonValue::Number(30.0)),
            ("y_min", JsonValue::Number(0.0)),
            ("y_max", JsonValue::Number(30.0)),
        ])
        .stringify_canonical();
        let _ = session.dispatch_json(&gate);
        let _ = session.apply_active_template_to_other_samples();
        let _ = session.set_sample_group_label("metric-coverage-alpha", "Control");
        let _ = session.set_sample_group_label("metric-coverage-beta", "Treated");

        let derived_metric_payload = JsonValue::object([
            ("kind", JsonValue::String("positive_fraction".to_string())),
            ("channel", JsonValue::String("CD3".to_string())),
            ("threshold", JsonValue::Number(1.5)),
        ])
        .stringify_canonical();
        let _ = session.set_derived_metric_from_json(&derived_metric_payload);

        let comparison = session.population_comparison("lymphocytes");
        assert_eq!(
            comparison.get("status").and_then(JsonValue::as_str),
            Some("ready")
        );
        let rows = comparison
            .get("population_comparison")
            .and_then(|value| value.get("samples"))
            .and_then(JsonValue::as_array)
            .expect("comparison rows");
        assert_eq!(
            rows[1].get("status").and_then(JsonValue::as_str),
            Some("available")
        );
        assert_eq!(
            rows[1]
                .get("derived_metric_status")
                .and_then(JsonValue::as_str),
            Some("missing_channel")
        );

        let group_summaries = comparison
            .get("population_comparison")
            .and_then(|value| value.get("group_summaries"))
            .and_then(JsonValue::as_array)
            .expect("group summaries");
        assert_eq!(group_summaries.len(), 2);
        assert_eq!(
            group_summaries[1]
                .get("group_label")
                .and_then(JsonValue::as_str),
            Some("Treated")
        );
        assert_eq!(
            group_summaries[1]
                .get("available_sample_count")
                .and_then(JsonValue::as_u64),
            Some(1)
        );
        assert_eq!(
            group_summaries[1]
                .get("derived_metric_available_sample_count")
                .and_then(JsonValue::as_u64),
            Some(0)
        );
        assert_eq!(
            group_summaries[1]
                .get("derived_metric_unavailable_sample_count")
                .and_then(JsonValue::as_u64),
            Some(1)
        );
        assert_eq!(
            group_summaries[1]
                .get("mean_derived_metric_value")
                .map(|value| matches!(value, JsonValue::Null)),
            Some(true)
        );

        let _ = fs::remove_file(alpha_path);
        let _ = fs::remove_file(beta_path);
    }

    #[test]
    fn ffi_can_export_batch_population_stats_csv() {
        let alpha_path = write_temp_test_fcs(
            "batch-export-alpha",
            build_test_fcs(
                vec!["FSC-A", "SSC-A"],
                vec![vec![10.0, 10.0], vec![20.0, 20.0]],
                None,
            ),
        );
        let beta_path = write_temp_test_fcs(
            "batch-export-beta",
            build_test_fcs(
                vec!["FSC-A", "SSC-A"],
                vec![vec![11.0, 11.0], vec![21.0, 21.0]],
                None,
            ),
        );
        let export_path = temp_stats_export_path("ffi-batch-stats");

        let session = flowjoish_desktop_session_new();
        assert!(!session.is_null());

        let import_payload = CString::new(
            JsonValue::Array(vec![
                JsonValue::String(alpha_path.to_string_lossy().to_string()),
                JsonValue::String(beta_path.to_string_lossy().to_string()),
            ])
            .stringify_canonical(),
        )
        .expect("import payload");
        let import_result =
            super::flowjoish_desktop_session_import_fcs_json(session, import_payload.as_ptr());
        unsafe { flowjoish_string_free(import_result) };

        let export_path_c =
            CString::new(export_path.to_string_lossy().to_string()).expect("export path");
        let export_payload =
            flowjoish_desktop_session_export_batch_stats_csv(session, export_path_c.as_ptr());
        assert!(!export_payload.is_null());
        unsafe { flowjoish_string_free(export_payload) };

        let gate_command = CString::new(
            JsonValue::object([
                ("kind", JsonValue::String("rectangle_gate".to_string())),
                (
                    "sample_id",
                    JsonValue::String("batch-export-alpha".to_string()),
                ),
                (
                    "population_id",
                    JsonValue::String("lymphocytes".to_string()),
                ),
                ("parent_population", JsonValue::Null),
                ("x_channel", JsonValue::String("FSC-A".to_string())),
                ("y_channel", JsonValue::String("SSC-A".to_string())),
                ("x_min", JsonValue::Number(0.0)),
                ("x_max", JsonValue::Number(30.0)),
                ("y_min", JsonValue::Number(0.0)),
                ("y_max", JsonValue::Number(30.0)),
            ])
            .stringify_canonical(),
        )
        .expect("gate command");
        let gate_payload = flowjoish_desktop_session_dispatch_json(session, gate_command.as_ptr());
        unsafe { flowjoish_string_free(gate_payload) };

        let apply_payload =
            flowjoish_desktop_session_apply_active_template_to_other_samples(session);
        assert!(!apply_payload.is_null());
        unsafe { flowjoish_string_free(apply_payload) };
        unsafe { flowjoish_desktop_session_free(session) };

        let exported = fs::read_to_string(&export_path).expect("read batch stats export");
        assert!(exported.contains("batch-export-alpha"));
        assert!(exported.contains("batch-export-beta"));

        let _ = fs::remove_file(alpha_path);
        let _ = fs::remove_file(beta_path);
        let _ = fs::remove_file(export_path);
    }

    #[test]
    fn ffi_can_export_population_comparison_csv() {
        let alpha_path = write_temp_test_fcs(
            "comparison-export-alpha",
            build_test_fcs(
                vec!["FSC-A", "SSC-A"],
                vec![vec![10.0, 10.0], vec![20.0, 20.0]],
                None,
            ),
        );
        let beta_path = write_temp_test_fcs(
            "comparison-export-beta",
            build_test_fcs(
                vec!["FSC-A", "SSC-A"],
                vec![vec![11.0, 11.0], vec![21.0, 21.0]],
                None,
            ),
        );
        let export_path = temp_stats_export_path("ffi-population-comparison");

        let session = flowjoish_desktop_session_new();
        assert!(!session.is_null());

        let import_payload = CString::new(
            JsonValue::Array(vec![
                JsonValue::String(alpha_path.to_string_lossy().to_string()),
                JsonValue::String(beta_path.to_string_lossy().to_string()),
            ])
            .stringify_canonical(),
        )
        .expect("import payload");
        let import_result =
            super::flowjoish_desktop_session_import_fcs_json(session, import_payload.as_ptr());
        unsafe { flowjoish_string_free(import_result) };

        let gate_command = CString::new(
            JsonValue::object([
                ("kind", JsonValue::String("rectangle_gate".to_string())),
                (
                    "sample_id",
                    JsonValue::String("comparison-export-alpha".to_string()),
                ),
                (
                    "population_id",
                    JsonValue::String("lymphocytes".to_string()),
                ),
                ("parent_population", JsonValue::Null),
                ("x_channel", JsonValue::String("FSC-A".to_string())),
                ("y_channel", JsonValue::String("SSC-A".to_string())),
                ("x_min", JsonValue::Number(0.0)),
                ("x_max", JsonValue::Number(30.0)),
                ("y_min", JsonValue::Number(0.0)),
                ("y_max", JsonValue::Number(30.0)),
            ])
            .stringify_canonical(),
        )
        .expect("gate command");
        let gate_payload = flowjoish_desktop_session_dispatch_json(session, gate_command.as_ptr());
        unsafe { flowjoish_string_free(gate_payload) };
        let alpha_id = CString::new("comparison-export-alpha").expect("alpha sample id");
        let control_group = CString::new("Control").expect("control group");
        let alpha_group_payload = flowjoish_desktop_session_set_sample_group_label(
            session,
            alpha_id.as_ptr(),
            control_group.as_ptr(),
        );
        unsafe { flowjoish_string_free(alpha_group_payload) };
        let beta_id = CString::new("comparison-export-beta").expect("beta sample id");
        let treated_group = CString::new("Treated").expect("treated group");
        let beta_group_payload = flowjoish_desktop_session_set_sample_group_label(
            session,
            beta_id.as_ptr(),
            treated_group.as_ptr(),
        );
        unsafe { flowjoish_string_free(beta_group_payload) };

        let population_key = CString::new("lymphocytes").expect("population key");
        let export_path_c =
            CString::new(export_path.to_string_lossy().to_string()).expect("export path");
        let export_payload = flowjoish_desktop_session_export_population_comparison_csv(
            session,
            population_key.as_ptr(),
            export_path_c.as_ptr(),
        );
        assert!(!export_payload.is_null());
        unsafe { flowjoish_string_free(export_payload) };
        unsafe { flowjoish_desktop_session_free(session) };

        let exported = fs::read_to_string(&export_path).expect("read population comparison export");
        assert!(exported.starts_with(
            "population_key,population_id,active_sample_id,active_group_label,sample_id,display_name,group_label,source_path,status,is_active_sample,matched_events,parent_events,frequency_of_all,frequency_of_parent,delta_frequency_of_all,delta_frequency_of_parent\n"
        ));
        assert!(exported.contains("lymphocytes"));
        assert!(exported.contains("comparison-export-alpha"));
        assert!(exported.contains("comparison-export-beta"));

        let _ = fs::remove_file(alpha_path);
        let _ = fs::remove_file(beta_path);
        let _ = fs::remove_file(export_path);
    }

    #[test]
    fn ffi_can_export_population_group_summary_csv() {
        let alpha_path = write_temp_test_fcs(
            "group-summary-alpha",
            build_test_fcs(
                vec!["FSC-A", "SSC-A"],
                vec![vec![10.0, 10.0], vec![20.0, 20.0]],
                None,
            ),
        );
        let beta_path = write_temp_test_fcs(
            "group-summary-beta",
            build_test_fcs(
                vec!["FSC-A", "SSC-A"],
                vec![vec![11.0, 11.0], vec![21.0, 21.0]],
                None,
            ),
        );
        let export_path = temp_stats_export_path("ffi-group-summary");

        let session = flowjoish_desktop_session_new();
        assert!(!session.is_null());
        let import_payload = CString::new(
            JsonValue::Array(vec![
                JsonValue::String(alpha_path.to_string_lossy().to_string()),
                JsonValue::String(beta_path.to_string_lossy().to_string()),
            ])
            .stringify_canonical(),
        )
        .expect("import payload");
        let import_result =
            super::flowjoish_desktop_session_import_fcs_json(session, import_payload.as_ptr());
        unsafe { flowjoish_string_free(import_result) };
        let alpha_id = CString::new("group-summary-alpha").expect("alpha sample id");
        let control_group = CString::new("Control").expect("control group");
        let alpha_group_payload = flowjoish_desktop_session_set_sample_group_label(
            session,
            alpha_id.as_ptr(),
            control_group.as_ptr(),
        );
        unsafe { flowjoish_string_free(alpha_group_payload) };
        let beta_id = CString::new("group-summary-beta").expect("beta sample id");
        let treated_group = CString::new("Treated").expect("treated group");
        let beta_group_payload = flowjoish_desktop_session_set_sample_group_label(
            session,
            beta_id.as_ptr(),
            treated_group.as_ptr(),
        );
        unsafe { flowjoish_string_free(beta_group_payload) };

        let gate_command = CString::new(
            JsonValue::object([
                ("kind", JsonValue::String("rectangle_gate".to_string())),
                (
                    "sample_id",
                    JsonValue::String("group-summary-alpha".to_string()),
                ),
                (
                    "population_id",
                    JsonValue::String("lymphocytes".to_string()),
                ),
                ("parent_population", JsonValue::Null),
                ("x_channel", JsonValue::String("FSC-A".to_string())),
                ("y_channel", JsonValue::String("SSC-A".to_string())),
                ("x_min", JsonValue::Number(0.0)),
                ("x_max", JsonValue::Number(30.0)),
                ("y_min", JsonValue::Number(0.0)),
                ("y_max", JsonValue::Number(30.0)),
            ])
            .stringify_canonical(),
        )
        .expect("gate command");
        let gate_payload = flowjoish_desktop_session_dispatch_json(session, gate_command.as_ptr());
        unsafe { flowjoish_string_free(gate_payload) };
        let apply_payload =
            flowjoish_desktop_session_apply_active_template_to_other_samples(session);
        unsafe { flowjoish_string_free(apply_payload) };

        let population_key = CString::new("lymphocytes").expect("population key");
        let export_path_c =
            CString::new(export_path.to_string_lossy().to_string()).expect("export path");
        let export_payload = flowjoish_desktop_session_export_population_group_summary_csv(
            session,
            population_key.as_ptr(),
            export_path_c.as_ptr(),
        );
        assert!(!export_payload.is_null());
        unsafe { flowjoish_string_free(export_payload) };
        unsafe { flowjoish_desktop_session_free(session) };

        let exported = fs::read_to_string(&export_path).expect("read cohort summary export");
        assert!(exported.starts_with(
            "population_key,population_id,active_sample_id,active_group_label,group_label,is_active_group,sample_count,available_sample_count,missing_sample_count,derived_metric_available_sample_count,derived_metric_unavailable_sample_count,total_matched_events,total_parent_events,mean_frequency_of_all,mean_frequency_of_parent,delta_mean_frequency_of_all,delta_mean_frequency_of_parent,mean_derived_metric_value,delta_mean_derived_metric_value\n"
        ));
        assert!(exported.contains("lymphocytes"));

        let _ = fs::remove_file(alpha_path);
        let _ = fs::remove_file(beta_path);
        let _ = fs::remove_file(export_path);
    }

    #[test]
    fn ffi_can_set_and_export_population_derived_metric_csv() {
        let alpha_path = write_temp_test_fcs(
            "derived-metric-alpha",
            build_test_fcs(
                vec!["FSC-A", "SSC-A", "CD3"],
                vec![vec![10.0, 10.0, 1.0], vec![20.0, 20.0, 5.0]],
                None,
            ),
        );
        let beta_path = write_temp_test_fcs(
            "derived-metric-beta",
            build_test_fcs(
                vec!["FSC-A", "SSC-A", "CD3"],
                vec![vec![11.0, 11.0, 2.0], vec![21.0, 21.0, 4.0]],
                None,
            ),
        );
        let export_path = temp_stats_export_path("ffi-derived-metric");

        let session = flowjoish_desktop_session_new();
        assert!(!session.is_null());

        let import_payload = CString::new(
            JsonValue::Array(vec![
                JsonValue::String(alpha_path.to_string_lossy().to_string()),
                JsonValue::String(beta_path.to_string_lossy().to_string()),
            ])
            .stringify_canonical(),
        )
        .expect("import payload");
        let import_result =
            super::flowjoish_desktop_session_import_fcs_json(session, import_payload.as_ptr());
        unsafe { flowjoish_string_free(import_result) };

        let gate_command = CString::new(
            JsonValue::object([
                ("kind", JsonValue::String("rectangle_gate".to_string())),
                (
                    "sample_id",
                    JsonValue::String("derived-metric-alpha".to_string()),
                ),
                (
                    "population_id",
                    JsonValue::String("lymphocytes".to_string()),
                ),
                ("parent_population", JsonValue::Null),
                ("x_channel", JsonValue::String("FSC-A".to_string())),
                ("y_channel", JsonValue::String("SSC-A".to_string())),
                ("x_min", JsonValue::Number(0.0)),
                ("x_max", JsonValue::Number(30.0)),
                ("y_min", JsonValue::Number(0.0)),
                ("y_max", JsonValue::Number(30.0)),
            ])
            .stringify_canonical(),
        )
        .expect("gate command");
        let gate_payload = flowjoish_desktop_session_dispatch_json(session, gate_command.as_ptr());
        unsafe { flowjoish_string_free(gate_payload) };
        let apply_payload =
            flowjoish_desktop_session_apply_active_template_to_other_samples(session);
        unsafe { flowjoish_string_free(apply_payload) };

        let metric_payload = CString::new(
            JsonValue::object([
                ("kind", JsonValue::String("positive_fraction".to_string())),
                ("channel", JsonValue::String("CD3".to_string())),
                ("threshold", JsonValue::Number(1.5)),
            ])
            .stringify_canonical(),
        )
        .expect("metric payload");
        let metric_result =
            flowjoish_desktop_session_set_derived_metric_json(session, metric_payload.as_ptr());
        assert!(!metric_result.is_null());
        unsafe { flowjoish_string_free(metric_result) };

        let population_key = CString::new("lymphocytes").expect("population key");
        let export_path_c =
            CString::new(export_path.to_string_lossy().to_string()).expect("export path");
        let export_payload = flowjoish_desktop_session_export_population_derived_metric_csv(
            session,
            population_key.as_ptr(),
            export_path_c.as_ptr(),
        );
        assert!(!export_payload.is_null());
        unsafe { flowjoish_string_free(export_payload) };
        unsafe { flowjoish_desktop_session_free(session) };

        let exported = fs::read_to_string(&export_path).expect("read derived metric export");
        assert!(exported.starts_with(
            "population_key,population_id,active_sample_id,metric_kind,metric_label,sample_id,display_name,group_label,status,is_active_sample,value,delta_value,message\n"
        ));
        assert!(exported.contains("positive_fraction"));
        assert!(exported.contains("derived-metric-alpha"));
        assert!(exported.contains("derived-metric-beta"));
        assert!(exported.contains("0.500000"));

        let _ = fs::remove_file(alpha_path);
        let _ = fs::remove_file(beta_path);
        let _ = fs::remove_file(export_path);
    }

    fn write_temp_test_fcs(prefix: &str, bytes: Vec<u8>) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "flowjoish-desktop-bridge-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).expect("create temp directory");
        let path = directory.join(format!("{prefix}.fcs"));
        fs::write(&path, bytes).expect("write fcs file");
        path
    }

    fn temp_workspace_path(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "flowjoish-desktop-bridge-{prefix}-{}-{nanos}.parallax.json",
            std::process::id()
        ))
    }

    fn temp_stats_export_path(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "flowjoish-desktop-bridge-{prefix}-{}-{nanos}.csv",
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
}
