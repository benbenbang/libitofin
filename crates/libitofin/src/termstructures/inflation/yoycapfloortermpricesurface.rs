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
//! ATM year-on-year swap rates and bootstrap a year-on-year curve from them;
//! the calculations land with the follow-up commits of #907.
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

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::indexes::inflationindex::{CpiInterpolationType, InflationIndex, YoYInflationIndex};
use crate::math::matrix::Matrix;
use crate::patterns::observable::{AsObservable, Observable};
use crate::require;
use crate::settings::Settings;
use crate::shared::Shared;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;
use crate::types::{Natural, Rate};

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
}

/// The concrete surface, C++'s
/// `InterpolatedYoYCapFloorTermPriceSurface<Bicubic, Cubic>` (`hpp:148-232`):
/// bicubic-spline price surfaces over the quoted grid and a cubic ATM
/// year-on-year swap rate curve through their intersections.
///
/// The calculations - `intersect()` and `calculateYoYTermStructure()`
/// (`hpp:288-298`) - land with the follow-up commits of #907; construction
/// only validates and stores the quotes.
pub struct InterpolatedYoYCapFloorTermPriceSurface {
    base: YoYCapFloorTermPriceSurfaceBase,
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
        })
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

    /// C++ answers the bootstrapped curve's maximum date (`hpp:171`); until
    /// that curve lands (#907 follow-up commits) the reference date stands in,
    /// with the null date as last resort, on the
    /// [`PiecewiseYoYInflationCurve`] fallback pattern.
    ///
    /// [`PiecewiseYoYInflationCurve`]: super::piecewiseyoyinflationcurve::PiecewiseYoYInflationCurve
    fn max_date(&self) -> Date {
        self.base
            .term_structure_base()
            .reference_date()
            .unwrap_or_else(|_| Date::null())
    }
}

impl YoYCapFloorTermPriceSurface for InterpolatedYoYCapFloorTermPriceSurface {
    fn surface_base(&self) -> &YoYCapFloorTermPriceSurfaceBase {
        &self.base
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

    /// Construction smoke: the fixture builds, its inspectors read the EU data
    /// back, and the moving reference date sits on the evaluation date.
    #[test]
    fn the_eu_fixture_builds_and_reads_back() {
        let fixture = a_price_surface();
        let surface = &fixture.surface;

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
}
