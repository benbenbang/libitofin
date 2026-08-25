//! Year-on-year cap/floor term price surface.
//!
//! Port of `ql/experimental/inflation/yoycapfloortermpricesurface.{hpp,cpp}`:
//! [`YoYCapFloorTermPriceSurface`] is the abstract base (`hpp:42-145`), a
//! [`TermStructure`] over quoted year-on-year cap and floor price *matrices* -
//! the prices are input and interpolated, no cap/floor is ever priced - with
//! [`YoYCapFloorTermPriceSurfaceBase`] holding its members (`hpp:127-144`)
//! behind the base constructor (`cpp:25-101`).
//! [`InterpolatedYoYCapFloorTermPriceSurface`] is the concrete surface
//! (`hpp:148-232`), whose calculations intersect the two price surfaces into
//! ATM year-on-year swap rates and bootstrap a year-on-year curve from them.
//!
//! ## Divergences from QuantLib
//!
//! - C++ templates the concrete surface over `<Interpolator2D,
//!   Interpolator1D>`; only the `<Bicubic, Cubic>` instantiation - the one the
//!   test suite builds - is ported, the splines stored directly, mirroring how
//!   [`PiecewiseYoYInflationCurve`] fixes `Linear`.
//! - The C++ constructor runs `performCalculations()` eagerly (`hpp:276`);
//!   here the calculations run lazily on first use, and - like C++, whose
//!   `update()` only notifies (`hpp:281-285`) - exactly once: an evaluation
//!   date move after the first calculation leaves them stale on both sides.
//! - The moving reference date (settlement days 0, `cpp:39`) takes the shared
//!   [`Settings`] handle explicitly per D5.
//! - `indexIsInterpolated_` is `detail::CPI::isInterpolated(interpolation,
//!   yoyIndex)` in C++ (`cpp:42`, defined `inflationindex.cpp:428-431`); with
//!   the deprecated `AsIndex` arm unported that reduces to `interpolation ==
//!   Linear`.
//!
//! [`PiecewiseYoYInflationCurve`]: super::piecewiseyoyinflationcurve::PiecewiseYoYInflationCurve

use std::cell::RefCell;

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::indexes::inflationindex::{
    CpiInterpolationType, InflationIndex, YoYInflationIndex, inflation_period,
};
use crate::math::interpolations::bicubic::{Bicubic, BicubicSpline};
use crate::math::interpolations::cubic::{Cubic, CubicInterpolation};
use crate::math::interpolations::linear::Linear;
use crate::math::interpolations::{Interpolation, Interpolation2D, Interpolator, Interpolator2D};
use crate::math::matrix::Matrix;
use crate::math::solver1d::Solver1D;
use crate::math::solvers1d::brent::Brent;
use crate::patterns::observable::{AsObservable, Observable};
use crate::quotes::{Quote, SimpleQuote};
use crate::settings::Settings;
use crate::shared::{Shared, shared};
use crate::termstructures::inflation::inflationhelpers::{
    YearOnYearInflationSwapHelper, YoYInflationHelper,
};
use crate::termstructures::inflation::inflationtermstructure::YoYInflationTermStructure;
use crate::termstructures::inflation::piecewiseyoyinflationcurve::PiecewiseYoYInflationCurve;
use crate::termstructures::yields::Pillar;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;
use crate::types::{Natural, Rate, Real, Time};
use crate::{fail, require};

/// Shared holder for year-on-year cap/floor price surfaces: the base-class
/// members (`hpp:127-144`) plus the validated strike union, behind the
/// abstract-base constructor (`cpp:25-101`).
///
/// Concrete surfaces embed one and expose it through the abstract-base trait.
pub struct YoYCapFloorTermPriceSurfaceBase {
    term: TermStructureBase,
    fixing_days: Natural,
    business_day_convention: BusinessDayConvention,
    yoy_index: Shared<YoYInflationIndex>,
    observation_lag: Period,
    nominal_term_structure: Handle<dyn YieldTermStructure>,
    c_strikes: Vec<Rate>,
    f_strikes: Vec<Rate>,
    cf_maturities: Vec<Period>,
    c_price: Matrix,
    f_price: Matrix,
    index_is_interpolated: bool,
    cf_strikes: Vec<Rate>,
    settings: Shared<Settings<Date>>,
}

