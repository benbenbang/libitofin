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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::date::Month;
    use crate::time::timeunit::TimeUnit;

    const OPTION_TIMES: [Time; 3] = [1.0, 2.0, 3.0];
    const SWAP_LENGTHS: [Time; 2] = [5.0, 10.0];
    const N_LAYERS: Size = 3;

    fn node(layer: usize, option_idx: usize, swap_idx: usize) -> Real {
        100.0 * layer as Real + 10.0 * option_idx as Real + swap_idx as Real
    }

    fn option_date(i: usize) -> Date {
        Date::new(10 + i as i32, Month::January, 2026)
    }

    fn swap_tenor(k: usize) -> Period {
        Period::new(5 * (k as i64 + 1) as i32, TimeUnit::Years)
    }

    /// A 3-option x 2-swap (non-square) cube with distinct per-node per-layer
    /// values `node(l, j, k) = 100*l + 10*j + k`, interpolators refreshed.
    fn built_cube() -> Cube {
        let option_dates = vec![option_date(0), option_date(1), option_date(2)];
        let swap_tenors = vec![swap_tenor(0), swap_tenor(1)];
        let mut cube = Cube::new(
            option_dates,
            swap_tenors,
            OPTION_TIMES.to_vec(),
            SWAP_LENGTHS.to_vec(),
            N_LAYERS,
            true,
            false,
        )
        .unwrap();
        for l in 0..N_LAYERS {
            let mut m = Matrix::with_size(OPTION_TIMES.len(), SWAP_LENGTHS.len());
            for j in 0..OPTION_TIMES.len() {
                for k in 0..SWAP_LENGTHS.len() {
                    m[(j, k)] = node(l, j, k);
                }
            }
            cube.set_layer(l, m).unwrap();
        }
        cube.update_interpolators().unwrap();
        cube
    }

    fn assert_exact(got: Real, expected: Real) {
        assert!(
            (got - expected).abs() <= 1e-15 * (1.0 + expected.abs()),
            "got {got}, expected {expected}"
        );
    }

    #[test]
    fn recovers_every_node_value_on_a_non_square_grid() {
        let cube = built_cube();
        for (j, &option_time) in OPTION_TIMES.iter().enumerate() {
            for (k, &swap_length) in SWAP_LENGTHS.iter().enumerate() {
                let v = cube.value(option_time, swap_length).unwrap();
                assert_eq!(v.len(), N_LAYERS);
                for (l, &got) in v.iter().enumerate() {
                    assert_exact(got, node(l, j, k));
                }
            }
        }
    }

    #[test]
    fn interpolates_linearly_between_adjacent_nodes() {
        let cube = built_cube();
        let mid_option = cube.value(1.5, SWAP_LENGTHS[0]).unwrap();
        for (l, &got) in mid_option.iter().enumerate() {
            assert_exact(got, 0.5 * (node(l, 0, 0) + node(l, 1, 0)));
        }
        let mid_swap = cube.value(OPTION_TIMES[0], 7.5).unwrap();
        for (l, &got) in mid_swap.iter().enumerate() {
            assert_exact(got, 0.5 * (node(l, 0, 0) + node(l, 0, 1)));
        }
    }

    #[test]
    fn extrapolates_flat_past_every_edge_and_corner() {
        let cube = built_cube();
        let low_corner = cube.value(0.0, 1.0).unwrap();
        let high_corner = cube.value(9.0, 99.0).unwrap();
        let swap_edge = cube.value(OPTION_TIMES[1], 99.0).unwrap();
        let option_edge = cube.value(0.0, SWAP_LENGTHS[1]).unwrap();
        for l in 0..N_LAYERS {
            assert_exact(low_corner[l], node(l, 0, 0));
            assert_exact(high_corner[l], node(l, 2, 1));
            assert_exact(swap_edge[l], node(l, 1, 1));
            assert_exact(option_edge[l], node(l, 0, 1));
        }
    }

    #[test]
    fn set_point_on_existing_node_overwrites_without_expanding() {
        let mut cube = built_cube();
        let new_point: Vec<Real> = (0..N_LAYERS).map(|l| 900.0 + l as Real).collect();
        cube.set_point(
            option_date(1),
            swap_tenor(0),
            OPTION_TIMES[1],
            SWAP_LENGTHS[0],
            new_point.clone(),
        )
        .unwrap();

        assert_eq!(cube.option_times(), &OPTION_TIMES[..]);
        assert_eq!(cube.swap_lengths(), &SWAP_LENGTHS[..]);
        for (l, &expected) in new_point.iter().enumerate() {
            assert_eq!(cube.points()[l].rows(), 3);
            assert_eq!(cube.points()[l].columns(), 2);
            assert_exact(cube.points()[l][(1, 0)], expected);
            assert_exact(cube.points()[l][(0, 0)], node(l, 0, 0));
        }
    }

    #[test]
    fn set_point_with_new_option_time_expands_grid_and_keeps_old_nodes() {
        let mut cube = built_cube();
        let new_point: Vec<Real> = (0..N_LAYERS).map(|l| 700.0 + l as Real).collect();
        cube.set_point(
            option_date(0),
            swap_tenor(0),
            2.5,
            SWAP_LENGTHS[0],
            new_point.clone(),
        )
        .unwrap();

        assert_eq!(cube.option_times(), &[1.0, 2.0, 2.5, 3.0][..]);
        assert_eq!(cube.swap_lengths(), &SWAP_LENGTHS[..]);
        for (l, &expected) in new_point.iter().enumerate() {
            assert_eq!(cube.points()[l].rows(), 4);
            assert_eq!(cube.points()[l].columns(), 2);
            assert_exact(cube.points()[l][(0, 0)], node(l, 0, 0));
            assert_exact(cube.points()[l][(1, 1)], node(l, 1, 1));
            assert_exact(cube.points()[l][(3, 0)], node(l, 2, 0));
            assert_exact(cube.points()[l][(3, 1)], node(l, 2, 1));
            assert_exact(cube.points()[l][(2, 0)], expected);
            assert_exact(cube.points()[l][(2, 1)], 0.0);
        }
    }

    #[test]
    fn expand_layers_inserts_zeroed_row_and_column() {
        let mut cube = built_cube();
        cube.expand_layers(1, true, 1, true).unwrap();
        for l in 0..N_LAYERS {
            assert_eq!(cube.points()[l].rows(), 4);
            assert_eq!(cube.points()[l].columns(), 3);
            for k in 0..3 {
                assert_exact(cube.points()[l][(1, k)], 0.0);
            }
            for j in 0..4 {
                assert_exact(cube.points()[l][(j, 1)], 0.0);
            }
            assert_exact(cube.points()[l][(3, 2)], node(l, 2, 1));
        }
    }

    #[test]
    fn backward_flat_is_a_documented_deferral() {
        let err = Cube::new(
            vec![option_date(0), option_date(1), option_date(2)],
            vec![swap_tenor(0), swap_tenor(1)],
            OPTION_TIMES.to_vec(),
            SWAP_LENGTHS.to_vec(),
            N_LAYERS,
            true,
            true,
        )
        .err()
        .expect("backward_flat = true must be rejected");
        assert!(err.to_string().contains("596"));
    }

    #[test]
    fn constructor_rejects_undersized_axes() {
        let short = Cube::new(
            vec![option_date(0)],
            vec![swap_tenor(0), swap_tenor(1)],
            vec![1.0],
            SWAP_LENGTHS.to_vec(),
            N_LAYERS,
            true,
            false,
        );
        assert!(short.is_err());
    }
}
