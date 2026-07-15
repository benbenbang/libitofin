//! The swap-rate index.
//!
//! Port of `ql/indexes/swapindex.{hpp,cpp}`. [`SwapIndex`] is the concrete
//! [`InterestRateIndex`] whose fixing is the fair rate of an on-the-fly vanilla
//! swap: [`forecast_fixing`](InterestRateIndex::forecast_fixing) builds
//! [`underlying_swap`](SwapIndex::underlying_swap) and returns its fair rate,
//! and [`maturity_date`](InterestRateIndex::maturity_date) returns that swap's
//! maturity. The swap is assembled through [`MakeVanillaSwap`] from the index's
//! tenor, its forecasting [`IborIndex`] and the fixed-leg conventions, off the
//! value date the fixing date implies.
//!
//! ## Divergences from QuantLib
//!
//! - **The `lastSwap_` / `lastFixingDate_` cache is dropped.** In C++ it is pure
//!   memoization (`swapindex.cpp:81` comment "caching mechanism"): a repeated
//!   fixing date returns the same swap object. The [`InterestRateIndex`] trait
//!   takes `&self`, so a cache would force a `RefCell` and its re-entrancy
//!   hazard for no behavioural gain - the swap is rebuilt on every call and the
//!   fair rate is numerically identical (D10). Each call therefore returns a
//!   fresh owned [`VanillaSwap`], not a shared cached one.
//! - **The `iborIndex_` is held as a [`Shared`]**, not by value: [`MakeVanillaSwap`]
//!   and [`MakeSwaption`](crate::instruments::MakeSwaption) consume the index as
//!   a shared handle, and an `Rc` cannot project a field, so the shared identity
//!   must be held from the start (mirrors `OvernightIndex`).
//! - **The `clone` family and `OvernightIndexedSwapIndex` are deferred.**
//!   `SwapIndex::clone(forwarding)` / `clone(forwarding, discounting)` /
//!   `clone(tenor)` (`swapindex.cpp:111-177`) re-curve or re-tenor the index;
//!   nothing ported consumes them yet (`MakeSwaption` and the oracles do not).
//!   `OvernightIndexedSwapIndex` (`swapindex.hpp:109`) builds its underlying
//!   through `MakeOIS`, which #344 tracks as too thin - it lands with that work.

use crate::currency::Currency;
use crate::errors::QlResult;
use crate::handle::Handle;
use crate::indexes::IborIndex;
use crate::indexes::index::Index;
use crate::indexes::interestrateindex::{InterestRateIndex, InterestRateIndexBase};
use crate::instruments::MakeVanillaSwap;
use crate::instruments::VanillaSwap;
use crate::require;
use crate::settings::Settings;
use crate::shared::Shared;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;
use crate::types::{Natural, Rate};

/// A swap-rate index (`SwapIndex`) whose fixing is a vanilla swap's fair rate.
///
/// Wraps an [`InterestRateIndexBase`] with the forecasting [`IborIndex`] and the
/// fixed-leg conventions the underlying swap needs, plus an optional exogenous
/// discounting curve. Built with [`new`](Self::new) (forecasting and discounting
/// both off the ibor index's forwarding curve) or
/// [`with_exogenous_discount`](Self::with_exogenous_discount) (a separate
/// discounting curve).
pub struct SwapIndex {
    base: InterestRateIndexBase,
    swap_tenor: Period,
    ibor_index: Shared<IborIndex>,
    fixed_leg_tenor: Period,
    fixed_leg_convention: BusinessDayConvention,
    exogenous_discount: bool,
    discount: Handle<dyn YieldTermStructure>,
}

