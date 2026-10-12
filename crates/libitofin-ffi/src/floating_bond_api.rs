//! Floating bonds sharing the common bond and retained coupon pricer graph.
use crate::bond_api::{ItofinBondConfig, NativeBond, money, optional_date};
use crate::boundary::*;
use crate::indexes_api::NativeIbor;
use crate::rates_api::{finite, settings};
use crate::ratevol_api::NativeOptionletGuard;
use crate::time_api::{convention, day_counter, time_unit};
use libitofin::cashflow::Leg;
use libitofin::cashflows::{
    BlackIborCouponPricer, CashFlows, Coupon, FloatingRateCouponPricer, IborCoupon,
};
use libitofin::handle::Handle;
use libitofin::indexes::{Index, InterestRateIndex};
use libitofin::instrument::Instrument;
use libitofin::instruments::{Bond, FloatingRateBond};
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::volatility::OptionletVolatilityStructure;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::date::Date;
use libitofin::time::{
    businessdayconvention::BusinessDayConvention, period::Period, schedule::Schedule,
    timeunit::TimeUnit,
};
use libitofin::utilities::null::Null;
use std::cell::{Cell, RefCell};

#[derive(Clone)]
pub(crate) struct NativeBlackIborPricer {
    pricer: SharedMut<dyn FloatingRateCouponPricer>,
    guard: Option<NativeOptionletGuard>,
}
fn adjust(
    calendar: &crate::calendar_api::NativeCalendar,
    date: Date,
    rule: BusinessDayConvention,
) -> BindingResult<Date> {
    crate::time_api::date(date.serial_number())?;
    if rule == BusinessDayConvention::Unadjusted {
        Ok(date)
    } else {
        crate::time_api::adjust(calendar, date, rule)
    }
}
pub(crate) fn advance(
    calendar: &crate::calendar_api::NativeCalendar,
    date: Date,
    steps: i32,
    unit: TimeUnit,
    rule: BusinessDayConvention,
    eom: bool,
) -> BindingResult<Date> {
    crate::time_api::date(date.serial_number())?;
    if steps == 0 {
        return adjust(calendar, date, rule);
    }
    if unit == TimeUnit::Days {
        let direction = if steps > 0 { 1 } else { -1 };
        let mut result = date;
        for _ in 0..i64::from(steps).abs() {
            result = crate::time_api::date(
                result
                    .serial_number()
                    .checked_add(direction)
                    .ok_or_else(|| BindingError::invalid("date overflow"))?,
            )?;
            while calendar.is_holiday(result)? {
                result = crate::time_api::date(
                    result
                        .serial_number()
                        .checked_add(direction)
                        .ok_or_else(|| BindingError::invalid("date overflow"))?,
                )?;
            }
        }
        return Ok(result);
    }
    let arithmetic = crate::calendar_api::NativeCalendar {
        inner: libitofin::time::calendars::NullCalendar::new(),
        horizon: None,
        first_year: 1901,
    };
    let shifted = crate::time_api::advance(
        &arithmetic,
        date,
        steps,
        unit,
        BusinessDayConvention::Unadjusted,
        rule == BusinessDayConvention::Unadjusted && eom,
    )?;
    if unit != TimeUnit::Weeks
        && eom
        && rule != BusinessDayConvention::Unadjusted
        && date
            >= adjust(
                calendar,
                Date::end_of_month(date),
                BusinessDayConvention::Preceding,
            )?
    {
        return adjust(
            calendar,
            Date::end_of_month(shifted),
            BusinessDayConvention::Preceding,
        );
    }
    adjust(calendar, shifted, rule)
}
type DiscountGuard = Option<(Handle<dyn YieldTermStructure>, Option<bool>)>;
#[derive(Clone)]
pub(crate) struct NativeFloating {
    index: NativeIbor,
    coupons: Shared<Vec<Option<Shared<IborCoupon>>>>,
    optionlets: Shared<Vec<bool>>,
    fixing_days: i32,
    fixing_convention: BusinessDayConvention,
    policy: Shared<Cell<bool>>,
    pricer_guard: Shared<RefCell<Option<NativeOptionletGuard>>>,
    engine_guard: Shared<RefCell<DiscountGuard>>,
    frozen: Shared<Cell<bool>>,
}
impl NativeFloating {
    pub(crate) fn refresh(&self, bond: &SharedMut<Bond>) {
        let flag = Index::settings(&*self.index.inner).using_at_par_coupons();
        if self.policy.replace(flag) != flag {
            bond.borrow().base().observer().borrow_mut().update();
        }
    }
    pub(crate) fn set_engine_guard(
        &self,
        discount: Handle<dyn YieldTermStructure>,
        include: Option<bool>,
    ) {
        *self.engine_guard.borrow_mut() = Some((discount, include));
    }
    pub(crate) fn freeze(&self, frozen: bool) {
        self.frozen.set(frozen);
    }
    pub(crate) fn guard_bond(
        &self,
        bond: &Bond,
        settlement: Date,
        force: bool,
    ) -> BindingResult<()> {
        if bond.is_expired()? || (!force && (bond.base().is_calculated() || self.frozen.get())) {
            return Ok(());
        }
        let engine = self.engine_guard.borrow().clone();
        if let Some((curve, include)) = engine {
            let valuation = curve.current_link()?.reference_date()?;
            self.guard_leg(
                bond.cashflows(),
                bond.settings(),
                Some(include.unwrap_or_else(|| bond.settings().include_reference_date_events())),
                Some(valuation),
            )?;
            self.guard_leg(
                bond.cashflows(),
                bond.settings(),
                Some(false),
                Some(settlement),
            )?;
        }
        Ok(())
    }
    pub(crate) fn guard_accrued(&self, bond: &Bond, settlement: Date) -> BindingResult<()> {
        if bond.notional(Some(settlement))? == 0.0 {
            return Ok(());
        }
        let leg = bond.cashflows();
        if let Some(first) =
            CashFlows::next_cash_flow(leg, bond.settings(), Some(false), Some(settlement))?
        {
            for (i, flow) in leg
                .iter()
                .enumerate()
                .skip(first)
                .take_while(|(_, flow)| flow.date() == leg[first].date())
            {
                if flow.as_coupon().is_some_and(|coupon| {
                    settlement > coupon.accrual_start_date() && settlement <= flow.date()
                }) {
                    self.guard_coupon(i)?;
                }
            }
        }
        Ok(())
    }
    pub(crate) fn guard_leg(
        &self,
        leg: &Leg,
        settings: &Settings<Date>,
        include: Option<bool>,
        settlement: Option<Date>,
    ) -> BindingResult<()> {
        for (i, flow) in leg.iter().enumerate() {
            if flow.has_occurred(settings, settlement, include)?
                || flow.trading_ex_coupon(settings, settlement)?
            {
                continue;
            }
            self.guard_coupon(i)?;
        }
        Ok(())
    }
    pub(crate) fn cashflow(&self, index: usize) -> NativeFloatingCashflow {
        NativeFloatingCashflow {
            floating: self.clone(),
            coupon: index,
        }
    }
    fn guard_coupon(&self, position: usize) -> BindingResult<()> {
        let Some(Some(coupon)) = self.coupons.get(position) else {
            return Ok(());
        };
        let index = &self.index.inner;
        let settings = Index::settings(&**index);
        let days = i32::try_from(index.fixing_days())
            .map_err(|_| BindingError::invalid("index fixing days overflow"))?;
        let fixing = advance(
            &self.index.fixing_calendar,
            coupon.accrual_start_date(),
            -self.fixing_days,
            TimeUnit::Days,
            self.fixing_convention,
            false,
        )?;
        self.index.fixing_calendar.checked(fixing)?;
        let Some(today) = settings.evaluation_date() else {
            return Ok(());
        };
        let fixed = fixing < today
            || (fixing == today
                && (settings.enforces_todays_historic_fixings()
                    || index.has_historical_fixing(fixing)));
        if fixed {
            return Ok(());
        }
        let value = advance(
            &self.index.fixing_calendar,
            fixing,
            days,
            TimeUnit::Days,
            BusinessDayConvention::Following,
            false,
        )?;
        let tenor = index.tenor();
        advance(
            &self.index.maturity_calendar,
            value,
            tenor.length(),
            tenor.units(),
            index.business_day_convention(),
            index.end_of_month(),
        )?;
        let natural_value = advance(
            &self.index.value_calendar,
            fixing,
            days,
            TimeUnit::Days,
            BusinessDayConvention::Following,
            false,
        )?;
        let natural_value = adjust(
            &self.index.maturity_calendar,
            natural_value,
            BusinessDayConvention::Following,
        )?;
        advance(
            &self.index.maturity_calendar,
            natural_value,
            tenor.length(),
            tenor.units(),
            index.business_day_convention(),
            index.end_of_month(),
        )?;
        if settings.using_at_par_coupons() {
            let next = advance(
                &self.index.fixing_calendar,
                coupon.accrual_end_date(),
                -self.fixing_days,
                TimeUnit::Days,
                BusinessDayConvention::Following,
                false,
            )?;
            advance(
                &self.index.fixing_calendar,
                next,
                days,
                TimeUnit::Days,
                BusinessDayConvention::Following,
                false,
            )?;
            crate::time_api::date(
                value
                    .serial_number()
                    .checked_add(1)
                    .ok_or_else(|| BindingError::invalid("coupon value date overflow"))?,
            )?;
        }
        if fixing > today && self.optionlets[position] {
            let guard = self.pricer_guard.borrow().clone();
            if let Some(guard) = guard {
                guard.check()?;
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
pub(crate) struct NativeFloatingCashflow {
    floating: NativeFloating,
    coupon: usize,
}
impl NativeFloatingCashflow {
    pub(crate) fn guard(&self) -> BindingResult<()> {
        self.floating.guard_coupon(self.coupon)
    }
}
#[repr(C)]
pub struct ItofinFloatingBondConfig {
    pub bond: ItofinBondConfig,
    pub schedule: u64,
    pub index: u64,
    pub day_counter: u64,
    pub fixing_days: u32,
    pub has_fixing_days: u8,
    pub fixing_convention: i32,
    /// Zero selects NullCalendar when an ex-coupon period is present.
    pub ex_coupon_calendar: u64,
    pub ex_coupon_length: i32,
    pub ex_coupon_unit: i32,
    pub ex_coupon_convention: i32,
    pub has_ex_coupon: u8,
    pub ex_coupon_end_of_month: u8,
}
#[repr(C)]
/// Each vector has its own length. Caps/floors use f32::MAX promoted to double for absence.
pub struct ItofinFloatingBondVectors {
    pub gearings: *const f64,
    pub gearings_count: usize,
    pub spreads: *const f64,
    pub spreads_count: usize,
    pub caps: *const f64,
    pub caps_count: usize,
    pub floors: *const f64,
    pub floors_count: usize,
}
#[unsafe(no_mangle)]
/// Construct without requiring coupon histories or a cap/floor pricer.
/// The common calendar field must be zero: floating bonds use their schedule calendar.
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_floating_rate_bond_new(
    ctx: *mut Context,
    a: ItofinFloatingBondConfig,
    v: ItofinFloatingBondVectors,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            money(&a.bond)?;
            if a.bond.calendar != 0
                || a.has_fixing_days > 1
                || a.has_ex_coupon > 1
                || a.ex_coupon_end_of_month > 1
            {
                return Err(BindingError::invalid("invalid floating bond option"));
            }
            let schedule = c.get::<Schedule>(a.schedule)?;
            let periods = schedule
                .len()
                .checked_sub(1)
                .filter(|n| *n > 0)
                .ok_or_else(|| BindingError::invalid("schedule spans no period"))?;
            if [
                v.gearings_count,
                v.spreads_count,
                v.caps_count,
                v.floors_count,
            ]
            .iter()
            .any(|n| *n > periods)
            {
                return Err(BindingError::invalid(
                    "coupon vector exceeds schedule periods",
                ));
            }
            let gearings = input_slice(v.gearings, v.gearings_count)?
                .iter()
                .map(|&x| finite(x))
                .collect::<BindingResult<Vec<_>>>()?;
            let spreads = input_slice(v.spreads, v.spreads_count)?
                .iter()
                .map(|&x| finite(x))
                .collect::<BindingResult<Vec<_>>>()?;
            let caps = input_slice(v.caps, v.caps_count)?
                .iter()
                .map(|&x| finite(x))
                .collect::<BindingResult<Vec<_>>>()?;
            let floors = input_slice(v.floors, v.floors_count)?
                .iter()
                .map(|&x| finite(x))
                .collect::<BindingResult<Vec<_>>>()?;
            let calendar = crate::time_api::schedule_calendar(c, a.schedule)?;
            let index = c.get::<NativeIbor>(a.index)?;
            let shared_settings = settings(c, a.bond.settings)?;
            if !std::ptr::eq(&*shared_settings, Index::settings(&*index.inner)) {
                return Err(BindingError::invalid("bond and index settings differ"));
            }
            let fixing_days = if a.has_fixing_days == 1 {
                Some(a.fixing_days)
            } else {
                None
            };
            let days = i32::try_from(fixing_days.unwrap_or(index.inner.fixing_days()))
                .map_err(|_| BindingError::invalid("fixing days overflow"))?;
            i32::try_from(index.inner.fixing_days())
                .map_err(|_| BindingError::invalid("index fixing days overflow"))?;
            let fixing_rule = convention(a.fixing_convention)?;
            let payment_rule = convention(a.bond.payment_convention)?;
            let ex_rule = convention(a.ex_coupon_convention)?;
            let ex = if a.has_ex_coupon == 1 {
                if a.ex_coupon_length < 0 {
                    return Err(BindingError::invalid(
                        "ex-coupon period must be nonnegative",
                    ));
                }
                Some(Period::new(
                    a.ex_coupon_length,
                    time_unit(a.ex_coupon_unit)?,
                ))
            } else {
                None
            };
            let ex_calendar = if ex.is_some_and(|p| p.length() != 0) && a.ex_coupon_calendar != 0 {
                c.get::<crate::calendar_api::NativeCalendar>(a.ex_coupon_calendar)?
            } else {
                crate::calendar_api::NativeCalendar {
                    inner: libitofin::time::calendars::NullCalendar::new(),
                    horizon: None,
                    first_year: 1901,
                }
            };
            for i in 0..periods {
                let start = schedule.date(i);
                let end = schedule.date(i + 1);
                let payment = adjust(&calendar, end, payment_rule)?;
                if let Some(p) = ex.filter(|p| p.length() != 0) {
                    advance(
                        &ex_calendar,
                        payment,
                        -p.length(),
                        p.units(),
                        ex_rule,
                        a.ex_coupon_end_of_month == 1,
                    )?;
                }
                if gearings
                    .get(i)
                    .or_else(|| gearings.last())
                    .is_none_or(|g| *g != 0.0)
                {
                    advance(
                        &index.fixing_calendar,
                        start,
                        -days,
                        TimeUnit::Days,
                        fixing_rule,
                        false,
                    )?;
                }
                if schedule.has_tenor() && schedule.has_is_regular() {
                    let p = schedule.tenor();
                    if i == 0 && !schedule.is_regular_at(1) {
                        advance(
                            &calendar,
                            end,
                            -p.length(),
                            p.units(),
                            schedule.business_day_convention(),
                            false,
                        )?;
                    }
                    if i == periods - 1 && !schedule.is_regular_at(i + 1) {
                        advance(
                            &calendar,
                            start,
                            p.length(),
                            p.units(),
                            schedule.business_day_convention(),
                            false,
                        )?;
                    }
                }
            }
            let optionlets = (0..periods)
                .map(|i| {
                    caps.get(i)
                        .or_else(|| caps.last())
                        .is_some_and(|rate| !rate.is_null())
                        || floors
                            .get(i)
                            .or_else(|| floors.last())
                            .is_some_and(|rate| !rate.is_null())
                })
                .collect();
            let bond = FloatingRateBond::new(
                a.bond.settlement_days,
                a.bond.face_amount,
                schedule,
                index.inner.clone(),
                day_counter(c, a.day_counter)?,
                payment_rule,
                fixing_days,
                gearings,
                spreads,
                caps,
                floors,
                a.bond.redemption,
                optional_date(a.bond.issue_date)?,
                ex,
                ex_calendar.inner,
                ex_rule,
                a.ex_coupon_end_of_month == 1,
                fixing_rule,
                shared_settings.clone(),
            )?;
            let metadata = NativeFloating {
                index,
                coupons: shared(bond.ibor_coupons().to_vec()),
                optionlets: shared(optionlets),
                fixing_days: days,
                fixing_convention: fixing_rule,
                policy: shared(Cell::new(shared_settings.using_at_par_coupons())),
                pricer_guard: shared(RefCell::new(None)),
                engine_guard: shared(RefCell::new(None)),
                frozen: shared(Cell::new(false)),
            };
            let mut native = NativeBond::new_unpriced(bond.into_bond(), calendar, None);
            native.floating = Some(metadata);
            output(out, c.insert(native)?)
        })
    }
}
#[unsafe(no_mangle)]
/// Volatility zero creates the existing no-volatility Black coupon pricer.
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_black_ibor_coupon_pricer_new(
    ctx: *mut Context,
    volatility: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let (pricer, guard) = if volatility == 0 {
                (BlackIborCouponPricer::new(), None)
            } else {
                (
                    BlackIborCouponPricer::with_vol(
                        c.get::<Handle<dyn OptionletVolatilityStructure>>(volatility)?,
                    ),
                    c.optional_metadata::<NativeOptionletGuard>(volatility),
                )
            };
            output(
                out,
                c.insert(NativeBlackIborPricer {
                    pricer: shared_mut(pricer),
                    guard,
                })?,
            )
        })
    }
}
#[unsafe(no_mangle)]
/// Install/replace one retained pricer on every nonzero-gearing coupon.
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_floating_bond_set_pricer(
    ctx: *mut Context,
    id: u64,
    pricer: u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let native = c.get::<NativeBond>(id)?;
            let floating = native
                .floating
                .ok_or_else(|| BindingError::invalid("not a floating bond"))?;
            let pricer = c.get::<NativeBlackIborPricer>(pricer)?;
            if let Some(guard) = &pricer.guard
                && !std::ptr::eq(&*guard.settings, Index::settings(&*floating.index.inner))
            {
                return Err(BindingError::invalid(
                    "bond and moving volatility settings differ",
                ));
            }
            *floating.pricer_guard.borrow_mut() = pricer.guard;
            for coupon in floating.coupons.iter().flatten() {
                coupon.set_pricer(pricer.pricer.clone());
            }
            Ok(())
        })
    }
}
#[unsafe(no_mangle)]
/// Return an independent handle retaining the original index and calendar metadata.
/// # Safety
/// Pointers and thread ownership must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_floating_bond_index(
    ctx: *mut Context,
    id: u64,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let floating = c
                .get::<NativeBond>(id)?
                .floating
                .ok_or_else(|| BindingError::invalid("not a floating bond"))?;
            output(out, c.insert(floating.index)?)
        })
    }
}

#[cfg(test)]
#[path = "floating_bond_api_tests.rs"]
mod tests;
