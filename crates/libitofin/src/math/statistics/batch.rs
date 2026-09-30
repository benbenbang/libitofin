//! Validated, stateless access to weighted empirical statistics.

use crate::errors::QlResult;
use crate::types::Real;
use crate::{ensure, require};

use super::{EmpiricalStatistics, GeneralStatistics, MeanStdDev, RiskStatistics, Statistics};

/// A batch statistic evaluated from signed observations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BatchStatistic {
    /// Weighted arithmetic mean.
    Mean,
    /// Count-corrected sample variance.
    Variance,
    /// Square root of the sample variance.
    StandardDeviation,
    /// Lower empirical percentile.
    Percentile,
    /// Positive magnitude of a loss at the selected confidence.
    ValueAtRisk,
    /// Positive mean loss strictly beyond value at risk.
    ExpectedShortfall,
    /// Count-corrected conditional variance below the weighted mean.
    SemiVariance,
    /// Square root of semi variance.
    SemiDeviation,
    /// Count-corrected conditional variance below zero.
    DownsideVariance,
    /// Square root of downside variance.
    DownsideDeviation,
    /// Count-corrected conditional variance below a target.
    Regret,
    /// Nonnegative gain at a selected confidence.
    PotentialUpside,
    /// Weighted probability of falling strictly below a target.
    Shortfall,
    /// Mean distance below a target, conditional on falling below it.
    AverageShortfall,
    /// Upper empirical percentile.
    TopPercentile,
}

/// Evaluate one statistic without retaining the sample set.
///
/// Omit `weights` for unit weights. The fourth argument selects a percentile
/// for `Percentile` and `TopPercentile` (`(0, 1]`), a confidence for
/// `ValueAtRisk`, `ExpectedShortfall` and `PotentialUpside` (`[0.9, 1)`), or
/// a target for `Regret`, `Shortfall` and `AverageShortfall`. It is ignored by
/// the other measures. Risk observations are signed returns: losses are
/// negative, and VaR and expected shortfall return nonnegative loss magnitudes.
/// Variance and conditional risk measures use the core's count correction,
/// including observations with zero weight in the sample count.
///
/// # Errors
///
/// Returns an error for empty or nonfinite samples, invalid weights, invalid
/// probability or target, insufficient samples for variance or conditional
/// risk measures, a zero-weight conditional tail, or nonfinite arithmetic.
pub fn evaluate_batch(
    observations: &[Real],
    weights: Option<&[Real]>,
    measure: BatchStatistic,
    probability: Real,
) -> QlResult<Real> {
    require!(!observations.is_empty(), "empty sample set");
    if let Some(weights) = weights {
        require!(
            weights.len() == observations.len(),
            "observation and weight lengths differ"
        );
    }
    for (index, &value) in observations.iter().enumerate() {
        require!(value.is_finite(), "nonfinite observation at index {index}");
    }
    let total_weight = if let Some(weights) = weights {
        for (index, &weight) in weights.iter().enumerate() {
            require!(
                weight.is_finite() && weight >= 0.0,
                "invalid weight at index {index}"
            );
        }
        weights.iter().sum::<Real>()
    } else {
        observations.len() as Real
    };
    require!(
        total_weight.is_finite() && total_weight > 0.0,
        "total weight must be finite and positive"
    );
    match measure {
        BatchStatistic::Percentile | BatchStatistic::TopPercentile => {
            require!(
                probability.is_finite() && probability > 0.0 && probability <= 1.0,
                "percentile must be in (0, 1]"
            );
        }
        BatchStatistic::ValueAtRisk
        | BatchStatistic::ExpectedShortfall
        | BatchStatistic::PotentialUpside => {
            require!(
                probability.is_finite() && (0.9..1.0).contains(&probability),
                "confidence must be in [0.9, 1)"
            );
        }
        BatchStatistic::Regret | BatchStatistic::Shortfall | BatchStatistic::AverageShortfall => {
            require!(probability.is_finite(), "target must be finite");
        }
        _ => {}
    }

    let mut statistics = GeneralStatistics::new();
    statistics.reserve(observations.len());
    for (index, &value) in observations.iter().enumerate() {
        statistics.add_weighted(value, weights.map_or(1.0, |weights| weights[index]))?;
    }
    let result = match measure {
        BatchStatistic::Mean => statistics.mean()?,
        BatchStatistic::Variance => statistics.variance()?,
        BatchStatistic::StandardDeviation => statistics.standard_deviation()?,
        BatchStatistic::Percentile => statistics.percentile(probability)?,
        BatchStatistic::ValueAtRisk => statistics.value_at_risk(probability)?,
        BatchStatistic::ExpectedShortfall => {
            let threshold = -statistics.value_at_risk(probability)?;
            ensure!(
                statistics
                    .data()
                    .iter()
                    .any(|&(value, weight)| value < threshold && weight > 0.0),
                "no positive-weight data below the value-at-risk threshold"
            );
            statistics.expected_shortfall(probability)?
        }
        BatchStatistic::SemiVariance | BatchStatistic::SemiDeviation => {
            let threshold = statistics.mean()?;
            ensure!(threshold.is_finite(), "statistic result is nonfinite");
            require_positive_tail(&statistics, threshold)?;
            if measure == BatchStatistic::SemiVariance {
                statistics.semi_variance()?
            } else {
                statistics.semi_deviation()?
            }
        }
        BatchStatistic::DownsideVariance | BatchStatistic::DownsideDeviation => {
            require_positive_tail(&statistics, 0.0)?;
            if measure == BatchStatistic::DownsideVariance {
                statistics.downside_variance()?
            } else {
                statistics.downside_deviation()?
            }
        }
        BatchStatistic::Regret => {
            require_positive_tail(&statistics, probability)?;
            statistics.regret(probability)?
        }
        BatchStatistic::PotentialUpside => statistics.potential_upside(probability)?,
        BatchStatistic::Shortfall => statistics.shortfall(probability)?,
        BatchStatistic::AverageShortfall => {
            require_positive_tail(&statistics, probability)?;
            statistics.average_shortfall(probability)?
        }
        BatchStatistic::TopPercentile => statistics.top_percentile(probability)?,
    };
    ensure!(result.is_finite(), "statistic result is nonfinite");
    Ok(result)
}