impl SwapIndex {
    /// Builds a swap index forecasting off `ibor_index`'s forwarding curve
    /// (`swapindex.cpp:29`), registering with that index so a relinked curve
    /// notifies observers.
    ///
    /// The raw `tenor` is stored for the underlying-swap construction (the C++
    /// `tenor_`), separate from the base's normalized [`tenor`](InterestRateIndex::tenor).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        family_name: String,
        tenor: Period,
        settlement_days: Natural,
        currency: Currency,
        fixing_calendar: Calendar,
        fixed_leg_tenor: Period,
        fixed_leg_convention: BusinessDayConvention,
        fixed_leg_day_counter: DayCounter,
        ibor_index: Shared<IborIndex>,
        settings: Shared<Settings<Date>>,
    ) -> SwapIndex {
        let base = InterestRateIndexBase::new(
            family_name,
            tenor,
            settlement_days,
            currency,
            fixing_calendar,
            fixed_leg_day_counter,
            settings,
        );
        ibor_index
            .base()
            .observable()
            .register_observer(&base.observer());
        SwapIndex {
            base,
            swap_tenor: tenor,
            ibor_index,
            fixed_leg_tenor,
            fixed_leg_convention,
            exogenous_discount: false,
            discount: Handle::empty(),
        }
    }

    /// Builds a swap index forecasting off `ibor_index`'s forwarding curve but
    /// discounting off the separate `discount` curve (`swapindex.cpp:45`),
    /// registering with both.
    #[allow(clippy::too_many_arguments)]
    pub fn with_exogenous_discount(
        family_name: String,
        tenor: Period,
        settlement_days: Natural,
        currency: Currency,
        fixing_calendar: Calendar,
        fixed_leg_tenor: Period,
        fixed_leg_convention: BusinessDayConvention,
        fixed_leg_day_counter: DayCounter,
        ibor_index: Shared<IborIndex>,
        discount: Handle<dyn YieldTermStructure>,
        settings: Shared<Settings<Date>>,
    ) -> SwapIndex {
        let base = InterestRateIndexBase::new(
            family_name,
            tenor,
            settlement_days,
            currency,
            fixing_calendar,
            fixed_leg_day_counter,
            settings,
        );
        ibor_index
            .base()
            .observable()
            .register_observer(&base.observer());
        discount.register_observer(&base.observer());
        SwapIndex {
            base,
            swap_tenor: tenor,
            ibor_index,
            fixed_leg_tenor,
            fixed_leg_convention,
            exogenous_discount: true,
            discount,
        }
    }

    /// The fixed-leg tenor (`fixedLegTenor()`).
    pub fn fixed_leg_tenor(&self) -> Period {
        self.fixed_leg_tenor
    }

    /// The fixed-leg business-day convention (`fixedLegConvention()`).
    pub fn fixed_leg_convention(&self) -> BusinessDayConvention {
        self.fixed_leg_convention
    }

    /// The forecasting index (`iborIndex()`), sharing its identity.
    pub fn ibor_index(&self) -> Shared<IborIndex> {
        Shared::clone(&self.ibor_index)
    }

    /// The forwarding curve the underlying swap forecasts off
    /// (`forwardingTermStructure()`), taken from the ibor index.
    pub fn forwarding_term_structure(&self) -> Handle<dyn YieldTermStructure> {
        self.ibor_index.forwarding_term_structure().clone()
    }

    /// The exogenous discounting curve (`discountingTermStructure()`), empty when
    /// discounting is not exogenous.
    pub fn discounting_term_structure(&self) -> Handle<dyn YieldTermStructure> {
        self.discount.clone()
    }

    /// Whether the index discounts off a separate curve (`exogenousDiscount()`).
    pub fn exogenous_discount(&self) -> bool {
        self.exogenous_discount
    }

    /// The vanilla swap whose fair rate is the fixing at `fixing_date`
    /// (`underlyingSwap`, `swapindex.cpp:76`).
    ///
    /// Assembles a par (zero fixed rate) [`VanillaSwap`] through
    /// [`MakeVanillaSwap`] from the value date the fixing date implies, the
    /// index tenor and forecasting index, and the fixed-leg conventions;
    /// discounts off the exogenous curve when set, else the forwarding curve.
    ///
    /// # Errors
    ///
    /// The fixing date must be non-null and a valid fixing date; propagates the
    /// swap construction.
    pub fn underlying_swap(&self, fixing_date: Date) -> QlResult<VanillaSwap> {
        require!(fixing_date != Date::null(), "null fixing date");
        let effective = self.value_date(fixing_date)?;
        let mut maker = MakeVanillaSwap::new(
            self.swap_tenor,
            Shared::clone(&self.ibor_index),
            Some(0.0),
            Period::new(0, TimeUnit::Days),
            self.base.settings().clone(),
        )
        .with_effective_date(effective)
        .with_fixed_leg_calendar(self.fixing_calendar())
        .with_fixed_leg_day_count(self.day_counter().clone())
        .with_fixed_leg_tenor(self.fixed_leg_tenor)
        .with_fixed_leg_convention(self.fixed_leg_convention)
        .with_fixed_leg_termination_date_convention(self.fixed_leg_convention);
        if self.exogenous_discount {
            maker = maker.with_discounting_term_structure(self.discount.clone());
        }
        maker.build()
    }
}

