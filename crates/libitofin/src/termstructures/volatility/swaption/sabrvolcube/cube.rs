//! Multi-layer, node-indexed parameter store for the SABR swaption vol cube.
//!
//! Port of the inner `Cube` class nested in
//! `ql/termstructures/volatility/swaption/sabrswaptionvolatilitycube.hpp:130-181`.
//! Over an `optionTimes x swapLengths` grid it holds a set of LAYERS (the SABR
//! parameters alpha/beta/nu/rho, the forward, plus calibration metadata), each an
//! `nOptions x nSwaps` matrix, and one bilinear interpolator per layer.
//! [`Cube::value`] evaluates every layer at an off-node `(optionTime, swapLength)`.
//!
//! ## Axis layout (equivalent to C++, per D10)
//!
//! C++ keeps `points_[k]` in `[optionRow][swapCol]` layout plus a
//! `transposedPoints_[k] = transpose(points_[k])`, builds
//! `BilinearInterpolation(x = optionTimes, y = swapLengths, z = transposedPoints_[k])`
//! and evaluates `(*interpolators_[k])(optionTime, swapLength)`
//! (hpp:1010, 1026-1031, 1204-1211). This port keeps the same natural
//! `[optionRow][swapCol]` `points[k]` but builds the bilinear UNTRANSPOSED as
//! `x = swap_lengths`, `y = option_times`, `z = points[k]` rows. Since the Rust
//! bilinear convention is `z[j][i] = value at (x[i], y[j])`, that is the identical
//! surface: `points[k][(a, b)]` is the value at option node `a`, swap node `b`
//! either way. Public [`Cube::value`] keeps the C++ order `(option_time, swap_length)`
//! and internally queries `value(swap_length, option_time)`. The non-square oracle
//! grid pins it: cross the axes and the bilinear fails to build.
//!
//! ## Stale interpolators (mirrors C++)
//!
//! The mutators change `points` but do NOT rebuild the interpolators; only
//! [`update_interpolators`](Cube::update_interpolators) does, as in C++ where
//! `operator()` reads cached `interpolators_`. A consumer mutates then calls
//! `update_interpolators` before querying. The type is meant to live inside the
//! SABR cube's own `RefCell` state, so plain `&mut self` mutators suffice.
//!
//! ## Deferrals (documented omissions)
//!
//! - The `backward_flat = true` path (`BackwardflatLinearInterpolation`, used only
//!   for a single-node axis) is not ported: the constructor returns `Err`. The SABR
//!   oracle grids have >= 2 nodes per axis. Deferred under #596.
//! - `browse()` (hpp:1245, a debugging matrix dump), the copy constructor and
//!   `operator=` are omitted: Rust move semantics cover the `RefCell`-held usage.
#![allow(dead_code)]

use crate::errors::QlResult;
use crate::math::interpolations::Interpolation2D;
use crate::math::interpolations::bilinear::BilinearInterpolation;
use crate::math::interpolations::flatextrapolator2d::FlatExtrapolator2D;
use crate::math::matrix::Matrix;
use crate::time::date::Date;
use crate::time::period::Period;
use crate::types::{Real, Size, Time};
use crate::{fail, require};

/// A per-layer flat-extrapolating bilinear interpolator.
type LayerInterpolator = FlatExtrapolator2D<BilinearInterpolation>;

/// The multi-layer parameter store the SABR swaption vol cube interpolates.
pub(crate) struct Cube {
    option_times: Vec<Time>,
    swap_lengths: Vec<Time>,
    option_dates: Vec<Date>,
    swap_tenors: Vec<Period>,
    n_layers: Size,
    points: Vec<Matrix>,
    extrapolation: bool,
    interpolators: Vec<LayerInterpolator>,
}

impl Cube {
    /// Builds a cube over the `optionTimes x swapLengths` grid with `n_layers`
    /// zero-initialised parameter matrices and their interpolators.
    /// `option_dates`/`swap_tenors` are the identity axes for the numeric
    /// `option_times`/`swap_lengths`. `extrapolation` is carried onto each layer
    /// interpolator; the flat wrapper always clamps into the grid, so the inner
    /// flag never fires, mirroring C++'s unconditional `enableExtrapolation()`.
    /// `backward_flat = true` is a documented deferral.
    pub(crate) fn new(
        option_dates: Vec<Date>,
        swap_tenors: Vec<Period>,
        option_times: Vec<Time>,
        swap_lengths: Vec<Time>,
        n_layers: Size,
        extrapolation: bool,
        backward_flat: bool,
    ) -> QlResult<Self> {
        if backward_flat {
            fail!(
                "Cube backward-flat interpolation (single-node-axis path) is not ported; \
                 deferred under #596 (SABR cube). The SABR oracle grids have >= 2 nodes per axis."
            );
        }
        require!(
            option_times.len() > 1,
            "Cube::new: option_times.len() < 2 (got {})",
            option_times.len()
        );
        require!(
            swap_lengths.len() > 1,
            "Cube::new: swap_lengths.len() < 2 (got {})",
            swap_lengths.len()
        );
        require!(
            option_times.len() == option_dates.len(),
            "Cube::new: option_times/option_dates size mismatch ({} vs {})",
            option_times.len(),
            option_dates.len()
        );
        require!(
            swap_tenors.len() == swap_lengths.len(),
            "Cube::new: swap_tenors/swap_lengths size mismatch ({} vs {})",
            swap_tenors.len(),
            swap_lengths.len()
        );

        let points = vec![Matrix::with_size(option_times.len(), swap_lengths.len()); n_layers];
        let interpolators =
            build_interpolators(&option_times, &swap_lengths, &points, extrapolation)?;
        Ok(Cube {
            option_times,
            swap_lengths,
            option_dates,
            swap_tenors,
            n_layers,
            points,
            extrapolation,
            interpolators,
        })
    }

