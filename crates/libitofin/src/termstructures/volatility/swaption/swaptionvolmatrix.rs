//! Bilinear-interpolated at-the-money swaption volatility matrix.
//!
//! Port of `ql/termstructures/volatility/swaption/swaptionvolmatrix.{hpp,cpp}`:
//! `class SwaptionVolatilityMatrix : public SwaptionVolatilityDiscrete`. The
//! surface interpolates a grid of market volatilities indexed by option date
//! (rows) and swap tenor (columns) with a [`BilinearInterpolation`], embedding
//! [`SwaptionVolatilityDiscrete`] for the shared tenor/date/time grid.
//!
//! ## Axis convention
//!
//! The volatility matrix `M` is stored as C++ documents it: `M[i][j]` is the
//! vol for the `i`-th option date and `j`-th swap tenor. The bilinear is built
//! with `x = swap_lengths`, `y = option_times` and `z = M` unchanged, because
//! [`BilinearInterpolation`] evaluates `z[j][i]` at `(x[i], y[j])`. So
//! [`volatility_impl`](SwaptionVolatilityMatrix) and
//! [`shift_impl`](SwaptionVolatilityMatrix) call `value(swap_length,
//! option_time)`, matching C++'s `interpolation_(swapLength, optionTime, true)`.
//!
//! ## Lazy refresh
//!
//! C++ builds the interpolation once over a `Matrix` it aliases by reference, so
//! a quote refresh in `performCalculations` is seen by the interpolation for
//! free. The Rust [`BilinearInterpolation`] owns its `z` data, so refreshing the
//! quotes without rebuilding would serve stale vols. The two interpolations
//! therefore live behind a [`RefCell`] and every query routes through
//! [`calculate`](SwaptionVolatilityMatrix::calculate) first;
//! `perform_calculations` re-reads the quote handles and rebuilds both
//! interpolations. A [`MatrixUpdater`] registered on the base observable
//! invalidates the lazy state on a quote bump (routed there through the base
//! updater the quotes notify) or an evaluation-date move.
//!
//! ## Divergences from QuantLib
//!
//! - Two of the five C++ constructors are ported: the floating-reference +
//!   `Handle<Quote>` matrix form and the fixed-reference + `Matrix` form. The
//!   remaining three (fixed-reference + handles, floating-reference + `Matrix`,
//!   and the fixed-reference + option-dates + `Matrix` form) are convenience
//!   shapes deferred to #570.
//! - C++'s `flatExtrapolation` flag is exposed through the dedicated
//!   [`moving_flat`](SwaptionVolatilityMatrix::moving_flat) and
//!   [`new_flat`](SwaptionVolatilityMatrix::new_flat) constructors (#569), which
//!   wrap both interpolations in a [`FlatExtrapolator2D`] so out-of-grid queries
//!   clamp to the nearest edge or corner vol. The plain `moving`/`new`
//!   constructors keep the boundary-extending bilinear (extrapolation enabled,
//!   mirroring the `true` flag C++ passes on every interpolation call).
//! - The C++ `mutable Matrix volatilities_` field is not held separately: the
//!   Rust bilinear owns its `z`, so the load-bearing state is the interpolation
//!   rebuilt in `perform_calculations`, not a second copy of the vols.
//! - `smileSectionImpl` is omitted, as the swaption trait base documents: the
//!   smile-section layer is unported and [`ConstantSwaptionVolatility`]
//!   (super::ConstantSwaptionVolatility) omits it too.

use std::cell::RefCell;

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::math::interpolations::Interpolation2D;
use crate::math::interpolations::bilinear::BilinearInterpolation;
use crate::math::interpolations::flatextrapolator2d::FlatExtrapolator2D;
use crate::math::matrix::Matrix;
use crate::patterns::lazyobject::LazyObject;
use crate::patterns::observable::{AsObservable, Observable, Observer};
use crate::quotes::{Quote, make_quote_handle};
use crate::require;
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut, shared_mut};
use crate::termstructures::volatility::{VolatilityTermStructure, VolatilityType};
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::period::Period;
use crate::types::{Rate, Real, Time, Volatility};

use super::{SwaptionVolatilityDiscrete, SwaptionVolatilityStructure};

