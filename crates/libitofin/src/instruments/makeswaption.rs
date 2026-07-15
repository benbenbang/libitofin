//! Swaption builder (`MakeSwaption`).
//!
//! Port of `ql/instruments/makeswaption.{hpp,cpp}`: the comfortable way to
//! instantiate a standard market [`Swaption`]. From a [`SwapIndex`], an option
//! tenor (or an explicit fixing date) and a strike it derives the exercise date,
//! builds the underlying [`VanillaSwap`] through [`MakeVanillaSwap`] and wraps it
//! in a [`Swaption`]. C++'s `operator Swaption()` /
//! `operator shared_ptr<Swaption>()` become [`MakeSwaption::build`].
//!
//! ## Exercise date (`makeswaption.cpp:56-75`)
//!
//! The exercise calendar defaults to the swap index's fixing calendar unless
//! [`with_exercise_calendar`](Self::with_exercise_calendar) overrides it. The
//! evaluation date is rolled to the next business day on that calendar; when no
//! explicit fixing date is given it is advanced by the option tenor under the
//! option convention. The exercise is European on that fixing date, or on an
//! explicit [`with_exercise_date`](Self::with_exercise_date) that must not be
//! after the fixing date.
//!
//! ## Deferred knobs
//!
//! - **The `MakeOIS` underlying path is not ported.** C++ builds the underlying
//!   through `MakeOIS` when the index is an `OvernightIndexedSwapIndex`
//!   (`makeswaption.cpp:112-125`); that index is itself deferred (it needs the
//!   `MakeOIS` extension #344 tracks), so only the [`MakeVanillaSwap`] path lands
//!   here. A non-overnight [`SwapIndex`] always takes it.
//! - **`withUnderlyingType` is not ported.** It threads `Swap::Type` into the
//!   underlying (`makeswaption.cpp:137`), but [`MakeVanillaSwap`] defers its own
//!   `withType` and always builds a [`Payer`](crate::instruments::SwapType), so
//!   there is nothing to thread it into. The underlying is always payer.
//! - **`withAtParCoupons`** is subsumed by
//!   [`with_indexed_coupons`](Self::with_indexed_coupons), which carries the D5
//!   refusal semantics [`MakeVanillaSwap`] already enforces.

use crate::errors::QlResult;
use crate::exercise::{EuropeanExercise, Exercise};
use crate::indexes::SwapIndex;
use crate::indexes::index::Index;
use crate::indexes::interestrateindex::InterestRateIndex;
use crate::instrument::Instrument;
use crate::instruments::MakeVanillaSwap;
use crate::instruments::swaption::{SettlementMethod, SettlementType, Swaption};
use crate::pricingengine::PricingEngine;
use crate::shared::{Shared, SharedMut, shared, shared_mut};
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;
use crate::types::{Rate, Real};
use crate::{fail, require};

/// Builder for a [`Swaption`] (`ql/instruments/makeswaption.hpp`).
///
/// Construct with [`new`](Self::new) (an option tenor) or
/// [`with_fixing_date`](Self::with_fixing_date) (an explicit fixing date), chain
/// the ported `with_*` overrides, then [`build`](Self::build).
pub struct MakeSwaption {
    swap_index: Shared<SwapIndex>,
    delivery: SettlementType,
    settlement_method: SettlementMethod,
    option_tenor: Option<Period>,
    option_convention: BusinessDayConvention,
    fixing_date: Option<Date>,
    exercise_date: Option<Date>,
    exercise_calendar: Option<Calendar>,
    strike: Option<Rate>,
    nominal: Real,
    use_indexed_coupons: Option<bool>,
    engine: Option<SharedMut<dyn PricingEngine>>,
}

impl MakeSwaption {
    /// Starts a builder whose exercise date is `option_tenor` from the (rolled)
    /// evaluation date (`makeswaption.cpp:34`).
    ///
    /// `strike` is the C++ `Null<Rate>()`-defaulted strike: `Some(k)` uses `k`,
    /// `None` builds at the money off the swap index's underlying-swap fair rate.
    pub fn new(
        swap_index: Shared<SwapIndex>,
        option_tenor: Period,
        strike: Option<Rate>,
    ) -> MakeSwaption {
        MakeSwaption {
            swap_index,
            delivery: SettlementType::Physical,
            settlement_method: SettlementMethod::PhysicalOTC,
            option_tenor: Some(option_tenor),
            option_convention: BusinessDayConvention::ModifiedFollowing,
            fixing_date: None,
            exercise_date: None,
            exercise_calendar: None,
            strike,
            nominal: 1.0,
            use_indexed_coupons: None,
            engine: None,
        }
    }

