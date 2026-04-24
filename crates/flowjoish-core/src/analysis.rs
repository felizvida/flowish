use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{self, Display, Formatter};

use crate::gating::{SampleError, SampleFrame};

#[derive(Clone, Debug, PartialEq)]
pub struct CompensationMatrix {
    pub source_key: String,
    pub dimension: usize,
    pub parameter_names: Vec<String>,
    pub values: Vec<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ChannelTransform {
    Linear,
    SignedLog10,
    Asinh {
        cofactor: f64,
    },
    Biexponential {
        width_basis: f64,
        positive_decades: f64,
        negative_decades: f64,
    },
    Logicle {
        decades: f64,
        linear_width: f64,
    },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SampleAnalysisProfile {
    pub compensation_enabled: bool,
    pub transforms: BTreeMap<String, ChannelTransform>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AnalysisError {
    MissingCompensation,
    InvalidCompensation(String),
    MissingCompensationChannel(String),
    UnknownTransformChannel(String),
    InvalidTransform(String),
    Sample(SampleError),
}

impl Display for AnalysisError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingCompensation => write!(f, "sample does not provide a compensation matrix"),
            Self::InvalidCompensation(message) => f.write_str(message),
            Self::MissingCompensationChannel(channel) => {
                write!(
                    f,
                    "compensation matrix references unknown channel '{channel}'"
                )
            }
            Self::UnknownTransformChannel(channel) => {
                write!(f, "transform references unknown channel '{channel}'")
            }
            Self::InvalidTransform(message) => f.write_str(message),
            Self::Sample(error) => Display::fmt(error, f),
        }
    }
}

impl Error for AnalysisError {}

impl From<SampleError> for AnalysisError {
    fn from(value: SampleError) -> Self {
        Self::Sample(value)
    }
}

impl ChannelTransform {
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::SignedLog10 => "signed_log10",
            Self::Asinh { .. } => "asinh",
            Self::Biexponential { .. } => "biexponential",
            Self::Logicle { .. } => "logicle",
        }
    }

    pub fn apply(&self, value: f64) -> Result<f64, AnalysisError> {
        if !value.is_finite() {
            return Err(AnalysisError::InvalidTransform(
                "transform input must be finite".to_string(),
            ));
        }

        let transformed = match self {
            Self::Linear => value,
            Self::SignedLog10 => value.signum() * (1.0 + value.abs()).log10(),
            Self::Asinh { cofactor } => {
                require_positive_finite(*cofactor, "asinh cofactor")?;
                (value / cofactor).asinh()
            }
            Self::Biexponential {
                width_basis,
                positive_decades,
                negative_decades,
            } => {
                require_positive_finite(*width_basis, "biexponential width basis")?;
                require_positive_finite(*positive_decades, "biexponential positive decades")?;
                require_positive_finite(*negative_decades, "biexponential negative decades")?;

                let decades = if value >= 0.0 {
                    *positive_decades
                } else {
                    *negative_decades
                };
                let normalized = value.abs() / width_basis;
                let linear_component = normalized / (1.0 + normalized);
                let log_component = normalized.ln_1p() / std::f64::consts::LN_10;
                value.signum() * ((linear_component + log_component) / decades)
            }
            Self::Logicle {
                decades,
                linear_width,
            } => {
                require_positive_finite(*decades, "logicle decades")?;
                require_positive_finite(*linear_width, "logicle linear width")?;

                let normalized = value.abs() / linear_width;
                let linear_component = (normalized / (1.0 + normalized)) * linear_width;
                let log_component = normalized.ln_1p() / std::f64::consts::LN_10;
                value.signum() * ((linear_component + log_component) / decades)
            }
        };

        if transformed.is_finite() {
            Ok(transformed)
        } else {
            Err(AnalysisError::InvalidTransform(
                "transform produced a non-finite result".to_string(),
            ))
        }
    }
}

fn require_positive_finite(value: f64, label: &str) -> Result<(), AnalysisError> {
    if !value.is_finite() || value <= 0.0 {
        return Err(AnalysisError::InvalidTransform(format!(
            "{label} must be a positive finite number"
        )));
    }
    Ok(())
}

impl SampleAnalysisProfile {
    pub fn transform_for(&self, channel: &str) -> ChannelTransform {
        self.transforms
            .get(channel)
            .cloned()
            .unwrap_or(ChannelTransform::Linear)
    }
}

pub fn apply_sample_analysis(
    raw_sample: &SampleFrame,
    compensation: Option<&CompensationMatrix>,
    profile: &SampleAnalysisProfile,
) -> Result<SampleFrame, AnalysisError> {
    let mut events = raw_sample.events().to_vec();

    if profile.compensation_enabled {
        let matrix = compensation.ok_or(AnalysisError::MissingCompensation)?;
        apply_compensation_in_place(&mut events, raw_sample, matrix)?;
    }

    for (channel, transform) in &profile.transforms {
        let channel_index = raw_sample
            .channel_index(channel)
            .ok_or_else(|| AnalysisError::UnknownTransformChannel(channel.clone()))?;
        for row in &mut events {
            row[channel_index] = transform.apply(row[channel_index])?;
        }
    }

    SampleFrame::new(
        raw_sample.sample_id().to_string(),
        raw_sample.channels().to_vec(),
        events,
    )
    .map_err(AnalysisError::from)
}

