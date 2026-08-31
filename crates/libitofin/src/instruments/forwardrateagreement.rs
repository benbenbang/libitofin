//! Forward rate agreement.
//!
//! Port of `ql/instruments/forwardrateagreement.{hpp,cpp}`. A
//! [`ForwardRateAgreement`] settles and expires on its value date - the day
//! the underlying loan or deposit begins - not on the later maturity date;
//! `(maturity - value)` is the tenor of the underlying loan.
//!
//! The FRA prices without an engine, so it overrides
//! [`perform_calculations`](Instrument::perform_calculations) (the C++
//! `performCalculations`, `forwardrateagreement.cpp:89`) and
//! [`setup_expired`](Instrument::setup_expired), which on top of zeroing the
//! results still computes the forward rate (`:85-87`) so
//! [`forward_rate`](ForwardRateAgreement::forward_rate) works on an expired
//! FRA.
//!
//! Deviations, all by standing decision: the constructors return `Result` for
//! the C++ `QL_REQUIRE` guards (D4); the `Settings` registration is wired from
//! the index's own settings rather than a singleton (D5); and
//! [`amount`](ForwardRateAgreement::amount) on an expired FRA is an error
//! where C++ reads the never-initialized `amount_` member (undefined
//! behaviour), per D10.

use crate::errors::QlResult;
use crate::event::event_has_occurred;
use crate::fail;
use crate::handle::Handle;
use crate::indexes::iborindex::IborIndex;
use crate::indexes::index::Index;
use crate::indexes::interestrateindex::InterestRateIndex;
use crate::instrument::{Instrument, InstrumentBase, InstrumentResults};
use crate::interestrate::{Compounding, InterestRate};
use crate::position::Position;
use crate::require;
use crate::settings::Settings;
use crate::shared::Shared;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::types::{Rate, Real};

/// Forward rate agreement (FRA) over an Ibor index.
///
/// Choose [`Position::Long`] for an "FRA purchase" (future long loan, short
/// deposit) and [`Position::Short`] for an "FRA sale" (future short loan, long
/// deposit).
///
/// The forward rate and the settlement amount are cached on the instrument
/// itself (the C++ `mutable` members `forwardRate_`/`amount_`), refreshed by
/// the lazy [`calculate`](Instrument::calculate), so the accessors take
/// `&mut self`.
pub struct ForwardRateAgreement {
    base: InstrumentBase,
    settings: Shared<Settings<Date>>,
    fra_type: Position,
    forward_rate: Option<InterestRate>,
    strike_forward_rate: InterestRate,
    notional_amount: Real,
    index: Shared<IborIndex>,
    use_indexed_coupon: bool,
    day_counter: DayCounter,
    calendar: Calendar,
    business_day_convention: BusinessDayConvention,
    value_date: Date,
    maturity_date: Date,
    discount_curve: Handle<dyn YieldTermStructure>,
    amount: Option<Real>,
}

impl ForwardRateAgreement {
    /// Builds a FRA whose forward rate is forecast by the passed index (the
    /// indexed-coupon constructor, `forwardrateagreement.cpp:28`): the
    /// maturity is the index's own maturity of `value_date`, and the rate is
    /// the index fixing. Corresponds to `useIndexedCoupon = true` in the
    /// `FraRateHelper`.
    ///
    /// # Errors
    ///
    /// Propagates the maturity calculation and the guards of
    /// [`with_maturity`](Self::with_maturity).
    pub fn new(
        index: Shared<IborIndex>,
        value_date: Date,
        fra_type: Position,
        strike_forward_rate: Rate,
        notional_amount: Real,
        discount_curve: Handle<dyn YieldTermStructure>,
    ) -> QlResult<ForwardRateAgreement> {
        let maturity_date = index.maturity_date(value_date)?;
        let mut fra = Self::with_maturity(
            index,
            value_date,
            maturity_date,
            fra_type,
            strike_forward_rate,
            notional_amount,
            discount_curve,
        )?;
        fra.use_indexed_coupon = true;
        Ok(fra)
    }

