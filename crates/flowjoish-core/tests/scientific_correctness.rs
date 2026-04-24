use std::collections::BTreeMap;

use flowjoish_core::{
    ChannelTransform, Command, CommandLog, CompensationMatrix, Point2D, ReplayEnvironment,
    SampleAnalysisProfile, SampleFrame, apply_sample_analysis, compute_population_stats_table,
};

const EPSILON: f64 = 1e-9;

fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= EPSILON,
        "expected {expected:.12}, got {actual:.12}"
    );
}

fn raw_sample() -> SampleFrame {
    SampleFrame::new(
        "golden-sample",
        vec![
            "FSC-A".to_string(),
            "SSC-A".to_string(),
            "FL1".to_string(),
            "FL2".to_string(),
            "CD3".to_string(),
            "CD4".to_string(),
        ],
        vec![
            vec![10.0, 10.0, 110.0, 70.0, 1.0, 9.0],
            vec![20.0, 20.0, 220.0, 140.0, 5.0, 8.0],
            vec![30.0, 30.0, 330.0, 210.0, 9.0, 1.0],
            vec![80.0, 80.0, 500.0, 250.0, 4.0, 2.0],
        ],
    )
    .expect("valid golden sample")
}

fn compensation_matrix() -> CompensationMatrix {
    CompensationMatrix {
        source_key: "$SPILLOVER".to_string(),
        dimension: 2,
        parameter_names: vec!["FL1".to_string(), "FL2".to_string()],
        values: vec![1.0, 0.2, 0.0, 1.0],
    }
}

fn analysis_profile() -> SampleAnalysisProfile {
    let mut transforms = BTreeMap::new();
    transforms.insert("FL1".to_string(), ChannelTransform::SignedLog10);
    transforms.insert(
        "FL2".to_string(),
        ChannelTransform::Asinh { cofactor: 70.0 },
    );
    SampleAnalysisProfile {
        compensation_enabled: true,
        transforms,
    }
}

fn processed_sample() -> SampleFrame {
    apply_sample_analysis(
        &raw_sample(),
        Some(&compensation_matrix()),
        &analysis_profile(),
    )
    .expect("analysis succeeds")
}

fn gating_log() -> CommandLog {
    let mut log = CommandLog::new();
    log.append(Command::RectangleGate {
        sample_id: "golden-sample".to_string(),
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
        sample_id: "golden-sample".to_string(),
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
    log
}

#[test]
fn golden_compensation_and_transforms_are_numerically_stable() {
    let processed = processed_sample();
    let events = processed.events();

    assert_close(events[0][2], 1.9867717342662448);
    assert_close(events[0][3], 0.881373587019543);
    assert_close(events[1][2], 2.2855573090077736);
    assert_close(events[1][3], 1.4436354751788103);
    assert_close(events[2][2], 2.4608978427565478);
    assert_close(events[2][3], 1.8184464592320668);
    assert_close(events[3][2], 2.6541765418779604);
    assert_close(events[3][3], 1.985160492293);

    assert_eq!(events[0][0], 10.0);
    assert_eq!(events[0][1], 10.0);
    assert_eq!(events[0][4], 1.0);
    assert_eq!(events[0][5], 9.0);
}

#[test]
fn golden_gating_replay_and_membership_are_deterministic() {
    let processed = processed_sample();
    let mut environment = ReplayEnvironment::new();
    environment
        .insert_sample(processed)
        .expect("sample insert succeeds");

    let first = gating_log().replay(&environment).expect("first replay");
    let second = gating_log().replay(&environment).expect("second replay");

    assert_eq!(first.execution_hash, second.execution_hash);
    assert_eq!(first.execution_graph, second.execution_graph);

    let lymphocytes = first
        .populations
        .get("lymphocytes")
        .expect("lymphocytes population");
    let child = first
        .populations
        .get("cd3_cd4")
        .expect("cd3_cd4 population");

    assert_eq!(
        lymphocytes.mask.iter_ones().collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    assert_eq!(child.mask.iter_ones().collect::<Vec<_>>(), vec![0, 1]);
    assert_eq!(lymphocytes.matched_events, 3);
    assert_eq!(child.matched_events, 2);
}

#[test]
fn golden_population_stats_match_expected_values() {
    let processed = processed_sample();
    let mut environment = ReplayEnvironment::new();
    environment
        .insert_sample(processed.clone())
        .expect("sample insert succeeds");
    let state = gating_log().replay(&environment).expect("replay succeeds");
    let table = compute_population_stats_table(&processed, &state).expect("stats table");

    let all_events = table
        .iter()
        .find(|entry| entry.population_id == "All Events")
        .expect("all-events stats");
    let lymphocytes = table
        .iter()
        .find(|entry| entry.population_id == "lymphocytes")
        .expect("lymphocyte stats");
    let child = table
        .iter()
        .find(|entry| entry.population_id == "cd3_cd4")
        .expect("child stats");

    assert_eq!(all_events.matched_events, 4);
    assert_eq!(lymphocytes.matched_events, 3);
    assert_eq!(child.matched_events, 2);
    assert_eq!(child.parent_events, 3);
    assert_close(child.frequency_of_all, 0.5);
    assert_close(child.frequency_of_parent, 2.0 / 3.0);

    let child_fl1 = child
        .channel_stats
        .iter()
        .find(|stats| stats.channel == "FL1")
        .expect("FL1 child stats");
    let child_fl2 = child
        .channel_stats
        .iter()
        .find(|stats| stats.channel == "FL2")
        .expect("FL2 child stats");
    let child_cd4 = child
        .channel_stats
        .iter()
        .find(|stats| stats.channel == "CD4")
        .expect("CD4 child stats");

    assert_close(child_fl1.mean.expect("FL1 mean"), 2.1361645216370094);
    assert_close(child_fl1.median.expect("FL1 median"), 2.1361645216370094);
    assert_close(child_fl2.mean.expect("FL2 mean"), 1.1625045310991767);
    assert_close(child_fl2.median.expect("FL2 median"), 1.1625045310991767);
    assert_close(child_cd4.mean.expect("CD4 mean"), 8.5);
    assert_close(child_cd4.median.expect("CD4 median"), 8.5);
}
