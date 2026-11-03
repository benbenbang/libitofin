//! Brownian-bridge construction of Wiener paths.
//!
//! Port of `ql/methods/montecarlo/brownianbridge.{hpp,cpp}`: turns a sequence of
//! Gaussian variates into normalized (unit-variance) path variations, drawing
//! the global step first and then bisecting, so the first variates carry the
//! most variance (the property quasi-random generators exploit).
//!
//! Divergences from `brownianbridge.hpp`, all deliberate:
//! - **fallible constructors**: C++ indexes `size_-1` in `initialize()` without
//!   checking, so zero steps (or a single-node [`TimeGrid`]) is undefined
//!   behaviour; here the constructors return `Err` (D4/D10).
//! - **`transform` is fallible**: the two `QL_REQUIRE`s
//!   (`brownianbridge.hpp:110-111`) become `Err`, and the unchecked output
//!   iterator becomes a slice whose length is checked too, so `transform`
//!   never panics.

use crate::errors::QlResult;
use crate::math::timegrid::TimeGrid;
use crate::require;
use crate::types::{Real, Size, Time};

/// Builds Wiener process paths from Gaussian variates by Brownian bridging.
#[derive(Clone, Debug, PartialEq)]
pub struct BrownianBridge {
    size: Size,
    t: Vec<Time>,
    sqrtdt: Vec<Real>,
    bridge_index: Vec<Size>,
    left_index: Vec<Size>,
    right_index: Vec<Size>,
    left_weight: Vec<Real>,
    right_weight: Vec<Real>,
    std_dev: Vec<Real>,
}

impl BrownianBridge {
    /// A bridge over `steps` unit-length steps, times `1..=steps`
    /// (`brownianbridge.cpp:34`).
    ///
    /// # Errors
    /// Returns `Err` when `steps` is zero.
    pub fn from_steps(steps: Size) -> QlResult<Self> {
        let times: Vec<Time> = (1..=steps).map(|i| i as Time).collect();
        Self::from_times(&times)
    }

    /// A bridge over the given step times (`brownianbridge.cpp:43`); the
    /// starting time 0 is implied and must not be included.
    ///
    /// # Errors
    /// Returns `Err` when `times` is empty.
    pub fn from_times(times: &[Time]) -> QlResult<Self> {
        require!(
            !times.is_empty(),
            "brownian bridge requires at least one step"
        );
        let size = times.len();
        let mut bridge = BrownianBridge {
            size,
            t: times.to_vec(),
            sqrtdt: vec![0.0; size],
            bridge_index: vec![0; size],
            left_index: vec![0; size],
            right_index: vec![0; size],
            left_weight: vec![0.0; size],
            right_weight: vec![0.0; size],
            std_dev: vec![0.0; size],
        };
        bridge.initialize();
        Ok(bridge)
    }

    /// A bridge over the nodes of `time_grid` after the initial one
    /// (`brownianbridge.cpp:50`).
    ///
    /// # Errors
    /// Returns `Err` when the grid has fewer than two nodes.
    pub fn from_time_grid(time_grid: &TimeGrid) -> QlResult<Self> {
        Self::from_times(time_grid.times().get(1..).unwrap_or(&[]))
    }

    fn initialize(&mut self) {
        let size = self.size;
        let t = &self.t;
        self.sqrtdt[0] = t[0].sqrt();
        for i in 1..size {
            self.sqrtdt[i] = (t[i] - t[i - 1]).sqrt();
        }

        let mut map: Vec<Size> = vec![0; size];
        map[size - 1] = 1;
        self.bridge_index[0] = size - 1;
        self.std_dev[0] = t[size - 1].sqrt();
        self.left_weight[0] = 0.0;
        self.right_weight[0] = 0.0;
        let mut j = 0;
        for i in 1..size {
            while map[j] != 0 {
                j += 1;
            }
            let mut k = j;
            while map[k] == 0 {
                k += 1;
            }
            let l = j + ((k - 1 - j) >> 1);
            map[l] = i;
            self.bridge_index[i] = l;
            self.left_index[i] = j;
            self.right_index[i] = k;
            if j != 0 {
                self.left_weight[i] = (t[k] - t[l]) / (t[k] - t[j - 1]);
                self.right_weight[i] = (t[l] - t[j - 1]) / (t[k] - t[j - 1]);
                self.std_dev[i] = (((t[l] - t[j - 1]) * (t[k] - t[l])) / (t[k] - t[j - 1])).sqrt();
            } else {
                self.left_weight[i] = (t[k] - t[l]) / t[k];
                self.right_weight[i] = t[l] / t[k];
                self.std_dev[i] = (t[l] * (t[k] - t[l]) / t[k]).sqrt();
            }
            j = k + 1;
            if j >= size {
                j = 0;
            }
        }
    }