    /// Builds a FRA over an explicit `[value_date, maturity_date]` window,
    /// forward-rated by the par-rate approximation off the index's forecast
    /// curve (the explicit-maturity constructor, `forwardrateagreement.cpp:39`).
    /// Corresponds to `useIndexedCoupon = false` in the `FraRateHelper`.
    ///
    /// The maturity is adjusted on the index's fixing calendar under the
    /// index's business day convention (`:52`). The FRA registers with the
    /// settings evaluation date, the discount curve and the index
    /// (`:55-56,:64`); per D5 the settings are the index's own.
    ///
    /// # Errors
    ///
    /// The notional must be positive and the value date earlier than the
    /// adjusted maturity date (`:57-58`).
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    pub fn with_maturity(
        index: Shared<IborIndex>,
        value_date: Date,
        maturity_date: Date,
        fra_type: Position,
        strike_forward_rate: Rate,
        notional_amount: Real,
        discount_curve: Handle<dyn YieldTermStructure>,
    ) -> QlResult<ForwardRateAgreement> {
        let day_counter = index.day_counter().clone();
        let calendar = index.fixing_calendar();
        let business_day_convention = index.business_day_convention();
        let maturity_date = calendar.adjust(maturity_date, business_day_convention);

        require!(notional_amount > 0.0, "notionalAmount must be positive");
        require!(
            value_date < maturity_date,
            "valueDate must be earlier than maturityDate"
        );

        let strike_forward_rate = InterestRate::new(
            strike_forward_rate,
            day_counter.clone(),
            Compounding::Simple,
            Frequency::Once,
        )?;
        let settings = index.base().settings().clone();
        let base = InstrumentBase::new();
        settings.register_eval_date_observer(&base.observer());
        discount_curve.register_observer(&base.observer());
        base.register_with(index.observable());

        Ok(ForwardRateAgreement {
            base,
            settings,
            fra_type,
            forward_rate: None,
            strike_forward_rate,
            notional_amount,
            index,
            use_indexed_coupon: false,
            day_counter,
            calendar,
            business_day_convention,
            value_date,
            maturity_date,
            discount_curve,
            amount: None,
        })
    }

    /// The index's fixing calendar.
    pub fn calendar(&self) -> &Calendar {
        &self.calendar
    }

    /// The convention the maturity date was adjusted under.
    pub fn business_day_convention(&self) -> BusinessDayConvention {
        self.business_day_convention
    }

    /// The index's day counter.
    pub fn day_counter(&self) -> &DayCounter {
        &self.day_counter
    }

    /// The term structure the settlement amount is discounted on (e.g. a repo
    /// curve); empty means the index's forwarding curve stands in.
    pub fn discount_curve(&self) -> &Handle<dyn YieldTermStructure> {
        &self.discount_curve
    }

    /// The value date the forward rate accrues from.
    pub fn value_date(&self) -> Date {
        self.value_date
    }

    /// The maturity date the forward rate accrues to (adjusted at construction).
    pub fn maturity_date(&self) -> Date {
        self.maturity_date
    }

    /// The index's fixing date for the value date.
    pub fn fixing_date(&self) -> Date {
        self.index.fixing_date(self.value_date)
    }

    /// The payoff on the value date (`amount`).
    ///
    /// # Errors
    ///
    /// Fails on an expired FRA: C++ reads the never-initialized `amount_`
    /// there (`setupExpired` computes only the forward rate), which the port
    /// surfaces as an error instead (D10).
    pub fn amount(&mut self) -> QlResult<Real> {
        self.calculate()?;
        match self.amount {
            Some(amount) => Ok(amount),
            None => fail!("amount not provided"),
        }
    }

