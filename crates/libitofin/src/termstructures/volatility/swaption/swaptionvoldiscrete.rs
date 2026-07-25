//! Discrete-grid base for interpolated swaption volatility surfaces.
//!
//! Port of `ql/termstructures/volatility/swaption/swaptionvoldiscrete.{hpp,cpp}`:
//! `class SwaptionVolatilityDiscrete : public LazyObject, public
//! SwaptionVolatilityStructure`. This is the intermediate base that
//! `SwaptionVolatilityMatrix` and the swaption cube extend; it holds the shared
//! discrete tenor/date/time grid machinery, not a volatility of its own.
//!
//! ## Composition, not a trait
//!
//! C++ multiply-inherits `LazyObject` and `SwaptionVolatilityStructure`. Rust
//! has neither, so this is a reusable **struct** an interpolated surface embeds,
//! following the [`TermStructureBase`] precedent: it carries the grid state plus
//! a [`LazyObject`], and the surface delegates its [`TermStructure`],
//! [`VolatilityTermStructure`](crate::termstructures::volatility::VolatilityTermStructure)
//! and [`SwaptionVolatilityStructure`](super::SwaptionVolatilityStructure) impls
//! through it, routing each query through [`calculate`](SwaptionVolatilityDiscrete::calculate).
//!
//! The grid is `option_tenors -> option_dates -> option_times` (day-counter year
//! fractions from the reference date) and `swap_tenors -> swap_lengths` (tenor to
//! years). It is built eagerly in the constructor and, for a moving structure,
//! rebuilt lazily whenever the evaluation date moves the reference date, exactly
//! as C++'s `performCalculations` gates on `cachedReferenceDate_`.
//!
//! ## Divergences from QuantLib
//!
//! - The moving constructor resolves the reference date at construction (the C++
//!   constructor calls `initializeOptionDatesAndTimes` -> `referenceDate()`), so
//!   per D5 it takes the shared [`Settings`] handle and returns `Err` when no
//!   evaluation date is set, rather than falling back to a system-clock today.
//!   This differs from [`ConstantSwaptionVolatility`](super::ConstantSwaptionVolatility)'s
//!   moving constructor, which defers reference resolution.
//! - The option-dates constructor stores no option tenors, so
//!   [`option_tenors`](SwaptionVolatilityDiscrete::option_tenors) returns an empty
//!   slice for it (C++ returns a vector of default-constructed periods).

use std::cell::{Cell, RefCell};

use crate::errors::QlResult;
use crate::math::interpolations::Interpolation;
use crate::math::interpolations::linear::LinearInterpolation;
use crate::patterns::lazyobject::LazyObject;
use crate::patterns::observable::{Observable, Observer};
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut, shared_mut};
use crate::termstructures::TermStructureBase;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::{Date, SerialNumber};
use crate::time::daycounter::DayCounter;
use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;
use crate::types::{Natural, Real, Time};
use crate::{fail, require};

/// How the option axis was specified: by tenor (recomputed off the reference
/// date) or by fixed date.
enum OptionSpec {
    Tenors(Vec<Period>),
    Dates(Vec<Date>),
}

/// The recomputed grid state (C++'s `mutable` date/time/length vectors and the
/// option-date interpolator).
struct DiscreteGrid {
    option_dates: Vec<Date>,
    option_times: Vec<Time>,
    swap_lengths: Vec<Time>,
    interpolator: LinearInterpolation,
}

/// Invalidates the embedded lazy object when the reference date moves, so the
/// next [`calculate`](SwaptionVolatilityDiscrete::calculate) rebuilds the grid.
struct DiscreteUpdater {
    lazy: SharedMut<LazyObject>,
}

impl Observer for DiscreteUpdater {
    fn update(&mut self) {
        self.lazy.borrow_mut().invalidate_silently();
    }
}

