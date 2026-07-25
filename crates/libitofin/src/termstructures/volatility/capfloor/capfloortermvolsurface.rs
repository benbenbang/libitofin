//! Market cap/floor term-volatility surface.
//!
//! Port of `ql/termstructures/volatility/capfloor/capfloortermvolsurface.{hpp,cpp}`:
//! `class CapFloorTermVolSurface : public LazyObject, public
//! CapFloorTermVolatilityStructure`. The surface interpolates a grid of market
//! term volatilities indexed by cap/floor option time (rows) and strike
//! (columns) with a [`BicubicSpline`], the input the optionlet stripper consumes.
//!
//! ## Axis convention
//!
//! The vol grid `M[i][j]` is the volatility for the `i`-th option tenor and the
//! `j`-th strike. C++ builds `BicubicSpline(strikes.begin(), strikes.end(),
//! optionTimes.begin(), optionTimes.end(), vols_)`, so `x = strikes`,
//! `y = option_times` and `z = M` unchanged (the Rust [`BicubicSpline`] takes
//! `z[y][x]`, and `M` is already row-per-option-time, column-per-strike).
//! [`volatility_impl`](CapFloorTermVolSurface) therefore calls `value(strike, t)`,
//! matching C++'s `interpolation_(strike, t, true)`.
//!
//! ## Own grid handling and lazy refresh
//!
//! C++ `CapFloorTermVolSurface` embeds its own option-tenor -> date -> time grid
//! rather than deriving from a discrete base;
//! [`SwaptionVolatilityDiscrete`](super::super::SwaptionVolatilityDiscrete)
//! carries a swap-tenor axis with no counterpart here (the second axis is
//! strikes), so this port keeps that grid handling directly on
//! [`TermStructureBase`]. As with the swaption matrix, the Rust [`BicubicSpline`]
//! owns its `z` where C++ aliases `vols_` by reference, so the spline lives
//! behind a [`RefCell`] and every query routes through
//! [`calculate`](CapFloorTermVolSurface::calculate) first; `perform_calculations`
//! re-reads the quotes, recomputes the option times off the current reference
//! date and rebuilds the spline. A [`SurfaceUpdater`] on the base observable
//! invalidates the lazy state on a quote bump or an evaluation-date move.
//!
//! ## Divergences from QuantLib
//!
//! - All four C++ constructors are ported. Only the two `Handle<Quote>` forms
//!   register with market data, exactly as C++'s `registerWithMarketData()` is
//!   called only from them; the two `Matrix` forms fabricate internal, unobserved
//!   quotes.
//! - The C++ `mutable Matrix vols_` field is not held separately: the bicubic
//!   owns its `z`, so the load-bearing state is the spline rebuilt in
//!   `perform_calculations`, not a second copy of the vols.

use std::cell::RefCell;

use super::CapFloorTermVolatilityStructure;
use crate::errors::QlResult;
use crate::handle::Handle;
use crate::math::interpolations::Interpolation2D;
use crate::math::interpolations::bicubic::BicubicSpline;
use crate::math::matrix::Matrix;
use crate::patterns::lazyobject::LazyObject;
use crate::patterns::observable::{AsObservable, Observable, Observer};
use crate::quotes::{Quote, make_quote_handle};
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut, shared_mut};
use crate::termstructures::volatility::VolatilityTermStructure;
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::period::Period;
use crate::types::{Natural, Rate, Time, Volatility};
use crate::{fail, require};

/// The recomputed surface state: the option times (the spline's `y` axis) and the
/// bicubic spline rebuilt on every refresh.
struct SurfaceState {
    option_times: Vec<Time>,
    interpolation: BicubicSpline,
}

/// Invalidates the surface's lazy state when a quote bumps or the reference date
/// moves, so the next [`calculate`](CapFloorTermVolSurface::calculate) rebuilds
/// the spline.
struct SurfaceUpdater {
    lazy: SharedMut<LazyObject>,
}

impl Observer for SurfaceUpdater {
    fn update(&mut self) {
        self.lazy.borrow_mut().invalidate_silently();
    }
}

/// Market cap/floor term-volatility surface, bicubic over an option-time x strike
/// grid.
pub struct CapFloorTermVolSurface {
    base: TermStructureBase,
    business_day_convention: BusinessDayConvention,
    option_tenors: Vec<Period>,
    strikes: Vec<Rate>,
    vol_handles: Vec<Vec<Handle<dyn Quote>>>,
    interp: RefCell<SurfaceState>,
    lazy: SharedMut<LazyObject>,
    _updater: SharedMut<SurfaceUpdater>,
}

