use std::collections::BTreeMap;
use std::ffi::{CStr, CString};
use std::fs;
use std::os::raw::c_char;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::ptr;

use flowjoish_core::{
    BitMask, ChannelTransform, Command, CommandLog, CompensationMatrix, JsonValue,
    ReplayEnvironment, SampleAnalysisProfile, SampleFrame, StableHasher, WorkspaceState,
    apply_sample_analysis,
};
use flowjoish_fcs::parse as parse_fcs;

#[derive(Clone, Debug)]
struct DesktopSampleInfo {
    display_name: String,
    source_path: Option<String>,
}

#[derive(Clone, Debug)]
struct DesktopSampleArtifact {
    raw_sample: SampleFrame,
    compensation: Option<CompensationMatrix>,
}

#[derive(Clone, Debug, PartialEq)]
enum AnalysisAction {
    SetCompensationEnabled { sample_id: String, enabled: bool },
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
                    value.get("transform")
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
                    profile.transforms.insert(channel.clone(), transform.clone());
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
        plots: &[(String, String, String, String)],
    ) -> Result<BTreeMap<String, PlotRangeState>, String> {
        let mut ranges = BTreeMap::new();
        for plot in plots {
            ranges.insert(plot.0.clone(), auto_plot_range(sample, plot)?);
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
                .find(|candidate| candidate.0 == record.action.plot_id())
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
                    let current = ranges
                        .get(record.action.plot_id())
                        .cloned()
                        .ok_or_else(|| {
                            format!("missing active range for plot '{}'", record.action.plot_id())
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
            value.get("action")
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
            value.get("action")
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
    source: WorkspaceSampleSource,
}

enum WorkspaceSampleSource {
    EmbeddedDemo,
    FcsFile(String),
}

struct WorkspaceDocument {
    active_sample_id: String,
    sample_specs: Vec<WorkspaceSampleSpec>,
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

        Ok(Self {
            environment,
            sample_artifacts,
            sample_id,
            sample_order: vec!["desktop-demo".to_string()],
            sample_info,
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
        let (processed_sample, analysis_profile, state, execution_hash) = self.active_replay_state()?;
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
            (
                "can_undo",
                JsonValue::Bool(!command_log.is_empty()),
            ),
            (
                "can_redo",
                JsonValue::Bool(!redo_stack.is_empty()),
            ),
            (
                "command_log_hash",
                JsonValue::String(format!("{:016x}", command_log.execution_hash())),
            ),
            (
                "execution_hash",
                JsonValue::String(format!("{:016x}", execution_hash)),
            ),
            ("commands", commands_json(command_log)),
            ("analysis_actions", analysis_actions_json(analysis_log)),
            ("populations", populations_json(sample, &state)),
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
        if let Err(error) = self
            .active_command_log()
            .and_then(|command_log| {
                command_log
                    .replay(&replay_environment)
                    .map_err(|error| error.to_string())
            })
        {
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
            None => return error_json_value(format!("missing command log for sample '{sample_id}'")),
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
            None => return error_json_value(format!("missing redo state for sample '{sample_id}'")),
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
        self.command_logs = imported.command_logs;
        self.analysis_logs = imported.analysis_logs;
        self.view_logs = imported.view_logs;
        self.redo_stacks = imported.redo_stacks;

        self.snapshot_value()
            .unwrap_or_else(|message| error_json_value(message))
    }

    fn active_command_log(&self) -> Result<&CommandLog, String> {
        self.command_logs
            .get(&self.sample_id)
            .ok_or_else(|| format!("missing command log for sample '{}'", self.sample_id))
    }

    fn active_analysis_log(&self) -> Result<&AnalysisActionLog, String> {
        self.analysis_logs
            .get(&self.sample_id)
            .ok_or_else(|| format!("missing analysis log for sample '{}'", self.sample_id))
    }

    fn active_view_log(&self) -> Result<&ViewActionLog, String> {
        self.view_logs
            .get(&self.sample_id)
            .ok_or_else(|| format!("missing view log for sample '{}'", self.sample_id))
    }

    fn active_sample_artifact(&self) -> Result<&DesktopSampleArtifact, String> {
        self.sample_artifacts
            .get(&self.sample_id)
            .ok_or_else(|| format!("missing sample artifact for sample '{}'", self.sample_id))
    }

    fn active_processed_environment(
        &self,
    ) -> Result<(SampleFrame, SampleAnalysisProfile, u64, ReplayEnvironment), String> {
        let (processed_sample, analysis_profile, _, execution_hash) = self.active_replay_state()?;
        let mut environment = ReplayEnvironment::new();
        environment
            .insert_sample(processed_sample.clone())
            .map_err(|error| error.to_string())?;
        Ok((processed_sample, analysis_profile, execution_hash, environment))
    }

    fn active_replay_state(
        &self,
    ) -> Result<(SampleFrame, SampleAnalysisProfile, WorkspaceState, u64), String> {
        let artifact = self.active_sample_artifact()?;
        let analysis_log = self.active_analysis_log()?;
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
            .active_command_log()?
            .replay(&environment)
            .map_err(|error| error.to_string())?;

        let mut hasher = StableHasher::new();
        hasher.update_u64(analysis_log.execution_hash());
        hasher.update_u64(state.execution_hash);

        Ok((processed_sample, profile, state, hasher.finish_u64()))
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
            (
                "kind",
                JsonValue::String("parallax_workspace".to_string()),
            ),
            ("version", JsonValue::Number(1.0)),
            (
                "active_sample_id",
                JsonValue::String(self.sample_id.clone()),
            ),
            ("samples", JsonValue::Array(samples)),
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

        Ok(Self {
            environment,
            sample_artifacts,
            active_sample_id,
            sample_order,
            sample_info,
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
                .map_err(|error| format!("failed to load workspace sample '{}': {error}", spec.id))?;
            sample_artifacts.insert(spec.id.clone(), artifact);
            sample_order.push(spec.id.clone());
            sample_info.insert(
                spec.id.clone(),
                DesktopSampleInfo {
                    display_name: spec.display_name.clone(),
                    source_path: spec.source.path().map(str::to_string),
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
        if value
            .get("kind")
            .and_then(JsonValue::as_str)
            != Some("parallax_workspace")
        {
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
            if sample_specs.iter().any(|existing: &WorkspaceSampleSpec| existing.id == spec.id) {
                return Err(format!("workspace contains duplicate sample id '{}'", spec.id));
            }
            sample_specs.push(spec);
        }

        let command_logs_object = value
            .get("command_logs")
            .and_then(JsonValue::as_object)
            .ok_or_else(|| "workspace document must contain a command_logs object".to_string())?;
        let analysis_logs_object = value
            .get("analysis_logs")
            .and_then(JsonValue::as_object);
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
                Some(value) => ViewActionLog::from_json_value(value)
                    .map_err(|error| format!("invalid view log for sample '{}': {error}", spec.id))?,
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
            "fcs_file" => WorkspaceSampleSource::FcsFile(source_path.ok_or_else(|| {
                format!("workspace sample '{}' is missing source_path", id)
            })?),
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
        Ok(file_paths_json) => {
            with_session_payload(session, |session| Ok(session.import_fcs_json(file_paths_json)))
        }
        Err(error) => payload_to_ptr(error_json_value(error.to_string()).stringify_canonical()),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn flowjoish_desktop_session_select_sample(
    session: *mut DesktopSession,
    sample_id: *const c_char,
) -> *mut c_char {
    if sample_id.is_null() {
        return payload_to_ptr(error_json_value("sample id pointer was null").stringify_canonical());
    }

    let sample_id = unsafe { CStr::from_ptr(sample_id) };
    match sample_id.to_str() {
        Ok(sample_id) => with_session_payload(session, |session| Ok(session.select_sample(sample_id))),
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
            with_session_payload(session, |session| Ok(session.save_workspace(workspace_path)))
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
            with_session_payload(session, |session| Ok(session.load_workspace(workspace_path)))
        }
        Err(error) => payload_to_ptr(error_json_value(error.to_string()).stringify_canonical()),
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
            JsonValue::String(
                "Fast, trustworthy, reproducible cytometry analysis".to_string(),
            ),
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
                                        positive_decades,
                                        ..
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
                                        negative_decades,
                                        ..
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
        ChannelTransform::Linear => JsonValue::object([(
            "kind",
            JsonValue::String("linear".to_string()),
        )]),
        ChannelTransform::SignedLog10 => JsonValue::object([(
            "kind",
            JsonValue::String("signed_log10".to_string()),
        )]),
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

fn plots_json(
    sample: &SampleFrame,
    state: &WorkspaceState,
    plot_specs: &[(String, String, String, String)],
    plot_ranges: &BTreeMap<String, PlotRangeState>,
) -> Result<JsonValue, String> {
    let plots = plot_specs
        .into_iter()
        .map(|(id, title, x_channel, y_channel)| {
            plot_json(
                sample,
                state,
                id,
                title,
                x_channel,
                y_channel,
                plot_ranges.get(id),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(JsonValue::Array(plots))
}

fn default_plot_specs(sample: &SampleFrame) -> Vec<(String, String, String, String)> {
    let mut plots = Vec::new();

    if let Some((x_channel, y_channel)) = preferred_scatter_pair(sample) {
        plots.push((
            plot_id_for_channels(x_channel, y_channel),
            format!("{x_channel} vs {y_channel}"),
            x_channel.to_string(),
            y_channel.to_string(),
        ));
    }

    if let Some((x_channel, y_channel)) = secondary_pair(sample, &plots) {
        plots.push((
            plot_id_for_channels(&x_channel, &y_channel),
            format!("{x_channel} vs {y_channel}"),
            x_channel,
            y_channel,
        ));
    }

    if plots.is_empty() {
        let channels = sample.channels();
        if channels.len() >= 2 {
            plots.push((
                plot_id_for_channels(&channels[0], &channels[1]),
                format!("{} vs {}", channels[0], channels[1]),
                channels[0].clone(),
                channels[1].clone(),
            ));
        }
    }

    plots
}

fn plot_json(
    sample: &SampleFrame,
    state: &WorkspaceState,
    id: &str,
    title: &str,
    x_channel: &str,
    y_channel: &str,
    view_range: Option<&PlotRangeState>,
) -> Result<JsonValue, String> {
    let x_index = sample
        .channel_index(x_channel)
        .ok_or_else(|| format!("missing channel '{}'", x_channel))?;
    let y_index = sample
        .channel_index(y_channel)
        .ok_or_else(|| format!("missing channel '{}'", y_channel))?;

    let all_points = points_json(sample, x_index, y_index, None);
    let mut population_points = BTreeMap::new();
    population_points.insert("__all__".to_string(), JsonValue::Array(all_points.clone()));
    for population in state.populations.values() {
        population_points.insert(
            population.population_id.clone(),
            JsonValue::Array(points_json(
                sample,
                x_index,
                y_index,
                Some(&population.mask),
            )),
        );
    }

    let auto_range = auto_plot_range(
        sample,
        &(
            id.to_string(),
            title.to_string(),
            x_channel.to_string(),
            y_channel.to_string(),
        ),
    )?;
    let range = view_range.unwrap_or(&auto_range);

    Ok(JsonValue::object([
        ("id", JsonValue::String(id.to_string())),
        ("title", JsonValue::String(title.to_string())),
        ("x_channel", JsonValue::String(x_channel.to_string())),
        ("y_channel", JsonValue::String(y_channel.to_string())),
        ("view_summary", JsonValue::String(range.summary.clone())),
        ("all_points", JsonValue::Array(all_points)),
        ("population_points", JsonValue::Object(population_points)),
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

fn auto_plot_range(
    sample: &SampleFrame,
    plot: &(String, String, String, String),
) -> Result<PlotRangeState, String> {
    let x_index = sample
        .channel_index(&plot.2)
        .ok_or_else(|| format!("missing channel '{}'", plot.2))?;
    let y_index = sample
        .channel_index(&plot.3)
        .ok_or_else(|| format!("missing channel '{}'", plot.3))?;
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

fn focus_plot_range(
    sample: &SampleFrame,
    state: &WorkspaceState,
    plot: &(String, String, String, String),
    population_id: &str,
    padding_fraction: f64,
) -> Result<PlotRangeState, String> {
    if !padding_fraction.is_finite() || padding_fraction <= 0.0 {
        return Err("plot focus padding_fraction must be a positive finite number".to_string());
    }

    let x_index = sample
        .channel_index(&plot.2)
        .ok_or_else(|| format!("missing channel '{}'", plot.2))?;
    let y_index = sample
        .channel_index(&plot.3)
        .ok_or_else(|| format!("missing channel '{}'", plot.3))?;

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
    let Some((x_min, x_max, y_min, y_max)) = plot_bounds(sample, x_index, y_index, focus_mask) else {
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
        summary: if population_id == "__all__" {
            "Focused on All Events".to_string()
        } else {
            format!("Focused on {population_id}")
        },
    })
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

fn secondary_pair(
    sample: &SampleFrame,
    plots: &[(String, String, String, String)],
) -> Option<(String, String)> {
    let primary_channels = plots
        .first()
        .map(|(_, _, x_channel, y_channel)| (x_channel.as_str(), y_channel.as_str()));

    let fluorescence_channels = sample
        .channels()
        .iter()
        .filter(|channel| !is_time_channel(channel) && !is_structural_channel(channel))
        .filter(|channel| {
            match primary_channels {
                Some((x_channel, y_channel)) => {
                    channel.as_str() != x_channel && channel.as_str() != y_channel
                }
                None => true,
            }
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
        .filter(|channel| {
            match primary_channels {
                Some((x_channel, y_channel)) => {
                    channel.as_str() != x_channel && channel.as_str() != y_channel
                }
                None => true,
            }
        })
        .cloned()
        .collect::<Vec<_>>();
    if fallback_channels.len() >= 2 {
        return Some((fallback_channels[0].clone(), fallback_channels[1].clone()));
    }

    None
}

fn plot_id_for_channels(x_channel: &str, y_channel: &str) -> String {
    format!(
        "plot_{}_{}",
        sanitize_plot_segment(x_channel),
        sanitize_plot_segment(y_channel)
    )
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

fn points_json(
    sample: &SampleFrame,
    x_index: usize,
    y_index: usize,
    mask: Option<&BitMask>,
) -> Vec<JsonValue> {
    sample
        .events()
        .iter()
        .enumerate()
        .filter_map(|(event_index, row)| {
            if mask.is_some_and(|mask| !mask.contains(event_index)) {
                return None;
            }

            Some(JsonValue::object([
                ("x", JsonValue::Number(row[x_index])),
                ("y", JsonValue::Number(row[y_index])),
            ]))
        })
        .collect()
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
        DesktopSession, bootstrap_json_string, flowjoish_desktop_session_dispatch_json,
        flowjoish_desktop_session_free, flowjoish_desktop_session_load_workspace,
        flowjoish_desktop_session_new, flowjoish_desktop_session_redo,
        flowjoish_desktop_session_save_workspace, flowjoish_desktop_session_select_sample,
        flowjoish_desktop_session_undo, flowjoish_string_free,
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
        assert_eq!(snapshot.get("can_undo").and_then(JsonValue::as_bool), Some(true));
        assert_eq!(snapshot.get("can_redo").and_then(JsonValue::as_bool), Some(false));
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
        assert_eq!(undone.get("command_count").and_then(JsonValue::as_u64), Some(0));
        assert_eq!(undone.get("can_undo").and_then(JsonValue::as_bool), Some(false));
        assert_eq!(undone.get("can_redo").and_then(JsonValue::as_bool), Some(true));

        let redone = session.redo();
        assert_eq!(redone.get("command_count").and_then(JsonValue::as_u64), Some(1));
        assert_eq!(redone.get("can_undo").and_then(JsonValue::as_bool), Some(true));
        assert_eq!(redone.get("can_redo").and_then(JsonValue::as_bool), Some(false));
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
        assert_eq!(undone.get("command_count").and_then(JsonValue::as_u64), Some(0));
        let redone = JsonValue::parse(&redo_text).expect("redo json");
        assert_eq!(redone.get("command_count").and_then(JsonValue::as_u64), Some(1));
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
                vec![vec![1.0, 10.0, 11.0, 100.0, 150.0], vec![2.0, 20.0, 21.0, 200.0, 250.0]],
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
        assert_eq!(imported.get("status").and_then(JsonValue::as_str), Some("ready"));
        assert_eq!(imported.get("sample").and_then(|sample| sample.get("id")).and_then(JsonValue::as_str), Some("alpha"));
        assert_eq!(imported.get("samples").and_then(JsonValue::as_array).map(|samples| samples.len()), Some(2));

        let command = JsonValue::object([
            ("kind", JsonValue::String("rectangle_gate".to_string())),
            ("sample_id", JsonValue::String("alpha".to_string())),
            ("population_id", JsonValue::String("lymphocytes".to_string())),
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
        assert_eq!(gated.get("command_count").and_then(JsonValue::as_u64), Some(1));

        let beta_snapshot = session.select_sample("beta");
        assert_eq!(beta_snapshot.get("sample").and_then(|sample| sample.get("id")).and_then(JsonValue::as_str), Some("beta"));
        assert_eq!(beta_snapshot.get("command_count").and_then(JsonValue::as_u64), Some(0));
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

        let alpha_snapshot = session.select_sample("alpha");
        assert_eq!(alpha_snapshot.get("command_count").and_then(JsonValue::as_u64), Some(1));

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

        let import_result = super::flowjoish_desktop_session_import_fcs_json(session, import_payload.as_ptr());
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
        assert_eq!(imported.get("status").and_then(JsonValue::as_str), Some("ready"));

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
                JsonValue::object([(
                    "kind",
                    JsonValue::String("signed_log10".to_string()),
                )]),
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
        assert_eq!(imported.get("status").and_then(JsonValue::as_str), Some("ready"));
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
            ("population_id", JsonValue::String("lymphocytes".to_string())),
            ("parent_population", JsonValue::Null),
            ("x_channel", JsonValue::String("FSC-A".to_string())),
            ("y_channel", JsonValue::String("SSC-A".to_string())),
            ("x_min", JsonValue::Number(0.0)),
            ("x_max", JsonValue::Number(40.0)),
            ("y_min", JsonValue::Number(0.0)),
            ("y_max", JsonValue::Number(40.0)),
        ])
        .stringify_canonical();
        let _ = session.dispatch_json(&gate);

        let focus_payload = JsonValue::object([
            ("kind", JsonValue::String("focus_plot_population".to_string())),
            ("sample_id", JsonValue::String("view-sample".to_string())),
            ("plot_id", JsonValue::String("plot_fsc_a_ssc_a".to_string())),
            ("population_id", JsonValue::String("lymphocytes".to_string())),
            ("padding_fraction", JsonValue::Number(0.08)),
        ])
        .stringify_canonical();
        let focused = session.dispatch_json(&focus_payload);
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
        assert_eq!(after_undo.get("status").and_then(JsonValue::as_str), Some("ready"));
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
                vec![vec![11.0, 12.0, 100.0, 150.0], vec![21.0, 22.0, 200.0, 250.0]],
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
        assert_eq!(imported.get("status").and_then(JsonValue::as_str), Some("ready"));

        let alpha_gate = JsonValue::object([
            ("kind", JsonValue::String("rectangle_gate".to_string())),
            ("sample_id", JsonValue::String("save-alpha".to_string())),
            ("population_id", JsonValue::String("lymphocytes".to_string())),
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
                JsonValue::object([(
                    "kind",
                    JsonValue::String("signed_log10".to_string()),
                )]),
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
        assert_eq!(saved.get("status").and_then(JsonValue::as_str), Some("ready"));

        let mut reopened = DesktopSession::new().expect("session");
        let loaded = reopened.load_workspace(workspace_path.to_string_lossy().as_ref());
        assert_eq!(loaded.get("status").and_then(JsonValue::as_str), Some("ready"));
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
                .and_then(|sample| sample.get("compensation_enabled"))
                .and_then(JsonValue::as_bool),
            Some(true)
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
        assert_eq!(loaded.get("command_count").and_then(JsonValue::as_u64), Some(0));
        assert_eq!(loaded.get("can_redo").and_then(JsonValue::as_bool), Some(true));

        let alpha_snapshot = reopened.select_sample("save-alpha");
        assert_eq!(
            alpha_snapshot.get("command_count").and_then(JsonValue::as_u64),
            Some(1)
        );
        let beta_snapshot = reopened.select_sample("save-beta");
        assert_eq!(beta_snapshot.get("can_redo").and_then(JsonValue::as_bool), Some(true));
        let redone = reopened.redo();
        assert_eq!(redone.get("command_count").and_then(JsonValue::as_u64), Some(1));

        let _ = fs::remove_file(alpha_path);
        let _ = fs::remove_file(beta_path);
        let _ = fs::remove_file(workspace_path);
    }

    #[test]
    fn ffi_can_save_and_load_workspace() {
        let workspace_path = temp_workspace_path("ffi-workspace");
        let session = flowjoish_desktop_session_new();
        assert!(!session.is_null());

        let save_path = CString::new(workspace_path.to_string_lossy().to_string())
            .expect("workspace path");
        let save_payload =
            flowjoish_desktop_session_save_workspace(session, save_path.as_ptr());
        unsafe { flowjoish_string_free(save_payload) };

        let load_payload =
            flowjoish_desktop_session_load_workspace(session, save_path.as_ptr());
        assert!(!load_payload.is_null());
        let text = unsafe { CStr::from_ptr(load_payload) }
            .to_str()
            .expect("utf8 payload")
            .to_string();
        unsafe { flowjoish_string_free(load_payload) };
        unsafe { flowjoish_desktop_session_free(session) };

        let _ = fs::remove_file(workspace_path);

        let parsed = JsonValue::parse(&text).expect("json payload");
        assert_eq!(parsed.get("status").and_then(JsonValue::as_str), Some("ready"));
        assert_eq!(
            parsed
                .get("sample")
                .and_then(|sample| sample.get("id"))
                .and_then(JsonValue::as_str),
            Some("desktop-demo")
        );
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
