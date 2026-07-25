//! Optionlet stripper base (`OptionletStripper`).
//!
//! Port of `ql/termstructures/volatility/optionlet/optionletstripper.{hpp,cpp}`:
//! the [`StrippedOptionletBase`] specialization that holds a
//! [`CapFloorTermVolSurface`] and an [`IborIndex`] and derives, from the surface
//! option tenors and the index tenor, the optionlet tenors and the cap/floor
//! lengths the stripping algorithm prices (`optionletstripper.cpp:58-84`, C++
//! `commonSetup`). It allocates the per-maturity result arrays; the concrete
//! stripper (#575) fills them in its `strip`/`calculate` and routes the lazy
//! [`StrippedOptionletBase`] accessors through them.
//!
//! ## commonSetup
//!
//! The index tenor is the optionlet frequency when set, else the index's own
//! tenor. The first optionlet tenor is that index tenor and the first cap/floor
//! length is twice it; each further step adds one index tenor to the cap/floor
//! length (the previous cap/floor length becomes the next optionlet tenor) while
//! the length stays within the surface's longest option tenor. For a 6M index
//! over a surface reaching 4Y this yields optionlet tenors `6M,12M,...,42M` and
//! cap/floor lengths `12M,18M,...,48M`.
//!
//! ## Divergences from QuantLib
//!
//! - This struct is the immutable setup plus the allocated result caches (C++
//!   `mutable` members). The `LazyObject` laziness, the `strip` hook
//!   (`performCalculations`) and the lazy accessor bodies live on the concrete
//!   stripper (#575); the caches are exposed to it through
//!   [`caches`](Self::caches).
//! - The C++ overnight-index guard (`optionletstripper.cpp:49-51`, which requires
//!   an explicit frequency for an `OvernightIndex`) is deferred: a
//!   [`Shared<IborIndex>`] carries no runtime overnight tag to dispatch on, so
//!   the distinction is unavailable here (#577).
//! - Observer registration (surface / index / discount / evaluation date) is
//!   deferred to the concrete stripper, which owns the notification graph.

use std::cell::RefCell;
use std::cmp::Ordering;

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::indexes::IborIndex;
use crate::indexes::interestrateindex::InterestRateIndex;
use crate::shared::Shared;
use crate::termstructures::TermStructure;
use crate::termstructures::volatility::{
    CapFloorTermVolSurface, VolatilityTermStructure, VolatilityType,
};
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::period::Period;
use crate::types::{Natural, Rate, Real, Time, Volatility};

/// The per-maturity result arrays a concrete stripper fills (the C++ `mutable`
/// members of `OptionletStripper`), allocated by [`OptionletStripper`] to the
/// optionlet-tenor count.
///
/// Every field is empty of meaning until the stripping algorithm (#575) writes
/// it; only the shapes are fixed here.
#[derive(Default)]
pub struct OptionletStripperCaches {
    /// Optionlet strikes per maturity, seeded with the surface strikes.
    pub optionlet_strikes: Vec<Vec<Rate>>,
    /// Stripped optionlet volatilities per maturity and strike.
    pub optionlet_volatilities: Vec<Vec<Volatility>>,
    /// Optionlet fixing times, one per maturity.
    pub optionlet_times: Vec<Time>,
    /// Optionlet fixing dates, one per maturity.
    pub optionlet_dates: Vec<Date>,
    /// At-the-money optionlet forward rates, one per maturity.
    pub atm_optionlet_rate: Vec<Rate>,
    /// Optionlet payment dates, one per maturity.
    pub optionlet_payment_dates: Vec<Date>,
    /// Optionlet accrual periods, one per maturity.
    pub optionlet_accrual_periods: Vec<Time>,
}

/// Shared base for optionlet strippers (`OptionletStripper`).
pub struct OptionletStripper {
    term_vol_surface: Shared<CapFloorTermVolSurface>,
    ibor_index: Shared<IborIndex>,
    discount: Handle<dyn YieldTermStructure>,
    n_strikes: usize,
    optionlet_tenors: Vec<Period>,
    cap_floor_lengths: Vec<Period>,
    volatility_type: VolatilityType,
    displacement: Real,
    optionlet_frequency: Option<Period>,
    caches: RefCell<OptionletStripperCaches>,
}