    /// The number of steps.
    pub fn size(&self) -> Size {
        self.size
    }

    /// The step times.
    pub fn times(&self) -> &[Time] {
        &self.t
    }

    /// The path point constructed by each variate.
    pub fn bridge_index(&self) -> &[Size] {
        &self.bridge_index
    }

    /// The left-neighbour index used for each variate.
    pub fn left_index(&self) -> &[Size] {
        &self.left_index
    }

    /// The right-neighbour index used for each variate.
    pub fn right_index(&self) -> &[Size] {
        &self.right_index
    }

    /// The left-neighbour interpolation weights.
    pub fn left_weight(&self) -> &[Real] {
        &self.left_weight
    }

    /// The right-neighbour interpolation weights.
    pub fn right_weight(&self) -> &[Real] {
        &self.right_weight
    }

    /// The conditional standard deviation applied to each variate.
    pub fn std_deviation(&self) -> &[Real] {
        &self.std_dev
    }

    /// Transforms Gaussian variates `input` into normalized path variations
    /// written to `output` (`brownianbridge.hpp:105`).
    ///
    /// A zero first variate gives the canonical bridge pinned at both ends; a
    /// non-zero one gives a sloped bridge ending at `sqrt(t_last) * input[0]`.
    ///
    /// # Errors
    /// Returns `Err` when `input` or `output` does not have [`size`](Self::size)
    /// elements.
    pub fn transform(&self, input: &[Real], output: &mut [Real]) -> QlResult<()> {
        let size = self.size;
        require!(input.len() == size, "incompatible sequence size");
        require!(output.len() == size, "incompatible output sequence size");
        output[size - 1] = self.std_dev[0] * input[0];
        for (i, &variate) in input.iter().enumerate().skip(1) {
            let j = self.left_index[i];
            let k = self.right_index[i];
            let l = self.bridge_index[i];
            output[l] = if j != 0 {
                self.left_weight[i] * output[j - 1]
                    + self.right_weight[i] * output[k]
                    + self.std_dev[i] * variate
            } else {
                self.right_weight[i] * output[k] + self.std_dev[i] * variate
            };
        }
        for i in (1..size).rev() {
            output[i] -= output[i - 1];
            output[i] /= self.sqrtdt[i];
        }
        output[0] /= self.sqrtdt[0];
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::math::distributions::normal::InverseCumulativeNormal;
    use crate::math::randomnumbers::sobol::{DirectionIntegers, SobolRsg};
    use crate::math::randomnumbers::{InverseCumulativeRsg, SequenceGenerator, SobolSequence};

    fn max_diff(a: &[Real], b: &[Real]) -> Real {
        a.iter()
            .zip(b)
            .map(|(x, y)| (x - y).abs())
            .fold(0.0, Real::max)
    }

    pub(crate) struct Moments {
        n: usize,
        samples: usize,
        sum: Vec<Real>,
        quadratic_sum: Vec<Real>,
    }

    impl Moments {
        pub(crate) fn new(n: usize) -> Self {
            Moments {
                n,
                samples: 0,
                sum: vec![0.0; n],
                quadratic_sum: vec![0.0; n * n],
            }
        }

        pub(crate) fn add(&mut self, x: &[Real]) {
            for i in 0..self.n {
                for j in 0..self.n {
                    self.quadratic_sum[i * self.n + j] += x[i] * x[j];
                }
                self.sum[i] += x[i];
            }
            self.samples += 1;
        }

        pub(crate) fn mean(&self) -> Vec<Real> {
            let count = self.samples as Real;
            self.sum.iter().map(|s| s / count).collect()
        }

        pub(crate) fn covariance(&self) -> Vec<Real> {
            let count = self.samples as Real;
            let mean = self.mean();
            let mut result = vec![0.0; self.n * self.n];
            for i in 0..self.n {
                for j in 0..self.n {
                    result[i * self.n + j] = (self.quadratic_sum[i * self.n + j] / count
                        - mean[i] * mean[j])
                        * (count / (count - 1.0));
                }
            }
            result
        }
    }

    #[test]
    fn variates_match_brownian_moments() {
        // test-suite/brownianbridge.cpp:62 testVariates. The crate's
        // SequenceStatistics caps rows at 100_000 (sequence_shape.rs:7), so the
        // QL GenericSequenceStatistics sums (sequencestatistics.hpp:135-139) are
        // accumulated locally instead.
        let times = [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 2.0, 5.0];
        let n = times.len();
        let samples = 262143;
        let mut generator = InverseCumulativeRsg::new(
            SobolSequence::new(SobolRsg::new(n, 42, DirectionIntegers::Jaeckel)),
            InverseCumulativeNormal::standard(),
        );
        let bridge = BrownianBridge::from_times(&times).unwrap();
        let mut stats1 = Moments::new(n);
        let mut stats2 = Moments::new(n);
        let mut temp = vec![0.0; n];
        for _ in 0..samples {
            let sample = &generator.next_sequence().value;
            bridge.transform(sample, &mut temp).unwrap();
            stats1.add(&temp);
            temp[0] *= times[0].sqrt();
            for j in 1..n {
                temp[j] = temp[j - 1] + temp[j] * (times[j] - times[j - 1]).sqrt();
            }
            stats2.add(&temp);
        }

        let mean_tolerance = 1.0e-16;
        let expected_mean = vec![0.0; n];
        let mut expected_cov = vec![0.0; n * n];
        for i in 0..n {
            expected_cov[i * n + i] = 1.0;
        }
        let mean_error = max_diff(&stats1.mean(), &expected_mean);
        let cov_error = max_diff(&stats1.covariance(), &expected_cov);
        assert!(mean_error <= mean_tolerance, "mean error {mean_error}");
        assert!(cov_error <= 2.5e-4, "covariance error {cov_error}");

        for i in 0..n {
            for j in i..n {
                expected_cov[i * n + j] = times[i];
                expected_cov[j * n + i] = times[i];
            }
        }
        let mean_error = max_diff(&stats2.mean(), &expected_mean);
        let cov_error = max_diff(&stats2.covariance(), &expected_cov);
        assert!(mean_error <= mean_tolerance, "mean error {mean_error}");
        assert!(cov_error <= 6.0e-4, "covariance error {cov_error}");
    }

    #[test]
    fn constructors_agree_on_unit_steps() {
        let from_steps = BrownianBridge::from_steps(5).unwrap();
        let from_times = BrownianBridge::from_times(&[1.0, 2.0, 3.0, 4.0, 5.0]).unwrap();
        let grid = TimeGrid::new(5.0, 5).unwrap();
        let from_grid = BrownianBridge::from_time_grid(&grid).unwrap();
        assert_eq!(from_steps, from_times);
        assert_eq!(from_grid, from_times);
        assert_eq!(from_steps.size(), 5);
        assert_eq!(from_steps.times(), &[1.0, 2.0, 3.0, 4.0, 5.0]);
        assert_eq!(from_steps.bridge_index(), &[4, 1, 0, 2, 3]);
        assert_eq!(from_steps.left_index(), &[0, 0, 0, 2, 3]);
        assert_eq!(from_steps.right_index(), &[0, 4, 1, 4, 4]);
    }

    #[test]
    fn sloped_bridge_ends_at_scaled_first_variate() {
        let times = [0.5, 1.0, 3.0, 4.0];
        let bridge = BrownianBridge::from_times(&times).unwrap();
        let mut output = [0.0; 4];
        bridge
            .transform(&[1.5, 0.0, 0.0, 0.0], &mut output)
            .unwrap();
        let mut level = output[0] * times[0].sqrt();
        for j in 1..4 {
            level += output[j] * (times[j] - times[j - 1]).sqrt();
        }
        assert!((level - 4.0_f64.sqrt() * 1.5).abs() < 1e-14);
    }

    #[test]
    fn rejects_empty_and_mismatched_lengths() {
        assert!(BrownianBridge::from_times(&[]).is_err());
        assert!(BrownianBridge::from_steps(0).is_err());
        assert!(BrownianBridge::from_time_grid(&TimeGrid::default()).is_err());
        let bridge = BrownianBridge::from_steps(3).unwrap();
        let mut output = [0.0; 3];
        assert!(bridge.transform(&[0.0; 2], &mut output).is_err());
        assert!(bridge.transform(&[0.0; 3], &mut [0.0; 2]).is_err());
        assert!(bridge.transform(&[0.0; 3], &mut output).is_ok());
    }
}