    /// Starts a builder pinned to an explicit `fixing_date`
    /// (`makeswaption.cpp:42`); see [`new`](Self::new) for `strike`.
    pub fn with_fixing_date(
        swap_index: Shared<SwapIndex>,
        fixing_date: Date,
        strike: Option<Rate>,
    ) -> MakeSwaption {
        MakeSwaption {
            swap_index,
            delivery: SettlementType::Physical,
            settlement_method: SettlementMethod::PhysicalOTC,
            option_tenor: None,
            option_convention: BusinessDayConvention::ModifiedFollowing,
            fixing_date: Some(fixing_date),
            exercise_date: None,
            exercise_calendar: None,
            strike,
            nominal: 1.0,
            use_indexed_coupons: None,
            engine: None,
        }
    }

    /// Sets the underlying swap's nominal (`withNominal`).
    pub fn with_nominal(mut self, nominal: Real) -> MakeSwaption {
        self.nominal = nominal;
        self
    }

    /// Sets how the swaption settles on exercise (`withSettlementType`).
    pub fn with_settlement_type(mut self, delivery: SettlementType) -> MakeSwaption {
        self.delivery = delivery;
        self
    }

    /// Sets the settlement method (`withSettlementMethod`).
    pub fn with_settlement_method(mut self, method: SettlementMethod) -> MakeSwaption {
        self.settlement_method = method;
        self
    }

    /// Sets the convention rolling the option tenor to the fixing date
    /// (`withOptionConvention`).
    pub fn with_option_convention(mut self, bdc: BusinessDayConvention) -> MakeSwaption {
        self.option_convention = bdc;
        self
    }

    /// Sets an explicit exercise date, overriding the fixing date
    /// (`withExerciseDate`); it must not be after the fixing date.
    pub fn with_exercise_date(mut self, date: Date) -> MakeSwaption {
        self.exercise_date = Some(date);
        self
    }

    /// Overrides the calendar the exercise date is rolled on
    /// (`withExerciseCalendar`), defaulting to the swap index's fixing calendar.
    pub fn with_exercise_calendar(mut self, calendar: Calendar) -> MakeSwaption {
        self.exercise_calendar = Some(calendar);
        self
    }

    /// Requests indexed (`Some(true)`) or at-par (`Some(false)`) coupons on the
    /// underlying, checked against [`Settings`](crate::settings::Settings) at
    /// build time (`withIndexedCoupons`). See [`MakeVanillaSwap`] for the D5
    /// refusal semantics.
    pub fn with_indexed_coupons(mut self, use_indexed_coupons: Option<bool>) -> MakeSwaption {
        self.use_indexed_coupons = use_indexed_coupons;
        self
    }

    /// Sets the pricing engine installed on the built swaption
    /// (`withPricingEngine`).
    pub fn with_pricing_engine(mut self, engine: SharedMut<dyn PricingEngine>) -> MakeSwaption {
        self.engine = Some(engine);
        self
    }

    /// Builds the swaption (C++ `operator shared_ptr<Swaption>()`,
    /// `makeswaption.cpp:54`).
    ///
    /// # Errors
    ///
    /// Returns an error when no evaluation date is set, when an explicit exercise
    /// date is after the fixing date, when an at-the-money strike is requested off
    /// an empty forwarding curve, and propagates the underlying-swap construction.
    pub fn build(self) -> QlResult<Swaption> {
        let settings = self.swap_index.base().settings().clone();
        let calendar = self
            .exercise_calendar
            .clone()
            .unwrap_or_else(|| self.swap_index.fixing_calendar());

        let eval = match settings.evaluation_date() {
            Some(today) => today,
            None => fail!("no evaluation date set: MakeSwaption needs a reference date"),
        };
        let ref_date = calendar.adjust(eval, BusinessDayConvention::Following);
        let fixing_date = match self.fixing_date {
            Some(date) => date,
            None => {
                let tenor = self
                    .option_tenor
                    .expect("a MakeSwaption carries an option tenor or a fixing date");
                calendar.advance_by_period(ref_date, tenor, self.option_convention, false)
            }
        };

        let exercise: Shared<dyn Exercise> = match self.exercise_date {
            None => shared(EuropeanExercise::new(fixing_date)) as Shared<dyn Exercise>,
            Some(exercise_date) => {
                require!(
                    exercise_date <= fixing_date,
                    "exercise date ({exercise_date:?}) must be less than or equal to fixing date ({fixing_date:?})"
                );
                shared(EuropeanExercise::new(exercise_date)) as Shared<dyn Exercise>
            }
        };

        let used_strike = match self.strike {
            Some(strike) => strike,
            None => {
                require!(
                    !self.swap_index.forwarding_term_structure().is_empty(),
                    "null term structure set to this instance of {}",
                    self.swap_index.name()
                );
                let mut atm = self.swap_index.underlying_swap(fixing_date)?;
                atm.fixed_vs_floating_mut().fair_rate()?
            }
        };

        let bdc = self.swap_index.fixed_leg_convention();
        let underlying = MakeVanillaSwap::new(
            self.swap_index.tenor(),
            self.swap_index.ibor_index(),
            Some(used_strike),
            Period::new(0, TimeUnit::Days),
            settings.clone(),
        )
        .with_effective_date(self.swap_index.value_date(fixing_date)?)
        .with_fixed_leg_calendar(self.swap_index.fixing_calendar())
        .with_fixed_leg_day_count(self.swap_index.day_counter().clone())
        .with_fixed_leg_tenor(self.swap_index.fixed_leg_tenor())
        .with_fixed_leg_convention(bdc)
        .with_fixed_leg_termination_date_convention(bdc)
        .with_nominal(self.nominal)
        .with_indexed_coupons(self.use_indexed_coupons)
        .build()?;

        let swap = shared_mut(underlying.into_fixed_vs_floating());
        let mut swaption = Swaption::new(
            swap,
            exercise,
            self.delivery,
            self.settlement_method,
            settings,
        );
        if let Some(engine) = self.engine {
            swaption.base_mut().set_pricing_engine(engine);
        }
        Ok(swaption)
    }
}