impl YoYCapFloorTermPriceSurfaceBase {
    /// The abstract-base constructor (`cpp:25-101`): a moving term structure
    /// with zero settlement days (`cpp:39`), the full data-consistency gate
    /// (`cpp:44-80`) and the cap/floor strike union (`cpp:83-100`).
    ///
    /// `c_price` and `f_price` are quoted prices by strike (rows) and maturity
    /// (columns).
    ///
    /// # Errors
    ///
    /// Every `QL_REQUIRE` of the C++ constructor: too few strikes or
    /// maturities, matrix dimensions not matching them, non-positive or
    /// non-increasing maturities, non-positive floor prices or ones decreasing
    /// in strike, non-positive cap prices or ones increasing in strike, and a
    /// strike union that is too small or not strictly increasing.
    #[allow(clippy::too_many_arguments, clippy::neg_cmp_op_on_partial_ord)]
    pub fn new(
        fixing_days: Natural,
        yy_lag: Period,
        yoy_index: Shared<YoYInflationIndex>,
        interpolation: CpiInterpolationType,
        nominal_term_structure: Handle<dyn YieldTermStructure>,
        day_counter: DayCounter,
        calendar: Calendar,
        business_day_convention: BusinessDayConvention,
        c_strikes: Vec<Rate>,
        f_strikes: Vec<Rate>,
        cf_maturities: Vec<Period>,
        c_price: Matrix,
        f_price: Matrix,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<YoYCapFloorTermPriceSurfaceBase> {
        require!(f_strikes.len() > 1, "not enough floor strikes");
        require!(c_strikes.len() > 1, "not enough cap strikes");
        require!(cf_maturities.len() > 1, "not enough maturities");
        require!(
            f_strikes.len() == f_price.rows(),
            "floor strikes vs floor price rows not equal"
        );
        require!(
            c_strikes.len() == c_price.rows(),
            "cap strikes vs cap price rows not equal"
        );
        require!(
            cf_maturities.len() == f_price.columns(),
            "maturities vs floor price columns not equal"
        );
        require!(
            cf_maturities.len() == c_price.columns(),
            "maturities vs cap price columns not equal"
        );

        for j in 0..cf_maturities.len() {
            require!(
                cf_maturities[j] > Period::new(0, TimeUnit::Days),
                "non-positive maturities"
            );
            if j > 0 {
                require!(
                    cf_maturities[j] > cf_maturities[j - 1],
                    "non-increasing maturities"
                );
            }
            for i in 0..f_price.rows() {
                require!(
                    f_price[(i, j)] > 0.0,
                    "non-positive floor price: {}",
                    f_price[(i, j)]
                );
                if i > 0 {
                    require!(
                        f_price[(i, j)] >= f_price[(i - 1, j)],
                        "non-increasing floor prices"
                    );
                }
            }
            for i in 0..c_price.rows() {
                require!(
                    c_price[(i, j)] > 0.0,
                    "non-positive cap price: {}",
                    c_price[(i, j)]
                );
                if i > 0 {
                    require!(
                        c_price[(i, j)] <= c_price[(i - 1, j)],
                        "non-decreasing cap prices"
                    );
                }
            }
        }

        // Repeats and overlaps between the two strike sets are expected, but
        // the union must carry each strike once, so only cap strikes strictly
        // above the top floor strike are appended (`cpp:83-94`).
        let mut cf_strikes = f_strikes.clone();
        let eps = 0.000_000_1;
        let max_f_strike = *f_strikes.last().expect("more than one floor strike");
        for &k in &c_strikes {
            if k > max_f_strike + eps {
                cf_strikes.push(k);
            }
        }
        require!(cf_strikes.len() > 2, "overall not enough strikes");
        for i in 1..cf_strikes.len() {
            require!(
                cf_strikes[i] > cf_strikes[i - 1],
                "cfStrikes not increasing"
            );
        }

        Ok(YoYCapFloorTermPriceSurfaceBase {
            term: TermStructureBase::moving(
                0,
                calendar,
                Some(day_counter),
                Shared::clone(&settings),
            ),
            fixing_days,
            business_day_convention,
            yoy_index,
            observation_lag: yy_lag,
            nominal_term_structure,
            c_strikes,
            f_strikes,
            cf_maturities,
            c_price,
            f_price,
            index_is_interpolated: interpolation == CpiInterpolationType::Linear,
            cf_strikes,
            settings,
        })
    }

    /// The wrapped term-structure holder.
    pub fn term_structure_base(&self) -> &TermStructureBase {
        &self.term
    }

    /// The fixing days of the quoted instruments (`hpp:127`).
    pub fn fixing_days(&self) -> Natural {
        self.fixing_days
    }

    /// The business day convention of the quoted instruments (`hpp:128`).
    pub fn business_day_convention(&self) -> BusinessDayConvention {
        self.business_day_convention
    }

    /// The year-on-year index the surface is quoted on (`hpp:129`).
    pub fn yoy_index(&self) -> &Shared<YoYInflationIndex> {
        &self.yoy_index
    }

    /// The observation lag of the quoted instruments (`hpp:130`).
    pub fn observation_lag(&self) -> Period {
        self.observation_lag
    }

    /// The nominal curve the ATM bootstrap discounts on (`hpp:131`; C++ keeps
    /// it protected, with no public accessor).
    pub fn nominal_term_structure(&self) -> &Handle<dyn YieldTermStructure> {
        &self.nominal_term_structure
    }

    /// The quoted cap strikes (`hpp:133`).
    pub fn cap_strikes(&self) -> &[Rate] {
        &self.c_strikes
    }

    /// The quoted floor strikes (`hpp:134`).
    pub fn floor_strikes(&self) -> &[Rate] {
        &self.f_strikes
    }

    /// The quoted maturities (`hpp:135`).
    pub fn maturities(&self) -> &[Period] {
        &self.cf_maturities
    }

    /// The quoted cap prices, by strike (rows) and maturity (columns)
    /// (`hpp:137`).
    pub fn cap_price_matrix(&self) -> &Matrix {
        &self.c_price
    }

    /// The quoted floor prices, by strike (rows) and maturity (columns)
    /// (`hpp:138`).
    pub fn floor_price_matrix(&self) -> &Matrix {
        &self.f_price
    }

    /// Whether the observations interpolate the index (`hpp:139`).
    pub fn index_is_interpolated(&self) -> bool {
        self.index_is_interpolated
    }

    /// The cap/floor strike union: every floor strike, then the cap strikes
    /// above them (`hpp:141`).
    pub fn strikes(&self) -> &[Rate] {
        &self.cf_strikes
    }

    /// The shared settings the surface's moving reference date reads.
    pub fn settings(&self) -> &Shared<Settings<Date>> {
        &self.settings
    }
}

/// Abstract base for year-on-year cap/floor term price surfaces
/// (`YoYCapFloorTermPriceSurface`, `hpp:42-145`).
///
/// Downstream consumers (the stripped optionlet surfaces of #874) hold a
/// `Shared<dyn YoYCapFloorTermPriceSurface>` and read the quoted grid and the
/// interpolated prices through it. Note the C++ caveats (`hpp:74-80`): a
/// `price` alone does not say cap or floor without the ATM level, and ATM
/// prices are generally inaccurate, coming from extrapolation and
/// intersection.
pub trait YoYCapFloorTermPriceSurface: TermStructure {
    /// The embedded shared holder.
    fn surface_base(&self) -> &YoYCapFloorTermPriceSurfaceBase;

    /// Whether the observations interpolate the index (`hpp:237-239`).
    fn index_is_interpolated(&self) -> bool {
        self.surface_base().index_is_interpolated()
    }

    /// The observation lag of the quoted instruments (`hpp:241-243`).
    fn observation_lag(&self) -> Period {
        self.surface_base().observation_lag()
    }

    /// The frequency of the index the surface is quoted on (`hpp:245-247`).
    fn frequency(&self) -> Frequency {
        self.surface_base().yoy_index().frequency()
    }

    /// The business day convention of the quoted instruments (`hpp:82`).
    fn business_day_convention(&self) -> BusinessDayConvention {
        self.surface_base().business_day_convention()
    }

    /// The fixing days of the quoted instruments (`hpp:83`).
    fn fixing_days(&self) -> Natural {
        self.surface_base().fixing_days()
    }

    /// The year-on-year index the surface is quoted on (`hpp:72`).
    fn yoy_index(&self) -> &Shared<YoYInflationIndex> {
        self.surface_base().yoy_index()
    }

    /// The cap/floor strike union: every floor strike, then the cap strikes
    /// above them (`hpp:103`).
    fn strikes(&self) -> &[Rate] {
        self.surface_base().strikes()
    }

    /// The quoted cap strikes (`hpp:104`).
    fn cap_strikes(&self) -> &[Rate] {
        self.surface_base().cap_strikes()
    }

    /// The quoted floor strikes (`hpp:105`).
    fn floor_strikes(&self) -> &[Rate] {
        self.surface_base().floor_strikes()
    }

    /// The quoted maturities (`hpp:106`).
    fn maturities(&self) -> &[Period] {
        self.surface_base().maturities()
    }

    /// The lowest strike of the union (`hpp:107`).
    fn min_strike(&self) -> Rate {
        self.surface_base().strikes()[0]
    }

    /// The highest strike of the union (`hpp:108`).
    fn max_strike(&self) -> Rate {
        *self
            .surface_base()
            .strikes()
            .last()
            .expect("the union carries more than two strikes")
    }

    /// The first quoted maturity off the reference date (`hpp:109`, with its
    /// index-interpolation `\TODO` still open upstream).
    fn min_maturity(&self) -> QlResult<Date> {
        Ok(self.reference_date()? + self.surface_base().maturities()[0])
    }

    /// The last quoted maturity off the reference date (`hpp:110`).
    fn max_maturity(&self) -> QlResult<Date> {
        Ok(self.reference_date()?
            + *self
                .surface_base()
                .maturities()
                .last()
                .expect("more than one maturity"))
    }

    /// The option date a tenor quotes: the reference date advanced by it
    /// (`cpp:103-106`).
    fn yoy_option_date_from_tenor(&self, p: Period) -> QlResult<Date> {
        Ok(self.reference_date()? + p)
    }

    /// Whether `strike` lies within the quoted strike union (`hpp:116-118`).
    fn check_strike(&self, strike: Rate) -> bool {
        self.min_strike() <= strike && strike <= self.max_strike()
    }

    /// Whether `date` lies within the quoted maturities (`hpp:119-121`).
    fn check_maturity(&self, date: Date) -> QlResult<bool> {
        Ok(self.min_maturity()? <= date && date <= self.max_maturity()?)
    }

    /// ATM year-on-year swap rates from put-call parity on the cap/floor
    /// data, as (time, rate) vectors on yearly maturities (`hpp:64-65`).
    fn atm_yoy_swap_time_rates(&self) -> QlResult<(Vec<Time>, Vec<Rate>)>;

    /// ATM year-on-year swap rates as (date, rate) vectors (`hpp:66-67`).
    fn atm_yoy_swap_date_rates(&self) -> QlResult<(Vec<Date>, Vec<Rate>)>;

    /// The interpolated price at `(d, k)` (`hpp:85`): the cap price above the
    /// ATM swap level, the floor price below it.
    fn price(&self, d: Date, k: Rate) -> QlResult<Real>;