/// Discrete tenor/date/time grid shared by interpolated swaption vol surfaces.
pub struct SwaptionVolatilityDiscrete {
    base: TermStructureBase,
    business_day_convention: BusinessDayConvention,
    option_spec: OptionSpec,
    swap_tenors: Vec<Period>,
    grid: RefCell<DiscreteGrid>,
    lazy: SharedMut<LazyObject>,
    cached_reference_date: Cell<Date>,
    _updater: SharedMut<DiscreteUpdater>,
}

impl SwaptionVolatilityDiscrete {
    /// Moving reference date (advanced `settlement_days` off the evaluation
    /// date), option axis given by tenor. C++'s `settlementDays` constructor.
    #[allow(clippy::too_many_arguments)]
    pub fn moving(
        option_tenors: Vec<Period>,
        swap_tenors: Vec<Period>,
        settlement_days: Natural,
        calendar: Calendar,
        business_day_convention: BusinessDayConvention,
        day_counter: DayCounter,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<SwaptionVolatilityDiscrete> {
        check_option_tenors(&option_tenors)?;
        check_swap_tenors(&swap_tenors)?;
        let base =
            TermStructureBase::moving(settlement_days, calendar, Some(day_counter), settings);
        Self::assemble(
            base,
            business_day_convention,
            OptionSpec::Tenors(option_tenors),
            swap_tenors,
        )
    }

    /// Fixed reference date, option axis given by tenor. C++'s tenor +
    /// `referenceDate` convenience constructor.
    pub fn new(
        option_tenors: Vec<Period>,
        swap_tenors: Vec<Period>,
        reference_date: Date,
        calendar: Calendar,
        business_day_convention: BusinessDayConvention,
        day_counter: DayCounter,
    ) -> QlResult<SwaptionVolatilityDiscrete> {
        check_option_tenors(&option_tenors)?;
        check_swap_tenors(&swap_tenors)?;
        let base = TermStructureBase::with_reference_date(
            reference_date,
            Some(calendar),
            Some(day_counter),
        );
        Self::assemble(
            base,
            business_day_convention,
            OptionSpec::Tenors(option_tenors),
            swap_tenors,
        )
    }

    /// Fixed reference date, option axis given by explicit dates. C++'s
    /// `optionDates` + `referenceDate` constructor.
    pub fn with_option_dates(
        option_dates: Vec<Date>,
        swap_tenors: Vec<Period>,
        reference_date: Date,
        calendar: Calendar,
        business_day_convention: BusinessDayConvention,
        day_counter: DayCounter,
    ) -> QlResult<SwaptionVolatilityDiscrete> {
        check_option_dates(&option_dates, reference_date)?;
        check_swap_tenors(&swap_tenors)?;
        let base = TermStructureBase::with_reference_date(
            reference_date,
            Some(calendar),
            Some(day_counter),
        );
        Self::assemble(
            base,
            business_day_convention,
            OptionSpec::Dates(option_dates),
            swap_tenors,
        )
    }

    fn assemble(
        base: TermStructureBase,
        business_day_convention: BusinessDayConvention,
        option_spec: OptionSpec,
        swap_tenors: Vec<Period>,
    ) -> QlResult<SwaptionVolatilityDiscrete> {
        let reference = base.reference_date()?;
        let grid = build_grid(
            &base,
            business_day_convention,
            &option_spec,
            &swap_tenors,
            reference,
        )?;
        let lazy = shared_mut(LazyObject::new(true));
        let updater = shared_mut(DiscreteUpdater {
            lazy: SharedMut::clone(&lazy),
        });
        base.observable()
            .register_observer(&(SharedMut::clone(&updater) as SharedMut<dyn Observer>));
        Ok(SwaptionVolatilityDiscrete {
            base,
            business_day_convention,
            option_spec,
            swap_tenors,
            grid: RefCell::new(grid),
            lazy,
            cached_reference_date: Cell::new(reference),
            _updater: updater,
        })
    }

    /// The embedded term-structure holder, for an embedding surface to route its
    /// [`TermStructure`] impl through.
    pub fn base(&self) -> &TermStructureBase {
        &self.base
    }

    /// The observable notifying downstream observers of the surface.
    pub fn observable(&self) -> &Observable {
        self.base.observable()
    }

    /// The business-day convention used in tenor-to-date conversion.
    pub fn business_day_convention(&self) -> BusinessDayConvention {
        self.business_day_convention
    }

    /// Rebuilds the grid if the reference date has moved since it was last
    /// computed. An embedding surface calls this before every query, as C++'s
    /// derived surfaces call `LazyObject::calculate`.
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
        if self.cached_reference_date.get() != reference {
            self.cached_reference_date.set(reference);
            let grid = build_grid(
                &self.base,
                self.business_day_convention,
                &self.option_spec,
                &self.swap_tenors,
                reference,
            )?;
            *self.grid.borrow_mut() = grid;
        }
        Ok(())
    }

