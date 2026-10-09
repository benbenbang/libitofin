//! Checked, engine-less bond forwards with retained live underlying bonds.

use crate::errors::QlResult;
use crate::event::event_has_occurred;
use crate::handle::Handle;
use crate::instrument::{Instrument, InstrumentBase, InstrumentResults};
use crate::instruments::Bond;
use crate::position::Position;
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut};
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::types::{Natural, Real};
use crate::{fail, require};

/// Forward contract retaining its underlying bond, pricing engine and settings.
///
/// Matches QuantLib's dirty spot quote per 100 minus raw cash-flow income.
/// This upstream convention is dimensionally consistent for a non-amortizing
/// face-100 bond only; non-100 and amortizing bonds retain the upstream result.
/// Income includes all payments after forward settlement and on delivery,
/// including amortization, without filtering ex-coupon payments.
/// No pricing engine needs to be attached to this forward itself.
pub struct BondForward {
    base: InstrumentBase,
    bond: SharedMut<Bond>,
    settings: Shared<Settings<Date>>,
    discount_curve: Handle<dyn YieldTermStructure>,
    income_discount_curve: Handle<dyn YieldTermStructure>,
    value_date: Date,
    delivery_date: Date,
    position: Position,
    strike: Real,
    settlement_days: Natural,
    day_counter: DayCounter,
    calendar: Calendar,
    convention: BusinessDayConvention,
    forward_price: Option<Real>,
}