#[cfg(test)]
mod tests {
    //! Oracle for [`MakeSwaption`], ported from `swaption.cpp`'s
    //! `testMakeSwaptionWithExerciseCalendar` (:1148). The C++ fixture builds a
    //! `EuriborSwapIsdaFixA(5Y)`; per the review gate the same conventions are
    //! constructed as a base [`SwapIndex`] directly (`euriborswap.cpp:29-42`).
    //! The C++ test asserts only exercise dates (it never prices), so no vol or
    //! engine is needed; a fourth pin covers the at-the-money strike branch the
    //! calendar test's explicit strike never reaches.

    use super::*;
    use crate::handle::Handle;
    use crate::indexes::ibor::Euribor;
    use crate::interestrate::Compounding;
    use crate::settings::Settings;
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::calendars::target::Target;
    use crate::time::calendars::unitedstates::{Market, UnitedStates};
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::daycounters::thirty360::{Convention, Thirty360};
    use crate::time::frequency::Frequency;
    use crate::types::Natural;

    fn today() -> Date {
        Date::new(9, Month::October, 2015)
    }

    fn settings_today() -> Shared<Settings<Date>> {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today());
        settings
    }

    fn flat_curve(rate: Rate) -> Handle<dyn YieldTermStructure> {
        Handle::new(
            shared(crate::termstructures::yields::FlatForward::with_rate(
                today(),
                rate,
                Actual360::new(),
                Compounding::Continuous,
                Frequency::Annual,
            )) as Shared<dyn YieldTermStructure>,
        )
    }

    /// The `EuriborSwapIsdaFixA(5Y, curve)` conventions as a base `SwapIndex`.
    fn euribor_swap_5y(settings: &Shared<Settings<Date>>) -> Shared<SwapIndex> {
        let euribor6m = shared(Euribor::six_months(
            flat_curve(0.05),
            Shared::clone(settings),
        ));
        let settlement_days: Natural = 2;
        shared(SwapIndex::new(
            "EuriborSwapIsdaFixA".into(),
            Period::new(5, TimeUnit::Years),
            settlement_days,
            crate::currency::Currency::eur(),
            Target::new(),
            Period::new(1, TimeUnit::Years),
            BusinessDayConvention::ModifiedFollowing,
            Thirty360::with_convention(Convention::BondBasis),
            euribor6m,
            Shared::clone(settings),
        ))
    }

    /// `testMakeSwaptionWithExerciseCalendar` (`swaption.cpp:1148`): the default
    /// exercise date rolls on the swap index's TARGET calendar, an override rolls
    /// on the US Settlement calendar (they diverge because Oct 10 2016 is
    /// Columbus Day, a US holiday but not a TARGET one), and an explicit exercise
    /// date wins over both.
    #[test]
    fn exercise_date_rolls_on_the_chosen_calendar() {
        let settings = settings_today();
        let swap_index = euribor_swap_5y(&settings);
        let target = Target::new();
        let us = UnitedStates::new(Market::Settlement);
        let one_year = Period::new(1, TimeUnit::Years);

        let default_swaption = MakeSwaption::new(Shared::clone(&swap_index), one_year, Some(0.05))
            .build()
            .unwrap();
        let default_exercise = default_swaption.exercise().dates()[0];
        let expected = target.advance_by_period(
            target.adjust(today(), BusinessDayConvention::Following),
            one_year,
            BusinessDayConvention::ModifiedFollowing,
            false,
        );
        assert_eq!(default_exercise, expected);

        let custom_swaption = MakeSwaption::new(Shared::clone(&swap_index), one_year, Some(0.05))
            .with_exercise_calendar(us.clone())
            .build()
            .unwrap();
        let custom_exercise = custom_swaption.exercise().dates()[0];
        let expected_custom = us.advance_by_period(
            us.adjust(today(), BusinessDayConvention::Following),
            one_year,
            BusinessDayConvention::ModifiedFollowing,
            false,
        );
        assert_eq!(custom_exercise, expected_custom);
        assert_ne!(custom_exercise, default_exercise);

        let explicit_date = target.advance_by_period(
            today(),
            Period::new(6, TimeUnit::Months),
            BusinessDayConvention::Following,
            false,
        );
        let fixing_date =
            target.advance_by_period(today(), one_year, BusinessDayConvention::Following, false);
        let explicit_swaption =
            MakeSwaption::with_fixing_date(Shared::clone(&swap_index), fixing_date, Some(0.05))
                .with_exercise_calendar(us)
                .with_exercise_date(explicit_date)
                .build()
                .unwrap();
        assert_eq!(explicit_swaption.exercise().dates()[0], explicit_date);
    }

    /// An explicit exercise date after the fixing date is rejected
    /// (`makeswaption.cpp:70`).
    #[test]
    fn exercise_after_fixing_date_is_rejected() {
        let settings = settings_today();
        let swap_index = euribor_swap_5y(&settings);
        let target = Target::new();
        let fixing_date = target.advance_by_period(
            today(),
            Period::new(6, TimeUnit::Months),
            BusinessDayConvention::Following,
            false,
        );
        let past_fixing = target.advance_by_period(
            today(),
            Period::new(1, TimeUnit::Years),
            BusinessDayConvention::Following,
            false,
        );
        let result = MakeSwaption::with_fixing_date(swap_index, fixing_date, Some(0.05))
            .with_exercise_date(past_fixing)
            .build();
        assert!(result.is_err());
    }

    /// A `None` strike builds at the money (`makeswaption.cpp:79-106`): the
    /// underlying's fixed rate is the swap index's underlying-swap fair rate at
    /// the option fixing date.
    #[test]
    fn at_the_money_strike_is_the_underlying_fair_rate() {
        let settings = settings_today();
        let swap_index = euribor_swap_5y(&settings);
        let calendar = swap_index.fixing_calendar();
        let ref_date = calendar.adjust(today(), BusinessDayConvention::Following);
        let fixing_date = calendar.advance_by_period(
            ref_date,
            Period::new(1, TimeUnit::Years),
            BusinessDayConvention::ModifiedFollowing,
            false,
        );
        let expected = swap_index
            .underlying_swap(fixing_date)
            .unwrap()
            .fixed_vs_floating_mut()
            .fair_rate()
            .unwrap();

        let swaption = MakeSwaption::new(
            Shared::clone(&swap_index),
            Period::new(1, TimeUnit::Years),
            None,
        )
        .build()
        .unwrap();
        let used = swaption.underlying().borrow().fixed_rate();
        assert!(
            (used - expected).abs() < 1e-14,
            "ATM strike {used} vs underlying fair rate {expected}"
        );
    }

    /// An at-the-money strike off an empty forwarding curve is rejected
    /// (`makeswaption.cpp:81`).
    #[test]
    fn at_the_money_on_an_empty_curve_is_rejected() {
        let settings = settings_today();
        let euribor6m = shared(Euribor::six_months(
            Handle::<dyn YieldTermStructure>::empty(),
            Shared::clone(&settings),
        ));
        let swap_index = shared(SwapIndex::new(
            "EuriborSwapIsdaFixA".into(),
            Period::new(5, TimeUnit::Years),
            2,
            crate::currency::Currency::eur(),
            Target::new(),
            Period::new(1, TimeUnit::Years),
            BusinessDayConvention::ModifiedFollowing,
            Thirty360::with_convention(Convention::BondBasis),
            euribor6m,
            Shared::clone(&settings),
        ));
        let result = MakeSwaption::new(swap_index, Period::new(1, TimeUnit::Years), None).build();
        assert!(result.is_err());
    }
}
