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
}

/// Evaluate one statistic without retaining the sample set.
///
/// Omit `weights` for unit weights. The probability is used only by
/// [`BatchStatistic::Percentile`] (`(0, 1]`) and the two risk measures
/// (`[0.9, 1)`). Risk observations are signed returns: losses are negative,
/// and risk results are nonnegative loss magnitudes. Variance retains the
/// core's `N/(N-1)` correction based on the number of observations, including
/// those with zero weight.
///
/// # Errors
///
/// Returns an error for empty or nonfinite samples, invalid weights, invalid
/// probability, insufficient samples for variance, a zero-weight tail for
/// expected shortfall, or nonfinite arithmetic results.
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
        BatchStatistic::Percentile => {
            require!(
                probability.is_finite() && probability > 0.0 && probability <= 1.0,
                "percentile must be in (0, 1]"
            );
        }
        BatchStatistic::ValueAtRisk | BatchStatistic::ExpectedShortfall => {
            require!(
                probability.is_finite() && (0.9..1.0).contains(&probability),
                "confidence must be in [0.9, 1)"
            );
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
    };
    ensure!(result.is_finite(), "statistic result is nonfinite");
    Ok(result)
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
}