    /// The interpolated cap price at `(d, k)` (`hpp:86`).
    fn cap_price(&self, d: Date, k: Rate) -> QlResult<Real>;

    /// The interpolated floor price at `(d, k)` (`hpp:87`).
    fn floor_price(&self, d: Date, k: Rate) -> QlResult<Real>;

    /// The ATM year-on-year swap rate at `d`, off the intersection curve
    /// (`hpp:88-89`; C++ defaults `extrapolate` to `true`).
    fn atm_yoy_swap_rate(&self, d: Date, extrapolate: bool) -> QlResult<Rate>;

    /// The ATM year-on-year rate at `d` (`hpp:90-92`, `hpp:191-198`): the
    /// bootstrapped year-on-year curve read at `d` less the lag - the
    /// surface's own observation lag when `obs_lag` is `None`, C++'s
    /// `Period(-1, Days)` sentinel default.
    fn atm_yoy_rate(&self, d: Date, obs_lag: Option<Period>, extrapolate: bool) -> QlResult<Rate>;

    /// The year-on-year term structure derived from the ATM swap rates
    /// (`YoYTS`, `hpp:70`, `hpp:184`).
    fn yoy_ts(&self) -> QlResult<Shared<dyn YoYInflationTermStructure>>;

    /// The bootstrapped curve's base date (`hpp:84`, `hpp:172`); a `Result`
    /// because the bootstrap it reads runs on first use.
    fn base_date(&self) -> QlResult<Date>;

    /// The price a tenor quotes (`cpp:108-110`).
    fn price_by_tenor(&self, p: Period, k: Rate) -> QlResult<Real> {
        self.price(self.yoy_option_date_from_tenor(p)?, k)
    }

    /// The cap price a tenor quotes (`cpp:112-114`).
    fn cap_price_by_tenor(&self, p: Period, k: Rate) -> QlResult<Real> {
        self.cap_price(self.yoy_option_date_from_tenor(p)?, k)
    }

    /// The floor price a tenor quotes (`cpp:116-118`).
    fn floor_price_by_tenor(&self, p: Period, k: Rate) -> QlResult<Real> {
        self.floor_price(self.yoy_option_date_from_tenor(p)?, k)
    }

    /// The ATM year-on-year swap rate a tenor quotes (`cpp:120-123`).
    fn atm_yoy_swap_rate_by_tenor(&self, p: Period, extrapolate: bool) -> QlResult<Rate> {
        self.atm_yoy_swap_rate(self.yoy_option_date_from_tenor(p)?, extrapolate)
    }

    /// The ATM year-on-year rate a tenor quotes (`cpp:125-129`).
    fn atm_yoy_rate_by_tenor(
        &self,
        p: Period,
        obs_lag: Option<Period>,
        extrapolate: bool,
    ) -> QlResult<Rate> {
        self.atm_yoy_rate(self.yoy_option_date_from_tenor(p)?, obs_lag, extrapolate)
    }
}

/// The results of [`intersect`](InterpolatedYoYCapFloorTermPriceSurface::intersect)
/// (C++'s mutable members `capPrice_`/`floorPrice_`/`atmYoYSwapTimeRates_`/
/// `atmYoYSwapDateRates_`/`atmYoYSwapRateCurve_`, `hpp:223-231` and `:143-144`).
///
/// Both price splines carry `enableExtrapolation()` (`hpp:368,375`); the 1-D
/// swap rate curve extrapolates too, C++ passing `extrapolate` per query with
/// a `true` default - the range check lives in
/// [`atm_yoy_swap_rate`](YoYCapFloorTermPriceSurface::atm_yoy_swap_rate).
struct Intersection {
    cap_price: BicubicSpline,
    floor_price: BicubicSpline,
    atm_yoy_swap_time_rates: (Vec<Time>, Vec<Rate>),
    atm_yoy_swap_date_rates: (Vec<Date>, Vec<Rate>),
    atm_yoy_swap_rate_curve: CubicInterpolation,
}

/// The concrete surface, C++'s
/// `InterpolatedYoYCapFloorTermPriceSurface<Bicubic, Cubic>` (`hpp:148-232`):
/// bicubic-spline price surfaces over the quoted grid and a cubic ATM
/// year-on-year swap rate curve through their intersections.
///
/// Construction only validates and stores the quotes; the calculations run on
/// first use per the module divergences.
pub struct InterpolatedYoYCapFloorTermPriceSurface {
    base: YoYCapFloorTermPriceSurfaceBase,
    intersection: RefCell<Option<Intersection>>,
    yoy: RefCell<Option<Shared<dyn YoYInflationTermStructure>>>,
}

impl InterpolatedYoYCapFloorTermPriceSurface {
    /// Builds the surface over quoted cap and floor prices by strike (rows)
    /// and maturity (columns) (`hpp:252-277`, less the eager
    /// `performCalculations()` per the module divergences).
    ///
    /// # Errors
    ///
    /// The data-consistency gate of
    /// [`YoYCapFloorTermPriceSurfaceBase::new`].
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        fixing_days: Natural,
        yy_lag: Period,
        yoy_index: Shared<YoYInflationIndex>,
        interpolation: CpiInterpolationType,
        nominal_term_structure: Handle<dyn YieldTermStructure>,
        day_counter: DayCounter,
        calendar: Calendar,
        business_day_convention: BusinessDayConvention,
        c_strikes: Vec<Rate>,
        f_strikes: Vec<Rate>,
        cf_maturities: Vec<Period>,
        c_price: Matrix,
        f_price: Matrix,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<InterpolatedYoYCapFloorTermPriceSurface> {
        Ok(InterpolatedYoYCapFloorTermPriceSurface {
            base: YoYCapFloorTermPriceSurfaceBase::new(
                fixing_days,
                yy_lag,
                yoy_index,
                interpolation,
                nominal_term_structure,
                day_counter,
                calendar,
                business_day_convention,
                c_strikes,
                f_strikes,
                cf_maturities,
                c_price,
                f_price,
                settings,
            )?,
            intersection: RefCell::new(None),
            yoy: RefCell::new(None),
        })
    }

    /// Runs the calculations if they have not run yet (`performCalculations`,
    /// `hpp:288-298`): the cap/floor intersection, then the year-on-year
    /// bootstrap over its ATM swap rates.
    ///
    /// C++ runs this once, from the constructor, and `update()` only notifies
    /// (`hpp:281-285`); this port runs it once, on first use, and never again
    /// either - an evaluation date move afterwards leaves the cached results
    /// stale on both sides, a deferral the module divergences record.
    ///
    /// # Errors
    ///
    /// Whatever stops either phase: a failed crossover solve, an intersection
    /// outside its arbitrage bounds past the extrapolation horizon, a helper
    /// that cannot be built or bootstrapped, or the `1e-5` reprice gate.
    pub fn calculate(&self) -> QlResult<()> {
        self.intersect()?;
        if self.yoy.borrow().is_none() {
            let curve = self.calculate_yoy_term_structure()?;
            *self.yoy.borrow_mut() = Some(curve);
        }
        Ok(())
    }