/// The two interpolations rebuilt on every refresh. They are boxed as trait
/// objects so the `flat_extrapolation` path can store a
/// [`FlatExtrapolator2D`]-wrapped bilinear behind the same field as a plain
/// [`BilinearInterpolation`].
struct MatrixInterp {
    volatilities: Box<dyn Interpolation2D>,
    shifts: Box<dyn Interpolation2D>,
}

/// Invalidates the matrix's lazy state when a quote bumps or the reference date
/// moves, so the next [`calculate`](SwaptionVolatilityMatrix::calculate)
/// rebuilds both interpolations.
struct MatrixUpdater {
    lazy: SharedMut<LazyObject>,
}

impl Observer for MatrixUpdater {
    fn update(&mut self) {
        self.lazy.borrow_mut().invalidate_silently();
    }
}

/// At-the-money swaption volatility matrix, bilinear over an option-date x
/// swap-tenor grid.
pub struct SwaptionVolatilityMatrix {
    discrete: SwaptionVolatilityDiscrete,
    vol_handles: Vec<Vec<Handle<dyn Quote>>>,
    shift_values: Vec<Vec<Real>>,
    volatility_type: VolatilityType,
    flat_extrapolation: bool,
    interp: RefCell<MatrixInterp>,
    lazy: SharedMut<LazyObject>,
    _updater: SharedMut<MatrixUpdater>,
}