impl BondForward {
    /// Constructs a forward using the financing curve for income discounting.
    ///
    /// This explicit convenience default is not a QuantLib empty-handle fallback.
    /// Strike is a dirty delivery quote, not a cash notional. Value date is the
    /// earliest forward settlement; delivery is adjusted under `convention`.
    /// The bond's own settlement date and engine determine the dirty spot quote.
    ///
    /// # Errors
    /// Propagates [`Self::with_income_curve`] validation.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        bond: SharedMut<Bond>,
        value_date: Date,
        delivery_date: Date,
        position: Position,
        strike: Real,
        settlement_days: Natural,
        day_counter: DayCounter,
        calendar: Calendar,
        convention: BusinessDayConvention,
        discount_curve: Handle<dyn YieldTermStructure>,
    ) -> QlResult<Self> {
        Self::with_income_curve(
            bond,
            value_date,
            delivery_date,
            position,
            strike,
            settlement_days,
            day_counter,
            calendar,
            convention,
            discount_curve.clone(),
            discount_curve,
        )
    }

    /// Constructs a forward with independent financing and income curves.
    ///
    /// Retains the same bond and derives its evaluation-date settings from it.
    /// Empty curves may be relinked later; pricing then requires valid links.
    ///
    /// # Errors
    /// Rejects nonfinite or negative strike, null dates, delivery before value
    /// date after adjustment, date adjustment outside the supported range, and
    /// an exclusively borrowed bond.
    #[allow(clippy::too_many_arguments)]
    pub fn with_income_curve(
        bond: SharedMut<Bond>,
        value_date: Date,
        delivery_date: Date,
        position: Position,
        strike: Real,
        settlement_days: Natural,
        day_counter: DayCounter,
        calendar: Calendar,
        convention: BusinessDayConvention,
        discount_curve: Handle<dyn YieldTermStructure>,
        income_discount_curve: Handle<dyn YieldTermStructure>,
    ) -> QlResult<Self> {
        require!(
            strike.is_finite() && strike >= 0.0,
            "strike must be finite and nonnegative"
        );
        require!(value_date >= Date::min_date(), "null value date");
        require!(delivery_date >= Date::min_date(), "null delivery date");
        let delivery_date = adjust_date(&calendar, delivery_date, convention)?;
        require!(value_date <= delivery_date, "value date after delivery");
        let underlying = bond.try_borrow().map_err(|_| {
            crate::errors::QlError::new("bond is exclusively borrowed", file!(), line!())
        })?;
        let settings = underlying.settings_handle();
        let base = InstrumentBase::new();
        settings.register_eval_date_observer(&base.observer());
        underlying.base().register_observer(&base.observer());
        discount_curve.register_observer(&base.observer());
        income_discount_curve.register_observer(&base.observer());
        drop(underlying);
        Ok(Self {
            base,
            bond,
            settings,
            discount_curve,
            income_discount_curve,
            value_date,
            delivery_date,
            position,
            strike,
            settlement_days,
            day_counter,
            calendar,
            convention,
            forward_price: None,
        })
    }

    /// Forward settlement: max(value date, evaluation date plus business days).
    ///
    /// With zero settlement days, evaluation date is adjusted Following.
    /// # Errors
    /// Requires a valid evaluation date and a settlement within Date's range.
    pub fn settlement_date(&self) -> QlResult<Date> {
        let Some(mut date) = self.settings.evaluation_date() else {
            fail!("no evaluation date set");
        };
        require!(date >= Date::min_date(), "null evaluation date");
        date = advance_settlement(&self.calendar, date, self.settlement_days)?;
        Ok(date.max(self.value_date))
    }

    /// Dirty fair delivery quote `(bond.dirty_price() - income) / D(delivery)`.
    /// # Errors
    /// Propagates bond/curve errors; rejects expired forwards and nonfinite output.
    pub fn fair_forward_price(&mut self) -> QlResult<Real> {
        self.calculate()?;
        self.forward_price.ok_or_else(|| {
            crate::errors::QlError::new("forward price unavailable after expiry", file!(), line!())
        })
    }

    /// Dirty fair quote minus the retained bond's accrued quote at delivery.
    /// # Errors
    /// Propagates fair-price, borrowing and accrued-interest errors.
    pub fn clean_forward_price(&mut self) -> QlResult<Real> {
        let dirty = self.fair_forward_price()?;
        let bond = self.bond.try_borrow().map_err(|_| {
            crate::errors::QlError::new("bond is exclusively borrowed", file!(), line!())
        })?;
        let clean = dirty - bond.accrued_amount(Some(self.delivery_date))?;
        require!(clean.is_finite(), "nonfinite clean forward price");
        Ok(clean)
    }

    /// Adjusted delivery date, distinct from the underlying bond's maturity.
    pub fn delivery_date(&self) -> Date {
        self.delivery_date
    }
    /// Earliest forward settlement date.
    pub fn value_date(&self) -> Date {
        self.value_date
    }
    /// Agreed dirty delivery quote.
    pub fn strike(&self) -> Real {
        self.strike
    }
    /// Long or short delivery position.
    pub fn position(&self) -> Position {
        self.position
    }
    /// Forward settlement and delivery calendar.
    pub fn calendar(&self) -> &Calendar {
        &self.calendar
    }
    /// Delivery adjustment convention.
    pub fn business_day_convention(&self) -> BusinessDayConvention {
        self.convention
    }
    /// Retained day counter, matching the upstream forward contract metadata.
    pub fn day_counter(&self) -> &DayCounter {
        &self.day_counter
    }
    /// Live financing curve handle.
    pub fn discount_curve(&self) -> &Handle<dyn YieldTermStructure> {
        &self.discount_curve
    }
    /// Live income curve handle.
    pub fn income_discount_curve(&self) -> &Handle<dyn YieldTermStructure> {
        &self.income_discount_curve
    }
}