    /// The intersection of the cap and floor price surfaces (`hpp:344-518`).
    ///
    /// Bicubic surfaces are fitted over the quoted prices with extrapolation
    /// enabled; per maturity, the strike where cap and floor prices cross is
    /// bracketed by stepping from the top floor strike and solved by Brent to
    /// `1e-7` - that crossover strike is the ATM year-on-year swap rate by
    /// put-call parity. A maturity whose bracket search runs out of trials or
    /// whose solution violates the arbitrage lower bound is only an error past
    /// `maxExtrapolationMaturity`; before it, the rate is replaced by the
    /// `intrinsicValueAddOn` heuristic over the bound (`hpp:496-502`). A cubic
    /// curve through the (time, rate) pairs closes the phase (`hpp:514-517`).
    ///
    /// The valid arm of the fill loop reads `cfMaturities_.at(counter)` where
    /// the invalid arm reads `[i]` (`hpp:507` against `hpp:493-494`); the
    /// `counter` indexing is ported faithfully rather than normalised to `i` -
    /// unobservable while every maturity is valid (`counter == i`), divergent
    /// in the general case (#262 class).
    fn intersect(&self) -> QlResult<()> {
        if self.intersection.borrow().is_some() {
            return Ok(());
        }

        const MAX_SEARCH_RANGE: Real = 0.0201;
        const MAX_EXTRAPOLATION_MATURITY: Real = 5.01;
        const SEARCH_STEP: Real = 0.0050;
        const INTRINSIC_VALUE_ADD_ON: Real = 0.001;

        let base = &self.base;
        let maturities = base.maturities();
        let f_strikes = base.floor_strikes();
        let c_strikes = base.cap_strikes();
        let n = maturities.len();
        let mut valid_maturity = vec![false; n];

        let mut cf_maturity_times: Vec<Time> = Vec::with_capacity(n);
        for &maturity in maturities {
            cf_maturity_times
                .push(self.time_from_reference(self.yoy_option_date_from_tenor(maturity)?)?);
        }

        let rows_of = |m: &Matrix| (0..m.rows()).map(|i| m.row(i).to_vec()).collect();
        let cap_price = Bicubic
            .interpolate(
                cf_maturity_times.clone(),
                c_strikes.to_vec(),
                rows_of(base.cap_price_matrix()),
            )?
            .with_extrapolation(true);
        let floor_price = Bicubic
            .interpolate(
                cf_maturity_times.clone(),
                f_strikes.to_vec(),
                rows_of(base.floor_price_matrix()),
            )?
            .with_extrapolation(true);

        let nominal = base.nominal_term_structure().current_link()?;
        let solver_tolerance = 1e-7;
        let mut min_swap_rate_intersection = vec![0.0; n];
        let mut max_swap_rate_intersection = vec![0.0; n];
        let mut tmp_swap_maturities: Vec<Time> = Vec::new();
        let mut tmp_swap_rates: Vec<Rate> = Vec::new();
        for i in 0..n {
            let t = cf_maturity_times[i];
            let num_years = t.round() as usize;
            let mut sum_discount = 0.0;
            for j in 0..num_years {
                sum_discount += nominal.discount(j as Real + 1.0, false)?;
            }
            let mut tmp_min_swap_rate_intersection = -1.0e10;
            let mut tmp_max_swap_rate_intersection = 1.0e10;
            for &strike in f_strikes {
                let price = floor_price.value(t, strike)?;
                let min_swap_rate = strike - price / (sum_discount * 10_000.0);
                if min_swap_rate > tmp_min_swap_rate_intersection {
                    tmp_min_swap_rate_intersection = min_swap_rate;
                }
            }
            for &strike in c_strikes {
                let price = cap_price.value(t, strike)?;
                let max_swap_rate = strike + price / (sum_discount * 10_000.0);
                if max_swap_rate < tmp_max_swap_rate_intersection {
                    tmp_max_swap_rate_intersection = max_swap_rate;
                }
            }
            max_swap_rate_intersection[i] = tmp_max_swap_rate_intersection;
            min_swap_rate_intersection[i] = tmp_min_swap_rate_intersection;

            // Find the interval where the intersection lies (`hpp:413-452`).
            let top_strike = *f_strikes.last().expect("more than one floor strike");
            let mut trials_exceeded = false;
            let num_trials = (MAX_SEARCH_RANGE / SEARCH_STEP) as i32;
            let (lo, hi);
            if floor_price.value(t, top_strike)? > cap_price.value(t, top_strike)? {
                let mut counter = 1;
                let mut stop = false;
                let mut strike = 0.0;
                while !stop {
                    strike = top_strike - Real::from(counter) * SEARCH_STEP;
                    if floor_price.value(t, strike)? < cap_price.value(t, strike)? {
                        stop = true;
                    }
                    counter += 1;
                    if counter == num_trials + 1 && !stop {
                        stop = true;
                        trials_exceeded = true;
                    }
                }
                lo = strike;
                hi = strike + SEARCH_STEP;
            } else {
                let mut counter = 1;
                let mut stop = false;
                let mut strike = 0.0;
                while !stop {
                    strike = top_strike + Real::from(counter) * SEARCH_STEP;
                    if floor_price.value(t, strike)? > cap_price.value(t, strike)? {
                        stop = true;
                    }
                    counter += 1;
                    if counter == num_trials + 1 && !stop {
                        stop = true;
                        trials_exceeded = true;
                    }
                }
                lo = strike - SEARCH_STEP;
                hi = strike;
            }

            let guess = (hi + lo) / 2.0;

            if !trials_exceeded {
                // The objective allows extrapolation because the strike
                // overlap is typically insufficient (`hpp:336-341`); both
                // splines already extrapolate, and a failed read poisons the
                // solve as NaN, which surfaces as the solver's error.
                let objective = |k: Real| match (cap_price.value(t, k), floor_price.value(t, k)) {
                    (Ok(cap), Ok(floor)) => cap - floor,
                    _ => Real::NAN,
                };
                let k_i = match Brent::new().solve_bracketed(
                    objective,
                    solver_tolerance,
                    guess,
                    lo,
                    hi,
                ) {
                    Ok(k_i) => k_i,
                    Err(error) => fail!(
                        "cap/floor intersection finding failed at t = {t}, error msg: {error}"
                    ),
                };
                if k_i <= min_swap_rate_intersection[i] {
                    if t > MAX_EXTRAPOLATION_MATURITY {
                        fail!(
                            "cap/floor intersection finding failed at t = {t}, error msg: \
                             intersection value is below the arbitrage free lower bound {}",
                            min_swap_rate_intersection[i]
                        );
                    }
                } else {
                    tmp_swap_maturities.push(t);
                    tmp_swap_rates.push(k_i);
                    valid_maturity[i] = true;
                }
            } else if t > MAX_EXTRAPOLATION_MATURITY {
                fail!(
                    "cap/floor intersection finding failed at t = {t}, error msg: no \
                     intersection found inside the admissible range"
                );
            }
        }

        let reference = self.reference_date()?;
        let mut atm_yoy_swap_time_rates: (Vec<Time>, Vec<Rate>) = (Vec::new(), Vec::new());
        let mut atm_yoy_swap_date_rates: (Vec<Date>, Vec<Rate>) = (Vec::new(), Vec::new());
        let mut counter = 0;
        for i in 0..n {
            if !valid_maturity[i] {
                atm_yoy_swap_date_rates.0.push(reference + maturities[i]);
                atm_yoy_swap_time_rates
                    .0
                    .push(self.time_from_reference(reference + maturities[i])?);
                // Heuristic (`hpp:495-500`): a swap rate keeping every
                // option's intrinsic value below its price.
                let mut new_swap_rate = min_swap_rate_intersection[i] + INTRINSIC_VALUE_ADD_ON;
                if new_swap_rate > max_swap_rate_intersection[i] {
                    new_swap_rate =
                        0.5 * (min_swap_rate_intersection[i] + max_swap_rate_intersection[i]);
                }
                atm_yoy_swap_time_rates.1.push(new_swap_rate);
                atm_yoy_swap_date_rates.1.push(new_swap_rate);
            } else {
                atm_yoy_swap_time_rates.0.push(tmp_swap_maturities[counter]);
                atm_yoy_swap_time_rates.1.push(tmp_swap_rates[counter]);
                // `.at(counter)`, not `[i]` (`hpp:507`); see the method docs.
                atm_yoy_swap_date_rates
                    .0
                    .push(self.yoy_option_date_from_tenor(maturities[counter])?);
                atm_yoy_swap_date_rates.1.push(tmp_swap_rates[counter]);
                counter += 1;
            }
        }

        let atm_yoy_swap_rate_curve = Cubic
            .interpolate(&atm_yoy_swap_time_rates.0, &atm_yoy_swap_time_rates.1)?
            .with_extrapolation(true);

        *self.intersection.borrow_mut() = Some(Intersection {
            cap_price,
            floor_price,
            atm_yoy_swap_time_rates,
            atm_yoy_swap_date_rates,
            atm_yoy_swap_rate_curve,
        });
        Ok(())
    }

