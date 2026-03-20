use std::collections::BTreeMap;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;

use flowjoish_core::{
    BitMask, Command, CommandLog, JsonValue, ReplayEnvironment, SampleFrame, WorkspaceState,
};

pub struct DesktopSession {
    environment: ReplayEnvironment,
    sample_id: String,
    command_log: CommandLog,
    redo_stack: Vec<Command>,
}

impl DesktopSession {
    fn new() -> Result<Self, String> {
        let sample_id = "desktop-demo".to_string();
        let sample = SampleFrame::new(
            sample_id.clone(),
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

        Ok(Self {
            environment,
            sample_id,
            command_log: CommandLog::new(),
            redo_stack: Vec::new(),
        })
    }

    fn reset(&mut self) -> JsonValue {
        self.command_log = CommandLog::new();
        self.redo_stack.clear();
        self.snapshot_value()
            .unwrap_or_else(|message| error_json_value(message))
    }

    fn snapshot_value(&self) -> Result<JsonValue, String> {
        let sample = self
            .environment
            .sample(&self.sample_id)
            .ok_or_else(|| format!("missing sample '{}'", self.sample_id))?;
        let state = self
            .command_log
            .replay(&self.environment)
            .map_err(|error| error.to_string())?;

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
            ("sample", sample_json(sample)),
            (
                "command_count",
                JsonValue::Number(self.command_log.records().len() as f64),
            ),
            (
                "can_undo",
                JsonValue::Bool(!self.command_log.is_empty()),
            ),
            (
                "can_redo",
                JsonValue::Bool(!self.redo_stack.is_empty()),
            ),
            (
                "command_log_hash",
                JsonValue::String(format!("{:016x}", self.command_log.execution_hash())),
            ),
            (
                "execution_hash",
                JsonValue::String(format!("{:016x}", state.execution_hash)),
            ),
            ("commands", commands_json(&self.command_log)),
            ("populations", populations_json(sample, &state)),
            ("plots", plots_json(sample, &state)?),
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

        let mut next_log = self.command_log.clone();
        next_log.append(command);
        if let Err(error) = next_log.replay(&self.environment) {
            return error_json_value(error.to_string());
        }

        self.command_log = next_log;
        self.redo_stack.clear();
        self.snapshot_value()
            .unwrap_or_else(|message| error_json_value(message))
    }

    fn undo(&mut self) -> JsonValue {
        match self.command_log.pop() {
            Some(record) => {
                self.redo_stack.push(record.command);
                self.snapshot_value()
                    .unwrap_or_else(|message| error_json_value(message))
            }
            None => error_json_value("there is no command to undo"),
        }
    }

    fn redo(&mut self) -> JsonValue {
        match self.redo_stack.pop() {
            Some(command) => {
                let mut next_log = self.command_log.clone();
                next_log.append(command);
                if let Err(error) = next_log.replay(&self.environment) {
                    return error_json_value(error.to_string());
                }
                self.command_log = next_log;
                self.snapshot_value()
                    .unwrap_or_else(|message| error_json_value(message))
            }
            None => error_json_value("there is no command to redo"),
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

fn sample_json(sample: &SampleFrame) -> JsonValue {
    JsonValue::object([
        ("id", JsonValue::String(sample.sample_id().to_string())),
        (
            "event_count",
            JsonValue::Number(sample.event_count() as f64),
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
    ])
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

fn plots_json(sample: &SampleFrame, state: &WorkspaceState) -> Result<JsonValue, String> {
    let plots = default_plot_specs(sample)
        .into_iter()
        .map(|(id, title, x_channel, y_channel)| {
            plot_json(sample, state, id, title, x_channel, y_channel)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(JsonValue::Array(plots))
}

fn default_plot_specs(
    sample: &SampleFrame,
) -> Vec<(&'static str, &'static str, &'static str, &'static str)> {
    [
        ("fsc_ssc", "FSC-A vs SSC-A", "FSC-A", "SSC-A"),
        ("cd3_cd4", "CD3 vs CD4", "CD3", "CD4"),
    ]
    .into_iter()
    .filter(|(_, _, x_channel, y_channel)| {
        sample.channel_index(x_channel).is_some() && sample.channel_index(y_channel).is_some()
    })
    .collect()
}

fn plot_json(
    sample: &SampleFrame,
    state: &WorkspaceState,
    id: &str,
    title: &str,
    x_channel: &str,
    y_channel: &str,
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

    let (x_min, x_max) = axis_bounds(sample, x_index);
    let (y_min, y_max) = axis_bounds(sample, y_index);

    Ok(JsonValue::object([
        ("id", JsonValue::String(id.to_string())),
        ("title", JsonValue::String(title.to_string())),
        ("x_channel", JsonValue::String(x_channel.to_string())),
        ("y_channel", JsonValue::String(y_channel.to_string())),
        ("all_points", JsonValue::Array(all_points)),
        ("population_points", JsonValue::Object(population_points)),
        (
            "x_range",
            JsonValue::object([
                ("min", JsonValue::Number(x_min)),
                ("max", JsonValue::Number(x_max)),
            ]),
        ),
        (
            "y_range",
            JsonValue::object([
                ("min", JsonValue::Number(y_min)),
                ("max", JsonValue::Number(y_max)),
            ]),
        ),
    ]))
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
        flowjoish_desktop_session_free, flowjoish_desktop_session_new,
        flowjoish_desktop_session_redo, flowjoish_desktop_session_undo, flowjoish_string_free,
    };
    use std::ffi::{CStr, CString};

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
}