    /// Every layer's value at `(option_time, swap_length)` from the cached
    /// interpolators (refresh via `update_interpolators`); outside the grid clamps flat.
    pub(crate) fn value(&self, option_time: Time, swap_length: Time) -> QlResult<Vec<Real>> {
        self.interpolators
            .iter()
            .map(|interp| interp.value(swap_length, option_time))
            .collect()
    }

    /// Sets a single element `points[layer][row][col]`, bounds-checked.
    pub(crate) fn set_element(
        &mut self,
        layer: Size,
        row: Size,
        col: Size,
        x: Real,
    ) -> QlResult<()> {
        require!(
            layer < self.n_layers,
            "Cube::set_element: layer index {layer} out of {} layers",
            self.n_layers
        );
        require!(
            row < self.option_times.len(),
            "Cube::set_element: row index {row} out of {} option nodes",
            self.option_times.len()
        );
        require!(
            col < self.swap_lengths.len(),
            "Cube::set_element: column index {col} out of {} swap nodes",
            self.swap_lengths.len()
        );
        self.points[layer][(row, col)] = x;
        Ok(())
    }

    /// Replaces all layer matrices, checking layer count and the grid dimensions.
    pub(crate) fn set_points(&mut self, x: Vec<Matrix>) -> QlResult<()> {
        require!(
            x.len() == self.n_layers,
            "Cube::set_points: {} layers given, expected {}",
            x.len(),
            self.n_layers
        );
        require!(
            x[0].rows() == self.option_times.len(),
            "Cube::set_points: {} rows, expected {} option nodes",
            x[0].rows(),
            self.option_times.len()
        );
        require!(
            x[0].columns() == self.swap_lengths.len(),
            "Cube::set_points: {} columns, expected {} swap nodes",
            x[0].columns(),
            self.swap_lengths.len()
        );
        self.points = x;
        Ok(())
    }

    /// Replaces a single layer matrix, checking the grid dimensions.
    pub(crate) fn set_layer(&mut self, i: Size, x: Matrix) -> QlResult<()> {
        require!(
            i < self.n_layers,
            "Cube::set_layer: layer index {i} out of {} layers",
            self.n_layers
        );
        require!(
            x.rows() == self.option_times.len(),
            "Cube::set_layer: {} rows, expected {} option nodes",
            x.rows(),
            self.option_times.len()
        );
        require!(
            x.columns() == self.swap_lengths.len(),
            "Cube::set_layer: {} columns, expected {} swap nodes",
            x.columns(),
            self.swap_lengths.len()
        );
        self.points[i] = x;
        Ok(())
    }

    /// Writes `point` (one value per layer) at the grid node for
    /// `(option_time, swap_length)`, overwriting when the node exists and expanding
    /// the grid (via [`expand_layers`](Self::expand_layers)) when the option time or
    /// swap length is new. Records the identity `option_date`/`swap_tenor` too.
    pub(crate) fn set_point(
        &mut self,
        option_date: Date,
        swap_tenor: Period,
        option_time: Time,
        swap_length: Time,
        point: Vec<Real>,
    ) -> QlResult<()> {
        require!(
            point.len() == self.n_layers,
            "Cube::set_point: point has {} values, expected {} layers",
            point.len(),
            self.n_layers
        );

        let expand_option_times = !contains(&self.option_times, option_time);
        let expand_swap_lengths = !contains(&self.swap_lengths, swap_length);
        let option_times_index = lower_bound(&self.option_times, option_time);
        let swap_lengths_index = lower_bound(&self.swap_lengths, swap_length);

        if expand_option_times || expand_swap_lengths {
            self.expand_layers(
                option_times_index,
                expand_option_times,
                swap_lengths_index,
                expand_swap_lengths,
            )?;
        }

        for (k, &value) in point.iter().enumerate() {
            self.points[k][(option_times_index, swap_lengths_index)] = value;
        }
        self.option_times[option_times_index] = option_time;
        self.swap_lengths[swap_lengths_index] = swap_length;
        self.option_dates[option_times_index] = option_date;
        self.swap_tenors[swap_lengths_index] = swap_tenor;
        Ok(())
    }

