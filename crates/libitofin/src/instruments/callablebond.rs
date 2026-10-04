//! Fixed-rate callable and puttable bonds for short-rate lattice valuation.
//!
//! Port of the fixed-rate product and argument setup in
//! `ql/experimental/callablebonds/callablebond.{hpp,cpp}`. Prices are quoted per
//! 100 of face amount; clean exercise prices use indenture accrued interest,
//! independently of market ex-coupon conventions.

use std::any::Any;

use crate::errors::QlResult;
use crate::event::event_has_occurred;
use crate::instrument::{Instrument, InstrumentBase};
use crate::instruments::{Bond, BondPrice, FixedRateBond};
use crate::pricingengine::{Arguments, Results};
use crate::settings::Settings;
use crate::shared::Shared;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::calendars::nullcalendar::NullCalendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::period::Period;
use crate::time::schedule::Schedule;
use crate::types::{Natural, Rate, Real};
use crate::{fail, require};

/// The issuer's redemption right or the holder's sale right.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallabilityType {
    /// The issuer may redeem the bond at the exercise price.
    Call,
    /// The holder may sell the bond back at the exercise price.
    Put,
}

/// A dated hard call or put, with a clean or dirty price per 100.
#[derive(Clone, Debug)]
pub struct Callability {
    price: BondPrice,
    call_type: CallabilityType,
    date: Date,
}

impl Callability {
    /// Constructs an exercise right.
    ///
    /// # Errors
    ///
    /// Rejects negative or nonfinite prices and null exercise dates.
    pub fn new(price: BondPrice, call_type: CallabilityType, date: Date) -> QlResult<Self> {
        require!(
            price.amount().is_finite() && price.amount() >= 0.0,
            "callability price must be finite and nonnegative"
        );
        require!(date != Date::null(), "null callability date");
        Ok(Self {
            price,
            call_type,
            date,
        })
    }

    /// The exercise price per 100, including its quote convention.
    pub fn price(&self) -> BondPrice {
        self.price
    }

    /// The direction of the exercise right.
    pub fn call_type(&self) -> CallabilityType {
        self.call_type
    }

    /// The exercise date.
    pub fn date(&self) -> Date {
        self.date
    }
}

/// Dated hard calls and puts, applied in the order supplied.
pub type CallabilitySchedule = Vec<Callability>;

/// Cash flows and exercise events required by a callable-bond engine.
#[derive(Default)]
pub struct CallableBondArguments {
    /// The date at which the bond is bought and quoted.
    pub settlement_date: Option<Date>,
    /// Coupon payment dates for flows owned at settlement.
    pub coupon_dates: Vec<Date>,
    /// Coupon amounts aligned with payment dates, in currency units.
    pub coupon_amounts: Vec<Real>,
    /// Constant face amount used to convert exercise quotes into currency units.
    pub face_amount: Real,
    /// The final redemption amount in currency units.
    pub redemption: Real,
    /// The final redemption date.
    pub redemption_date: Option<Date>,
    /// Dirty exercise prices per 100, aligned with dates and types.
    pub callability_prices: Vec<Real>,
    /// Exercise directions aligned with dates and prices.
    pub callability_types: Vec<CallabilityType>,
    /// Exercise dates still available after settlement.
    pub callability_dates: Vec<Date>,
}

impl Arguments for CallableBondArguments {
    fn validate(&self) -> QlResult<()> {
        let Some(settlement) = self.settlement_date else {
            fail!("null settlement date");
        };
        let Some(redemption_date) = self.redemption_date else {
            fail!("null redemption date");
        };
        require!(
            settlement != Date::null() && redemption_date != Date::null(),
            "null bond date"
        );
        require!(settlement <= redemption_date, "settlement after redemption");
        require!(
            self.face_amount.is_finite() && self.face_amount > 0.0,
            "face amount must be finite and positive"
        );
        require!(
            self.redemption.is_finite() && self.redemption >= 0.0,
            "redemption must be finite and nonnegative"
        );
        require!(
            self.coupon_dates.len() == self.coupon_amounts.len(),
            "coupon dates/amounts length mismatch"
        );
        require!(
            self.callability_dates.len() == self.callability_prices.len()
                && self.callability_dates.len() == self.callability_types.len(),
            "callability dates/prices/types length mismatch"
        );
        require!(
            self.coupon_dates.windows(2).all(|pair| pair[0] <= pair[1]),
            "coupon dates must be ordered"
        );
        for (date, amount) in self.coupon_dates.iter().zip(&self.coupon_amounts) {
            require!(
                *date != Date::null() && *date > settlement && *date <= redemption_date,
                "coupon outside settlement/redemption interval"
            );
            require!(amount.is_finite(), "nonfinite coupon amount");
        }
        for (date, price) in self.callability_dates.iter().zip(&self.callability_prices) {
            require!(
                *date != Date::null() && *date > settlement && *date <= redemption_date,
                "callability outside settlement/redemption interval"
            );
            require!(
                price.is_finite() && *price >= 0.0,
                "invalid callability price"
            );
        }
        Ok(())
    }
}

