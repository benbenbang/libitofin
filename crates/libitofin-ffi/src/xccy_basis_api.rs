//! Constant-notional two-currency Ibor swaps with immutable spot snapshots.
use crate::boundary::*;
use crate::calendar_api::NativeCalendar;
use crate::float_float_swap_api::{check_coupon, check_schedule};
use crate::indexes_api::NativeIbor;
use crate::rates_api::{curve, settings};
use crate::time_api::{adjust, convention, date, day_counter, schedule_calendar};
use libitofin::currency::Currency;
use libitofin::exchangerate::ExchangeRate;
use libitofin::instruments::XccyBasisSwap;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared};
use libitofin::time::{
    businessdayconvention::BusinessDayConvention, date::Date, schedule::Schedule,
};

#[derive(Clone)]
struct NativeXccyBasisSwap {
    inner: Shared<XccyBasisSwap>,
    indexes: [NativeIbor; 2],
    settings: Shared<Settings<Date>>,
    valuation: Option<(Date, bool)>,
    _schedules: [Schedule; 2],
    _calendars: [NativeCalendar; 2],
}
impl NativeXccyBasisSwap {
    fn prepare(&self, query: i32) -> BindingResult<()> {
        for (side, (leg, index)) in [self.inner.base_leg(), self.inner.quote_leg()]
            .into_iter()
            .zip(&self.indexes)
            .enumerate()
        {
            if (query == 0 && side != 0) || ((query == 1 || query == 2) && side != 1) {
                continue;
            }
            let (settlement, include) = match self.valuation {
                Some((date, include)) => (Some(date), Some(include)),
                None => (self.settings.evaluation_date(), None),
            };
            for flow in leg {
                if !flow.has_occurred(&self.settings, settlement, include)?
                    && let Some(coupon) = flow.as_coupon()
                {
                    check_coupon(
                        index,
                        coupon.accrual_start_date(),
                        coupon.accrual_end_date(),
                        &self.settings,
                    )?;
                }
            }
        }
        Ok(())
    }
}
/// Pay base, receive quote, with mandatory initial and final principal exchanges.
/// Currency handles and immutable spot are copied; index/curve/Settings graphs are retained.
#[repr(C)]
pub struct ItofinXccyBasisSwapConfig {
    pub base_schedule: u64,
    pub base_index: u64,
    pub base_curve: u64,
    pub base_day_counter: u64,
    pub base_currency: u64,
    pub quote_schedule: u64,
    pub quote_index: u64,
    pub quote_curve: u64,
    pub quote_day_counter: u64,
    pub quote_currency: u64,
    pub settings: u64,
    pub base_notional: f64,
    pub quote_notional: f64,
    pub quote_spread: f64,
    /// Quote-currency units per base unit at reference or explicit spot settlement.
    pub spot: f64,
    pub has_payment_convention: u8,
    pub payment_convention: i32,
    pub has_valuation_dates: u8,
    pub settlement_date: i32,
    pub npv_date: i32,
    pub include_settlement_flows: u8,
    pub has_spot_settlement_date: u8,
    pub spot_settlement_date: i32,
}
fn flag(value: u8) -> BindingResult<bool> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(BindingError::invalid("flag must be zero or one")),
    }
}
/// Construct a plain Ibor swap. Omitted payment convention means Following.
/// # Safety
/// Follow the crate-level pointer and owning-thread contract. Output on success only.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_xccy_basis_swap_new(
    ctx: *mut Context,
    cfg: *const ItofinXccyBasisSwapConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            check_ptr(cfg)?;
            let a = &*cfg;
            let payment = if flag(a.has_payment_convention)? {
                Some(convention(a.payment_convention)?)
            } else {
                None
            };
            let valuation = if flag(a.has_valuation_dates)? {
                Some((
                    date(a.settlement_date)?,
                    date(a.npv_date)?,
                    flag(a.include_settlement_flows)?,
                ))
            } else {
                if flag(a.include_settlement_flows)? {
                    return Err(BindingError::invalid(
                        "include_settlement_flows requires fixed valuation dates",
                    ));
                }
                None
            };
            let spot_date = if flag(a.has_spot_settlement_date)? {
                Some(date(a.spot_settlement_date)?)
            } else {
                None
            };
            let indexes = [
                c.get::<NativeIbor>(a.base_index)?,
                c.get::<NativeIbor>(a.quote_index)?,
            ];
            let schedules = [
                c.get::<Schedule>(a.base_schedule)?,
                c.get::<Schedule>(a.quote_schedule)?,
            ];
            let calendars = [
                schedule_calendar(c, a.base_schedule)?,
                schedule_calendar(c, a.quote_schedule)?,
            ];
            for ((schedule, index), calendar) in schedules.iter().zip(&indexes).zip(&calendars) {
                check_schedule(
                    schedule,
                    calendar,
                    index,
                    Some(payment.unwrap_or(BusinessDayConvention::Following)),
                )?;
            }
            if schedules.iter().all(|schedule| schedule.len() >= 2) {
                let start = schedules[0].start_date().min(schedules[1].start_date());
                let end = schedules[0].end_date().max(schedules[1].end_date());
                for (schedule, calendar) in schedules.iter().zip(&calendars) {
                    adjust(calendar, start, schedule.business_day_convention())?;
                    adjust(calendar, end, schedule.business_day_convention())?;
                }
            }
            let settings = settings(c, a.settings)?;
            let spot = ExchangeRate::checked_new(
                c.get::<Currency>(a.base_currency)?,
                c.get::<Currency>(a.quote_currency)?,
                a.spot,
            )?;
            let mut inner = XccyBasisSwap::new(
                schedules[0].clone(),
                indexes[0].inner.clone(),
                curve(c, a.base_curve)?,
                a.base_notional,
                day_counter(c, a.base_day_counter)?,
                schedules[1].clone(),
                indexes[1].inner.clone(),
                curve(c, a.quote_curve)?,
                a.quote_notional,
                day_counter(c, a.quote_day_counter)?,
                a.quote_spread,
                spot,
                payment,
                settings.clone(),
            )?;
            if let Some((settlement, npv, include)) = valuation {
                inner = inner.with_valuation_dates(settlement, npv, include)?;
            }
            if let Some(date) = spot_date {
                inner = inner.with_spot_settlement_date(date)?;
            }
            let owner = NativeXccyBasisSwap {
                inner: shared(inner),
                indexes,
                settings,
                valuation: valuation.map(|(date, _, include)| (date, include)),
                _schedules: schedules,
                _calendars: calendars,
            };
            output(out, c.insert(owner)?)
        })
    }
}
/// Queries: 0 base-leg PV, 1 quote-leg PV, 2 quote-leg BPS, 3 base NPV,
/// 4 quote NPV, 5 absolute fair quote spread, 6 immutable spot.
/// Contractual leg PVs precede the paid-base/received-quote signs.
/// # Safety
/// Follow the crate-level pointer and owning-thread contract. Output on success only.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_xccy_basis_swap_value(
    ctx: *mut Context,
    id: u64,
    query: i32,
    out: *mut f64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            if !(0..=6).contains(&query) {
                return Err(BindingError::invalid("invalid cross-currency query"));
            }
            let owner = c.get::<NativeXccyBasisSwap>(id)?;
            if query != 6 {
                owner.prepare(query)?;
            }
            let value = match query {
                0 => owner.inner.base_leg_pv()?,
                1 => owner.inner.quote_leg_pv()?,
                2 => owner.inner.quote_leg_bps()?,
                3 => owner.inner.npv()?.value(),
                4 => owner.inner.npv_in_quote()?.value(),
                5 => owner.inner.fair_quote_basis_spread()?,
                _ => owner.inner.spot().rate(),
            };
            output(out, value)
        })
    }
}
/// Copy a currency into an independently owned handle: side zero base, one quote.
/// # Safety
/// Follow the crate-level pointer and owning-thread contract. Output on success only.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_xccy_basis_swap_currency(
    ctx: *mut Context,
    id: u64,
    side: i32,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            check_ptr(out)?;
            let owner = c.get::<NativeXccyBasisSwap>(id)?;
            let currency = match side {
                0 => owner.inner.spot().source(),
                1 => owner.inner.spot().target(),
                _ => return Err(BindingError::invalid("invalid currency side")),
            }
            .clone();
            output(out, c.insert(currency)?)
        })
    }
}

#[cfg(test)]
#[path = "xccy_basis_api_tests.rs"]
mod tests;
