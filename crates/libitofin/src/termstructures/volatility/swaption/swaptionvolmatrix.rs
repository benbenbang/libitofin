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
//! - `flatExtrapolation` and its `FlatExtrapolator2D` wrapper are not ported
//!   (#569). Only the plain-bilinear (`flatExtrapolation = false`) path exists,
//!   so the parameter is omitted entirely rather than accepted-and-rejected.
//!   The two interpolations are built with extrapolation enabled, mirroring the
//!   `true` flag C++ passes on every interpolation call.
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

/// The two bilinear interpolations rebuilt on every refresh.
struct MatrixInterp {
    volatilities: BilinearInterpolation,
    shifts: BilinearInterpolation,
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
        let discrete = SwaptionVolatilityDiscrete::moving(
            option_tenors,
            swap_tenors,
            0,
            calendar,
            business_day_convention,
            day_counter,
            settings,
        )?;
        Self::assemble(discrete, vols, volatility_type, shifts)
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
        Self::assemble(discrete, vols, volatility_type, shift_values)
    }

    fn assemble(
        discrete: SwaptionVolatilityDiscrete,
        vol_handles: Vec<Vec<Handle<dyn Quote>>>,
        volatility_type: VolatilityType,
        shift_values: Vec<Vec<Real>>,
    ) -> QlResult<SwaptionVolatilityMatrix> {
        check_inputs(&discrete, &vol_handles, &shift_values)?;
        let base_updater = discrete.base().updater();
        for row in &vol_handles {
            for handle in row {
                handle.register_observer(&base_updater);
            }
        }
        let interp = build_interps(&discrete, &vol_handles, &shift_values)?;
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
        let interp = build_interps(&self.discrete, &self.vol_handles, &self.shift_values)?;
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
        BilinearInterpolation::new(swap_lengths.clone(), option_times.clone(), volatilities)?
            .with_extrapolation(true);
    let shifts =
        BilinearInterpolation::new(swap_lengths, option_times, shifts)?.with_extrapolation(true);
    Ok(MatrixInterp {
        volatilities,
        shifts,
    })
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