/// A fixed-rate bond with dated hard issuer calls or holder puts.
///
/// This composes the existing fixed-rate bond and shares its instrument base,
/// expiry handling, cash-flow observers and settlement-result storage.
pub struct CallableFixedRateBond {
    bond: FixedRateBond,
    put_call_schedule: CallabilitySchedule,
    face_amount: Real,
    settings: Shared<Settings<Date>>,
}

impl CallableFixedRateBond {
    /// Constructs a callable fixed-rate bond without an ex-coupon period.
    ///
    /// # Errors
    ///
    /// Rejects invalid amounts, nonfinite coupons, exercise after maturity,
    /// and the existing fixed-rate bond's construction errors.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        settlement_days: Natural,
        face_amount: Real,
        schedule: Schedule,
        coupons: Vec<Rate>,
        accrual_day_counter: DayCounter,
        payment_convention: BusinessDayConvention,
        redemption: Real,
        issue_date: Option<Date>,
        put_call_schedule: CallabilitySchedule,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<Self> {
        Self::with_ex_coupon(
            settlement_days,
            face_amount,
            schedule,
            coupons,
            accrual_day_counter,
            payment_convention,
            redemption,
            issue_date,
            put_call_schedule,
            None,
            NullCalendar::new(),
            BusinessDayConvention::Unadjusted,
            false,
            settings,
        )
    }

    /// Constructs a callable bond with market ex-coupon conventions.
    ///
    /// Clean exercise quotes still include indenture accrued from the previous
    /// coupon, rather than the market's negative ex-coupon accrued interest.
    ///
    /// # Errors
    ///
    /// Propagates the same construction errors as [`new`](Self::new).
    #[allow(clippy::too_many_arguments)]
    pub fn with_ex_coupon(
        settlement_days: Natural,
        face_amount: Real,
        schedule: Schedule,
        coupons: Vec<Rate>,
        accrual_day_counter: DayCounter,
        payment_convention: BusinessDayConvention,
        redemption: Real,
        issue_date: Option<Date>,
        put_call_schedule: CallabilitySchedule,
        ex_coupon_period: Option<Period>,
        ex_coupon_calendar: Calendar,
        ex_coupon_convention: BusinessDayConvention,
        ex_coupon_end_of_month: bool,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<Self> {
        require!(
            schedule.len() >= 2,
            "callable bond schedule requires at least two dates"
        );
        require!(
            schedule.dates().iter().all(|date| *date != Date::null())
                && schedule.dates().windows(2).all(|pair| pair[0] < pair[1]),
            "callable bond schedule dates must be nonnull and strictly increasing"
        );
        require!(
            face_amount.is_finite() && face_amount > 0.0,
            "face amount must be finite and positive"
        );
        require!(
            redemption.is_finite() && redemption >= 0.0,
            "redemption must be finite and nonnegative"
        );
        require!(
            coupons.iter().all(|rate| rate.is_finite()),
            "nonfinite coupon rate"
        );
        require!(
            put_call_schedule
                .iter()
                .all(|call| call.date() <= schedule.end_date()),
            "bond cannot mature before the last call/put date"
        );
        let bond = FixedRateBond::new(
            settlement_days,
            face_amount,
            schedule,
            coupons,
            accrual_day_counter,
            payment_convention,
            redemption,
            issue_date,
            None,
            ex_coupon_period,
            ex_coupon_calendar,
            ex_coupon_convention,
            ex_coupon_end_of_month,
            None,
            Shared::clone(&settings),
        )?;
        Ok(Self {
            bond,
            put_call_schedule,
            face_amount,
            settings,
        })
    }

    /// The immutable exercise schedule.
    pub fn callability(&self) -> &CallabilitySchedule {
        &self.put_call_schedule
    }

    /// The existing bond base for schedule, notional, settlement and accrual queries.
    pub fn bond(&self) -> &Bond {
        self.bond.bond()
    }

    /// The engine's value at settlement, in currency units.
    ///
    /// # Errors
    ///
    /// Propagates pricing failures or a missing settlement value.
    pub fn settlement_value(&mut self) -> QlResult<Real> {
        self.calculate()?;
        self.bond.bond_mut().settlement_value()
    }

    /// The settlement price per 100 including accrued interest.
    ///
    /// # Errors
    ///
    /// Propagates pricing, settlement and notional errors.
    pub fn dirty_price(&mut self) -> QlResult<Real> {
        self.calculate()?;
        let notional = self.bond().notional(None)?;
        if notional == 0.0 {
            return Ok(0.0);
        }
        let price = (self.settlement_value()? / notional) * 100.0;
        require!(price.is_finite(), "callable dirty price overflow");
        Ok(price)
    }

    /// The settlement price per 100 excluding accrued interest.
    ///
    /// # Errors
    ///
    /// Propagates pricing, settlement, notional and accrual errors.
    pub fn clean_price(&mut self) -> QlResult<Real> {
        let price = self.dirty_price()? - self.bond().accrued_amount(None)?;
        require!(price.is_finite(), "callable clean price overflow");
        Ok(price)
    }
}

impl Instrument for CallableFixedRateBond {
    fn base(&self) -> &InstrumentBase {
        self.bond.bond().base()
    }
    fn base_mut(&mut self) -> &mut InstrumentBase {
        self.bond.bond_mut().base_mut()
    }
    fn is_expired(&self) -> QlResult<bool> {
        self.bond.bond().is_expired()
    }
    fn setup_expired(&mut self) {
        self.bond.bond_mut().setup_expired();
    }
    fn fetch_results(&mut self, results: &dyn Results) -> QlResult<()> {
        self.bond.bond_mut().fetch_results(results)
    }