    /// `atmYoYSwapRate` off the intersection alone (`hpp:188-190`), for the
    /// bootstrap phase to read without re-entering
    /// [`calculate`](Self::calculate). The range check is C++'s
    /// `Interpolation::checkRange` with the curve's own extrapolation flag
    /// unset: the `extrapolate` argument decides.
    fn atm_yoy_swap_rate_impl(&self, d: Date, extrapolate: bool) -> QlResult<Rate> {
        self.intersect()?;
        let t = self.time_from_reference(d)?;
        let guard = self.intersection.borrow();
        let curve = &guard
            .as_ref()
            .expect("intersect just filled the cache")
            .atm_yoy_swap_rate_curve;
        require!(
            extrapolate || (curve.x_min() <= t && t <= curve.x_max()),
            "atm yoy swap rate time ({t}) outside the curve range [{}, {}]: extrapolation not \
             allowed",
            curve.x_min(),
            curve.x_max()
        );
        curve.value(t)
    }

    /// The year-on-year bootstrap over the intersection's ATM swap rates
    /// (`calculateYoYTermStructure`, `hpp:521-570`): one
    /// [`YearOnYearInflationSwapHelper`] per year out to the last quoted
    /// maturity, each quoting the intersection curve's rate at the nominal
    /// curve's reference date advanced by that many years, bootstrapped into a
    /// [`PiecewiseYoYInflationCurve`] - `Linear` regardless of this surface's
    /// 1-D interpolator, hardcoded as C++ hardcodes it (`hpp:553-556`,
    /// "Linear is OK because we have every year"). The base date is the start
    /// of the inflation period one observation lag before the nominal
    /// reference date; the base rate is the ATM swap rate at this surface's
    /// own reference date, the curve end chosen for self-consistency
    /// (`hpp:544-550`).
    ///
    /// The Rust helper takes two arguments the C++ constructor lacks: the
    /// pillar choice, [`Pillar::LastRelevantDate`] here as C++ defaults it
    /// (`inflationhelpers.hpp:130`), and the settings handle (D5). The curve
    /// constructor likewise takes a seasonality the C++ six-argument one
    /// lacks: `None`.
    ///
    /// Closes with C++'s own reprice gate (`hpp:560-569`), part of the port
    /// and not only of its tests: every helper's implied quote must come back
    /// within `1e-5` of the rate it was quoted at.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    fn calculate_yoy_term_structure(&self) -> QlResult<Shared<dyn YoYInflationTermStructure>> {
        let base = &self.base;
        let nominal = base.nominal_term_structure().current_link()?;
        let nominal_reference = nominal.reference_date()?;
        let reference = self.reference_date()?;
        let frequency = base.yoy_index().frequency();
        let day_counter = self.require_day_counter()?;
        let Some(calendar) = self.calendar() else {
            fail!("the surface holds a calendar by construction");
        };
        let last_maturity = *base.maturities().last().expect("more than one maturity");
        let n_years = self.time_from_reference(reference + last_maturity)?.round() as i32;
        let interpolation = if base.index_is_interpolated() {
            CpiInterpolationType::Linear
        } else {
            CpiInterpolationType::Flat
        };

        let mut helpers: Vec<Shared<YearOnYearInflationSwapHelper>> = Vec::new();
        for i in 1..=n_years {
            let maturity = nominal_reference + Period::new(i, TimeUnit::Years);
            let quote = shared(SimpleQuote::new(Some(
                self.atm_yoy_swap_rate_impl(maturity, true)?,
            )));
            helpers.push(YearOnYearInflationSwapHelper::new(
                Handle::new(quote as Shared<dyn Quote>),
                base.observation_lag(),
                maturity,
                calendar.clone(),
                base.business_day_convention(),
                day_counter.clone(),
                base.yoy_index(),
                interpolation,
                base.nominal_term_structure().clone(),
                Pillar::LastRelevantDate,
                Shared::clone(base.settings()),
            )?);
        }

        let base_date = inflation_period(nominal_reference - base.observation_lag(), frequency)?.0;
        let base_yoy_rate = self.atm_yoy_swap_rate_impl(reference, true)?;

        let curve = PiecewiseYoYInflationCurve::<Linear>::new(
            nominal_reference,
            base_date,
            base_yoy_rate,
            frequency,
            day_counter,
            helpers
                .iter()
                .map(|helper| Shared::clone(helper) as Shared<dyn YoYInflationHelper>)
                .collect(),
            None,
        )?;
        curve.calculate()?;

        let eps = 1e-5;
        for (i, helper) in helpers.iter().enumerate() {
            let original = self.atm_yoy_swap_rate_impl(
                self.yoy_option_date_from_tenor(Period::new(i as i32 + 1, TimeUnit::Years))?,
                true,
            )?;
            let implied = helper.implied_quote()?;
            require!(
                (implied - original).abs() < eps,
                "could not reprice helper {i}, data {original}, implied quote {implied}"
            );
        }

        Ok(curve as Shared<dyn YoYInflationTermStructure>)
    }
}

impl AsObservable for InterpolatedYoYCapFloorTermPriceSurface {
    fn observable(&self) -> &Observable {
        self.base.term_structure_base().observable()
    }
}

impl TermStructure for InterpolatedYoYCapFloorTermPriceSurface {
    fn base(&self) -> &TermStructureBase {
        self.base.term_structure_base()
    }

    /// The bootstrapped curve's maximum date (`hpp:171`), triggering the
    /// calculations first as [`PiecewiseYoYInflationCurve`]'s own `max_date`
    /// does; should they fail, the reference date stands in, with the null
    /// date as last resort, on that curve's fallback pattern.
    fn max_date(&self) -> Date {
        let _ = self.calculate();
        match self.yoy.borrow().as_ref() {
            Some(yoy) => yoy.max_date(),
            None => self
                .base
                .term_structure_base()
                .reference_date()
                .unwrap_or_else(|_| Date::null()),
        }
    }
}

impl YoYCapFloorTermPriceSurface for InterpolatedYoYCapFloorTermPriceSurface {
    fn surface_base(&self) -> &YoYCapFloorTermPriceSurfaceBase {
        &self.base
    }

    /// `hpp:178-180`, running the calculations first as the C++ constructor
    /// already had.
    fn atm_yoy_swap_time_rates(&self) -> QlResult<(Vec<Time>, Vec<Rate>)> {
        self.calculate()?;
        let guard = self.intersection.borrow();
        Ok(guard
            .as_ref()
            .expect("calculate filled the intersection")
            .atm_yoy_swap_time_rates
            .clone())
    }

    /// `hpp:181-183`.
    fn atm_yoy_swap_date_rates(&self) -> QlResult<(Vec<Date>, Vec<Rate>)> {
        self.calculate()?;
        let guard = self.intersection.borrow();
        Ok(guard
            .as_ref()
            .expect("calculate filled the intersection")
            .atm_yoy_swap_date_rates
            .clone())
    }

    /// `hpp:311-316`: the cap price above the ATM swap level (read with
    /// extrapolation, C++'s default there), the floor price at or below it.
    fn price(&self, d: Date, k: Rate) -> QlResult<Real> {
        let atm = self.atm_yoy_swap_rate(d, true)?;
        if k > atm {
            self.cap_price(d, k)
        } else {
            self.floor_price(d, k)
        }
    }

