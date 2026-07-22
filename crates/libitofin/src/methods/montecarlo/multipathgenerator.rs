//! Multi-factor path generator.
//!
//! Port of `ql/methods/montecarlo/multipathgenerator.hpp`: evolves a
//! multi-factor [`StochasticProcess`] over a [`TimeGrid`], drawing the Gaussian
//! increments from a [`SequenceGenerator`]. Each asset's initial value sits at
//! `path[j][0]`; step `i` consumes the `factors`-wide draw block at
//! `offset = (i-1) * factors`, evolves the shared `asset` state through
//! `process.evolve`, and writes `path[j][i] = asset[j]`
//! (`multipathgenerator.hpp:131-147`).
//!
//! Divergences from `multipathgenerator.hpp`, all deliberate:
//! - **`next`/`antithetic` take `&mut self` and return by value**: C++ mutates a
//!   cached `next_` member through `const` methods and returns a reference
//!   (`multipathgenerator.hpp:58-59,120`). Rust's
//!   [`SequenceGenerator::next_sequence`] needs `&mut self`, and returning an
//!   owned `Sample<MultiPath>` per call keeps the borrow simple (the #450
//!   single-factor precedent).
//! - **Brownian bridge rejected at construction**: C++ stores the flag and
//!   `QL_FAIL`s at call time inside `next(bool)` (`multipathgenerator.hpp:106-108`).
//!   Here the constructor returns `Err` when `brownian_bridge` is `true`,
//!   matching the single-factor [`PathGenerator`](super::PathGenerator)
//!   precedent (`pathgenerator.rs:95-99`); the flag is never stored. Visible
//!   deferral naming #453.
//! - **Guard reorder**: C++ checks the dimension invariant before
//!   `times.size() > 1` (`multipathgenerator.hpp:79-87`). Here `times.size() > 1`
//!   is checked first so the `times.size() - 1` in the dimension message cannot
//!   underflow (C++'s unsigned wrap hides this; Rust's `usize` would panic).
//! - **`next` is fallible**: `evolve`/`initial_values` return `QlResult` on main
//!   (`stochasticprocess.rs:133,165`); a mid-path `Err` is a setup/data error,
//!   not a per-sample outcome, so it aborts the whole call.

use crate::errors::QlResult;
use crate::math::array::Array;
use crate::math::randomnumbers::rngtraits::SequenceGenerator;
use crate::math::timegrid::TimeGrid;
use crate::methods::montecarlo::{MultiPath, Sample};
use crate::require;
use crate::shared::Shared;
use crate::stochasticprocess::StochasticProcess;

/// Generates correlated multi-asset paths from a Gaussian sequence generator.
pub struct MultiPathGenerator<GSG> {
    generator: GSG,
    process: Shared<dyn StochasticProcess>,
    time_grid: TimeGrid,
}

impl<GSG: SequenceGenerator> MultiPathGenerator<GSG> {
    /// A generator driving `process` over `time_grid`
    /// (`multipathgenerator.hpp:54-57`).
    ///
    /// # Errors
    ///
    /// Returns `Err` if `brownian_bridge` is `true` (deferred, #453), if
    /// `time_grid` has one point or fewer (`multipathgenerator.hpp:86`), or if
    /// the generator's dimensionality does not equal
    /// `process.factors() * (time_grid.size() - 1)`
    /// (`multipathgenerator.hpp:79-84`).
    pub fn new(
        process: Shared<dyn StochasticProcess>,
        time_grid: TimeGrid,
        generator: GSG,
        brownian_bridge: bool,
    ) -> QlResult<Self> {
        require!(
            !brownian_bridge,
            "brownian bridge multi-path generation is not yet ported (see #453); \
             only the direct-copy variant is available"
        );
        require!(time_grid.size() > 1, "no times given");
        let factors = process.factors();
        let steps = time_grid.size() - 1;
        let expected = factors * steps;
        let dimension = generator.dimension();
        require!(
            dimension == expected,
            "dimension ({dimension}) is not equal to ({factors} * {steps}) \
             the number of factors times the number of time steps"
        );
        Ok(MultiPathGenerator {
            generator,
            process,
            time_grid,
        })
    }

    /// The time grid the paths are sampled on.
    pub fn time_grid(&self) -> &TimeGrid {
        &self.time_grid
    }