    fn setup_arguments(&self, arguments: &mut dyn Arguments) -> QlResult<()> {
        let Some(args) = (arguments as &mut dyn Any).downcast_mut::<CallableBondArguments>() else {
            fail!("wrong argument type");
        };
        let settlement = self.bond().settlement_date(None)?;
        let redemption =
            self.bond().redemptions().first().ok_or_else(|| {
                crate::errors::QlError::new("missing redemption", file!(), line!())
            })?;
        *args = CallableBondArguments {
            settlement_date: Some(settlement),
            face_amount: self.face_amount,
            redemption: redemption.amount()?,
            redemption_date: Some(redemption.date()),
            ..CallableBondArguments::default()
        };
        for flow in self.bond().cashflows() {
            if flow.as_coupon().is_some()
                && !flow.has_occurred(&self.settings, Some(settlement), Some(false))?
                && !flow.trading_ex_coupon(&self.settings, Some(settlement))?
            {
                args.coupon_dates.push(flow.date());
                args.coupon_amounts.push(flow.amount()?);
            }
        }
        for callability in &self.put_call_schedule {
            let call_date = callability.date();
            if event_has_occurred(call_date, &self.settings, Some(settlement), Some(false))? {
                continue;
            }
            let mut price = callability.price().amount();
            if let BondPrice::Clean(_) = callability.price() {
                for flow in self.bond().cashflows() {
                    if !event_has_occurred(
                        flow.date(),
                        &self.settings,
                        Some(call_date),
                        Some(false),
                    )? {
                        if let Some(coupon) = flow.as_coupon() {
                            let mut accrued = coupon.accrued_amount(call_date)?;
                            if coupon.trades_ex_coupon_on(call_date) {
                                accrued += flow.amount()?;
                            }
                            let notional = self.bond().notional(Some(call_date))?;
                            require!(
                                notional.is_finite() && notional > 0.0,
                                "invalid notional at call date"
                            );
                            price += accrued / notional * 100.0;
                        }
                        break;
                    }
                }
            }
            args.callability_dates.push(call_date);
            args.callability_types.push(callability.call_type());
            args.callability_prices.push(price);
        }
        Ok(())
    }
}