    /// The relevant forward rate associated with the FRA term (`forwardRate`).
    ///
    /// On an expired FRA the rate is the one `setup_expired` computed; a
    /// failure on that infallible path leaves it unset and is surfaced by
    /// recomputing here.
    pub fn forward_rate(&mut self) -> QlResult<InterestRate> {
        self.calculate()?;
        match &self.forward_rate {
            Some(rate) => Ok(rate.clone()),
            None => self.calculated_forward_rate(),
        }
    }

    /// The forward rate off the index (`calculateForwardRate`,
    /// `forwardrateagreement.cpp:96`): the index fixing on the indexed-coupon
    /// path, the par-coupon approximation
    /// `(disc(value)/disc(maturity) - 1) / yearFraction(value, maturity)` off
    /// the index's forwarding term structure otherwise; Simple/Once either
    /// way.
    fn calculated_forward_rate(&self) -> QlResult<InterestRate> {
        let rate = if self.use_indexed_coupon {
            self.index.fixing(self.fixing_date(), false)?
        } else {
            let curve = self.index.forwarding_term_structure().current_link()?;
            (curve.discount_date(self.value_date, false)?
                / curve.discount_date(self.maturity_date, false)?
                - 1.0)
                / self
                    .index
                    .day_counter()
                    .year_fraction(self.value_date, self.maturity_date)
        };
        InterestRate::new(
            rate,
            self.index.day_counter().clone(),
            Compounding::Simple,
            Frequency::Once,
        )
    }

    /// `calculateAmount` (`forwardrateagreement.cpp:110`): with `F` the
    /// forward rate, `K` the strike and `T` the year fraction of the FRA term,
    /// the settlement amount is `notional * sign * (F - K) * T / (1 + F * T)`,
    /// the rate difference accrued over the term and discounted from maturity
    /// back to the value date at `F`; `sign` is `+1` for a long position and
    /// `-1` for a short one.
    fn calculate_amount(&mut self) -> QlResult<()> {
        let forward = self.calculated_forward_rate()?;
        let sign = match self.fra_type {
            Position::Long => 1.0,
            Position::Short => -1.0,
        };
        let f = forward.rate();
        let k = self.strike_forward_rate.rate();
        let t = forward
            .day_counter()
            .year_fraction(self.value_date, self.maturity_date);
        self.amount = Some(self.notional_amount * sign * (f - k) * t / (1.0 + f * t));
        self.forward_rate = Some(forward);
        Ok(())
    }
}

impl Instrument for ForwardRateAgreement {
    fn base(&self) -> &InstrumentBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut InstrumentBase {
        &mut self.base
    }

    /// A FRA expires/settles on the value date (`isExpired`,
    /// `forwardrateagreement.cpp:70`: a simple event on the value date has
    /// occurred).
    fn is_expired(&self) -> QlResult<bool> {
        event_has_occurred(self.value_date, &self.settings, None, None)
    }

    /// The C++ `setupExpired` zeroes the results and still computes the
    /// forward rate (`forwardrateagreement.cpp:85-87`). The signature is
    /// infallible, so a failing forward-rate calculation leaves the cache
    /// unset for [`forward_rate`](ForwardRateAgreement::forward_rate) to
    /// surface; the amount stays unset, see
    /// [`amount`](ForwardRateAgreement::amount).
    fn setup_expired(&mut self) {
        let expired = InstrumentResults {
            value: Some(0.0),
            error_estimate: Some(0.0),
            ..InstrumentResults::default()
        };
        self.base_mut().store_results(&expired);
        self.forward_rate = self.calculated_forward_rate().ok();
    }

