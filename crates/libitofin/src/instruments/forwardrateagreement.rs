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