impl InterestRateIndex for SwapIndex {
    fn base(&self) -> &InterestRateIndexBase {
        &self.base
    }

    fn maturity_date(&self, value_date: Date) -> QlResult<Date> {
        let fix_date = self.fixing_date(value_date);
        self.underlying_swap(fix_date)?
            .fixed_vs_floating()
            .maturity_date()
    }

    fn forecast_fixing(&self, fixing_date: Date) -> QlResult<Rate> {
        let mut swap = self.underlying_swap(fixing_date)?;
        swap.fixed_vs_floating_mut().fair_rate()
    }
}

#[cfg(test)]
mod tests {
    //! Oracles for [`SwapIndex`]. QuantLib ships no dedicated `swapindex` test,
    //! so these are the pins that behaviour lacks upstream (D10): the fixing is
    //! the underlying swap's fair rate built from the index inspectors, and the
    //! maturity is that swap's maturity. Both use the `EuriborSwapIsdaFixA`
    //! conventions the ported `swaption.cpp` oracle constructs
    //! (`euriborswap.cpp:29-42`), built as a base `SwapIndex` directly.

    use super::*;
    use crate::indexes::ibor::Euribor;
    use crate::interestrate::Compounding;
    use crate::shared::shared;
    use crate::termstructures::yields::FlatForward;
    use crate::time::calendars::target::Target;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::daycounters::thirty360::{Convention, Thirty360};
    use crate::time::frequency::Frequency;

    fn today() -> Date {
        Date::new(9, Month::October, 2015)
    }