    /// The option tenors, or an empty slice for a date-specified structure.
    pub fn option_tenors(&self) -> &[Period] {
        match &self.option_spec {
            OptionSpec::Tenors(tenors) => tenors,
            OptionSpec::Dates(_) => &[],
        }
    }

    /// The swap tenors.
    pub fn swap_tenors(&self) -> &[Period] {
        &self.swap_tenors
    }

    /// The option (exercise) dates.
    pub fn option_dates(&self) -> QlResult<Vec<Date>> {
        self.calculate()?;
        Ok(self.grid.borrow().option_dates.clone())
    }

    /// The option times (year fractions from the reference date).
    pub fn option_times(&self) -> QlResult<Vec<Time>> {
        self.calculate()?;
        Ok(self.grid.borrow().option_times.clone())
    }

    /// The swap lengths in years.
    pub fn swap_lengths(&self) -> QlResult<Vec<Time>> {
        self.calculate()?;
        Ok(self.grid.borrow().swap_lengths.clone())
    }

    /// The option date recovered from an option time, via the reference-anchored
    /// linear interpolation over serial numbers. The interpolated serial is
    /// truncated toward zero, matching C++'s `static_cast<serial_type>`.
    pub fn option_date_from_time(&self, option_time: Time) -> QlResult<Date> {
        self.calculate()?;
        let serial = self.grid.borrow().interpolator.value(option_time)?;
        Ok(Date::from_serial(serial as SerialNumber))
    }
}

fn build_grid(
    base: &TermStructureBase,
    business_day_convention: BusinessDayConvention,
    option_spec: &OptionSpec,
    swap_tenors: &[Period],
    reference: Date,
) -> QlResult<DiscreteGrid> {
    let Some(day_counter) = base.day_counter() else {
        fail!("no day counter for swaption vol discrete");
    };
    let option_dates = match option_spec {
        OptionSpec::Tenors(tenors) => {
            let Some(calendar) = base.calendar() else {
                fail!("no calendar for swaption vol discrete");
            };
            tenors
                .iter()
                .map(|&tenor| {
                    calendar.advance_by_period(reference, tenor, business_day_convention, false)
                })
                .collect()
        }
        OptionSpec::Dates(dates) => dates.clone(),
    };

    let n = option_dates.len();
    let mut interpolator_times = Vec::with_capacity(n + 1);
    let mut interpolator_dates = Vec::with_capacity(n + 1);
    interpolator_times.push(0.0);
    interpolator_dates.push(reference.serial_number() as Real);
    let mut option_times = Vec::with_capacity(n);
    for &date in &option_dates {
        let time = day_counter.year_fraction(reference, date);
        option_times.push(time);
        interpolator_times.push(time);
        interpolator_dates.push(date.serial_number() as Real);
    }

    let swap_lengths = swap_tenors
        .iter()
        .map(|&tenor| swap_length_from_tenor(tenor))
        .collect::<QlResult<Vec<Time>>>()?;

    let interpolator =
        LinearInterpolation::new(interpolator_times, interpolator_dates)?.with_extrapolation(true);

    Ok(DiscreteGrid {
        option_dates,
        option_times,
        swap_lengths,
        interpolator,
    })
}

