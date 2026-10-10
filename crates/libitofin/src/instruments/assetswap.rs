//! Par asset swaps exchanging a bullet bond's cash flows for an Ibor leg.
//!
//! Implements the full-maturity par branch of QuantLib `AssetSwap`. Market
//! swaps, amortization, overnight indexes and custom final repayments are not
//! accepted. Pricing uses the existing generic swap-engine contract.

use crate::cashflow::{CashFlow, Leg};
use crate::cashflows::{IborLeg, SimpleCashFlow};
use crate::errors::QlResult;
use crate::indexes::{IborIndex, Index, InterestRateIndex};
use crate::instrument::{Instrument, InstrumentBase};
use crate::instruments::{Bond, Swap};
use crate::pricingengine::{Arguments, Results};
use crate::require;
use crate::shared::{Shared, shared};
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::schedule::{MakeSchedule, Schedule};
use crate::time::timeunit::TimeUnit;
use crate::types::{Real, Spread};

/// Full-maturity par asset swap over a bullet bond and an Ibor index.
///
/// `pay_bond_coupon` means paying the bond leg and receiving the floating leg,
/// including its dirty-price upfront and final notional exchange. Coupon
/// gearing is one. The bond and index must share the same settings handle.
pub struct AssetSwap {
    swap: Swap,
    bond: Shared<Bond>,
    clean_price: Real,
    spread: Spread,
    upfront_date: Date,
    notional: Real,
    pay_bond_coupon: bool,
}

impl AssetSwap {
    /// Builds a par swap. `clean_price` is per 100 at the floating start date.
    ///
    /// An absent schedule is generated backwards from bond settlement to
    /// maturity using the index tenor, calendar and convention, without EOM.
    /// Floating payments use Following; an absent day counter uses the index's.
    /// Bond coupons already occurred at the upfront date are discarded using
    /// the usual cash-flow settings rules. The redemption amount is preserved
    /// and paid at the Following-adjusted final floating date.
    ///
    /// # Errors
    /// Rejects non-finite prices/spreads, non-bullet bonds, day-tenor indexes,
    /// inconsistent settings, non-increasing schedules, and schedules not
    /// ending at the convention-adjusted full bond maturity. Propagates leg,
    /// notional, accrual and settlement errors.
    pub fn new(
        pay_bond_coupon: bool,
        bond: Shared<Bond>,
        clean_price: Real,
        index: Shared<IborIndex>,
        spread: Spread,
        floating_schedule: Option<Schedule>,
        floating_day_counter: Option<DayCounter>,
    ) -> QlResult<Self> {
        require!(clean_price.is_finite(), "clean price must be finite");
        require!(spread.is_finite(), "spread must be finite");
        require!(
            index.tenor().length() > 0 && index.tenor().units() != TimeUnit::Days,
            "day-tenor and overnight indexes are not supported"
        );
        let settings = bond.settings_handle();
        require!(
            Shared::ptr_eq(&settings, index.base().settings()),
            "bond and index settings must match"
        );
        require!(
            bond.redemptions().len() == 1,
            "asset swap requires a bullet bond with one redemption"
        );
        let maturity = bond.maturity_date()?;
        let schedule = match floating_schedule {
            Some(schedule) => schedule,
            None => {
                let settlement = bond.settlement_date(None)?;
                require!(settlement < maturity, "bond has matured");
                MakeSchedule::new()
                    .from(settlement)
                    .to(maturity)
                    .with_tenor(index.tenor())
                    .with_calendar(index.fixing_calendar())
                    .with_convention(index.business_day_convention())
                    .with_termination_date_convention(index.business_day_convention())
                    .backwards()
                    .end_of_month(false)
                    .build()
            }
        };
        require!(schedule.len() >= 2, "floating schedule needs two dates");
        require!(
            schedule.dates().iter().all(|date| *date != Date::null())
                && schedule.dates().windows(2).all(|w| w[0] < w[1]),
            "floating schedule dates must be strictly increasing"
        );
        require!(
            schedule.end_date()
                == schedule
                    .calendar()
                    .adjust(maturity, schedule.business_day_convention()),
            "floating schedule must end at full bond maturity"
        );
        let upfront_date = schedule.start_date();
        let final_date = schedule
            .calendar()
            .adjust(schedule.end_date(), BusinessDayConvention::Following);
        let notional = bond.notional(Some(upfront_date))?;
        require!(
            notional.is_finite() && notional > 0.0,
            "bond notional must be positive and finite"
        );
        let dirty_price = clean_price + bond.accrued_amount(Some(upfront_date))?;
        require!(dirty_price.is_finite(), "dirty price must be finite");
        let flows = bond.cashflows();
        require!(!flows.is_empty(), "no cashflows from bond");
        let mut bond_leg = Leg::new();
        let mut i = 0;
        while i + 1 < flows.len() && flows[i].date() <= schedule.end_date() {
            if !flows[i].has_occurred(&settings, Some(upfront_date), Some(false))? {
                bond_leg.push(Shared::clone(&flows[i]));
            }
            i += 1;
        }
        if i + 1 < flows.len()
            && let Some(coupon) = flows[i].as_coupon()
        {
            bond_leg.push(shared(SimpleCashFlow::new(
                coupon.accrued_amount(schedule.end_date())?,
                final_date,
            )?) as Shared<dyn CashFlow>);
        }
        bond_leg.push(shared(SimpleCashFlow::new(
            flows[flows.len() - 1].amount()?,
            final_date,
        )?) as Shared<dyn CashFlow>);
        let mut floating_leg = IborLeg::new(schedule.until(final_date), index.clone())
            .with_notionals(vec![notional])
            .with_payment_day_counter(
                floating_day_counter.unwrap_or_else(|| index.day_counter().clone()),
            )
            .with_payment_adjustment(BusinessDayConvention::Following)
            .with_spreads(vec![spread])
            .build()?;
        let upfront = (dirty_price - 100.0) / 100.0 * notional;
        require!(upfront.is_finite(), "upfront amount must be finite");
        floating_leg.insert(
            0,
            shared(SimpleCashFlow::new(upfront, upfront_date)?) as Shared<dyn CashFlow>,
        );
        floating_leg
            .push(shared(SimpleCashFlow::new(notional, final_date)?) as Shared<dyn CashFlow>);
        let swap = Swap::new(
            vec![bond_leg, floating_leg],
            vec![pay_bond_coupon, !pay_bond_coupon],
            settings,
        )?;
        Ok(Self {
            swap,
            bond,
            clean_price,
            spread,
            upfront_date,
            notional,
            pay_bond_coupon,
        })
    }