impl SwaptionVolatilityMatrix {
    /// Floating reference date (settlement 0 off the evaluation date), quote-backed
    /// market data. C++'s floating-reference + `vector<vector<Handle<Quote>>>`
    /// constructor. `shifts` may be empty (all-zero) or match the vol grid.
    #[allow(clippy::too_many_arguments)]
    pub fn moving(
        calendar: Calendar,
        business_day_convention: BusinessDayConvention,
        option_tenors: Vec<Period>,
        swap_tenors: Vec<Period>,
        vols: Vec<Vec<Handle<dyn Quote>>>,
        day_counter: DayCounter,
        volatility_type: VolatilityType,
        shifts: Vec<Vec<Real>>,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<SwaptionVolatilityMatrix> {
        Self::moving_with(
            calendar,
            business_day_convention,
            option_tenors,
            swap_tenors,
            vols,
            day_counter,
            volatility_type,
            shifts,
            settings,
            false,
        )
    }

    /// Floating-reference form with C++'s `flatExtrapolation = true`: queries past
    /// the grid clamp to the nearest edge or corner vol rather than extending the
    /// boundary bilinear surface. Signature mirrors [`moving`](Self::moving).
    #[allow(clippy::too_many_arguments)]
    pub fn moving_flat(
        calendar: Calendar,
        business_day_convention: BusinessDayConvention,
        option_tenors: Vec<Period>,
        swap_tenors: Vec<Period>,
        vols: Vec<Vec<Handle<dyn Quote>>>,
        day_counter: DayCounter,
        volatility_type: VolatilityType,
        shifts: Vec<Vec<Real>>,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<SwaptionVolatilityMatrix> {
        Self::moving_with(
            calendar,
            business_day_convention,
            option_tenors,
            swap_tenors,
            vols,
            day_counter,
            volatility_type,
            shifts,
            settings,
            true,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn moving_with(
        calendar: Calendar,
        business_day_convention: BusinessDayConvention,
        option_tenors: Vec<Period>,
        swap_tenors: Vec<Period>,
        vols: Vec<Vec<Handle<dyn Quote>>>,
        day_counter: DayCounter,
        volatility_type: VolatilityType,
        shifts: Vec<Vec<Real>>,
        settings: Shared<Settings<Date>>,
        flat_extrapolation: bool,
    ) -> QlResult<SwaptionVolatilityMatrix> {
        let discrete = SwaptionVolatilityDiscrete::moving(
            option_tenors,
            swap_tenors,
            0,
            calendar,
            business_day_convention,
            day_counter,
            settings,
        )?;
        Self::assemble(discrete, vols, volatility_type, shifts, flat_extrapolation)
    }

    /// Fixed reference date, fixed market data. C++'s fixed-reference + `Matrix`
    /// constructor. The cells are wrapped in unobservable quote handles so the
    /// refresh path is shared with the floating form. An empty `shifts` matrix
    /// means all-zero shifts.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        reference_date: Date,
        calendar: Calendar,
        business_day_convention: BusinessDayConvention,
        option_tenors: Vec<Period>,
        swap_tenors: Vec<Period>,
        volatilities: &Matrix,
        day_counter: DayCounter,
        volatility_type: VolatilityType,
        shifts: &Matrix,
    ) -> QlResult<SwaptionVolatilityMatrix> {
        Self::new_with(
            reference_date,
            calendar,
            business_day_convention,
            option_tenors,
            swap_tenors,
            volatilities,
            day_counter,
            volatility_type,
            shifts,
            false,
        )
    }

    /// Fixed-reference form with C++'s `flatExtrapolation = true`. Signature
    /// mirrors [`new`](Self::new); see [`moving_flat`](Self::moving_flat) for the
    /// clamping behaviour.
    #[allow(clippy::too_many_arguments)]
    pub fn new_flat(
        reference_date: Date,
        calendar: Calendar,
        business_day_convention: BusinessDayConvention,
        option_tenors: Vec<Period>,
        swap_tenors: Vec<Period>,
        volatilities: &Matrix,
        day_counter: DayCounter,
        volatility_type: VolatilityType,
        shifts: &Matrix,
    ) -> QlResult<SwaptionVolatilityMatrix> {
        Self::new_with(
            reference_date,
            calendar,
            business_day_convention,
            option_tenors,
            swap_tenors,
            volatilities,
            day_counter,
            volatility_type,
            shifts,
            true,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn new_with(
        reference_date: Date,
        calendar: Calendar,
        business_day_convention: BusinessDayConvention,
        option_tenors: Vec<Period>,
        swap_tenors: Vec<Period>,
        volatilities: &Matrix,
        day_counter: DayCounter,
        volatility_type: VolatilityType,
        shifts: &Matrix,
        flat_extrapolation: bool,
    ) -> QlResult<SwaptionVolatilityMatrix> {
        let discrete = SwaptionVolatilityDiscrete::new(
            option_tenors,
            swap_tenors,
            reference_date,
            calendar,
            business_day_convention,
            day_counter,
        )?;
        let vols = matrix_to_handles(volatilities);
        let shift_values = if shifts.is_empty() {
            Vec::new()
        } else {
            matrix_to_rows(shifts)
        };
        Self::assemble(
            discrete,
            vols,
            volatility_type,
            shift_values,
            flat_extrapolation,
        )
    }

    fn assemble(
        discrete: SwaptionVolatilityDiscrete,
        vol_handles: Vec<Vec<Handle<dyn Quote>>>,
        volatility_type: VolatilityType,
        shift_values: Vec<Vec<Real>>,
        flat_extrapolation: bool,
    ) -> QlResult<SwaptionVolatilityMatrix> {
        check_inputs(&discrete, &vol_handles, &shift_values)?;
        let base_updater = discrete.base().updater();
        for row in &vol_handles {
            for handle in row {
                handle.register_observer(&base_updater);
            }
        }
        let interp = build_interps(&discrete, &vol_handles, &shift_values, flat_extrapolation)?;
        let lazy = shared_mut(LazyObject::new(true));
        let updater = shared_mut(MatrixUpdater {
            lazy: SharedMut::clone(&lazy),
        });
        discrete
            .observable()
            .register_observer(&(SharedMut::clone(&updater) as SharedMut<dyn Observer>));
        Ok(SwaptionVolatilityMatrix {
            discrete,
            vol_handles,
            shift_values,
            volatility_type,
            flat_extrapolation,
            interp: RefCell::new(interp),
            lazy,
            _updater: updater,
        })
    }

    /// Rebuilds both interpolations if a quote or the reference date has changed
    /// since they were last computed. Every query calls this first, as C++'s
    /// `volatilityImpl`/`shiftImpl` call `calculate()`.
    pub fn calculate(&self) -> QlResult<()> {
        if !self.lazy.borrow_mut().start_calculation() {
            return Ok(());
        }
        let result = self.perform_calculations();
        self.lazy.borrow_mut().finish_calculation(&result);
        result
    }

    fn perform_calculations(&self) -> QlResult<()> {
        self.discrete.calculate()?;
        let interp = build_interps(
            &self.discrete,
            &self.vol_handles,
            &self.shift_values,
            self.flat_extrapolation,
        )?;
        *self.interp.borrow_mut() = interp;
        Ok(())
    }
}

fn matrix_to_handles(matrix: &Matrix) -> Vec<Vec<Handle<dyn Quote>>> {
    (0..matrix.rows())
        .map(|i| {
            (0..matrix.columns())
                .map(|j| make_quote_handle(matrix[(i, j)]).handle())
                .collect()
        })
        .collect()
}

fn matrix_to_rows(matrix: &Matrix) -> Vec<Vec<Real>> {
    (0..matrix.rows()).map(|i| matrix[i].to_vec()).collect()
}

fn check_inputs(
    discrete: &SwaptionVolatilityDiscrete,
    vol_handles: &[Vec<Handle<dyn Quote>>],
    shift_values: &[Vec<Real>],
) -> QlResult<()> {
    let n_options = discrete.option_tenors().len();
    let n_swaps = discrete.swap_tenors().len();
    require!(
        vol_handles.len() == n_options,
        "mismatch between number of option dates ({n_options}) and number of rows ({}) in the vol matrix",
        vol_handles.len()
    );
    for row in vol_handles {
        require!(
            row.len() == n_swaps,
            "mismatch between number of swap tenors ({n_swaps}) and number of columns ({}) in the vol matrix",
            row.len()
        );
    }
    if !shift_values.is_empty() {
        require!(
            shift_values.len() == n_options,
            "mismatch between number of option dates ({n_options}) and number of rows ({}) in the shift matrix",
            shift_values.len()
        );
        for row in shift_values {
            require!(
                row.len() == n_swaps,
                "mismatch between number of swap tenors ({n_swaps}) and number of columns ({}) in the shift matrix",
                row.len()
            );
        }
    }
    Ok(())
}

fn build_interps(
    discrete: &SwaptionVolatilityDiscrete,
    vol_handles: &[Vec<Handle<dyn Quote>>],
    shift_values: &[Vec<Real>],
    flat_extrapolation: bool,
) -> QlResult<MatrixInterp> {
    let swap_lengths = discrete.swap_lengths()?;
    let option_times = discrete.option_times()?;
    let mut volatilities = Vec::with_capacity(vol_handles.len());
    for row in vol_handles {
        let mut values = Vec::with_capacity(row.len());
        for handle in row {
            values.push(handle.current_link()?.value()?);
        }
        volatilities.push(values);
    }
    let shifts = if shift_values.is_empty() {
        vec![vec![0.0; swap_lengths.len()]; option_times.len()]
    } else {
        shift_values.to_vec()
    };
    let volatilities =
        BilinearInterpolation::new(swap_lengths.clone(), option_times.clone(), volatilities)?;
    let shifts = BilinearInterpolation::new(swap_lengths, option_times, shifts)?;
    Ok(MatrixInterp {
        volatilities: wrap(volatilities, flat_extrapolation),
        shifts: wrap(shifts, flat_extrapolation),
    })
}

/// Wraps a bilinear either in a flat-clamping [`FlatExtrapolator2D`] (queries
/// past the grid return the nearest edge or corner) or, in the plain path, with
/// boundary-extending extrapolation enabled (mirroring the `true` flag C++ passes
/// on every interpolation call). The flat path leaves the inner bilinear's own
/// extrapolation off: the wrapper clamps every query into the closed domain, so
/// the inner is always evaluated in range.
fn wrap(bilinear: BilinearInterpolation, flat_extrapolation: bool) -> Box<dyn Interpolation2D> {
    if flat_extrapolation {
        Box::new(FlatExtrapolator2D::new(bilinear))
    } else {
        Box::new(bilinear.with_extrapolation(true))
    }
}

impl AsObservable for SwaptionVolatilityMatrix {
    fn observable(&self) -> &Observable {
        self.discrete.observable()
    }
}

impl TermStructure for SwaptionVolatilityMatrix {
    fn base(&self) -> &TermStructureBase {
        self.discrete.base()
    }

    fn max_date(&self) -> Date {
        self.discrete
            .option_dates()
            .ok()
            .and_then(|dates| dates.last().copied())
            .unwrap_or_else(Date::max_date)
    }
}

impl VolatilityTermStructure for SwaptionVolatilityMatrix {
    fn business_day_convention(&self) -> BusinessDayConvention {
        self.discrete.business_day_convention()
    }

    fn min_strike(&self) -> Rate {
        Rate::MIN
    }

    fn max_strike(&self) -> Rate {
        Rate::MAX
    }
}

impl SwaptionVolatilityStructure for SwaptionVolatilityMatrix {
    fn volatility_impl(
        &self,
        option_time: Time,
        swap_length: Time,
        _strike: Rate,
    ) -> QlResult<Volatility> {
        self.calculate()?;
        self.interp
            .borrow()
            .volatilities
            .value(swap_length, option_time)
    }

    fn max_swap_tenor(&self) -> Period {
        self.discrete
            .swap_tenors()
            .last()
            .copied()
            .expect("swap tenors are non-empty by construction")
    }

    fn volatility_type(&self) -> VolatilityType {
        self.volatility_type
    }

    fn shift_impl(&self, option_time: Time, swap_length: Time) -> QlResult<Real> {
        self.calculate()?;
        self.interp.borrow().shifts.value(swap_length, option_time)
    }
}

/// These tests mirror `testSwaptionVolMatrixCoherence`'s node-recovery arm
/// (`QuantLib/test-suite/swaptionvolatilitymatrix.cpp`, `makeCoherenceTest`)
/// with the `swaptionvolstructuresutilities.hpp` fixture. The discriminating
/// oracle is node recovery to 1e-16 (bilinear is exact at nodes) plus the
/// lazy-refresh arm, which alone catches an interpolation that is built once and
/// never rebuilt on a quote bump.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::patterns::observable::AsObservable;
    use crate::quotes::SimpleQuote;
    use crate::shared::shared;
    use crate::test_support::{Flag, as_observer};
    use crate::time::calendars::target::Target;
    use crate::time::date::Month;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;
    use crate::time::timeunit::TimeUnit;

    const BDC: BusinessDayConvention = BusinessDayConvention::ModifiedFollowing;
    const TOL: Real = 1e-16;

    fn option_tenors() -> Vec<Period> {
        vec![
            Period::new(1, TimeUnit::Months),
            Period::new(6, TimeUnit::Months),
            Period::new(1, TimeUnit::Years),
            Period::new(5, TimeUnit::Years),
            Period::new(10, TimeUnit::Years),
            Period::new(30, TimeUnit::Years),
        ]
    }

    fn swap_tenors() -> Vec<Period> {
        vec![
            Period::new(1, TimeUnit::Years),
            Period::new(5, TimeUnit::Years),
            Period::new(10, TimeUnit::Years),
            Period::new(30, TimeUnit::Years),
        ]
    }

    fn vols() -> Vec<Vec<Real>> {
        vec![
            vec![0.1300, 0.1560, 0.1390, 0.1220],
            vec![0.1440, 0.1580, 0.1460, 0.1260],
            vec![0.1600, 0.1590, 0.1470, 0.1290],
            vec![0.1640, 0.1470, 0.1370, 0.1220],
            vec![0.1400, 0.1300, 0.1250, 0.1100],
            vec![0.1130, 0.1090, 0.1070, 0.0930],
        ]
    }

    type QuoteGrid = Vec<Vec<Shared<SimpleQuote>>>;
    type HandleGrid = Vec<Vec<Handle<dyn Quote>>>;

    /// Per-cell `SimpleQuote`s (a distinct quote per node, as the C++ fixture
    /// comment requires) plus the handles wrapping them.
    fn quote_grid() -> (QuoteGrid, HandleGrid) {
        let quotes: Vec<Vec<Shared<SimpleQuote>>> = vols()
            .iter()
            .map(|row| row.iter().map(|&v| shared(SimpleQuote::new(v))).collect())
            .collect();
        let handles = quotes
            .iter()
            .map(|row| {
                row.iter()
                    .map(|q| Handle::new(q.clone() as Shared<dyn Quote>))
                    .collect()
            })
            .collect();
        (quotes, handles)
    }

    fn moving_surface(
        handles: Vec<Vec<Handle<dyn Quote>>>,
        settings: Shared<Settings<Date>>,
    ) -> SwaptionVolatilityMatrix {
        SwaptionVolatilityMatrix::moving(
            Target::new(),
            BDC,
            option_tenors(),
            swap_tenors(),
            handles,
            Actual365Fixed::new(),
            VolatilityType::ShiftedLognormal,
            Vec::new(),
            settings,
        )
        .unwrap()
    }

    fn moving_flat_surface(
        handles: Vec<Vec<Handle<dyn Quote>>>,
        settings: Shared<Settings<Date>>,
    ) -> SwaptionVolatilityMatrix {
        SwaptionVolatilityMatrix::moving_flat(
            Target::new(),
            BDC,
            option_tenors(),
            swap_tenors(),
            handles,
            Actual365Fixed::new(),
            VolatilityType::ShiftedLognormal,
            Vec::new(),
            settings,
        )
        .unwrap()
    }

    fn settings_at(date: Date) -> Shared<Settings<Date>> {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(date);
        settings
    }

    #[test]
    fn recovers_every_node_vol_to_machine_precision() {
        let settings = settings_at(Date::new(15, Month::June, 2026));
        let (_quotes, handles) = quote_grid();
        let surface = moving_surface(handles, settings);
        for (i, option_tenor) in option_tenors().into_iter().enumerate() {
            for (j, swap_tenor) in swap_tenors().into_iter().enumerate() {
                let got = surface
                    .volatility_tenors(option_tenor, swap_tenor, 0.0, false)
                    .unwrap();
                assert!(
                    (got - vols()[i][j]).abs() <= TOL,
                    "node ({i},{j}): got {got}, expected {}",
                    vols()[i][j]
                );
            }
        }
    }

    #[test]
    fn quote_bump_refreshes_the_interpolation_and_notifies() {
        let settings = settings_at(Date::new(15, Month::June, 2026));
        let (quotes, handles) = quote_grid();
        let surface = moving_surface(handles, settings);

        let node = surface
            .volatility_tenors(option_tenors()[0], swap_tenors()[0], 0.0, false)
            .unwrap();
        assert!((node - 0.1300).abs() <= TOL);

        let flag = Flag::new();
        surface.observable().register_observer(&as_observer(&flag));

        quotes[0][0].set_value(0.2000);
        assert!(Flag::is_up(&flag), "quote bump must notify observers");

        let refreshed = surface
            .volatility_tenors(option_tenors()[0], swap_tenors()[0], 0.0, false)
            .unwrap();
        assert!(
            (refreshed - 0.2000).abs() <= TOL,
            "bumped node must serve the new vol, got {refreshed}"
        );
        let neighbor = surface
            .volatility_tenors(option_tenors()[0], swap_tenors()[1], 0.0, false)
            .unwrap();
        assert!(
            (neighbor - 0.1560).abs() <= TOL,
            "untouched neighbor must be unchanged, got {neighbor}"
        );
    }

    #[test]
    fn between_nodes_stays_within_the_surrounding_corners() {
        let settings = settings_at(Date::new(15, Month::June, 2026));
        let (_quotes, handles) = quote_grid();
        let surface = moving_surface(handles, settings);

        let t0 = surface
            .time_from_reference(surface.option_date_from_tenor(option_tenors()[0]).unwrap())
            .unwrap();
        let t1 = surface
            .time_from_reference(surface.option_date_from_tenor(option_tenors()[1]).unwrap())
            .unwrap();
        let option_time = 0.5 * (t0 + t1);
        let swap_length = 3.0;

        let got = surface
            .volatility_time(option_time, swap_length, 0.0, false)
            .unwrap();
        let corners = [vols()[0][0], vols()[0][1], vols()[1][0], vols()[1][1]];
        let lo = corners.iter().cloned().fold(Real::INFINITY, Real::min);
        let hi = corners.iter().cloned().fold(Real::NEG_INFINITY, Real::max);
        assert!(
            lo <= got && got <= hi,
            "between-node vol {got} outside [{lo}, {hi}]"
        );
    }

    #[test]
    fn max_date_and_max_swap_tenor_come_from_the_grids() {
        let settings = settings_at(Date::new(15, Month::June, 2026));
        let (_quotes, handles) = quote_grid();
        let surface = moving_surface(handles, settings);
        let last_option_date = surface
            .option_date_from_tenor(*option_tenors().last().unwrap())
            .unwrap();
        assert_eq!(surface.max_date(), last_option_date);
        assert_eq!(surface.max_swap_tenor(), Period::new(30, TimeUnit::Years));
    }

    #[test]
    fn volatility_type_round_trips_the_constructor_argument() {
        let settings = settings_at(Date::new(15, Month::June, 2026));
        let (_quotes, handles) = quote_grid();
        let surface = SwaptionVolatilityMatrix::moving(
            Target::new(),
            BDC,
            option_tenors(),
            swap_tenors(),
            handles,
            Actual365Fixed::new(),
            VolatilityType::Normal,
            Vec::new(),
            settings,
        )
        .unwrap();
        assert_eq!(surface.volatility_type(), VolatilityType::Normal);
    }

    #[test]
    fn fixed_reference_matrix_constructor_recovers_nodes() {
        let reference = Date::new(15, Month::June, 2026);
        let mut matrix = Matrix::with_size(6, 4);
        for (i, row) in vols().iter().enumerate() {
            for (j, &v) in row.iter().enumerate() {
                matrix[(i, j)] = v;
            }
        }
        let surface = SwaptionVolatilityMatrix::new(
            reference,
            Target::new(),
            BDC,
            option_tenors(),
            swap_tenors(),
            &matrix,
            Actual365Fixed::new(),
            VolatilityType::ShiftedLognormal,
            &Matrix::new(),
        )
        .unwrap();
        for (i, option_tenor) in option_tenors().into_iter().enumerate() {
            for (j, swap_tenor) in swap_tenors().into_iter().enumerate() {
                let got = surface
                    .volatility_tenors(option_tenor, swap_tenor, 0.0, false)
                    .unwrap();
                assert!(
                    (got - vols()[i][j]).abs() <= TOL,
                    "node ({i},{j}): got {got}"
                );
            }
        }
    }

    #[test]
    fn flat_matrix_recovers_every_node_vol() {
        let settings = settings_at(Date::new(15, Month::June, 2026));
        let (_quotes, handles) = quote_grid();
        let surface = moving_flat_surface(handles, settings);
        for (i, option_tenor) in option_tenors().into_iter().enumerate() {
            for (j, swap_tenor) in swap_tenors().into_iter().enumerate() {
                let got = surface
                    .volatility_tenors(option_tenor, swap_tenor, 0.0, false)
                    .unwrap();
                assert!(
                    (got - vols()[i][j]).abs() <= TOL,
                    "node ({i},{j}): got {got}, expected {}",
                    vols()[i][j]
                );
            }
        }
    }

    #[test]
    fn flat_clamps_far_query_to_corner_while_plain_extends() {
        let settings = settings_at(Date::new(15, Month::June, 2026));
        let (_qf, handles_flat) = quote_grid();
        let (_qp, handles_plain) = quote_grid();
        let flat = moving_flat_surface(handles_flat, Shared::clone(&settings));
        let plain = moving_surface(handles_plain, settings);

        // Both axes past the grid (max swap length 30y, max option tenor 30y):
        // the flat matrix clamps to the (max swap, max option) corner node,
        // which is vols[5][3] = 0.0930.
        let far_option_time = 60.0;
        let far_swap_length = 50.0;
        let corner = vols()[5][3];

        let flat_vol = flat
            .volatility_time(far_option_time, far_swap_length, 0.0, true)
            .unwrap();
        assert!(
            (flat_vol - corner).abs() <= TOL,
            "flat far query {flat_vol} must equal corner node {corner}"
        );

        let plain_vol = plain
            .volatility_time(far_option_time, far_swap_length, 0.0, true)
            .unwrap();
        assert!(
            (plain_vol - flat_vol).abs() > 1e-6,
            "plain bilinear must extend past the corner (got {plain_vol}), not clamp like flat ({flat_vol})"
        );
    }

    #[test]
    fn flat_matrix_quote_bump_re_wraps_and_refreshes() {
        let settings = settings_at(Date::new(15, Month::June, 2026));
        let (quotes, handles) = quote_grid();
        let surface = moving_flat_surface(handles, settings);

        let far_option_time = 60.0;
        let far_swap_length = 50.0;
        let before = surface
            .volatility_time(far_option_time, far_swap_length, 0.0, true)
            .unwrap();
        assert!((before - vols()[5][3]).abs() <= TOL);

        quotes[5][3].set_value(0.2500);
        let after = surface
            .volatility_time(far_option_time, far_swap_length, 0.0, true)
            .unwrap();
        assert!(
            (after - 0.2500).abs() <= TOL,
            "flat far query must serve the bumped corner vol after rebuild, got {after}"
        );
    }

    #[test]
    fn wrong_shaped_vol_matrix_is_rejected() {
        let settings = settings_at(Date::new(15, Month::June, 2026));
        let (_quotes, mut handles) = quote_grid();
        handles.pop();
        assert!(
            SwaptionVolatilityMatrix::moving(
                Target::new(),
                BDC,
                option_tenors(),
                swap_tenors(),
                handles,
                Actual365Fixed::new(),
                VolatilityType::ShiftedLognormal,
                Vec::new(),
                settings,
            )
            .is_err()
        );
    }
}