/// Period-to-years conversion for swap lengths, mirroring
/// [`SwaptionVolatilityStructure::swap_length_tenor`](super::SwaptionVolatilityStructure::swap_length_tenor).
fn swap_length_from_tenor(swap_tenor: Period) -> QlResult<Time> {
    require!(
        swap_tenor.length() > 0,
        "non-positive swap tenor ({swap_tenor}) given"
    );
    match swap_tenor.units() {
        TimeUnit::Months => Ok(swap_tenor.length() as Time / 12.0),
        TimeUnit::Years => Ok(swap_tenor.length() as Time),
        other => fail!("invalid time unit ({other}) for swap length"),
    }
}

fn check_option_tenors(option_tenors: &[Period]) -> QlResult<()> {
    require!(
        !option_tenors.is_empty(),
        "at least one option tenor is required"
    );
    require!(
        option_tenors[0].length() > 0,
        "first option tenor is negative ({})",
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
    Ok(())
}

fn check_option_dates(option_dates: &[Date], reference: Date) -> QlResult<()> {
    require!(
        !option_dates.is_empty(),
        "at least one option date is required"
    );
    require!(
        option_dates[0] > reference,
        "first option date ({}) must be greater than reference date ({reference})",
        option_dates[0]
    );
    for i in 1..option_dates.len() {
        require!(
            option_dates[i] > option_dates[i - 1],
            "non increasing option dates: {} is {}, {} is {}",
            i,
            option_dates[i - 1],
            i + 1,
            option_dates[i]
        );
    }
    Ok(())
}

fn check_swap_tenors(swap_tenors: &[Period]) -> QlResult<()> {
    require!(
        !swap_tenors.is_empty(),
        "at least one swap tenor is required"
    );
    require!(
        swap_tenors[0].length() > 0,
        "first swap tenor is negative ({})",
        swap_tenors[0]
    );
    for i in 1..swap_tenors.len() {
        let increasing = swap_tenors[i] > swap_tenors[i - 1];
        require!(
            increasing,
            "non increasing swap tenor: {} is {}, {} is {}",
            i,
            swap_tenors[i - 1],
            i + 1,
            swap_tenors[i]
        );
    }
    Ok(())
}

/// These tests are oracle-supporting: they pin the grid construction, the
/// reference-date round-trip and the moving-reference invalidation. The
/// discriminating numeric oracle is #568's swaption-matrix atm-vol recovery,
/// which prices through this grid.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::shared;
    use crate::time::calendars::target::Target;
    use crate::time::date::Month;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;

    const BDC: BusinessDayConvention = BusinessDayConvention::ModifiedFollowing;

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

    fn fixed() -> (Date, SwaptionVolatilityDiscrete) {
        let reference = Date::new(15, Month::June, 2026);
        let discrete = SwaptionVolatilityDiscrete::new(
            option_tenors(),
            swap_tenors(),
            reference,
            Target::new(),
            BDC,
            Actual365Fixed::new(),
        )
        .unwrap();
        (reference, discrete)
    }

    #[test]
    fn option_times_match_independently_advanced_dates() {
        let (reference, discrete) = fixed();
        let calendar = Target::new();
        let day_counter = Actual365Fixed::new();
        let dates = discrete.option_dates().unwrap();
        let times = discrete.option_times().unwrap();
        for (i, tenor) in option_tenors().into_iter().enumerate() {
            let expected_date = calendar.advance_by_period(reference, tenor, BDC, false);
            let expected_time = day_counter.year_fraction(reference, expected_date);
            assert_eq!(dates[i], expected_date);
            assert_eq!(times[i], expected_time);
        }
    }

    #[test]
    fn swap_lengths_equal_swap_tenor_year_fractions() {
        let (_, discrete) = fixed();
        let lengths = discrete.swap_lengths().unwrap();
        assert_eq!(lengths, vec![1.0, 5.0, 10.0, 30.0]);
    }

    #[test]
    fn option_date_from_time_round_trips_to_the_option_date() {
        let (_, discrete) = fixed();
        let dates = discrete.option_dates().unwrap();
        let times = discrete.option_times().unwrap();
        for (i, &time) in times.iter().enumerate() {
            assert_eq!(discrete.option_date_from_time(time).unwrap(), dates[i]);
        }
    }

    #[test]
    fn moving_grid_follows_the_evaluation_date() {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(Date::new(15, Month::January, 2026));
        let discrete = SwaptionVolatilityDiscrete::moving(
            option_tenors(),
            swap_tenors(),
            2,
            Target::new(),
            BDC,
            Actual365Fixed::new(),
            settings.clone(),
        )
        .unwrap();
        let before = discrete.option_dates().unwrap();

        settings.set_evaluation_date(Date::new(15, Month::February, 2026));
        let after = discrete.option_dates().unwrap();

        assert_ne!(before, after);
        let reference = discrete.base().reference_date().unwrap();
        let expected_first =
            Target::new().advance_by_period(reference, option_tenors()[0], BDC, false);
        assert_eq!(after[0], expected_first);
    }

    #[test]
    fn option_dates_form_reference_moves_grid_to_reference_date() {
        let reference = Date::new(15, Month::June, 2026);
        let option_dates = vec![reference + 30, reference + 180, reference + 365];
        let discrete = SwaptionVolatilityDiscrete::with_option_dates(
            option_dates.clone(),
            swap_tenors(),
            reference,
            Target::new(),
            BDC,
            Actual365Fixed::new(),
        )
        .unwrap();
        assert_eq!(discrete.option_dates().unwrap(), option_dates);
        let day_counter = Actual365Fixed::new();
        let times = discrete.option_times().unwrap();
        for (i, &date) in option_dates.iter().enumerate() {
            assert_eq!(times[i], day_counter.year_fraction(reference, date));
        }
        assert!(discrete.option_tenors().is_empty());
    }

    #[test]
    fn non_increasing_option_tenors_are_rejected() {
        let reference = Date::new(15, Month::June, 2026);
        let bad = vec![
            Period::new(1, TimeUnit::Years),
            Period::new(6, TimeUnit::Months),
        ];
        assert!(
            SwaptionVolatilityDiscrete::new(
                bad,
                swap_tenors(),
                reference,
                Target::new(),
                BDC,
                Actual365Fixed::new(),
            )
            .is_err()
        );
    }

    #[test]
    fn non_increasing_option_dates_are_rejected() {
        let reference = Date::new(15, Month::June, 2026);
        let bad = vec![reference + 180, reference + 30];
        assert!(
            SwaptionVolatilityDiscrete::with_option_dates(
                bad,
                swap_tenors(),
                reference,
                Target::new(),
                BDC,
                Actual365Fixed::new(),
            )
            .is_err()
        );
    }

    #[test]
    fn first_option_date_before_reference_is_rejected() {
        let reference = Date::new(15, Month::June, 2026);
        let bad = vec![reference - 5, reference + 180];
        assert!(
            SwaptionVolatilityDiscrete::with_option_dates(
                bad,
                swap_tenors(),
                reference,
                Target::new(),
                BDC,
                Actual365Fixed::new(),
            )
            .is_err()
        );
    }

    #[test]
    fn non_increasing_swap_tenors_are_rejected() {
        let reference = Date::new(15, Month::June, 2026);
        let bad = vec![
            Period::new(10, TimeUnit::Years),
            Period::new(5, TimeUnit::Years),
        ];
        assert!(
            SwaptionVolatilityDiscrete::new(
                option_tenors(),
                bad,
                reference,
                Target::new(),
                BDC,
                Actual365Fixed::new(),
            )
            .is_err()
        );
    }
}
