//! Par full-maturity asset swaps retaining plain bonds and Ibor ownership.
use crate::bond_api::NativeBond;
use crate::boundary::*;
use crate::calendar_api::NativeCalendar;
use crate::float_float_swap_api::{check_coupon, check_schedule};
use crate::indexes_api::NativeIbor;
use crate::rates_api::{curve, finite, settings};
use crate::time_api::{day_counter, schedule_calendar};
use libitofin::handle::Handle;
use libitofin::indexes::InterestRateIndex;
use libitofin::instrument::Instrument;
use libitofin::instruments::AssetSwap;
use libitofin::pricingengines::DiscountingSwapEngine;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::{
    businessdayconvention::BusinessDayConvention as Bdc,
    date::Date,
    schedule::{MakeSchedule, Schedule},
};
use std::cell::{Cell, RefCell};

type Flags = (bool, Option<bool>, bool, bool);
fn flags(s: &Settings<Date>) -> Flags {
    (
        s.include_reference_date_events(),
        s.include_todays_cash_flows(),
        s.enforces_todays_historic_fixings(),
        s.using_at_par_coupons(),
    )
}
#[derive(Clone)]
struct NativeAssetSwap {
    inner: SharedMut<AssetSwap>,
    _bond: NativeBond,
    index: NativeIbor,
    settings: Shared<Settings<Date>>,
    _schedule: Schedule,
    _calendar: NativeCalendar,
    discount: Shared<RefCell<Option<Handle<dyn YieldTermStructure>>>>,
    flags: Shared<Cell<Flags>>,
    frozen: Shared<Cell<bool>>,
}
impl NativeAssetSwap {
    fn prepare(&self, force: bool) -> BindingResult<()> {
        let swap = self.inner.borrow();
        if !self.frozen.get() && self.flags.get() != flags(&self.settings) {
            swap.base().observer().borrow_mut().update();
        }
        if !force
            && (self.frozen.get()
                || (swap.base().is_calculated() && self.flags.get() == flags(&self.settings)))
        {
            return Ok(());
        }
        if swap.is_expired()? {
            return Ok(());
        }
        let Some(discount) = self.discount.borrow().clone() else {
            return Ok(());
        };
        let settlement = discount.current_link()?.reference_date()?;
        for flow in swap.floating_leg() {
            if flow.has_occurred(
                &self.settings,
                Some(settlement),
                Some(self.settings.include_reference_date_events()),
            )? {
                continue;
            }
            if let Some(coupon) = flow.as_coupon() {
                check_coupon(
                    &self.index,
                    coupon.accrual_start_date(),
                    coupon.accrual_end_date(),
                    &self.settings,
                )?;
            }
        }
        Ok(())
    }
}
/// Par full-maturity terms. Zero schedule generates backwards from settlement;
/// zero day counter uses the index's.
#[repr(C)]
pub struct ItofinAssetSwapConfig {
    /// Existing plain fixed/zero bullet bond handle in this context.
    pub bond: u64,
    /// Ibor index handle sharing the bond's Settings.
    pub index: u64,
    /// Zero generates backwards from settlement; otherwise a full-maturity schedule handle.
    pub floating_schedule: u64,
    /// Zero uses the index's counter; otherwise a day-counter handle.
    pub floating_day_counter: u64,
    /// Forward clean quote per 100 at the floating schedule start.
    pub clean_price: f64,
    /// Floating coupon spread in decimal rate units.
    pub spread: f64,
    /// Zero receives bond coupons; one pays them.
    pub pay_bond_coupon: u8,
}
/// Construct over a plain fixed/zero bullet bond, retaining its native dependency graph.
/// # Safety
/// Follow the crate-level pointer and thread contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_asset_swap_new(
    ctx: *mut Context,
    cfg: *const ItofinAssetSwapConfig,
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
            let pay = match a.pay_bond_coupon {
                0 => false,
                1 => true,
                _ => return Err(BindingError::invalid("invalid pay-bond flag")),
            };
            let bond = c.get::<NativeBond>(a.bond)?;
            if bond.floating.is_some() {
                return Err(BindingError::invalid("fixed or zero plain bond required"));
            }
            let plain = bond.bond.plain()?.borrow();
            let settings = plain.settings_handle();
            let index = c.get::<NativeIbor>(a.index)?;
            let (schedule, calendar) = if a.floating_schedule == 0 {
                let settlement = bond.settlement(None)?;
                let maturity = plain.maturity_date()?;
                let tenor = index.inner.tenor();
                if settlement >= maturity
                    || tenor.length() <= 0
                    || tenor.units() == libitofin::time::timeunit::TimeUnit::Days
                {
                    return Err(BindingError::invalid(
                        "unmatured bond and positive non-day tenor required",
                    ));
                }
                let calendar = index.fixing_calendar.clone();
                crate::time_api::adjust(
                    &calendar,
                    maturity,
                    index.inner.business_day_convention(),
                )?;
                let mut count = 1i32;
                loop {
                    let amount = count
                        .checked_mul(tenor.length())
                        .and_then(i32::checked_neg)
                        .ok_or_else(|| BindingError::invalid("schedule tenor overflow"))?;
                    let previous = crate::time_api::advance(
                        &calendar,
                        maturity,
                        amount,
                        tenor.units(),
                        Bdc::Unadjusted,
                        false,
                    )?;
                    crate::time_api::adjust(
                        &calendar,
                        previous,
                        index.inner.business_day_convention(),
                    )?;
                    if previous < settlement {
                        break;
                    }
                    count += 1;
                }
                let schedule = MakeSchedule::new()
                    .from(settlement)
                    .to(maturity)
                    .with_tenor(tenor)
                    .with_calendar(calendar.inner.clone())
                    .with_convention(index.inner.business_day_convention())
                    .with_termination_date_convention(index.inner.business_day_convention())
                    .backwards()
                    .end_of_month(false)
                    .build();
                (schedule, calendar)
            } else {
                (
                    c.get::<Schedule>(a.floating_schedule)?,
                    schedule_calendar(c, a.floating_schedule)?,
                )
            };
            check_schedule(
                &schedule,
                &calendar,
                &index,
                Some(libitofin::time::businessdayconvention::BusinessDayConvention::Following),
            )?;
            let dc = if a.floating_day_counter == 0 {
                None
            } else {
                Some(day_counter(c, a.floating_day_counter)?)
            };
            let inner = AssetSwap::new(
                pay,
                shared(plain.unpriced_snapshot()),
                finite(a.clean_price)?,
                index.inner.clone(),
                finite(a.spread)?,
                Some(schedule.clone()),
                dc,
            )?;
            drop(plain);
            let owner = NativeAssetSwap {
                inner: shared_mut(inner),
                _bond: bond,
                index,
                flags: shared(Cell::new(flags(&settings))),
                settings,
                _schedule: schedule,
                _calendar: calendar,
                discount: shared(RefCell::new(None)),
                frozen: shared(Cell::new(false)),
            };
            output(out, c.insert(owner)?)
        })
    }
}
/// Attach a live discount curve with the bond's identical Settings and native defaults.
/// # Safety
/// Follow the crate-level pointer and thread contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_asset_swap_set_engine(
    ctx: *mut Context,
    id: u64,
    discount: u64,
    settings_id: u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            let owner = c.get::<NativeAssetSwap>(id)?;
            let s = settings(c, settings_id)?;
            if !Shared::ptr_eq(&s, &owner.settings) {
                return Err(BindingError::invalid("engine must share bond settings"));
            }
            let discount = curve(c, discount)?;
            let engine = shared_mut(DiscountingSwapEngine::new(
                discount.clone(),
                None,
                None,
                None,
                s,
            ));
            owner
                .inner
                .borrow_mut()
                .base_mut()
                .set_pricing_engine(engine);
            *owner.discount.borrow_mut() = Some(discount);
            Ok(())
        })
    }
}
/// Query 0 NPV, 1 bond NPV, 2 floating NPV, 3 floating BPS, 4 fair spread,
/// 5 fair clean price, 6 clean price, 7 spread, 8 upfront serial, 9 payer flag,
/// 10 calculated flag, 11 expiry. Failures preserve the output.
/// # Safety
/// Follow the crate-level pointer and thread contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_asset_swap_value(
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
            if !(0..=11).contains(&query) {
                return Err(BindingError::invalid("invalid asset-swap query"));
            }
            let owner = c.get::<NativeAssetSwap>(id)?;
            if query <= 5 {
                owner.prepare(false)?;
            }
            let mut swap = owner.inner.borrow_mut();
            let value = match query {
                0 => swap.npv()?,
                1 => swap.bond_leg_npv()?,
                2 => swap.floating_leg_npv()?,
                3 => swap.floating_leg_bps()?,
                4 => swap.fair_spread()?,
                5 => swap.fair_clean_price()?,
                6 => swap.clean_price(),
                7 => swap.spread(),
                8 => f64::from(swap.upfront_date().serial_number()),
                9 => f64::from(u8::from(swap.pay_bond_coupon())),
                10 => f64::from(u8::from(swap.base().is_calculated())),
                11 => f64::from(u8::from(swap.is_expired()?)),
                _ => unreachable!(),
            };
            if query <= 5 && !owner.frozen.get() {
                owner.flags.set(flags(&owner.settings));
            }
            output(out, finite(value)?)
        })
    }
}
/// Action 0 calculate, 1 recalculate, 2 freeze, 3 unfreeze.
/// # Safety
/// Follow the crate-level pointer and thread contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_asset_swap_action(
    ctx: *mut Context,
    id: u64,
    action: i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            if !error.is_null() {
                check_ptr(error)?;
            }
            if !(0..=3).contains(&action) {
                return Err(BindingError::invalid("invalid asset-swap action"));
            }
            let owner = c.get::<NativeAssetSwap>(id)?;
            if action <= 1 {
                owner.prepare(action == 1)?;
            }
            let mut swap = owner.inner.borrow_mut();
            match action {
                0 => swap.calculate()?,
                1 => swap.recalculate()?,
                2 => {
                    swap.base_mut().freeze();
                    owner.frozen.set(true);
                }
                3 => {
                    swap.base_mut().unfreeze();
                    owner.frozen.set(false);
                }
                _ => unreachable!(),
            }
            if action == 1 || (action == 0 && !owner.frozen.get()) {
                owner.flags.set(flags(&owner.settings));
            }
            Ok(())
        })
    }
}

#[cfg(test)]
#[path = "asset_swap_api_tests.rs"]
mod tests;