impl Instrument for BondForward {
    fn base(&self) -> &InstrumentBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut InstrumentBase {
        &mut self.base
    }
    fn is_expired(&self) -> QlResult<bool> {
        event_has_occurred(
            self.delivery_date,
            &self.settings,
            Some(self.settlement_date()?),
            None,
        )
    }
    fn setup_expired(&mut self) {
        self.forward_price = None;
        self.base.store_results(&InstrumentResults {
            value: Some(0.0),
            error_estimate: Some(0.0),
            ..InstrumentResults::default()
        });
    }
    fn perform_calculations(&mut self) -> QlResult<()> {
        let settlement = self.settlement_date()?;
        let curve = self.discount_curve.current_link()?;
        let income_curve = self.income_discount_curve.current_link()?;
        let discount = curve.discount_date(self.delivery_date, false)?;
        require!(
            discount.is_finite() && discount > 0.0,
            "financing discount must be finite and positive"
        );
        let mut bond = self.bond.try_borrow_mut().map_err(|_| {
            crate::errors::QlError::new("bond is already borrowed", file!(), line!())
        })?;
        let Some(evaluation) = self.settings.evaluation_date() else {
            fail!("no evaluation date set");
        };
        advance_settlement(bond.calendar(), evaluation, bond.settlement_days())?;
        let spot = bond.dirty_price()?;
        require!(spot.is_finite(), "nonfinite dirty spot price");
        let mut income = 0.0;
        for flow in bond.cashflows() {
            if !flow.has_occurred(&self.settings, Some(settlement), Some(false))? {
                if !flow.has_occurred(&self.settings, Some(self.delivery_date), Some(false))? {
                    break;
                }
                let amount = flow.amount()?;
                let df = income_curve.discount_date(flow.date(), false)?;
                require!(
                    amount.is_finite() && df.is_finite() && df > 0.0,
                    "invalid income amount or discount"
                );
                income += amount * df;
                require!(income.is_finite(), "nonfinite income present value");
            }
        }
        let forward = (spot - income) / discount;
        let long_value = (forward - self.strike) * discount;
        require!(
            forward.is_finite() && long_value.is_finite(),
            "nonfinite forward result"
        );
        self.forward_price = Some(forward);
        let npv = match self.position {
            Position::Long => long_value,
            Position::Short => -long_value,
        };
        self.base.store_results(&InstrumentResults {
            value: Some(npv),
            valuation_date: Some(curve.reference_date()?),
            ..InstrumentResults::default()
        });
        Ok(())
    }
}

fn advance_settlement(calendar: &Calendar, mut date: Date, days: Natural) -> QlResult<Date> {
    require!(date >= Date::min_date(), "null evaluation date");
    require!(
        u64::from(days) <= (Date::max_date() - date) as u64,
        "settlement outside date range"
    );
    if days == 0 {
        return roll(calendar, date, 1);
    }
    for _ in 0..days {
        date = roll(calendar, step(date, 1)?, 1)?;
    }
    Ok(date)
}

fn step(date: Date, direction: i32) -> QlResult<Date> {
    require!(
        (direction > 0 && date < Date::max_date()) || (direction < 0 && date > Date::min_date()),
        "business-day adjustment outside date range"
    );
    Ok(date + direction)
}

fn roll(calendar: &Calendar, mut date: Date, direction: i32) -> QlResult<Date> {
    while !calendar.is_business_day(date) {
        date = step(date, direction)?;
    }
    Ok(date)
}

fn adjust_date(
    calendar: &Calendar,
    date: Date,
    convention: BusinessDayConvention,
) -> QlResult<Date> {
    use BusinessDayConvention::*;
    match convention {
        Unadjusted => Ok(date),
        Following => roll(calendar, date, 1),
        Preceding => roll(calendar, date, -1),
        ModifiedFollowing | HalfMonthModifiedFollowing => {
            let following = roll(calendar, date, 1)?;
            if following.month() != date.month()
                || (convention == HalfMonthModifiedFollowing
                    && date.day_of_month() <= 15
                    && following.day_of_month() > 15)
            {
                roll(calendar, date, -1)
            } else {
                Ok(following)
            }
        }
        ModifiedPreceding => {
            let preceding = roll(calendar, date, -1)?;
            if preceding.month() != date.month() {
                roll(calendar, date, 1)
            } else {
                Ok(preceding)
            }
        }
        Nearest => {
            let mut before = date;
            let mut after = date;
            while !calendar.is_business_day(after) && !calendar.is_business_day(before) {
                after = step(after, 1)?;
                before = step(before, -1)?;
            }
            Ok(if calendar.is_business_day(after) {
                after
            } else {
                before
            })
        }
    }
}