    /// Grows every layer matrix by inserting a zero-filled row at option index `i`
    /// and/or a zero-filled column at swap index `j`, shifting existing nodes to keep
    /// their values. The identity axes gain a placeholder [`Date`]/[`Period`] there.
    pub(crate) fn expand_layers(
        &mut self,
        i: Size,
        expand_option_times: bool,
        j: Size,
        expand_swap_lengths: bool,
    ) -> QlResult<()> {
        require!(
            i <= self.option_times.len(),
            "Cube::expand_layers: row index {i} past {} option nodes",
            self.option_times.len()
        );
        require!(
            j <= self.swap_lengths.len(),
            "Cube::expand_layers: column index {j} past {} swap nodes",
            self.swap_lengths.len()
        );

        if expand_option_times {
            self.option_times.insert(i, 0.0);
            self.option_dates.insert(i, Date::default());
        }
        if expand_swap_lengths {
            self.swap_lengths.insert(j, 0.0);
            self.swap_tenors.insert(j, Period::default());
        }

        let mut new_points =
            vec![
                Matrix::with_size(self.option_times.len(), self.swap_lengths.len());
                self.n_layers
            ];
        for (k, new_layer) in new_points.iter_mut().enumerate() {
            let old = &self.points[k];
            for u in 0..old.rows() {
                let index_of_row = if u >= i && expand_option_times {
                    u + 1
                } else {
                    u
                };
                for v in 0..old.columns() {
                    let index_of_col = if v >= j && expand_swap_lengths {
                        v + 1
                    } else {
                        v
                    };
                    new_layer[(index_of_row, index_of_col)] = old[(u, v)];
                }
            }
        }
        self.set_points(new_points)
    }

    /// Rebuilds every layer interpolator from the current `points`. Call after any
    /// mutation before querying [`value`](Self::value).
    pub(crate) fn update_interpolators(&mut self) -> QlResult<()> {
        self.interpolators = build_interpolators(
            &self.option_times,
            &self.swap_lengths,
            &self.points,
            self.extrapolation,
        )?;
        Ok(())
    }

    /// The option times (numeric option axis).
    pub(crate) fn option_times(&self) -> &[Time] {
        &self.option_times
    }

    /// The swap lengths (numeric swap axis).
    pub(crate) fn swap_lengths(&self) -> &[Time] {
        &self.swap_lengths
    }

    /// The option dates (identity option axis).
    pub(crate) fn option_dates(&self) -> &[Date] {
        &self.option_dates
    }

    /// The swap tenors (identity swap axis).
    pub(crate) fn swap_tenors(&self) -> &[Period] {
        &self.swap_tenors
    }

    /// The per-layer parameter matrices.
    pub(crate) fn points(&self) -> &[Matrix] {
        &self.points
    }
}

/// Builds one flat-extrapolating bilinear interpolator per layer matrix.
fn build_interpolators(
    option_times: &[Time],
    swap_lengths: &[Time],
    points: &[Matrix],
    extrapolation: bool,
) -> QlResult<Vec<LayerInterpolator>> {
    points
        .iter()
        .map(|layer| build_layer(option_times, swap_lengths, layer, extrapolation))
        .collect()
}

/// Builds the interpolator for one layer: bilinear over
/// `x = swap_lengths`, `y = option_times`, `z = layer` rows, wrapped flat.
fn build_layer(
    option_times: &[Time],
    swap_lengths: &[Time],
    layer: &Matrix,
    extrapolation: bool,
) -> QlResult<LayerInterpolator> {
    let z: Vec<Vec<Real>> = (0..layer.rows()).map(|j| layer.row(j).to_vec()).collect();
    let bilinear = BilinearInterpolation::new(swap_lengths.to_vec(), option_times.to_vec(), z)?;
    let mut wrapper = FlatExtrapolator2D::new(bilinear);
    wrapper.set_extrapolation(extrapolation);
    Ok(wrapper)
}

/// Whether `sorted` (strictly increasing, finite) already contains `v`.
fn contains(sorted: &[Time], v: Time) -> bool {
    sorted
        .binary_search_by(|probe| {
            probe
                .partial_cmp(&v)
                .expect("cube axis values are finite and comparable")
        })
        .is_ok()
}

/// The index of the first element of `sorted` not less than `v` (C++ `lower_bound`).
fn lower_bound(sorted: &[Time], v: Time) -> Size {
    sorted.partition_point(|&probe| probe < v)
}