fn apply_compensation_in_place(
    events: &mut [Vec<f64>],
    sample: &SampleFrame,
    matrix: &CompensationMatrix,
) -> Result<(), AnalysisError> {
    if matrix.dimension == 0 {
        return Err(AnalysisError::InvalidCompensation(
            "compensation matrix dimension must be positive".to_string(),
        ));
    }
    if matrix.parameter_names.len() != matrix.dimension {
        return Err(AnalysisError::InvalidCompensation(format!(
            "compensation matrix expected {} parameter names but found {}",
            matrix.dimension,
            matrix.parameter_names.len()
        )));
    }
    if matrix.values.len() != matrix.dimension * matrix.dimension {
        return Err(AnalysisError::InvalidCompensation(format!(
            "compensation matrix expected {} values but found {}",
            matrix.dimension * matrix.dimension,
            matrix.values.len()
        )));
    }
    if matrix.values.iter().any(|value| !value.is_finite()) {
        return Err(AnalysisError::InvalidCompensation(
            "compensation matrix values must be finite".to_string(),
        ));
    }
    let mut seen_parameters = BTreeSet::new();
    for channel in &matrix.parameter_names {
        if !seen_parameters.insert(channel) {
            return Err(AnalysisError::InvalidCompensation(format!(
                "compensation matrix includes duplicate channel '{channel}'"
            )));
        }
    }

    let channel_indices = matrix
        .parameter_names
        .iter()
        .map(|channel| {
            sample
                .channel_index(channel)
                .ok_or_else(|| AnalysisError::MissingCompensationChannel(channel.clone()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let inverse = invert_matrix(matrix.dimension, &matrix.values)?;

    for row in events {
        let observed = channel_indices
            .iter()
            .map(|index| row[*index])
            .collect::<Vec<_>>();
        let corrected = multiply_matrix_vector(&inverse, matrix.dimension, &observed);
        for (position, channel_index) in channel_indices.iter().enumerate() {
            row[*channel_index] = corrected[position];
        }
    }

    Ok(())
}

fn invert_matrix(dimension: usize, values: &[f64]) -> Result<Vec<f64>, AnalysisError> {
    const EPSILON: f64 = 1e-12;

    let width = dimension * 2;
    let mut augmented = vec![0.0; dimension * width];
    for row in 0..dimension {
        for column in 0..dimension {
            augmented[row * width + column] = values[row * dimension + column];
        }
        augmented[row * width + dimension + row] = 1.0;
    }

    for pivot in 0..dimension {
        let mut best_row = pivot;
        let mut best_value = augmented[pivot * width + pivot].abs();
        for row in (pivot + 1)..dimension {
            let candidate = augmented[row * width + pivot].abs();
            if candidate > best_value {
                best_row = row;
                best_value = candidate;
            }
        }

        if best_value <= EPSILON {
            return Err(AnalysisError::InvalidCompensation(
                "compensation matrix is not invertible".to_string(),
            ));
        }

        if best_row != pivot {
            for column in 0..width {
                augmented.swap(pivot * width + column, best_row * width + column);
            }
        }

        let pivot_value = augmented[pivot * width + pivot];
        for column in 0..width {
            augmented[pivot * width + column] /= pivot_value;
        }

        for row in 0..dimension {
            if row == pivot {
                continue;
            }
            let factor = augmented[row * width + pivot];
            if factor.abs() <= EPSILON {
                continue;
            }
            for column in 0..width {
                augmented[row * width + column] -= factor * augmented[pivot * width + column];
            }
        }
    }

    let mut inverse = vec![0.0; dimension * dimension];
    for row in 0..dimension {
        for column in 0..dimension {
            inverse[row * dimension + column] = augmented[row * width + dimension + column];
        }
    }
    Ok(inverse)
}

fn multiply_matrix_vector(matrix: &[f64], dimension: usize, vector: &[f64]) -> Vec<f64> {
    (0..dimension)
        .map(|row| {
            (0..dimension)
                .map(|column| matrix[row * dimension + column] * vector[column])
                .sum()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{
        AnalysisError, ChannelTransform, CompensationMatrix, SampleAnalysisProfile,
        apply_sample_analysis,
    };
    use crate::gating::SampleFrame;

    fn sample() -> SampleFrame {
        SampleFrame::new(
            "sample-a",
            vec!["FL1".to_string(), "FL2".to_string(), "SSC-A".to_string()],
            vec![vec![110.0, 70.0, 10.0], vec![220.0, 140.0, 20.0]],
        )
        .expect("valid sample")
    }

    #[test]
    fn compensation_is_applied_by_inverting_spillover_matrix() {
        let matrix = CompensationMatrix {
            source_key: "$SPILLOVER".to_string(),
            dimension: 2,
            parameter_names: vec!["FL1".to_string(), "FL2".to_string()],
            values: vec![1.0, 0.2, 0.0, 1.0],
        };
        let profile = SampleAnalysisProfile {
            compensation_enabled: true,
            transforms: BTreeMap::new(),
        };

        let processed = apply_sample_analysis(&sample(), Some(&matrix), &profile)
            .expect("compensation succeeds");

        assert!((processed.events()[0][0] - 96.0).abs() < 1e-9);
        assert!((processed.events()[0][1] - 70.0).abs() < 1e-9);
        assert!((processed.events()[1][0] - 192.0).abs() < 1e-9);
    }

    #[test]
    fn transforms_are_applied_per_channel_after_compensation() {
        let mut transforms = BTreeMap::new();
        transforms.insert("FL1".to_string(), ChannelTransform::SignedLog10);
        transforms.insert("FL2".to_string(), ChannelTransform::Asinh { cofactor: 5.0 });
        let profile = SampleAnalysisProfile {
            compensation_enabled: false,
            transforms,
        };

        let processed =
            apply_sample_analysis(&sample(), None, &profile).expect("transforms succeed");

        assert!((processed.events()[0][0] - (111.0f64).log10()).abs() < 1e-9);
        assert!((processed.events()[0][1] - (14.0f64).asinh()).abs() < 1e-9);
        assert_eq!(processed.events()[0][2], 10.0);
    }

    #[test]
    fn biexponential_and_logicle_transforms_are_supported() {
        let mut transforms = BTreeMap::new();
        transforms.insert(
            "FL1".to_string(),
            ChannelTransform::Biexponential {
                width_basis: 150.0,
                positive_decades: 4.5,
                negative_decades: 1.0,
            },
        );
        transforms.insert(
            "FL2".to_string(),
            ChannelTransform::Logicle {
                decades: 4.5,
                linear_width: 12.0,
            },
        );
        let profile = SampleAnalysisProfile {
            compensation_enabled: false,
            transforms,
        };

        let processed =
            apply_sample_analysis(&sample(), None, &profile).expect("transforms succeed");

        assert!(processed.events()[0][0].is_finite());
        assert!(processed.events()[0][1].is_finite());
        assert!(processed.events()[1][0] > processed.events()[0][0]);
        assert!(processed.events()[1][1] > processed.events()[0][1]);
    }

    #[test]
    fn compensation_requires_a_matrix_when_enabled() {
        let profile = SampleAnalysisProfile {
            compensation_enabled: true,
            transforms: BTreeMap::new(),
        };
        let error = apply_sample_analysis(&sample(), None, &profile).expect_err("missing matrix");
        assert_eq!(error, AnalysisError::MissingCompensation);
    }

    #[test]
    fn rejects_non_finite_compensation_values() {
        let matrix = CompensationMatrix {
            source_key: "$SPILLOVER".to_string(),
            dimension: 2,
            parameter_names: vec!["FL1".to_string(), "FL2".to_string()],
            values: vec![1.0, f64::NAN, 0.0, 1.0],
        };
        let profile = SampleAnalysisProfile {
            compensation_enabled: true,
            transforms: BTreeMap::new(),
        };

        let error = apply_sample_analysis(&sample(), Some(&matrix), &profile)
            .expect_err("non-finite compensation should fail");

        assert_eq!(
            error,
            AnalysisError::InvalidCompensation(
                "compensation matrix values must be finite".to_string()
            )
        );
    }

    #[test]
    fn rejects_duplicate_compensation_channels() {
        let matrix = CompensationMatrix {
            source_key: "$SPILLOVER".to_string(),
            dimension: 2,
            parameter_names: vec!["FL1".to_string(), "FL1".to_string()],
            values: vec![1.0, 0.0, 0.0, 1.0],
        };
        let profile = SampleAnalysisProfile {
            compensation_enabled: true,
            transforms: BTreeMap::new(),
        };

        let error = apply_sample_analysis(&sample(), Some(&matrix), &profile)
            .expect_err("duplicate compensation channels should fail");

        assert_eq!(
            error,
            AnalysisError::InvalidCompensation(
                "compensation matrix includes duplicate channel 'FL1'".to_string()
            )
        );
    }

    #[test]
    fn transform_parameters_must_be_positive_and_finite() {
        let error = ChannelTransform::Logicle {
            decades: 4.5,
            linear_width: 0.0,
        }
        .apply(10.0)
        .expect_err("invalid transform");
        assert_eq!(
            error,
            AnalysisError::InvalidTransform(
                "logicle linear width must be a positive finite number".to_string()
            )
        );
    }
}