impl CapFloorTermVolSurface {
    /// Floating reference date (advanced `settlement_days` off the evaluation
    /// date), quote-backed market data. C++'s floating-reference +
    /// `vector<vector<Handle<Quote>>>` constructor; registers with market data.
    #[allow(clippy::too_many_arguments)]
    pub fn moving(
        settlement_days: Natural,
        calendar: Calendar,
        business_day_convention: BusinessDayConvention,
        option_tenors: Vec<Period>,
        strikes: Vec<Rate>,
        vols: Vec<Vec<Handle<dyn Quote>>>,
        day_counter: DayCounter,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<CapFloorTermVolSurface> {
        let base =
            TermStructureBase::moving(settlement_days, calendar, Some(day_counter), settings);
        Self::assemble(
            base,
            business_day_convention,
            option_tenors,
            strikes,
            vols,
            true,
        )
    }

    /// Fixed reference date, quote-backed market data. C++'s fixed-reference +
    /// `vector<vector<Handle<Quote>>>` constructor; registers with market data.
    #[allow(clippy::too_many_arguments)]
    pub fn with_reference_date(
        reference_date: Date,
        calendar: Calendar,
        business_day_convention: BusinessDayConvention,
        option_tenors: Vec<Period>,
        strikes: Vec<Rate>,
        vols: Vec<Vec<Handle<dyn Quote>>>,
        day_counter: DayCounter,
    ) -> QlResult<CapFloorTermVolSurface> {
        let base = TermStructureBase::with_reference_date(
            reference_date,
            Some(calendar),
            Some(day_counter),
        );
        Self::assemble(
            base,
            business_day_convention,
            option_tenors,
            strikes,
            vols,
            true,
        )
    }

    /// Fixed reference date, fixed market data. C++'s fixed-reference + `Matrix`
    /// constructor. The cells are wrapped in unobserved quote handles so the
    /// refresh path is shared with the quote forms; no market data is registered.
    #[allow(clippy::too_many_arguments)]
    pub fn with_reference_date_from_matrix(
        reference_date: Date,
        calendar: Calendar,
        business_day_convention: BusinessDayConvention,
        option_tenors: Vec<Period>,
        strikes: Vec<Rate>,
        vols: &Matrix,
        day_counter: DayCounter,
    ) -> QlResult<CapFloorTermVolSurface> {
        let base = TermStructureBase::with_reference_date(
            reference_date,
            Some(calendar),
            Some(day_counter),
        );
        Self::assemble(
            base,
            business_day_convention,
            option_tenors,
            strikes,
            matrix_to_handles(vols),
            false,
        )
    }

    /// Floating reference date, fixed market data. C++'s floating-reference +
    /// `Matrix` constructor. As with the fixed-reference `Matrix` form, the cells
    /// are wrapped in unobserved quote handles and no market data is registered.
    #[allow(clippy::too_many_arguments)]
    pub fn moving_from_matrix(
        settlement_days: Natural,
        calendar: Calendar,
        business_day_convention: BusinessDayConvention,
        option_tenors: Vec<Period>,
        strikes: Vec<Rate>,
        vols: &Matrix,
        day_counter: DayCounter,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<CapFloorTermVolSurface> {
        let base =
            TermStructureBase::moving(settlement_days, calendar, Some(day_counter), settings);
        Self::assemble(
            base,
            business_day_convention,
            option_tenors,
            strikes,
            matrix_to_handles(vols),
            false,
        )
    }

    fn assemble(
        base: TermStructureBase,
        business_day_convention: BusinessDayConvention,
        option_tenors: Vec<Period>,
        strikes: Vec<Rate>,
        vol_handles: Vec<Vec<Handle<dyn Quote>>>,
        register_market_data: bool,
    ) -> QlResult<CapFloorTermVolSurface> {
        check_inputs(&option_tenors, &strikes, &vol_handles)?;
        let reference = base.reference_date()?;
        let state = build_state(
            &base,
            business_day_convention,
            &option_tenors,
            &strikes,
            &vol_handles,
            reference,
        )?;
        if register_market_data {
            let base_updater = base.updater();
            for row in &vol_handles {
                for handle in row {
                    handle.register_observer(&base_updater);
                }
            }
        }
        let lazy = shared_mut(LazyObject::new(true));
        let updater = shared_mut(SurfaceUpdater {
            lazy: SharedMut::clone(&lazy),
        });
        base.observable()
            .register_observer(&(SharedMut::clone(&updater) as SharedMut<dyn Observer>));
        Ok(CapFloorTermVolSurface {
            base,
            business_day_convention,
            option_tenors,
            strikes,
            vol_handles,
            interp: RefCell::new(state),
            lazy,
            _updater: updater,
        })
    }

    /// Rebuilds the spline if a quote or the reference date has changed since it
    /// was last computed. Every query calls this first, as C++'s
    /// `volatilityImpl` calls `calculate()`.
    pub fn calculate(&self) -> QlResult<()> {
        if !self.lazy.borrow_mut().start_calculation() {
            return Ok(());
        }
        let result = self.perform_calculations();
        self.lazy.borrow_mut().finish_calculation(&result);
        result
    }

    fn perform_calculations(&self) -> QlResult<()> {
        let reference = self.base.reference_date()?;
        let state = build_state(
            &self.base,
            self.business_day_convention,
            &self.option_tenors,
            &self.strikes,
            &self.vol_handles,
            reference,
        )?;
        *self.interp.borrow_mut() = state;
        Ok(())
    }

    /// The cap/floor option tenors.
    pub fn option_tenors(&self) -> &[Period] {
        &self.option_tenors
    }

    /// The strike axis.
    pub fn strikes(&self) -> &[Rate] {
        &self.strikes
    }

    /// The option times (year fractions from the reference date).
    pub fn option_times(&self) -> QlResult<Vec<Time>> {
        self.calculate()?;
        Ok(self.interp.borrow().option_times.clone())
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

fn check_inputs(
    option_tenors: &[Period],
    strikes: &[Rate],
    vol_handles: &[Vec<Handle<dyn Quote>>],
) -> QlResult<()> {
    require!(!option_tenors.is_empty(), "empty option tenor vector");
    require!(
        option_tenors[0].length() > 0,
        "negative first option tenor: {}",
        option_tenors[0]
    );
    for i in 1..option_tenors.len() {
        let increasing = option_tenors[i] > option_tenors[i - 1];
        require!(
            increasing,
            "non increasing option tenor: {} is {}, {} is {}",
            i,
            option_tenors[i - 1],
            i + 1,
            option_tenors[i]
        );
    }
    require!(!strikes.is_empty(), "empty strike vector");
    for j in 1..strikes.len() {
        let increasing = strikes[j] > strikes[j - 1];
        require!(
            increasing,
            "non increasing strikes: {} is {}, {} is {}",
            j,
            strikes[j - 1],
            j + 1,
            strikes[j]
        );
    }
    require!(
        vol_handles.len() == option_tenors.len(),
        "mismatch between number of option tenors ({}) and number of volatility rows ({})",
        option_tenors.len(),
        vol_handles.len()
    );
    for row in vol_handles {
        require!(
            row.len() == strikes.len(),
            "mismatch between strikes ({}) and vol columns ({})",
            strikes.len(),
            row.len()
        );
    }
    Ok(())
}

fn build_state(
    base: &TermStructureBase,
    business_day_convention: BusinessDayConvention,
    option_tenors: &[Period],
    strikes: &[Rate],
    vol_handles: &[Vec<Handle<dyn Quote>>],
    reference: Date,
) -> QlResult<SurfaceState> {
    let Some(calendar) = base.calendar() else {
        fail!("no calendar for cap/floor term vol surface");
    };
    let Some(day_counter) = base.day_counter() else {
        fail!("no day counter for cap/floor term vol surface");
    };
    let mut option_times = Vec::with_capacity(option_tenors.len());
    for &tenor in option_tenors {
        let date = calendar.advance_by_period(reference, tenor, business_day_convention, false);
        option_times.push(day_counter.year_fraction(reference, date));
    }
    let mut vols = Vec::with_capacity(vol_handles.len());
    for row in vol_handles {
        let mut values = Vec::with_capacity(row.len());
        for handle in row {
            values.push(handle.current_link()?.value()?);
        }
        vols.push(values);
    }
    let interpolation =
        BicubicSpline::new(strikes.to_vec(), option_times.clone(), vols)?.with_extrapolation(true);
    Ok(SurfaceState {
        option_times,
        interpolation,
    })
}

impl AsObservable for CapFloorTermVolSurface {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl TermStructure for CapFloorTermVolSurface {
    fn base(&self) -> &TermStructureBase {
        &self.base
    }

    fn max_date(&self) -> Date {
        self.option_tenors
            .last()
            .and_then(|&tenor| self.option_date_from_tenor(tenor).ok())
            .unwrap_or_else(Date::max_date)
    }
}

impl VolatilityTermStructure for CapFloorTermVolSurface {
    fn business_day_convention(&self) -> BusinessDayConvention {
        self.business_day_convention
    }

    fn min_strike(&self) -> Rate {
        self.strikes[0]
    }

    fn max_strike(&self) -> Rate {
        self.strikes[self.strikes.len() - 1]
    }
}

impl CapFloorTermVolatilityStructure for CapFloorTermVolSurface {
    fn volatility_impl(&self, length: Time, strike: Rate) -> QlResult<Volatility> {
        self.calculate()?;
        self.interp.borrow().interpolation.value(strike, length)
    }
}

/// The oracle has two discriminating arms. Node recovery at 1e-12 over a
/// non-flat, non-square 4x5 grid (a transpose fails as a bicubic dimension
/// mismatch, an axis swap fails numerically). The required quote-bump arm, built
/// through a `Handle<Quote>` constructor, catches a spline that is built once and
/// never rebuilt on a quote bump. A between-nodes sanity check and the
/// min/max-strike + max-date pins round it out.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::quotes::SimpleQuote;
    use crate::shared::shared;
    use crate::test_support::{Flag, as_observer};
    use crate::time::calendars::target::Target;
    use crate::time::date::Month;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;
    use crate::types::Real;

    const BDC: BusinessDayConvention = BusinessDayConvention::ModifiedFollowing;
    const TOL: Real = 1e-12;
    const REFERENCE: fn() -> Date = || Date::new(15, Month::June, 2026);

    fn option_tenors() -> Vec<Period> {
        (1..=4)
            .map(|n| Period::new(n, crate::time::timeunit::TimeUnit::Years))
            .collect()
    }

    fn strikes() -> Vec<Rate> {
        vec![0.01, 0.02, 0.03, 0.04, 0.05]
    }

    fn vols() -> Vec<Vec<Real>> {
        (0..4)
            .map(|i| {
                (0..5)
                    .map(|j| 0.10 + 0.01 * i as Real + 0.001 * j as Real)
                    .collect()
            })
            .collect()
    }

    type QuoteGrid = Vec<Vec<Shared<SimpleQuote>>>;
    type HandleGrid = Vec<Vec<Handle<dyn Quote>>>;

    fn quote_grid() -> (QuoteGrid, HandleGrid) {
        let quotes: QuoteGrid = vols()
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

    fn fixed_handle_surface(handles: HandleGrid) -> CapFloorTermVolSurface {
        CapFloorTermVolSurface::with_reference_date(
            REFERENCE(),
            Target::new(),
            BDC,
            option_tenors(),
            strikes(),
            handles,
            Actual365Fixed::new(),
        )
        .unwrap()
    }

    fn vols_matrix() -> Matrix {
        let mut m = Matrix::with_size(4, 5);
        for (i, row) in vols().iter().enumerate() {
            for (j, &v) in row.iter().enumerate() {
                m[(i, j)] = v;
            }
        }
        m
    }

    fn settings_at(date: Date) -> Shared<Settings<Date>> {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(date);
        settings
    }

    #[test]
    fn recovers_every_node_vol_to_1e_12() {
        let (_quotes, handles) = quote_grid();
        let surface = fixed_handle_surface(handles);
        for (i, tenor) in option_tenors().into_iter().enumerate() {
            for (j, &strike) in strikes().iter().enumerate() {
                let got = surface.volatility_tenor(tenor, strike, false).unwrap();
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
        let (quotes, handles) = quote_grid();
        let surface = fixed_handle_surface(handles);

        let node = surface
            .volatility_tenor(option_tenors()[0], strikes()[0], false)
            .unwrap();
        assert!((node - 0.100).abs() <= TOL);

        let flag = Flag::new();
        surface.observable().register_observer(&as_observer(&flag));

        quotes[0][0].set_value(0.5);
        assert!(Flag::is_up(&flag), "quote bump must notify observers");

        let refreshed = surface
            .volatility_tenor(option_tenors()[0], strikes()[0], false)
            .unwrap();
        assert!(
            (refreshed - 0.5).abs() <= TOL,
            "bumped node must serve the new vol, got {refreshed}"
        );
        let neighbor = surface
            .volatility_tenor(option_tenors()[0], strikes()[1], false)
            .unwrap();
        assert!(
            (neighbor - 0.101).abs() <= TOL,
            "untouched neighbor must be unchanged, got {neighbor}"
        );
    }

    #[test]
    fn between_nodes_stays_near_the_surrounding_nodes() {
        let (_quotes, handles) = quote_grid();
        let surface = fixed_handle_surface(handles);

        let times = surface.option_times().unwrap();
        let option_time = 0.5 * (times[0] + times[1]);
        let strike = 0.5 * (strikes()[0] + strikes()[1]);

        let got = surface.volatility_time(option_time, strike, false).unwrap();
        let corners = [vols()[0][0], vols()[0][1], vols()[1][0], vols()[1][1]];
        let lo = corners.iter().cloned().fold(Real::INFINITY, Real::min);
        let hi = corners.iter().cloned().fold(Real::NEG_INFINITY, Real::max);
        let band = 1e-2;
        assert!(
            lo - band <= got && got <= hi + band,
            "between-node vol {got} outside [{lo}, {hi}] +/- {band}"
        );
    }

    #[test]
    fn min_max_strike_and_max_date_come_from_the_grid() {
        let (_quotes, handles) = quote_grid();
        let surface = fixed_handle_surface(handles);
        assert_eq!(surface.min_strike(), 0.01);
        assert_eq!(surface.max_strike(), 0.05);
        let last = surface
            .option_date_from_tenor(*option_tenors().last().unwrap())
            .unwrap();
        assert_eq!(surface.max_date(), last);
    }

    #[test]
    fn fixed_matrix_constructor_recovers_nodes_without_registering_quotes() {
        let surface = CapFloorTermVolSurface::with_reference_date_from_matrix(
            REFERENCE(),
            Target::new(),
            BDC,
            option_tenors(),
            strikes(),
            &vols_matrix(),
            Actual365Fixed::new(),
        )
        .unwrap();
        for (i, tenor) in option_tenors().into_iter().enumerate() {
            for (j, &strike) in strikes().iter().enumerate() {
                let got = surface.volatility_tenor(tenor, strike, false).unwrap();
                assert!(
                    (got - vols()[i][j]).abs() <= TOL,
                    "node ({i},{j}): got {got}"
                );
            }
        }
    }

    #[test]
    fn moving_constructors_build_and_price_at_a_node() {
        let settings = settings_at(Date::new(15, Month::January, 2026));
        let (_quotes, handles) = quote_grid();
        let moving = CapFloorTermVolSurface::moving(
            2,
            Target::new(),
            BDC,
            option_tenors(),
            strikes(),
            handles,
            Actual365Fixed::new(),
            settings.clone(),
        )
        .unwrap();
        let from_matrix = CapFloorTermVolSurface::moving_from_matrix(
            2,
            Target::new(),
            BDC,
            option_tenors(),
            strikes(),
            &vols_matrix(),
            Actual365Fixed::new(),
            settings,
        )
        .unwrap();
        for surface in [&moving, &from_matrix] {
            let got = surface
                .volatility_tenor(option_tenors()[1], strikes()[2], false)
                .unwrap();
            assert!((got - vols()[1][2]).abs() <= TOL, "moving node: got {got}");
        }
    }

    #[test]
    fn malformed_grids_are_rejected() {
        let (_quotes, mut handles) = quote_grid();
        handles.pop();
        assert!(
            CapFloorTermVolSurface::with_reference_date(
                REFERENCE(),
                Target::new(),
                BDC,
                option_tenors(),
                strikes(),
                handles,
                Actual365Fixed::new(),
            )
            .is_err(),
            "row/tenor count mismatch must be rejected"
        );

        let (_quotes, handles) = quote_grid();
        let bad_strikes = vec![0.02, 0.01, 0.03, 0.04, 0.05];
        assert!(
            CapFloorTermVolSurface::with_reference_date(
                REFERENCE(),
                Target::new(),
                BDC,
                option_tenors(),
                bad_strikes,
                handles,
                Actual365Fixed::new(),
            )
            .is_err(),
            "non-increasing strikes must be rejected"
        );
    }
}