    fn settings_today() -> Shared<Settings<Date>> {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today());
        settings
    }

    fn flat_curve(rate: Rate) -> Handle<dyn YieldTermStructure> {
        Handle::new(shared(FlatForward::with_rate(
            today(),
            rate,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>)
    }

    /// The `EuriborSwapIsdaFixA(5Y, curve)` conventions built as a base
    /// `SwapIndex` (`euriborswap.cpp:29`): 5Y, 2 settlement days, EUR, TARGET,
    /// annual fixed leg, `ModifiedFollowing`, `Thirty360(BondBasis)` against a
    /// 6M Euribor float leg.
    fn euribor_swap_5y(
        curve: Handle<dyn YieldTermStructure>,
        settings: &Shared<Settings<Date>>,
    ) -> (SwapIndex, Shared<IborIndex>) {
        let euribor6m = shared(Euribor::six_months(curve, Shared::clone(settings)));
        let index = SwapIndex::new(
            "EuriborSwapIsdaFixA".into(),
            Period::new(5, TimeUnit::Years),
            2,
            Currency::eur(),
            Target::new(),
            Period::new(1, TimeUnit::Years),
            BusinessDayConvention::ModifiedFollowing,
            Thirty360::with_convention(Convention::BondBasis),
            Shared::clone(&euribor6m),
            Shared::clone(settings),
        );
        (index, euribor6m)
    }

    /// `forecastFixing` (`swapindex.cpp:72`) returns the fair rate of the
    /// underlying vanilla swap. Pinned against an independently built
    /// `MakeVanillaSwap` on literal `EuriborSwapIsdaFixA` conventions (not read
    /// back off the index), sharing the same forecasting index - so this pins the
    /// delegation and the inspector plumbing that feed the swap, not a tautology.
    #[test]
    fn forecast_fixing_is_the_underlying_swap_fair_rate() {
        let settings = settings_today();
        let curve = flat_curve(0.05);
        let (index, euribor6m) = euribor_swap_5y(curve, &settings);

        let fixing_date = Date::new(15, Month::October, 2015);
        let got = index.forecast_fixing(fixing_date).unwrap();

        let effective = index.value_date(fixing_date).unwrap();
        let mut reference = MakeVanillaSwap::new(
            Period::new(5, TimeUnit::Years),
            euribor6m,
            Some(0.0),
            Period::new(0, TimeUnit::Days),
            Shared::clone(&settings),
        )
        .with_effective_date(effective)
        .with_fixed_leg_calendar(Target::new())
        .with_fixed_leg_day_count(Thirty360::with_convention(Convention::BondBasis))
        .with_fixed_leg_tenor(Period::new(1, TimeUnit::Years))
        .with_fixed_leg_convention(BusinessDayConvention::ModifiedFollowing)
        .with_fixed_leg_termination_date_convention(BusinessDayConvention::ModifiedFollowing)
        .build()
        .unwrap();
        let expected = reference.fixed_vs_floating_mut().fair_rate().unwrap();

        assert!(
            (got - expected).abs() < 1e-14,
            "forecast fixing {got} vs underlying swap fair rate {expected}"
        );
        assert!(got > 0.0, "a positive swap rate off a 5% curve");
    }

    /// `maturityDate` (`swapindex.cpp:106`) is the underlying swap's maturity for
    /// the fixing date the value date implies. Pins the `fixingDate` ->
    /// `underlyingSwap` -> `maturityDate` delegation, and that it lands roughly a
    /// 5Y swap out from the value date.
    #[test]
    fn maturity_date_is_the_underlying_swap_maturity() {
        let settings = settings_today();
        let curve = flat_curve(0.05);
        let (index, _euribor6m) = euribor_swap_5y(curve, &settings);

        let value_date = Date::new(15, Month::October, 2015);
        let fix_date = index.fixing_date(value_date);
        let expected = index
            .underlying_swap(fix_date)
            .unwrap()
            .fixed_vs_floating()
            .maturity_date()
            .unwrap();

        assert_eq!(index.maturity_date(value_date).unwrap(), expected);
        assert!(
            expected > value_date + Period::new(4, TimeUnit::Years)
                && expected < value_date + Period::new(6, TimeUnit::Years),
            "a 5Y swap matures about five years out, got {expected:?}"
        );
    }

    /// A null fixing date is rejected (`underlyingSwap`, `swapindex.cpp:79`).
    #[test]
    fn null_fixing_date_is_rejected() {
        let settings = settings_today();
        let curve = flat_curve(0.05);
        let (index, _euribor6m) = euribor_swap_5y(curve, &settings);
        assert!(index.underlying_swap(Date::null()).is_err());
    }

    /// The inspectors report the constructed conventions, and a plain index has
    /// no exogenous discounting (`exogenousDiscount()` is false, the discounting
    /// curve empty).
    #[test]
    fn inspectors_report_the_conventions() {
        let settings = settings_today();
        let curve = flat_curve(0.05);
        let (index, _euribor6m) = euribor_swap_5y(curve, &settings);

        assert_eq!(index.fixed_leg_tenor(), Period::new(1, TimeUnit::Years));
        assert_eq!(
            index.fixed_leg_convention(),
            BusinessDayConvention::ModifiedFollowing
        );
        assert!(!index.exogenous_discount());
        assert!(index.discounting_term_structure().is_empty());
        assert!(!index.forwarding_term_structure().is_empty());
    }
}