impl OptionletStripper {
    /// Builds the base from a term-vol `surface` and an `ibor_index`, running
    /// `commonSetup` (`optionletstripper.cpp:29-84`).
    ///
    /// `discount` is the discount curve the derived stripper prices caps on
    /// (empty by default), `volatility_type` / `displacement` the model the
    /// stripped volatilities are expressed in, and `optionlet_frequency` an
    /// optional override of the index tenor as the optionlet step.
    ///
    /// # Errors
    ///
    /// Rejects a non-zero `displacement` under the [`Normal`](VolatilityType::Normal)
    /// model, an empty surface, a surface too short to hold the first cap/floor
    /// length, and an undecidable optionlet/surface tenor comparison.
    pub fn new(
        surface: Shared<CapFloorTermVolSurface>,
        ibor_index: Shared<IborIndex>,
        discount: Handle<dyn YieldTermStructure>,
        volatility_type: VolatilityType,
        displacement: Real,
        optionlet_frequency: Option<Period>,
    ) -> QlResult<OptionletStripper> {
        if volatility_type == VolatilityType::Normal && displacement != 0.0 {
            crate::fail!("non-null displacement is not allowed with Normal model");
        }

        let n_strikes = surface.strikes().len();
        let index_tenor = optionlet_frequency.unwrap_or_else(|| ibor_index.tenor());

        let Some(&max_cap_floor_tenor) = surface.option_tenors().last() else {
            crate::fail!("cap/floor term vol surface has no option tenors");
        };

        let mut optionlet_tenors = vec![index_tenor];
        let mut cap_floor_lengths = vec![index_tenor + index_tenor];
        match max_cap_floor_tenor.partial_cmp(&cap_floor_lengths[0]) {
            Some(Ordering::Less) => crate::fail!(
                "too short ({max_cap_floor_tenor}) capfloor term vol surface for optionlet tenor {index_tenor}"
            ),
            None => crate::fail!(
                "undecidable comparison between surface tenor {max_cap_floor_tenor} and capfloor length {}",
                cap_floor_lengths[0]
            ),
            _ => {}
        }

        let mut next = cap_floor_lengths[0] + index_tenor;
        loop {
            match next.partial_cmp(&max_cap_floor_tenor) {
                Some(Ordering::Greater) => break,
                None => crate::fail!(
                    "undecidable comparison between capfloor length {next} and surface tenor {max_cap_floor_tenor}"
                ),
                _ => {}
            }
            optionlet_tenors.push(*cap_floor_lengths.last().expect("non-empty"));
            cap_floor_lengths.push(next);
            next += index_tenor;
        }

        let n_optionlet_tenors = optionlet_tenors.len();
        let caches = OptionletStripperCaches {
            optionlet_strikes: vec![surface.strikes().to_vec(); n_optionlet_tenors],
            optionlet_volatilities: vec![vec![0.0; n_strikes]; n_optionlet_tenors],
            optionlet_times: vec![0.0; n_optionlet_tenors],
            optionlet_dates: vec![Date::null(); n_optionlet_tenors],
            atm_optionlet_rate: vec![0.0; n_optionlet_tenors],
            optionlet_payment_dates: vec![Date::null(); n_optionlet_tenors],
            optionlet_accrual_periods: vec![0.0; n_optionlet_tenors],
        };

        Ok(OptionletStripper {
            term_vol_surface: surface,
            ibor_index,
            discount,
            n_strikes,
            optionlet_tenors,
            cap_floor_lengths,
            volatility_type,
            displacement,
            optionlet_frequency,
            caches: RefCell::new(caches),
        })
    }

    /// The optionlet fixing tenors (`optionletFixingTenors`).
    pub fn optionlet_fixing_tenors(&self) -> &[Period] {
        &self.optionlet_tenors
    }

    /// The cap/floor lengths the stripper prices, one longer than each optionlet
    /// tenor by the index tenor.
    pub fn cap_floor_lengths(&self) -> &[Period] {
        &self.cap_floor_lengths
    }