    /// Draws the next forward path (`multipathgenerator.hpp:92-93`).
    ///
    /// # Errors
    ///
    /// Propagates any `initial_values`/`evolve` error from the process, aborting
    /// the path.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> QlResult<Sample<MultiPath>> {
        self.generate(false)
    }

    /// Draws the antithetic partner of the last forward path: the same draws
    /// negated (`multipathgenerator.hpp:97-99`).
    ///
    /// # Errors
    ///
    /// Propagates any `initial_values`/`evolve` error from the process, aborting
    /// the path.
    pub fn antithetic(&mut self) -> QlResult<Sample<MultiPath>> {
        self.generate(true)
    }

    /// The `next(bool)` core (`multipathgenerator.hpp:102-151`, the
    /// `brownian_bridge == false` branch, which is the only one constructed).
    fn generate(&mut self, antithetic: bool) -> QlResult<Sample<MultiPath>> {
        let (weight, draws) = {
            let sequence = if antithetic {
                self.generator.last_sequence()
            } else {
                self.generator.next_sequence()
            };
            (sequence.weight, sequence.value.clone())
        };

        let m = self.process.size();
        let n = self.process.factors();

        let mut path = MultiPath::new(m, &self.time_grid)?;
        let mut asset = self.process.initial_values()?;
        for j in 0..m {
            *path[j].front_mut() = asset[j];
        }

        for i in 1..path.path_size() {
            let offset = (i - 1) * n;
            let t = self.time_grid[i - 1];
            let dt = self.time_grid.dt(i - 1);
            let mut temp = Array::with_size(n);
            for k in 0..n {
                let draw = draws[offset + k];
                temp[k] = if antithetic { -draw } else { draw };
            }
            asset = self.process.evolve(t, &asset, dt, &temp)?;
            for j in 0..m {
                path[j][i] = asset[j];
            }
        }

        Ok(Sample::new(path, weight))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use crate::math::matrix::Matrix;
    use crate::patterns::observable::{AsObservable, Observable};
    use crate::shared::shared;
    use crate::types::{Real, Size, Time};

    /// A deterministic sequence generator returning a fixed, known vector on
    /// every draw. Both `next_sequence` and `last_sequence` yield the same
    /// value, so `antithetic()` sees the exact negation of `next()`.
    struct StubSequence {
        sample: Sample<Vec<Real>>,
    }

    impl StubSequence {
        fn new(values: Vec<Real>) -> Self {
            StubSequence {
                sample: Sample::new(values, 1.0),
            }
        }
    }

    impl SequenceGenerator for StubSequence {
        fn next_sequence(&mut self) -> &Sample<Vec<Real>> {
            &self.sample
        }

        fn last_sequence(&self) -> &Sample<Vec<Real>> {
            &self.sample
        }

        fn dimension(&self) -> usize {
            self.sample.value.len()
        }
    }

    /// A process that records the `dw` block handed to each `evolve` call and
    /// leaves the state untouched. Sits directly at the generator->process
    /// boundary (not wrapped in a `StochasticProcessArray`), so it observes the
    /// raw per-step draw slice rather than a correlated `dz`.
    struct RecordingProcess {
        n: usize,
        records: RefCell<Vec<Vec<Real>>>,
        observable: Shared<Observable>,
    }

    impl RecordingProcess {
        fn new(n: usize) -> Self {
            RecordingProcess {
                n,
                records: RefCell::new(Vec::new()),
                observable: shared(Observable::new()),
            }
        }

        fn take_records(&self) -> Vec<Vec<Real>> {
            self.records.borrow_mut().drain(..).collect()
        }
    }

    impl AsObservable for RecordingProcess {
        fn observable(&self) -> &Observable {
            &self.observable
        }
    }

    impl StochasticProcess for RecordingProcess {
        fn size(&self) -> Size {
            self.n
        }

        fn factors(&self) -> Size {
            self.n
        }

        fn initial_values(&self) -> QlResult<Array> {
            Ok(Array::with_size(self.n))
        }

        fn drift(&self, _t: Time, _x: &Array) -> QlResult<Array> {
            Ok(Array::with_size(self.n))
        }

        fn diffusion(&self, _t: Time, _x: &Array) -> QlResult<Matrix> {
            Ok(Matrix::with_size(self.n, self.n))
        }

        fn evolve(&self, _t0: Time, x0: &Array, _dt: Time, dw: &Array) -> QlResult<Array> {
            self.records.borrow_mut().push(dw.to_vec());
            Ok(x0.clone())
        }
    }

    fn recording_process(n: usize) -> (Shared<RecordingProcess>, Shared<dyn StochasticProcess>) {
        let rec = shared(RecordingProcess::new(n));
        let proc = Shared::clone(&rec) as Shared<dyn StochasticProcess>;
        (rec, proc)
    }

    /// Oracle 4 (the offset pin - the REAL bit-exactness gate). With a known,
    /// distinct sequence `[1, 2, ..., factors*steps]` fed to a recording
    /// process, step `i` must consume exactly `sequence[(i-1)*n .. i*n]`. This
    /// pins the absolute `(i-1)*factors` offset (`multipathgenerator.hpp:132`)
    /// that the correlation pin cannot, and catches an in-bounds interior
    /// transposition (not just an off-the-end read).
    #[test]
    fn per_step_draw_blocks_use_the_exact_offset() {
        let (rec, proc) = recording_process(2);
        let stub = StubSequence::new(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]);
        let grid = TimeGrid::new(1.0, 4).unwrap();
        let mut mpg = MultiPathGenerator::new(proc, grid, stub, false).unwrap();

        mpg.next().unwrap();
        let blocks = rec.take_records();
        assert_eq!(
            blocks,
            vec![
                vec![1.0, 2.0],
                vec![3.0, 4.0],
                vec![5.0, 6.0],
                vec![7.0, 8.0],
            ]
        );
    }

    /// Oracle 2 (antithetic negation pin): the antithetic path consumes the
    /// exact negation of the forward draws, block for block
    /// (`multipathgenerator.hpp:135-139`).
    #[test]
    fn antithetic_negates_the_forward_draws() {
        let (rec, proc) = recording_process(2);
        let stub = StubSequence::new(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]);
        let grid = TimeGrid::new(1.0, 4).unwrap();
        let mut mpg = MultiPathGenerator::new(proc, grid, stub, false).unwrap();

        mpg.next().unwrap();
        let forward = rec.take_records();
        mpg.antithetic().unwrap();
        let anti = rec.take_records();

        assert_eq!(
            forward,
            vec![
                vec![1.0, 2.0],
                vec![3.0, 4.0],
                vec![5.0, 6.0],
                vec![7.0, 8.0],
            ]
        );
        for (f_block, a_block) in forward.iter().zip(anti.iter()) {
            for (f, a) in f_block.iter().zip(a_block.iter()) {
                assert_eq!(*a, -*f, "antithetic draw must negate the forward draw");
            }
        }
    }

    #[test]
    fn dimension_mismatch_is_rejected() {
        // multipathgenerator.hpp:79-84: dimension == factors * steps.
        let (_rec, proc) = recording_process(2);
        let grid = TimeGrid::new(1.0, 8).unwrap();
        let stub = StubSequence::new(vec![0.0; 10]);
        match MultiPathGenerator::new(proc, grid, stub, false) {
            Err(e) => {
                assert!(e.message().contains("dimension"));
                assert!(e.message().contains("is not equal"));
            }
            Ok(_) => panic!("dimension 10 != 2 * 8 must be rejected"),
        }
    }

    #[test]
    fn no_times_is_rejected() {
        // multipathgenerator.hpp:86: QL_REQUIRE(times.size() > 1).
        let (_rec, proc) = recording_process(2);
        let stub = StubSequence::new(vec![]);
        match MultiPathGenerator::new(proc, TimeGrid::default(), stub, false) {
            Err(e) => assert_eq!(e.message(), "no times given"),
            Ok(_) => panic!("an empty time grid must be rejected"),
        }
    }

    #[test]
    fn brownian_bridge_is_rejected_as_deferred() {
        let (_rec, proc) = recording_process(2);
        let grid = TimeGrid::new(1.0, 8).unwrap();
        let stub = StubSequence::new(vec![0.0; 16]);
        match MultiPathGenerator::new(proc, grid, stub, true) {
            Err(e) => assert!(e.message().contains("brownian bridge")),
            Ok(_) => panic!("brownian_bridge = true must be rejected as deferred"),
        }
    }
}