    /// `hpp:319-324`: a pure surface lookup at `timeFromReference(d)`; the
    /// spline extrapolates, as C++ enables on it.
    fn cap_price(&self, d: Date, k: Rate) -> QlResult<Real> {
        self.calculate()?;
        let t = self.time_from_reference(d)?;
        let guard = self.intersection.borrow();
        guard
            .as_ref()
            .expect("calculate filled the intersection")
            .cap_price
            .value(t, k)
    }

    /// `hpp:327-332`.
    fn floor_price(&self, d: Date, k: Rate) -> QlResult<Real> {
        self.calculate()?;
        let t = self.time_from_reference(d)?;
        let guard = self.intersection.borrow();
        guard
            .as_ref()
            .expect("calculate filled the intersection")
            .floor_price
            .value(t, k)
    }

    /// `hpp:188-190`.
    fn atm_yoy_swap_rate(&self, d: Date, extrapolate: bool) -> QlResult<Rate> {
        self.calculate()?;
        self.atm_yoy_swap_rate_impl(d, extrapolate)
    }

    /// `hpp:191-198`: work in terms of the maturity of the instruments, so
    /// the curve is asked at the date less the observation lag.
    fn atm_yoy_rate(&self, d: Date, obs_lag: Option<Period>, extrapolate: bool) -> QlResult<Rate> {
        self.calculate()?;
        let p = obs_lag.unwrap_or_else(|| self.base.observation_lag());
        let guard = self.yoy.borrow();
        guard
            .as_ref()
            .expect("calculate bootstrapped the curve")
            .yoy_rate_date(d - p, extrapolate)
    }

    /// `hpp:184`.
    fn yoy_ts(&self) -> QlResult<Shared<dyn YoYInflationTermStructure>> {
        self.calculate()?;
        let guard = self.yoy.borrow();
        Ok(Shared::clone(
            guard.as_ref().expect("calculate bootstrapped the curve"),
        ))
    }

    /// `hpp:172`: the bootstrapped curve's own base date.
    fn base_date(&self) -> QlResult<Date> {
        self.calculate()?;
        let guard = self.yoy.borrow();
        Ok(guard
            .as_ref()
            .expect("calculate bootstrapped the curve")
            .base_date())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexes::inflation::EuHicp;
    use crate::shared::shared;
    use crate::time::calendars::target::Target;
    use crate::time::date::Month::November;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;

    fn eval_date() -> Date {
        Date::new(23, November, 2007)
    }

    fn years(n: i32) -> Period {
        Period::new(n, TimeUnit::Years)
    }

    fn matrix_from(rows: &[&[f64]]) -> Matrix {
        let mut m = Matrix::with_size(rows.len(), rows[0].len());
        for (i, row) in rows.iter().enumerate() {
            for (j, &value) in row.iter().enumerate() {
                m[(i, j)] = value;
            }
        }
        m
    }

    /// Floor strikes overlapping the cap strikes at 0.02, cap prices
    /// non-increasing and floor prices non-decreasing down the strike rows;
    /// the nominal handle stays empty, nothing here discounting on it.
    fn a_base_with(
        cf_maturities: Vec<Period>,
        f_price: Matrix,
    ) -> QlResult<YoYCapFloorTermPriceSurfaceBase> {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(eval_date());
        let index = shared(YoYInflationIndex::from_underlying(shared(EuHicp::new(
            Shared::clone(&settings),
        ))));
        YoYCapFloorTermPriceSurfaceBase::new(
            0,
            Period::new(3, TimeUnit::Months),
            index,
            CpiInterpolationType::Linear,
            Handle::empty(),
            Actual365Fixed::new(),
            Target::new(),
            BusinessDayConvention::ModifiedFollowing,
            vec![0.01, 0.02, 0.03],
            vec![-0.01, 0.00, 0.02],
            cf_maturities,
            matrix_from(&[&[3.0, 4.0], &[2.0, 3.0], &[1.0, 2.0]]),
            f_price,
            settings,
        )
    }

    fn rejection<T>(built: QlResult<T>) -> crate::errors::QlError {
        match built {
            Ok(_) => panic!("expected a construction error"),
            Err(err) => err,
        }
    }

    /// The union keeps every floor strike and appends only the cap strikes
    /// strictly above the top floor strike (`cpp:83-100`): the 0.01 and 0.02
    /// cap strikes repeat or overlap the floors and are dropped.
    #[test]
    fn the_strike_union_drops_repeats_and_overlaps_and_stays_increasing() {
        let base = a_base_with(
            vec![years(1), years(2)],
            matrix_from(&[&[1.0, 2.0], &[2.0, 3.0], &[3.0, 4.0]]),
        )
        .expect("a consistent surface");
        assert_eq!(base.strikes(), &[-0.01, 0.00, 0.02, 0.03]);
        assert_eq!(base.cap_strikes(), &[0.01, 0.02, 0.03]);
        assert_eq!(base.floor_strikes(), &[-0.01, 0.00, 0.02]);
    }

    #[test]
    fn construction_rejects_inconsistent_input() {
        let err = rejection(a_base_with(
            vec![years(2), years(1)],
            matrix_from(&[&[1.0, 2.0], &[2.0, 3.0], &[3.0, 4.0]]),
        ));
        assert!(err.message().contains("non-increasing maturities"));

        let err = rejection(a_base_with(
            vec![years(1), years(2)],
            matrix_from(&[&[1.0, 2.0], &[2.0, 3.0]]),
        ));
        assert!(
            err.message()
                .contains("floor strikes vs floor price rows not equal")
        );

        let err = rejection(a_base_with(
            vec![years(1), years(2)],
            matrix_from(&[&[1.0, 2.0], &[2.0, 0.0], &[3.0, 4.0]]),
        ));
        assert!(err.message().contains("non-positive floor price"));

        let err = rejection(a_base_with(
            vec![years(1), years(2)],
            matrix_from(&[&[3.0, 4.0], &[2.0, 3.0], &[1.0, 2.0]]),
        ));
        assert!(err.message().contains("non-increasing floor prices"));
    }

    /// The concrete surface over the same fixture, read through the
    /// abstract-base trait as #874's consumers will read it.
    fn a_surface_with(
        interpolation: CpiInterpolationType,
    ) -> InterpolatedYoYCapFloorTermPriceSurface {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(eval_date());
        let index = shared(YoYInflationIndex::from_underlying(shared(EuHicp::new(
            Shared::clone(&settings),
        ))));
        InterpolatedYoYCapFloorTermPriceSurface::new(
            0,
            Period::new(3, TimeUnit::Months),
            index,
            interpolation,
            Handle::empty(),
            Actual365Fixed::new(),
            Target::new(),
            BusinessDayConvention::ModifiedFollowing,
            vec![0.01, 0.02, 0.03],
            vec![-0.01, 0.00, 0.02],
            vec![years(1), years(2)],
            matrix_from(&[&[3.0, 4.0], &[2.0, 3.0], &[1.0, 2.0]]),
            matrix_from(&[&[1.0, 2.0], &[2.0, 3.0], &[3.0, 4.0]]),
            settings,
        )
        .expect("a consistent surface")
    }

    /// The inspectors report the constructor arguments, and the maturity and
    /// tenor dates move off the evaluation date the settings carry (D5).
    #[test]
    fn inspectors_report_the_constructor_arguments() {
        let surface = a_surface_with(CpiInterpolationType::Linear);

        assert_eq!(surface.reference_date().unwrap(), eval_date());
        assert_eq!(surface.observation_lag(), Period::new(3, TimeUnit::Months));
        assert_eq!(surface.frequency(), Frequency::Monthly);
        assert_eq!(surface.fixing_days(), 0);
        assert_eq!(
            surface.business_day_convention(),
            BusinessDayConvention::ModifiedFollowing
        );
        assert_eq!(surface.strikes(), &[-0.01, 0.00, 0.02, 0.03]);
        assert_eq!(surface.min_strike(), -0.01);
        assert_eq!(surface.max_strike(), 0.03);
        assert_eq!(surface.maturities(), &[years(1), years(2)]);
        assert_eq!(surface.min_maturity().unwrap(), eval_date() + years(1));
        assert_eq!(surface.max_maturity().unwrap(), eval_date() + years(2));
        assert_eq!(
            surface.yoy_option_date_from_tenor(years(7)).unwrap(),
            eval_date() + years(7)
        );
        assert!(surface.check_strike(0.0));
        assert!(!surface.check_strike(0.05));
        assert!(surface.check_maturity(eval_date() + years(1)).unwrap());
        assert!(!surface.check_maturity(eval_date() + years(3)).unwrap());
        assert!(surface.index_is_interpolated());
    }

    /// The `AsIndex` arm being unported, the flag is the interpolation choice
    /// alone (`inflationindex.cpp:428-431`).
    #[test]
    fn a_flat_interpolation_choice_reads_back_as_not_interpolated() {
        let surface = a_surface_with(CpiInterpolationType::Flat);
        assert!(!surface.index_is_interpolated());
    }
}

#[cfg(test)]
mod yoy_price_surface_to_atm_oracle {
    //! `test-suite/inflationvolatility.cpp`'s `testYoYPriceSurfaceToATM`
    //! (`:354-390`), the numeric oracle of this port: the EU fixture of
    //! `setup()` (`:91-240`) and `setupPriceSurface()` (`:243-268`) - a
    //! 25-node cubic EUR nominal curve, the EU HICP year-on-year index, and
    //! 6x7 cap and floor price matrices - built into the surface, whose ATM
    //! year-on-year swap and curve rates are then pinned against the C++
    //! arrays `crv[]`/`swaps[]`/`ayoy[]` to `2e-5`.
    //!
    //! `setup()`'s GBP curve and its `InterpolatedYoYInflationCurve<Linear>`
    //! over `yoyEUrates` (`:112-123`, `:164-190`) are consumed only by
    //! `testYoYPriceSurfaceToVol` (the #874 stripper oracle), never by the ATM
    //! test: the surface prices through index copies linked to its own
    //! bootstrap, so neither is built here. The index itself is the *ratio*
    //! `YoYInflationIndex(EUHICP, ...)` of `:98`, not the quoted `YYEUHICP`.