    /// The number of optionlet maturities (`optionletMaturities`).
    pub fn optionlet_maturities(&self) -> usize {
        self.optionlet_tenors.len()
    }

    /// The number of strikes per maturity.
    pub fn n_strikes(&self) -> usize {
        self.n_strikes
    }

    /// The underlying cap/floor term-vol surface (`termVolSurface`).
    pub fn term_vol_surface(&self) -> &Shared<CapFloorTermVolSurface> {
        &self.term_vol_surface
    }

    /// The ibor index (`iborIndex`).
    pub fn ibor_index(&self) -> &Shared<IborIndex> {
        &self.ibor_index
    }

    /// The discount curve the derived stripper prices caps on.
    pub fn discount(&self) -> &Handle<dyn YieldTermStructure> {
        &self.discount
    }

    /// The optionlet step frequency, when overridden (`optionletFrequency`).
    pub fn optionlet_frequency(&self) -> Option<Period> {
        self.optionlet_frequency
    }

    /// The model the stripped volatilities are expressed in (`volatilityType`).
    pub fn volatility_type(&self) -> VolatilityType {
        self.volatility_type
    }

    /// The lognormal shift (`displacement`).
    pub fn displacement(&self) -> Real {
        self.displacement
    }

    /// The day counter, delegated to the surface (`dayCounter`).
    pub fn day_counter(&self) -> Option<DayCounter> {
        self.term_vol_surface.day_counter()
    }

    /// The calendar, delegated to the surface (`calendar`).
    pub fn calendar(&self) -> Option<Calendar> {
        self.term_vol_surface.calendar()
    }

    /// The settlement days, delegated to the surface (`settlementDays`).
    pub fn settlement_days(&self) -> QlResult<Natural> {
        self.term_vol_surface.settlement_days()
    }

    /// The business-day convention, delegated to the surface
    /// (`businessDayConvention`).
    pub fn business_day_convention(&self) -> BusinessDayConvention {
        self.term_vol_surface.business_day_convention()
    }

    /// The result caches the derived stripper's `strip` fills and its lazy
    /// accessors read (the C++ `mutable` members).
    pub fn caches(&self) -> &RefCell<OptionletStripperCaches> {
        &self.caches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quotes::SimpleQuote;
    use crate::settings::Settings;
    use crate::shared::shared;
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendars::target::Target;
    use crate::time::date::Month;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;
    use crate::time::timeunit::TimeUnit;
    use crate::types::Real;

    const BDC: BusinessDayConvention = BusinessDayConvention::ModifiedFollowing;

    fn settings_on(today: Date) -> Shared<Settings<Date>> {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today);
        settings
    }

    fn euribor6m(settings: Shared<Settings<Date>>) -> Shared<IborIndex> {
        shared(crate::indexes::ibor::Euribor::six_months(
            Handle::<dyn YieldTermStructure>::empty(),
            settings,
        ))
    }

    fn months(n: i64) -> Period {
        Period::new(n as crate::types::Integer, TimeUnit::Months)
    }

    /// Builds a fixed-reference surface over `option_tenors` with a flat 20%
    /// vol grid across `n_strikes` strikes.
    fn surface(option_tenors: Vec<Period>, n_strikes: usize) -> Shared<CapFloorTermVolSurface> {
        let strikes: Vec<Rate> = (0..n_strikes).map(|j| 0.01 + 0.01 * j as Real).collect();
        let vols: Vec<Vec<Handle<dyn crate::quotes::Quote>>> = option_tenors
            .iter()
            .map(|_| {
                (0..n_strikes)
                    .map(|_| {
                        let quote: Shared<dyn crate::quotes::Quote> =
                            shared(SimpleQuote::new(Some(0.20)));
                        Handle::new(quote)
                    })
                    .collect()
            })
            .collect();
        shared(
            CapFloorTermVolSurface::with_reference_date(
                Date::new(15, Month::June, 2026),
                Target::new(),
                BDC,
                option_tenors,
                strikes,
                vols,
                Actual365Fixed::new(),
            )
            .unwrap(),
        )
    }