    /// Whether this is a par asset swap, always true for this API.
    pub fn par_swap(&self) -> bool {
        true
    }

    /// The underlying bond, with shared coupon ownership.
    pub fn bond(&self) -> &Shared<Bond> {
        &self.bond
    }

    /// The generic swap base and its leg inspectors.
    pub fn swap(&self) -> &Swap {
        &self.swap
    }

    /// The generic swap base, for pricing-engine attachment and leg results.
    pub fn swap_mut(&mut self) -> &mut Swap {
        &mut self.swap
    }

    /// Whether the bond leg is paid.
    pub fn pay_bond_coupon(&self) -> bool {
        self.pay_bond_coupon
    }

    /// The quoted forward clean price, per 100.
    pub fn clean_price(&self) -> Real {
        self.clean_price
    }

    /// The floating spread, in decimal rate units.
    pub fn spread(&self) -> Spread {
        self.spread
    }

    /// The date of the dirty-price upfront payment.
    pub fn upfront_date(&self) -> Date {
        self.upfront_date
    }

    /// The bond leg, including final redemption.
    pub fn bond_leg(&self) -> &Leg {
        &self.swap.legs()[0]
    }

    /// The floating leg, including upfront and final notional exchange.
    pub fn floating_leg(&self) -> &Leg {
        &self.swap.legs()[1]
    }

    /// Signed bond-leg NPV.
    ///
    /// # Errors
    /// Propagates pricing errors or an unavailable engine result.
    pub fn bond_leg_npv(&mut self) -> QlResult<Real> {
        self.swap.leg_npv(0)
    }

    /// Signed floating-leg NPV.
    ///
    /// # Errors
    /// Propagates pricing errors or an unavailable engine result.
    pub fn floating_leg_npv(&mut self) -> QlResult<Real> {
        self.swap.leg_npv(1)
    }

    /// Signed floating-leg value change for a one-basis-point spread increase.
    ///
    /// # Errors
    /// Propagates pricing errors or an unavailable engine result.
    pub fn floating_leg_bps(&mut self) -> QlResult<Real> {
        self.swap.leg_bps(1)
    }

    /// Spread that zeroes NPV: `spread - NPV / floatingBPS * 1e-4`.
    ///
    /// # Errors
    /// Requires available pricing results and a finite, nonzero floating BPS.
    pub fn fair_spread(&mut self) -> QlResult<Spread> {
        let npv = self.swap.npv()?;
        let bps = self.floating_leg_bps()?;
        require!(bps.is_finite() && bps != 0.0, "fair spread not available");
        let fair = self.spread - npv / bps * 1e-4;
        require!(fair.is_finite(), "fair spread not available");
        Ok(fair)
    }

    /// Native fair-clean-price formula with the current spread, per 100.
    /// It zeroes NPV only when the upfront payment is included in valuation.
    ///
    /// # Errors
    /// Requires available pricing results and a positive upfront discount.
    /// Seasoned swaps whose upfront predates the curve reference date fail.
    pub fn fair_clean_price(&mut self) -> QlResult<Real> {
        let npv = self.swap.npv()?;
        let discount = self.swap.start_discounts(1)?;
        let npv_discount = self.swap.npv_date_discount()?;
        require!(
            discount.is_finite() && discount > 0.0,
            "fair clean price not available for seasoned deal"
        );
        let sign = if self.pay_bond_coupon { 1.0 } else { -1.0 };
        let fair =
            self.clean_price - sign * npv * npv_discount / discount / (self.notional / 100.0);
        require!(fair.is_finite(), "fair clean price not available");
        Ok(fair)
    }
}

impl Instrument for AssetSwap {
    fn base(&self) -> &InstrumentBase {
        self.swap.base()
    }
    fn base_mut(&mut self) -> &mut InstrumentBase {
        self.swap.base_mut()
    }
    fn is_expired(&self) -> QlResult<bool> {
        self.swap.is_expired()
    }
    fn setup_expired(&mut self) {
        self.swap.setup_expired();
    }
    fn setup_arguments(&self, arguments: &mut dyn Arguments) -> QlResult<()> {
        self.swap.setup_arguments(arguments)
    }
    fn fetch_results(&mut self, results: &dyn Results) -> QlResult<()> {
        self.swap.fetch_results(results)
    }
}