    use super::*;
    use crate::indexes::inflation::EuHicp;
    use crate::math::interpolations::cubic::Cubic;
    use crate::shared::shared;
    use crate::termstructures::yields::InterpolatedZeroCurve;
    use crate::time::calendars::target::Target;
    use crate::time::date::Month::November;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;
    use crate::types::Real;

    /// EUR nominal zero times, in years of 365 days (`:101-105`).
    const TIMES_EUR: [Real; 25] = [
        0.0109589, 0.0684932, 0.263014, 0.317808, 0.567123, 0.816438, 1.06575, 1.31507, 1.56438,
        2.0137, 3.01918, 4.01644, 5.01644, 6.01644, 7.01644, 8.01644, 9.02192, 10.0192, 12.0192,
        15.0247, 20.0301, 25.0356, 30.0329, 40.0384, 50.0466,
    ];

    /// EUR nominal zero rates (`:106-110`).
    const RATES_EUR: [Real; 25] = [
        0.0415600, 0.0426840, 0.0470980, 0.0458506, 0.0449550, 0.0439784, 0.0431887, 0.0426604,
        0.0422925, 0.0424591, 0.0421477, 0.0421853, 0.0424016, 0.0426969, 0.0430804, 0.0435011,
        0.0439368, 0.0443825, 0.0452589, 0.0463389, 0.0472636, 0.0473401, 0.0470629, 0.0461092,
        0.0450794,
    ];

    /// EU cap strikes (`:196`).
    const C_STRIKES_EU: [Real; 6] = [0.02, 0.025, 0.03, 0.035, 0.04, 0.05];

    /// EU floor strikes (`:207`).
    const F_STRIKES_EU: [Real; 6] = [-0.01, 0.00, 0.005, 0.01, 0.015, 0.02];

    /// EU cap prices by strike (rows) and maturity (columns) (`:199-205`).
    const C_PRICES_EU: [[Real; 7]; 6] = [
        [116.225, 204.945, 296.285, 434.29, 654.47, 844.775, 1132.33],
        [34.305, 71.575, 114.1, 184.33, 307.595, 421.395, 602.35],
        [6.37, 19.085, 35.635, 66.42, 127.69, 189.685, 296.195],
        [1.325, 5.745, 12.585, 26.945, 58.95, 94.08, 158.985],
        [0.501, 2.37, 5.38, 13.065, 31.91, 53.95, 96.97],
        [0.501, 0.695, 1.47, 4.415, 12.86, 23.75, 46.7],
    ];

    /// EU floor prices by strike (rows) and maturity (columns) (`:208-214`).
    const F_PRICES_EU: [[Real; 7]; 6] = [
        [0.501, 0.851, 2.44, 6.645, 16.23, 26.85, 46.365],
        [0.501, 2.236, 5.555, 13.075, 28.46, 44.525, 73.08],
        [1.025, 3.935, 9.095, 19.64, 39.93, 60.375, 96.02],
        [2.465, 7.885, 16.155, 31.6, 59.34, 86.21, 132.045],
        [6.9, 17.92, 32.085, 56.08, 95.95, 132.85, 194.18],
        [23.52, 47.625, 74.085, 114.355, 175.72, 229.565, 316.285],
    ];

    /// The evaluation date `setup()` fixes (`:94-95`).
    fn eval_date() -> Date {
        Date::new(23, November, 2007)
    }

    /// EU cap/floor maturities (`:197-198`).
    fn cf_maturities_eu() -> Vec<Period> {
        [3, 5, 7, 10, 15, 20, 30]
            .into_iter()
            .map(|n| Period::new(n, TimeUnit::Years))
            .collect()
    }

    fn matrix_of(rows: &[[Real; 7]; 6]) -> Matrix {
        let mut m = Matrix::with_size(6, 7);
        for (i, row) in rows.iter().enumerate() {
            for (j, &value) in row.iter().enumerate() {
                m[(i, j)] = value;
            }
        }
        m
    }

    /// The EUR nominal curve (`:129-140`): each time is split into whole years
    /// plus truncated 365ths exactly as the C++ loop casts do, and the nodes
    /// feed a cubic-interpolated zero curve.
    fn eur_nominal_curve() -> Handle<dyn YieldTermStructure> {
        let eval = eval_date();
        let dates: Vec<Date> = TIMES_EUR
            .iter()
            .map(|&t| {
                let ys = t.floor() as i32;
                let ds = ((t - Real::from(ys)) * 365.0) as i32;
                eval + Period::new(ys, TimeUnit::Years) + Period::new(ds, TimeUnit::Days)
            })
            .collect();
        let curve = InterpolatedZeroCurve::<Cubic>::new(
            dates,
            RATES_EUR.to_vec(),
            Actual365Fixed::new(),
            Cubic,
        )
        .expect("25 well-ordered nodes");
        Handle::new(shared(curve) as Shared<dyn YieldTermStructure>)
    }

    struct Fixture {
        settings: Shared<Settings<Date>>,
        surface: InterpolatedYoYCapFloorTermPriceSurface,
    }

    /// `setupPriceSurface()` (`:243-268`): fixing days 0, a 3-month lag (the
    /// interpolated EU index needs 3), Actual/365F, TARGET, modified
    /// following, `CPI::Linear`, over the EU strike and price data.
    fn a_price_surface() -> Fixture {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(eval_date());
        let index = shared(YoYInflationIndex::from_underlying(shared(EuHicp::new(
            Shared::clone(&settings),
        ))));
        let surface = InterpolatedYoYCapFloorTermPriceSurface::new(
            0,
            Period::new(3, TimeUnit::Months),
            index,
            CpiInterpolationType::Linear,
            eur_nominal_curve(),
            Actual365Fixed::new(),
            Target::new(),
            BusinessDayConvention::ModifiedFollowing,
            C_STRIKES_EU.to_vec(),
            F_STRIKES_EU.to_vec(),
            cf_maturities_eu(),
            matrix_of(&C_PRICES_EU),
            matrix_of(&F_PRICES_EU),
            Shared::clone(&settings),
        )
        .expect("the EU fixture is consistent");
        Fixture { settings, surface }
    }