    /// `commonSetup` for a 6M index over a surface reaching 4Y yields optionlet
    /// tenors `6M..42M` and cap/floor lengths `12M..48M`, each length one index
    /// tenor longer than its optionlet tenor (`optionletstripper.cpp:58-84`).
    #[test]
    fn common_setup_builds_the_optionlet_and_capfloor_tenors() {
        let settings = settings_on(Date::new(15, Month::June, 2026));
        let index = euribor6m(settings.clone());
        let option_tenors = (1..=4)
            .map(|n| Period::new(n, TimeUnit::Years))
            .collect::<Vec<_>>();
        let surface = surface(option_tenors, 5);

        let stripper = OptionletStripper::new(
            surface,
            index,
            Handle::<dyn YieldTermStructure>::empty(),
            VolatilityType::ShiftedLognormal,
            0.0,
            None,
        )
        .unwrap();

        let expected_optionlets: Vec<Period> = [6, 12, 18, 24, 30, 36, 42]
            .iter()
            .map(|&m| months(m))
            .collect();
        let expected_lengths: Vec<Period> = [12, 18, 24, 30, 36, 42, 48]
            .iter()
            .map(|&m| months(m))
            .collect();
        assert_eq!(
            stripper.optionlet_fixing_tenors(),
            expected_optionlets.as_slice()
        );
        assert_eq!(stripper.cap_floor_lengths(), expected_lengths.as_slice());
        assert_eq!(stripper.optionlet_maturities(), 7);
        assert_eq!(stripper.n_strikes(), 5);
    }

    /// The result caches are allocated to the optionlet-tenor count, strikes
    /// seeded from the surface, volatilities zeroed at `n_tenors x n_strikes`.
    #[test]
    fn common_setup_allocates_the_result_caches() {
        let settings = settings_on(Date::new(15, Month::June, 2026));
        let index = euribor6m(settings.clone());
        let surface = surface(
            (1..=4).map(|n| Period::new(n, TimeUnit::Years)).collect(),
            5,
        );

        let stripper = OptionletStripper::new(
            surface,
            index,
            Handle::<dyn YieldTermStructure>::empty(),
            VolatilityType::ShiftedLognormal,
            0.0,
            None,
        )
        .unwrap();

        let caches = stripper.caches().borrow();
        assert_eq!(caches.optionlet_strikes.len(), 7);
        assert_eq!(
            caches.optionlet_strikes[0],
            vec![0.01, 0.02, 0.03, 0.04, 0.05]
        );
        assert_eq!(caches.optionlet_volatilities.len(), 7);
        assert!(
            caches
                .optionlet_volatilities
                .iter()
                .all(|row| row.len() == 5)
        );
        assert_eq!(caches.optionlet_dates.len(), 7);
        assert_eq!(caches.atm_optionlet_rate.len(), 7);
    }

    /// A surface whose longest tenor is shorter than the first cap/floor length
    /// (`indexTenor + indexTenor`) is rejected (`optionletstripper.cpp:64-66`).
    #[test]
    fn a_too_short_surface_is_rejected() {
        let settings = settings_on(Date::new(15, Month::June, 2026));
        let index = euribor6m(settings.clone());
        let surface = surface(vec![months(3), months(6), months(9), months(11)], 5);

        let err = OptionletStripper::new(
            surface,
            index,
            Handle::<dyn YieldTermStructure>::empty(),
            VolatilityType::ShiftedLognormal,
            0.0,
            None,
        )
        .err()
        .unwrap();
        assert!(err.message().contains("too short"));
    }

    /// A non-zero displacement under the Normal model is rejected
    /// (`optionletstripper.cpp:44-46`).
    #[test]
    fn normal_model_forbids_a_displacement() {
        let settings = settings_on(Date::new(15, Month::June, 2026));
        let index = euribor6m(settings.clone());
        let surface = surface(
            (1..=4).map(|n| Period::new(n, TimeUnit::Years)).collect(),
            5,
        );

        let err = OptionletStripper::new(
            surface,
            index,
            Handle::<dyn YieldTermStructure>::empty(),
            VolatilityType::Normal,
            0.01,
            None,
        )
        .err()
        .unwrap();
        assert!(err.message().contains("Normal model"));
    }
}