    /// Engine-less pricing (`performCalculations`,
    /// `forwardrateagreement.cpp:89-93`): NPV is the settlement amount
    /// discounted to the value date on the discount curve, with the index's
    /// forwarding curve standing in when the discount handle is empty.
    fn perform_calculations(&mut self) -> QlResult<()> {
        self.calculate_amount()?;
        let discount = if self.discount_curve.is_empty() {
            self.index.forwarding_term_structure().clone()
        } else {
            self.discount_curve.clone()
        };
        let amount = self.amount.expect("calculate_amount just set it");
        let npv = amount
            * discount
                .current_link()?
                .discount_date(self.value_date, false)?;
        let results = InstrumentResults {
            value: Some(npv),
            ..InstrumentResults::default()
        };
        self.base_mut().store_results(&results);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    //! Oracles: `forwardrateagreement.cpp` `testConstructionWithoutACurve`
    //! (`:37`) plus a standalone analytic pin the C++ suite lacks - both suite
    //! oracles run with the discount handle equal to the forwarding curve and
    //! `K == F`, so neither can see a wrong curve in the par formula, the
    //! [`Position`] sign, or the amount/NPV path at all.
    //!
    //! The regression arm `piecewiseyieldcurve.cpp` `testParFraRegression`
    //! (`:794`) follows in this module as well.

    use super::*;
    use crate::handle::RelinkableHandle;
    use crate::indexes::ibor::euribor::Euribor;
    use crate::indexes::ibor::usdlibor::UsdLibor;
    use crate::math::interpolations::linear::Linear;
    use crate::quotes::{Quote, SimpleQuote};
    use crate::shared::shared;
    use crate::termstructures::bootstraphelper::RateHelper;
    use crate::termstructures::bootstraptraits::{ForwardRate, ZeroYield};
    use crate::termstructures::yields::{FlatForward, FraRateHelper, PiecewiseYieldCurve, Pillar};
    use crate::time::calendars::target::Target;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::period::Period;
    use crate::time::timeunit::TimeUnit;
    use crate::types::{Integer, Natural};

    fn settings_on(today: Date) -> Shared<Settings<Date>> {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today);
        settings
    }