    /// The cached ATM year-on-year swap curve rates (`crv[]`, `:366-367`) and
    /// the swap rates read back through the intersection curve at the same
    /// dates (`swaps[]`, `:368-369`), both to `eps = 2e-5` (`:372`).
    const CRV: [Real; 7] = [
        0.024586, 0.0247575, 0.0249396, 0.0252596, 0.0258498, 0.0262883, 0.0267915,
    ];
    const SWAPS: [Real; 7] = [
        0.024586, 0.0247575, 0.0249396, 0.0252596, 0.0258498, 0.0262883, 0.0267915,
    ];

    /// The cached ATM year-on-year curve rates (`ayoy[]`, `:370-371`), read
    /// back through the bootstrapped year-on-year term structure.
    const AYOY: [Real; 7] = [
        0.0247659, 0.0251437, 0.0255945, 0.0265015, 0.0280457, 0.0285534, 0.0295884,
    ];
    const EPS: Real = 2e-5;

    /// Construction smoke: the fixture builds, its inspectors read the EU data
    /// back, and the moving reference date sits on the evaluation date.
    /// Construction runs no calculation (the C++ constructor's eager
    /// `performCalculations()` is deferred to first use per the module
    /// divergences).
    #[test]
    fn the_eu_fixture_builds_and_reads_back() {
        let fixture = a_price_surface();
        let surface = &fixture.surface;

        assert!(surface.intersection.borrow().is_none());
        assert_eq!(surface.reference_date().unwrap(), eval_date());
        assert_eq!(surface.maturities(), cf_maturities_eu().as_slice());
        assert_eq!(
            surface.strikes(),
            &[
                -0.01, 0.00, 0.005, 0.01, 0.015, 0.02, 0.025, 0.03, 0.035, 0.04, 0.05
            ]
        );
        assert_eq!(surface.cap_strikes(), C_STRIKES_EU.as_slice());
        assert_eq!(surface.floor_strikes(), F_STRIKES_EU.as_slice());
        assert_eq!(
            surface.min_maturity().unwrap(),
            eval_date() + Period::new(3, TimeUnit::Years)
        );
        assert_eq!(
            surface.max_maturity().unwrap(),
            eval_date() + Period::new(30, TimeUnit::Years)
        );
        assert_eq!(surface.observation_lag(), Period::new(3, TimeUnit::Months));
        assert_eq!(surface.frequency(), Frequency::Monthly);
        assert!(surface.index_is_interpolated());

        let _ = fixture.settings;
    }

    /// The C++ loop truncates the day fraction (`:131-132`): the first node
    /// time, 0.0109589, is a hair under 4/365, so the first curve date lands 3
    /// days after the evaluation date, not 4. A rounding rewrite would move
    /// every nominal discount read off the C++ fixture.
    #[test]
    fn the_nominal_dates_truncate_the_day_fraction_as_the_cpp_casts_do() {
        let nominal = eur_nominal_curve();
        let reference = nominal.current_link().unwrap().reference_date().unwrap();
        assert_eq!(reference, eval_date() + 3);
    }

    /// The first oracle loop (`:373-377`): the intersection's cached (time,
    /// rate) swap curve reproduces `crv[]`. Every one of the seven maturities
    /// intersects validly, so the heuristic fallback never fires here.
    #[test]
    fn the_atm_yoy_swap_time_rates_reproduce_the_cached_curve() {
        let fixture = a_price_surface();
        let (times, rates) = fixture.surface.atm_yoy_swap_time_rates().unwrap();

        assert_eq!(times.len(), 7);
        assert_eq!(rates.len(), 7);
        for (i, (rate, expected)) in rates.iter().zip(CRV).enumerate() {
            assert!(
                (rate - expected).abs() < EPS,
                "could not recover cached yoy swap curve at {i}: {rate} vs {expected}"
            );
        }
    }

    /// The second oracle loop (`:379-383`): the ATM swap rate read back
    /// through the cubic intersection curve at each cached date reproduces
    /// `swaps[]`.
    #[test]
    fn the_atm_yoy_swap_rates_reproduce_the_cached_swaps() {
        let fixture = a_price_surface();
        let (dates, _) = fixture.surface.atm_yoy_swap_date_rates().unwrap();

        assert_eq!(dates.len(), 7);
        for (i, (date, expected)) in dates.iter().zip(SWAPS).enumerate() {
            let rate = fixture.surface.atm_yoy_swap_rate(*date, true).unwrap();
            assert!(
                (rate - expected).abs() < EPS,
                "could not recover yoy swap curve at {i} ({date}): {rate} vs {expected}"
            );
        }
    }

    /// The intersection is computed lazily on the first read and cached: the
    /// cubic curve answers its own nodes back, and the internal read rejects
    /// an out-of-range time unless extrapolation is asked for (the
    /// `extrapolate` argument of `hpp:188-190`, whose C++ default is `true`).
    #[test]
    fn the_intersection_is_cached_and_extrapolation_gated() {
        let fixture = a_price_surface();
        let surface = &fixture.surface;

        assert!(surface.intersection.borrow().is_none());
        let (dates, rates) = surface.atm_yoy_swap_date_rates().unwrap();
        assert!(surface.intersection.borrow().is_some());

        for (date, rate) in dates.iter().zip(&rates) {
            let read = surface.atm_yoy_swap_rate(*date, true).unwrap();
            assert!((read - rate).abs() < 1e-14);
        }
        let early = surface.atm_yoy_swap_rate(eval_date() + Period::new(1, TimeUnit::Years), false);
        assert!(early.is_err(), "below the first node needs extrapolation");
    }

    /// The third oracle loop (`:384-388`): the ATM year-on-year rate at each
    /// cached date - the bootstrapped curve read one observation lag back -
    /// reproduces `ayoy[]`. This is the loop that exercises the whole
    /// `calculateYoYTermStructure()` phase: thirty swap helpers, the Linear
    /// piecewise bootstrap, and its internal `1e-5` reprice gate.
    #[test]
    fn the_atm_yoy_rates_reproduce_the_cached_yoy_curve() {
        let fixture = a_price_surface();
        let (dates, _) = fixture.surface.atm_yoy_swap_date_rates().unwrap();

        assert_eq!(dates.len(), 7);
        for (i, (date, expected)) in dates.iter().zip(AYOY).enumerate() {
            let rate = fixture.surface.atm_yoy_rate(*date, None, true).unwrap();
            assert!(
                (rate - expected).abs() < EPS,
                "could not recover cached yoy curve at {i} ({date}): {rate} vs {expected}"
            );
        }
    }

    /// The bootstrap's frame (`hpp:527-556`): the base date is the start of
    /// the inflation period one 3-month lag before the nominal reference date
    /// (which itself sits 3 days after the evaluation date, the fixture's
    /// first node time truncating so), and the curve is exposed whole through
    /// `YoYTS`.
    #[test]
    fn the_bootstrap_exposes_its_base_date_and_term_structure() {
        let fixture = a_price_surface();

        assert_eq!(
            fixture.surface.base_date().unwrap(),
            Date::new(1, crate::time::date::Month::August, 2007)
        );
        let yoy = fixture.surface.yoy_ts().unwrap();
        assert_eq!(yoy.reference_date().unwrap(), eval_date() + 3);
        assert_eq!(fixture.surface.max_date(), yoy.max_date());
    }
}