fn require_positive_tail(statistics: &GeneralStatistics, threshold: Real) -> QlResult<()> {
    ensure!(
        statistics
            .data()
            .iter()
            .any(|&(value, weight)| value < threshold && weight > 0.0),
        "no positive-weight data below the target"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::distributions::normal::{CumulativeNormalDistribution, NormalDistribution};
    use crate::math::statistics::testutil::{check, sobol_normal_samples};

    #[test]
    fn weighted_values_match_general_statistics_conventions() {
        let values = [-4.0, -2.0, 2.0, 8.0];
        let weights = [1.0, 2.0, 1.0, 0.0];
        let measure = |measure, probability| {
            evaluate_batch(&values, Some(&weights), measure, probability).unwrap()
        };
        assert_eq!(measure(BatchStatistic::Mean, 0.0), -1.5);
        assert_eq!(measure(BatchStatistic::Variance, 0.0), 19.0 / 3.0);
        assert_eq!(
            measure(BatchStatistic::StandardDeviation, 0.0),
            (19.0_f64 / 3.0).sqrt()
        );
        assert_eq!(measure(BatchStatistic::Percentile, 0.25), -4.0);
        assert_eq!(measure(BatchStatistic::Percentile, 0.75), -2.0);
        assert_eq!(measure(BatchStatistic::Percentile, 1.0), 2.0);
        assert_eq!(measure(BatchStatistic::ValueAtRisk, 0.9), 4.0);
        assert!(
            evaluate_batch(
                &values,
                Some(&weights),
                BatchStatistic::ExpectedShortfall,
                0.9
            )
            .is_err()
        );

        let values = [-8.0, -4.0, -2.0, 2.0];
        let weights = [0.1, 2.0, 1.0, 1.0];
        assert_eq!(
            evaluate_batch(
                &values,
                Some(&weights),
                BatchStatistic::ExpectedShortfall,
                0.9
            )
            .unwrap(),
            8.0
        );
        assert!(
            evaluate_batch(
                &values,
                Some(&[0.0, 2.0, 1.0, 1.0]),
                BatchStatistic::ExpectedShortfall,
                0.9
            )
            .is_err()
        );
        assert_eq!(
            evaluate_batch(&[3.0], None, BatchStatistic::Mean, 0.0).unwrap(),
            3.0
        );
    }

    #[test]
    fn invalid_samples_and_parameters_fail() {
        let values = [1.0, 2.0];
        let check = |values: &[Real], weights: Option<&[Real]>, measure, probability| {
            evaluate_batch(values, weights, measure, probability).is_err()
        };
        assert!(check(&[], None, BatchStatistic::Mean, 0.0));
        assert!(check(&values, Some(&[1.0]), BatchStatistic::Mean, 0.0));
        assert!(check(&[Real::NAN], None, BatchStatistic::Mean, 0.0));
        assert!(check(&[Real::INFINITY], None, BatchStatistic::Mean, 0.0));
        assert!(check(
            &values,
            Some(&[1.0, -1.0]),
            BatchStatistic::Mean,
            0.0
        ));
        assert!(check(
            &values,
            Some(&[1.0, Real::NAN]),
            BatchStatistic::Mean,
            0.0
        ));
        assert!(check(&values, Some(&[0.0, 0.0]), BatchStatistic::Mean, 0.0));
        assert!(check(&[1.0], None, BatchStatistic::Variance, 0.0));
        assert!(check(&[1.0], None, BatchStatistic::StandardDeviation, 0.0));
        assert!(check(&values, None, BatchStatistic::Percentile, Real::NAN));
        assert!(check(&values, None, BatchStatistic::Percentile, 0.0));
        assert!(check(&values, None, BatchStatistic::Percentile, 1.1));
        assert!(check(&values, None, BatchStatistic::ValueAtRisk, Real::NAN));
        assert!(check(&values, None, BatchStatistic::ValueAtRisk, 0.89));
        assert!(check(&values, None, BatchStatistic::ExpectedShortfall, 1.0));
        assert!(check(
            &[Real::MAX, Real::MAX],
            None,
            BatchStatistic::Variance,
            0.0
        ));
    }

    #[test]
    fn batch_facade_matches_quantlib_riskstats_oracle() {
        let samples = sobol_normal_samples(0.0, 1.0);
        let confidence = CumulativeNormalDistribution::new(0.0, 1.0)
            .unwrap()
            .value(2.0);
        let density = NormalDistribution::new(0.0, 1.0).unwrap();
        let evaluate =
            |measure, probability| evaluate_batch(&samples, None, measure, probability).unwrap();
        check(
            "mean",
            0.0,
            1.0,
            evaluate(BatchStatistic::Mean, 0.0),
            0.0,
            1e-13,
        );
        check(
            "variance",
            0.0,
            1.0,
            evaluate(BatchStatistic::Variance, 0.0),
            1.0,
            1e-1,
        );
        check(
            "percentile",
            0.0,
            1.0,
            evaluate(BatchStatistic::Percentile, 0.5),
            0.0,
            1e-3,
        );
        check(
            "value-at-risk",
            0.0,
            1.0,
            evaluate(BatchStatistic::ValueAtRisk, confidence),
            2.0,
            2e-3,
        );
        let expected_shortfall = density.value(-2.0) / (1.0 - confidence);
        check(
            "expected shortfall",
            0.0,
            1.0,
            evaluate(BatchStatistic::ExpectedShortfall, confidence),
            expected_shortfall,
            expected_shortfall * 1e-2,
        );
        check(
            "potential upside",
            0.0,
            1.0,
            evaluate(BatchStatistic::PotentialUpside, confidence),
            2.0,
            2e-3,
        );
        check(
            "shortfall",
            0.0,
            1.0,
            evaluate(BatchStatistic::Shortfall, 0.0),
            0.5,
            5e-4,
        );
        let average_shortfall = (2.0 / std::f64::consts::PI).sqrt();
        check(
            "average shortfall",
            0.0,
            1.0,
            evaluate(BatchStatistic::AverageShortfall, 0.0),
            average_shortfall,
            average_shortfall * 1e-3,
        );
        for kind in [
            BatchStatistic::SemiVariance,
            BatchStatistic::DownsideVariance,
            BatchStatistic::Regret,
        ] {
            check(
                "conditional variance",
                0.0,
                1.0,
                evaluate(kind, 0.0),
                1.0,
                1e-1,
            );
        }
    }

    #[test]
    fn weighted_risk_parity_vector() {
        let values = [-5.0, -2.0, 1.0, 2.0];
        let weights = [1.0, 10.0, 1.0, 8.0];
        assert_eq!(
            evaluate_batch(&values, Some(&weights), BatchStatistic::ValueAtRisk, 0.9).unwrap(),
            2.0
        );
        assert_eq!(
            evaluate_batch(
                &values,
                Some(&weights),
                BatchStatistic::ExpectedShortfall,
                0.9
            )
            .unwrap(),
            5.0
        );
    }

    #[test]
    fn remaining_weighted_risk_measures_follow_conditional_conventions() {
        let values = [-4.0, -2.0, 2.0, 8.0];
        let weights = [1.0, 2.0, 1.0, 0.0];
        let measure =
            |kind, argument| evaluate_batch(&values, Some(&weights), kind, argument).unwrap();
        assert_eq!(measure(BatchStatistic::SemiVariance, 0.0), 4.5);
        assert_eq!(measure(BatchStatistic::SemiDeviation, 0.0), 4.5_f64.sqrt());
        assert_eq!(measure(BatchStatistic::DownsideVariance, 0.0), 16.0);
        assert_eq!(measure(BatchStatistic::DownsideDeviation, 0.0), 4.0);
        assert_eq!(measure(BatchStatistic::Regret, 1.0), 86.0 / 3.0);
        assert_eq!(measure(BatchStatistic::PotentialUpside, 0.9), 2.0);
        assert_eq!(measure(BatchStatistic::Shortfall, 1.0), 0.75);
        assert_eq!(measure(BatchStatistic::AverageShortfall, 1.0), 11.0 / 3.0);
        assert_eq!(measure(BatchStatistic::TopPercentile, 0.5), -2.0);
        assert_eq!(measure(BatchStatistic::TopPercentile, 0.25), 2.0);
        assert_eq!(measure(BatchStatistic::Shortfall, -5.0), 0.0);
    }

    #[test]
    fn conditional_measures_reject_empty_or_zero_weight_tails() {
        for kind in [
            BatchStatistic::SemiVariance,
            BatchStatistic::SemiDeviation,
            BatchStatistic::DownsideVariance,
            BatchStatistic::DownsideDeviation,
        ] {
            assert!(evaluate_batch(&[1.0, 2.0], None, kind, 0.0).is_err());
        }
        for kind in [
            BatchStatistic::SemiVariance,
            BatchStatistic::SemiDeviation,
            BatchStatistic::Regret,
            BatchStatistic::AverageShortfall,
        ] {
            let target =
                if kind == BatchStatistic::Regret || kind == BatchStatistic::AverageShortfall {
                    1.5
                } else {
                    0.0
                };
            assert!(evaluate_batch(&[-1.0, 2.0], Some(&[0.0, 1.0]), kind, target).is_err());
        }
        assert!(evaluate_batch(&[-1.0, 2.0], None, BatchStatistic::Regret, -1.0).is_err());
        assert!(
            evaluate_batch(&[-1.0, 2.0], None, BatchStatistic::AverageShortfall, -1.0).is_err()
        );
        assert!(evaluate_batch(&[-1.0, 2.0], None, BatchStatistic::Regret, 0.0).is_err());
        assert!(evaluate_batch(&[-1.0, 2.0], None, BatchStatistic::AverageShortfall, 0.0).is_ok());
    }

    #[test]
    fn new_parameters_and_nonfinite_arithmetic_fail() {
        let values = [-1.0, 2.0];
        for kind in [
            BatchStatistic::Regret,
            BatchStatistic::Shortfall,
            BatchStatistic::AverageShortfall,
        ] {
            assert!(evaluate_batch(&values, None, kind, Real::NAN).is_err());
            assert!(evaluate_batch(&values, None, kind, Real::INFINITY).is_err());
        }
        for kind in [
            BatchStatistic::PotentialUpside,
            BatchStatistic::TopPercentile,
        ] {
            assert!(evaluate_batch(&values, None, kind, Real::NAN).is_err());
        }
        assert!(evaluate_batch(&values, None, BatchStatistic::PotentialUpside, 0.89).is_err());
        assert!(evaluate_batch(&values, None, BatchStatistic::PotentialUpside, 1.0).is_err());
        assert!(evaluate_batch(&values, None, BatchStatistic::TopPercentile, 0.0).is_err());
        assert!(evaluate_batch(&values, None, BatchStatistic::TopPercentile, 1.1).is_err());
        assert!(evaluate_batch(&[1e300, -1e300], None, BatchStatistic::Regret, 0.0).is_err());
    }
}