    fn flat_curve(reference: Date, rate: Rate) -> Handle<dyn YieldTermStructure> {
        Handle::new(shared(FlatForward::with_rate(
            reference,
            rate,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>)
    }

    /// `testConstructionWithoutACurve` (`forwardrateagreement.cpp:37`): a
    /// USDLibor3M curve bootstrapped from three 12x15/24x27/36x39 FRA helpers
    /// at 0.01/0.02/0.03 (quotes filled only after the first FRA is built),
    /// off which a 12-month FRA built via the indexed constructor and one via
    /// the explicit-maturity constructor both reprice the first helper's rate,
    /// 0.01, within 1e-6.
    ///
    /// DIVERGENCE from the C++ case, documented rather than silent: the C++
    /// curve is `PiecewiseYieldCurve<ForwardRate, Cubic>`, but the global
    /// `Cubic` interpolator is not wired into the Rust bootstrap (it needs the
    /// unported convergence loop, #543; `piecewiseyieldcurve.rs` rejects it).
    /// `<ForwardRate, Linear>` stands in: the assertion is bootstrap
    /// self-consistency at the first helper's own window, which reprices
    /// regardless of the interpolator between nodes.
    #[test]
    fn construction_without_a_curve() {
        let today = Date::new(15, Month::June, 2026);
        let settings = settings_on(today);

        let curve_handle: RelinkableHandle<dyn YieldTermStructure> = RelinkableHandle::empty();
        let index = shared(
            UsdLibor::new(
                Period::new(3, TimeUnit::Months),
                curve_handle.handle(),
                settings.clone(),
            )
            .expect("a 3M USDLibor tenor is valid"),
        );

        let settlement_date = index.fixing_calendar().advance(
            today,
            index.fixing_days() as Integer,
            TimeUnit::Days,
            BusinessDayConvention::Following,
            false,
        );

        let quotes = [
            shared(SimpleQuote::default()),
            shared(SimpleQuote::default()),
            shared(SimpleQuote::default()),
        ];
        let helpers: Vec<Shared<dyn RateHelper>> = quotes
            .iter()
            .enumerate()
            .map(|(i, quote)| {
                FraRateHelper::new(
                    Handle::new(Shared::clone(quote) as Shared<dyn Quote>),
                    Period::new(i as Integer + 1, TimeUnit::Years),
                    index.as_ref(),
                    true,
                    Pillar::LastRelevantDate,
                ) as Shared<dyn RateHelper>
            })
            .collect();
        let curve = PiecewiseYieldCurve::<ForwardRate, Linear>::new(
            today,
            helpers,
            index.day_counter().clone(),
            Linear,
        )
        .unwrap();
        curve_handle.link_to(curve as Shared<dyn YieldTermStructure>);

        let mut fra = ForwardRateAgreement::new(
            Shared::clone(&index),
            settlement_date + Period::new(12, TimeUnit::Months),
            Position::Long,
            0.0,
            1.0,
            curve_handle.handle(),
        )
        .unwrap();

        quotes[0].set_value(0.01);
        quotes[1].set_value(0.02);
        quotes[2].set_value(0.03);

        let rate = fra.forward_rate().unwrap().rate();
        assert!(
            (rate - 0.01).abs() <= 1.0e-6,
            "FRA without maturityDate: got rate {rate}, expected 0.01"
        );

        let mut fra2 = ForwardRateAgreement::with_maturity(
            index,
            settlement_date + Period::new(12, TimeUnit::Months),
            settlement_date + Period::new(15, TimeUnit::Months),
            Position::Long,
            0.0,
            1.0,
            curve_handle.handle(),
        )
        .unwrap();
        let rate2 = fra2.forward_rate().unwrap().rate();
        assert!(
            (rate2 - 0.01).abs() <= 1.0e-6,
            "FRA with maturityDate: got rate {rate2}, expected 0.01"
        );
    }

    /// The standalone analytic pin, with the forwarding and discount curves
    /// deliberately DIFFERENT flat curves so each shows up only where it
    /// belongs: the forward rate must be the hand-computed
    /// `(D(v)/D(m) - 1) / yf(v, m)` off the FORWARDING curve
    /// (`forwardrateagreement.cpp:102-107`; a port reading the discount curve
    /// fails by ~2%), the amount `notional * sign * (F - K) * T / (1 + F T)`
    /// for both positions with `K != F` (the only [`Position`] coverage), and
    /// the NPV `amount * D(v)` off the DISCOUNT curve, falling back to the
    /// forwarding curve when the discount handle is empty (`:89-93`).
    #[test]
    fn par_forward_rate_amount_and_npv_match_the_closed_form() {
        let today = Date::new(15, Month::June, 2026);
        let settings = settings_on(today);
        let forward_flat = 0.04;
        let discount_flat = 0.06;
        let index = shared(Euribor::three_months(
            flat_curve(today, forward_flat),
            settings,
        ));

        let value_date = Date::new(17, Month::August, 2026);
        let maturity_date = Date::new(17, Month::November, 2026);
        let strike = 0.02;
        let dc = Actual360::new();
        let disc = |rate: Rate, d: Date| (-rate * dc.year_fraction(today, d)).exp();

        let mut fra = ForwardRateAgreement::with_maturity(
            Shared::clone(&index),
            value_date,
            maturity_date,
            Position::Long,
            strike,
            100.0,
            flat_curve(today, discount_flat),
        )
        .unwrap();

        let expected_forward = (disc(forward_flat, value_date) / disc(forward_flat, maturity_date)
            - 1.0)
            / dc.year_fraction(value_date, maturity_date);
        let forward = fra.forward_rate().unwrap();
        assert!((forward.rate() - expected_forward).abs() < 1.0e-12);
        assert_eq!(forward.compounding(), Compounding::Simple);

        let t = dc.year_fraction(value_date, maturity_date);
        let expected_amount =
            100.0 * (expected_forward - strike) * t / (1.0 + expected_forward * t);
        assert!((fra.amount().unwrap() - expected_amount).abs() < 1.0e-12);

        let expected_npv = expected_amount * disc(discount_flat, value_date);
        assert!((fra.npv().unwrap() - expected_npv).abs() < 1.0e-12);

        let mut short_fra = ForwardRateAgreement::with_maturity(
            Shared::clone(&index),
            value_date,
            maturity_date,
            Position::Short,
            strike,
            100.0,
            flat_curve(today, discount_flat),
        )
        .unwrap();
        assert!((short_fra.amount().unwrap() + expected_amount).abs() < 1.0e-12);

        let mut undiscounted = ForwardRateAgreement::with_maturity(
            index,
            value_date,
            maturity_date,
            Position::Long,
            strike,
            100.0,
            Handle::empty(),
        )
        .unwrap();
        let expected_fallback_npv = expected_amount * disc(forward_flat, value_date);
        assert!(
            (undiscounted.npv().unwrap() - expected_fallback_npv).abs() < 1.0e-12,
            "an empty discount handle must fall back to the forwarding curve"
        );
    }

    /// The stored value/maturity dates are surfaced verbatim (#958): the
    /// indexed constructor derives the maturity from the index, and both
    /// getters return exactly what construction computed - `value_date()` the
    /// input and `maturity_date()` the index's own maturity of it.
    #[test]
    fn value_and_maturity_dates_are_exposed() {
        let today = Date::new(15, Month::June, 2026);
        let settings = settings_on(today);
        let index = shared(Euribor::three_months(flat_curve(today, 0.04), settings));

        let value_date = Date::new(17, Month::August, 2026);
        let expected_maturity = index.maturity_date(value_date).unwrap();

        let fra = ForwardRateAgreement::new(
            Shared::clone(&index),
            value_date,
            Position::Long,
            0.02,
            100.0,
            Handle::empty(),
        )
        .unwrap();

        assert_eq!(fra.value_date(), value_date);
        assert_eq!(fra.maturity_date(), expected_maturity);
    }

    /// The constructor guards (`forwardrateagreement.cpp:57-58`) as `Result`
    /// errors (D4).
    #[test]
    fn constructor_guards_notional_and_date_order() {
        let today = Date::new(15, Month::June, 2026);
        let settings = settings_on(today);
        let index = shared(Euribor::three_months(flat_curve(today, 0.04), settings));

        let err = ForwardRateAgreement::with_maturity(
            Shared::clone(&index),
            Date::new(17, Month::August, 2026),
            Date::new(17, Month::November, 2026),
            Position::Long,
            0.02,
            0.0,
            Handle::empty(),
        )
        .err()
        .unwrap();
        assert_eq!(err.message(), "notionalAmount must be positive");

        let err = ForwardRateAgreement::with_maturity(
            index,
            Date::new(17, Month::November, 2026),
            Date::new(17, Month::August, 2026),
            Position::Long,
            0.02,
            100.0,
            Handle::empty(),
        )
        .err()
        .unwrap();
        assert_eq!(err.message(), "valueDate must be earlier than maturityDate");
    }

    /// A FRA expires on its value date (`isExpired`, `:70`); the expired path
    /// (`setupExpired`, `:85-87`) zeroes the NPV but still computes the
    /// forward rate. The amount is an error there: C++ reads the
    /// never-initialized `amount_`, which the port refuses (D10).
    #[test]
    fn expired_fra_has_zero_npv_but_still_a_forward_rate() {
        let today = Date::new(15, Month::June, 2026);
        let settings = settings_on(Date::new(1, Month::December, 2026));
        let index = shared(Euribor::three_months(flat_curve(today, 0.04), settings));

        let value_date = Date::new(17, Month::August, 2026);
        let maturity_date = Date::new(17, Month::November, 2026);
        let mut fra = ForwardRateAgreement::with_maturity(
            index,
            value_date,
            maturity_date,
            Position::Long,
            0.02,
            100.0,
            Handle::empty(),
        )
        .unwrap();

        assert!(fra.is_expired().unwrap());
        assert_eq!(fra.npv().unwrap(), 0.0);

        let dc = Actual360::new();
        let disc = |d: Date| (-0.04 * dc.year_fraction(today, d)).exp();
        let expected_forward = (disc(value_date) / disc(maturity_date) - 1.0)
            / dc.year_fraction(value_date, maturity_date);
        assert!((fra.forward_rate().unwrap().rate() - expected_forward).abs() < 1.0e-12);

        let err = fra.amount().unwrap_err();
        assert_eq!(err.message(), "amount not provided");
    }

    /// `testParFraRegression` (`piecewiseyieldcurve.cpp:794`): a
    /// `<ZeroYield, Linear>` curve bootstrapped on Actual/360 from the
    /// suite's five Euribor3M par FRA helpers (`fraData`, 1x4 through 9x12,
    /// `useIndexedFra = false`), settling off 23 February 2023; a par FRA
    /// built over each helper's own window via the explicit-maturity
    /// constructor reprices that helper's rate within 1e-6.
    ///
    /// The helpers were frozen on main before this port, so the pin is not
    /// circular: the FRA's par formula, day counter, discount direction and
    /// date construction are the new code under test.
    #[test]
    fn par_fra_regression() {
        let fra_data: [(Integer, Rate); 5] =
            [(1, 4.581), (2, 4.573), (3, 4.557), (6, 4.496), (9, 4.490)];

        let calendar = Target::new();
        let today = calendar.adjust(
            Date::new(23, Month::February, 2023),
            BusinessDayConvention::Following,
        );
        let settings = settings_on(today);
        let settlement = calendar.advance(
            today,
            2,
            TimeUnit::Days,
            BusinessDayConvention::Following,
            false,
        );

        let helper_index = Euribor::three_months(Handle::empty(), settings.clone());
        let helpers: Vec<Shared<dyn RateHelper>> = fra_data
            .iter()
            .map(|&(n, rate)| {
                FraRateHelper::from_months(
                    Handle::new(shared(SimpleQuote::new(rate / 100.0)) as Shared<dyn Quote>),
                    n as Natural,
                    &helper_index,
                    false,
                    Pillar::LastRelevantDate,
                ) as Shared<dyn RateHelper>
            })
            .collect();

        let curve_handle: RelinkableHandle<dyn YieldTermStructure> = RelinkableHandle::empty();
        let curve = PiecewiseYieldCurve::<ZeroYield, Linear>::new(
            settlement,
            helpers,
            Actual360::new(),
            Linear,
        )
        .unwrap();
        curve_handle.link_to(curve as Shared<dyn YieldTermStructure>);
        let euribor3m = shared(Euribor::three_months(curve_handle.handle(), settings));

        for (i, &(n, rate)) in fra_data.iter().enumerate() {
            let start = calendar.advance(
                settlement,
                n,
                TimeUnit::Months,
                euribor3m.business_day_convention(),
                euribor3m.end_of_month(),
            );
            let end = calendar.advance(
                settlement,
                3 + n,
                TimeUnit::Months,
                euribor3m.business_day_convention(),
                euribor3m.end_of_month(),
            );
            let mut fra = ForwardRateAgreement::with_maturity(
                Shared::clone(&euribor3m),
                start,
                end,
                Position::Long,
                rate / 100.0,
                100.0,
                curve_handle.handle(),
            )
            .unwrap();
            let expected_rate = rate / 100.0;
            let estimated_rate = fra.forward_rate().unwrap().rate();
            assert!(
                (expected_rate - estimated_rate).abs() <= 1.0e-6,
                "FRA {} (at par) failure: estimated {estimated_rate}, expected {expected_rate}",
                i + 1
            );
        }
    }
}
