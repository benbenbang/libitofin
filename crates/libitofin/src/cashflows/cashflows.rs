//! Analytics over a [`Leg`].
//!
//! Port of `ql/cashflows/cashflows.{hpp,cpp}`: the [`CashFlows`] namespace of
//! free functions that inspect a leg's dates, price it off a discount curve,
//! and price it off a single flat yield.
//!
//! ## Oracle
//!
//! The QuantLib test-suite never calls `bps`, `npvbps` or `atmRate`, in any
//! overload, and its only calls to the discount-curve [`npv`](CashFlows::npv)
//! come from the multiple-reset and inflation test files, which need index
//! machinery this port does not have. No ported test case stands behind those
//! numbers: they are checked against the definitions in `cashflows.cpp` and
//! against each other. Each test says which it is.
//!
//! The yield analytics fare better:
//! [`npv_at_yield`](CashFlows::npv_at_yield) is a direct port of
//! `cashflows.cpp::testSettings`, and `bonds.cpp::testExCouponGilt` pins
//! [`solve_yield`](CashFlows::solve_yield), [`duration`](CashFlows::duration)
//! and [`convexity`](CashFlows::convexity) against Bloomberg values.
//! [`basis_point_value`](CashFlows::basis_point_value) and
//! [`yield_value_basis_point`](CashFlows::yield_value_basis_point) are reached
//! by `bonds.cpp::testBasisPointValue` through the `BondFunctions::` wrappers
//! that delegate to them (`bondfunctions.cpp:449` and `:474`), which add a
//! tradability guard and nothing else.
//!
//! ## Divergences from QuantLib
//!
//! C++ computes the coupon split twice: `bps` and `atmRate` through a
//! `BPSCalculator` `AcyclicVisitor`, `npvbps` through `coupon_cast`. The port
//! keeps only the `coupon_cast` path ([`CashFlow::as_coupon`]) and runs every
//! analytic off one pass. One consequence: `bps` evaluates the amount of every
//! surviving flow, where the visitor evaluates it only for the flows that are
//! not coupons. A coupon whose [`amount`](CashFlow::amount) fails - a floating
//! one missing its fixing - therefore makes `bps` fail, where C++ returns a
//! number.
//!
//! Each yield analytic has two C++ overloads: one taking an `InterestRate`, one
//! taking the `(Rate, DayCounter, Compounding, Frequency)` it is built from and
//! forwarding. Rust has no overloading and the second carries no behaviour, so
//! only the `InterestRate` form is ported; a caller spells the conversion
//! [`InterestRate::new`], which is fallible where the C++ constructor throws.
//! [`solve_yield`](CashFlows::solve_yield) is the exception: it takes the four
//! pieces, because the rate it would take is the one it is solving for.
//!
//! C++ deletes every constructor to make `CashFlows` a namespace; the port uses
//! an uninhabited type, which cannot be constructed at all.
//!
//! The settlement date is passed as an [`Option`] rather than a null [`Date`],
//! and resolving it against an unset evaluation date is an error rather than a
//! fall back to the system clock (D10). The [`Settings`] travel as an argument
//! (D5).
//!
//! `previousCashFlow` and `nextCashFlow` return iterators into the leg, which
//! the `*Date` and `*Amount` helpers dereference and, in the amount's case,
//! walk on from. The port returns the index instead, `leg.rend()` and
//! `leg.end()` becoming `None`. The `*Amount` helpers likewise return `None`
//! where C++ returns a default-constructed `Real`, which no caller can tell
//! from a genuinely zero flow.

use std::cell::RefCell;
use std::cmp::Ordering;

use crate::cashflow::{CashFlow, Leg};
use crate::cashflows::Duration;
use crate::errors::{QlError, QlResult};
use crate::event::reference_date;
use crate::interestrate::{Compounding, InterestRate};
use crate::math::solver1d::{DerivativeSolver, Function1D};
use crate::math::solvers1d::newtonsafe::NewtonSafe;
use crate::require;
use crate::settings::Settings;
use crate::shared::Shared;
use crate::termstructures::yields::FlatForward;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;
use crate::types::{DiscountFactor, Rate, Real, Spread, Time};

/// The `basisPoint_` of `cashflows.cpp`.
const BASIS_POINT: Spread = 1.0e-4;

/// The one pass over a leg that the discount-curve analytics share, as `npvbps`
/// already does in C++.
///
/// Every sum is raw: undivided by the discount factor at the NPV date and
/// unscaled by [`BASIS_POINT`], which is how [`CashFlows::atm_rate`] needs them.
#[derive(Default)]
struct Totals {
    /// `sum(amount * df)` over every surviving flow.
    npv: Real,
    /// `sum(nominal * accrual_period * df)` over the surviving coupons.
    bps: Real,
    /// `sum(amount * df)` over the surviving flows that are not coupons. Dead
    /// weight in `bps`, which is why C++ computes it there too, and load-bearing
    /// in [`atm_rate`](CashFlows::atm_rate).
    non_sens_npv: Real,
}

/// The objective function of [`CashFlows::solve_yield`]: the leg's NPV at a
/// trial yield, less the NPV asked for.
///
/// [`Function1D`] cannot fail, where [`CashFlows::npv_at_yield`] can. A failed
/// evaluation is parked in `failure` and reported as a NaN, which no bracket
/// and no refinement step accepts; `solve_yield` returns the parked error in
/// place of whatever the solver made of the NaN.
struct IrrFinder<'a> {
    leg: &'a Leg,
    npv: Real,
    day_counter: DayCounter,
    compounding: Compounding,
    frequency: Frequency,
    settings: &'a Settings<Date>,
    include_settlement_date_flows: Option<bool>,
    settlement: Date,
    npv_date: Date,
    failure: &'a RefCell<Option<QlError>>,
}

impl IrrFinder<'_> {
    /// The leg's NPV at the trial yield `y`, and the rate itself.
    fn present_value(&self, y: Rate) -> QlResult<(InterestRate, Real)> {
        let rate = InterestRate::new(
            y,
            self.day_counter.clone(),
            self.compounding,
            self.frequency,
        )?;
        let npv = CashFlows::npv_at_yield(
            self.leg,
            &rate,
            self.settings,
            self.include_settlement_date_flows,
            Some(self.settlement),
            Some(self.npv_date),
        )?;
        Ok((rate, npv))
    }

    /// Park the first failure and hand the solver a NaN.
    fn park(&self, error: QlError) -> Real {
        self.failure.borrow_mut().get_or_insert(error);
        Real::NAN
    }
}

impl Function1D for IrrFinder<'_> {
    fn value(&mut self, y: Rate) -> Real {
        match self.present_value(y) {
            Ok((_, npv)) => npv - self.npv,
            Err(error) => self.park(error),
        }
    }

    /// `dP/dy = -D_modified * P`.
    fn derivative(&mut self, y: Rate) -> Real {
        let (rate, npv) = match self.present_value(y) {
            Ok(found) => found,
            Err(error) => return self.park(error),
        };
        match CashFlows::duration(
            self.leg,
            &rate,
            Duration::Modified,
            self.settings,
            self.include_settlement_date_flows,
            Some(self.settlement),
            Some(self.npv_date),
        ) {
            Ok(modified) => -modified * npv,
            Err(error) => self.park(error),
        }
    }
}

/// The `sign` template of `cashflows.cpp`, over the exact zero it compares to.
fn sign(x: Real) -> i32 {
    match x.partial_cmp(&0.0) {
        Some(Ordering::Greater) => 1,
        Some(Ordering::Less) => -1,
        _ => 0,
    }
}

/// The `CashFlows` namespace of `cashflows.hpp`.
pub enum CashFlows {}

impl CashFlows {
    /// The earliest date the leg accrues from: the first accrual start over the
    /// coupons, and the payment date of anything that is not one.
    ///
    /// # Errors
    ///
    /// The leg must not be empty.
    pub fn start_date(leg: &Leg) -> QlResult<Date> {
        require!(!leg.is_empty(), "empty leg");
        Ok(leg
            .iter()
            .map(|flow| Self::accrual_start_or_payment(flow.as_ref()))
            .min()
            .expect("a non-empty leg has a minimum"))
    }

    /// The latest date the leg accrues to: the last accrual end over the
    /// coupons, and the payment date of anything that is not one.
    ///
    /// # Errors
    ///
    /// The leg must not be empty.
    pub fn maturity_date(leg: &Leg) -> QlResult<Date> {
        require!(!leg.is_empty(), "empty leg");
        Ok(leg
            .iter()
            .map(|flow| Self::accrual_end_or_payment(flow.as_ref()))
            .max()
            .expect("a non-empty leg has a maximum"))
    }

    /// The index of the last flow paying before or at `settlement_date`.
    ///
    /// # Errors
    ///
    /// Propagates [`CashFlow::has_occurred`].
    pub fn previous_cash_flow(
        leg: &Leg,
        settings: &Settings<Date>,
        include_settlement_date_flows: Option<bool>,
        settlement_date: Option<Date>,
    ) -> QlResult<Option<usize>> {
        for index in (0..leg.len()).rev() {
            if leg[index].has_occurred(settings, settlement_date, include_settlement_date_flows)? {
                return Ok(Some(index));
            }
        }
        Ok(None)
    }

    /// The index of the first flow paying after `settlement_date`.
    ///
    /// # Errors
    ///
    /// Propagates [`CashFlow::has_occurred`].
    pub fn next_cash_flow(
        leg: &Leg,
        settings: &Settings<Date>,
        include_settlement_date_flows: Option<bool>,
        settlement_date: Option<Date>,
    ) -> QlResult<Option<usize>> {
        for (index, flow) in leg.iter().enumerate() {
            if !flow.has_occurred(settings, settlement_date, include_settlement_date_flows)? {
                return Ok(Some(index));
            }
        }
        Ok(None)
    }

    /// The payment date of the [`previous_cash_flow`](Self::previous_cash_flow).
    ///
    /// # Errors
    ///
    /// Propagates [`CashFlow::has_occurred`].
    pub fn previous_cash_flow_date(
        leg: &Leg,
        settings: &Settings<Date>,
        include_settlement_date_flows: Option<bool>,
        settlement_date: Option<Date>,
    ) -> QlResult<Option<Date>> {
        let found = Self::previous_cash_flow(
            leg,
            settings,
            include_settlement_date_flows,
            settlement_date,
        )?;
        Ok(found.map(|index| leg[index].date()))
    }

    /// The payment date of the [`next_cash_flow`](Self::next_cash_flow).
    ///
    /// # Errors
    ///
    /// Propagates [`CashFlow::has_occurred`].
    pub fn next_cash_flow_date(
        leg: &Leg,
        settings: &Settings<Date>,
        include_settlement_date_flows: Option<bool>,
        settlement_date: Option<Date>,
    ) -> QlResult<Option<Date>> {
        let found = Self::next_cash_flow(
            leg,
            settings,
            include_settlement_date_flows,
            settlement_date,
        )?;
        Ok(found.map(|index| leg[index].date()))
    }

    /// The total amount paid on the date of the
    /// [`previous_cash_flow`](Self::previous_cash_flow), summed over every flow
    /// sharing it.
    ///
    /// # Errors
    ///
    /// Propagates [`CashFlow::has_occurred`] and [`CashFlow::amount`].
    pub fn previous_cash_flow_amount(
        leg: &Leg,
        settings: &Settings<Date>,
        include_settlement_date_flows: Option<bool>,
        settlement_date: Option<Date>,
    ) -> QlResult<Option<Real>> {
        let Some(index) = Self::previous_cash_flow(
            leg,
            settings,
            include_settlement_date_flows,
            settlement_date,
        )?
        else {
            return Ok(None);
        };
        Self::amount_on_payment_date(leg[..=index].iter().rev(), leg[index].date()).map(Some)
    }

    /// The total amount paid on the date of the
    /// [`next_cash_flow`](Self::next_cash_flow), summed over every flow sharing
    /// it.
    ///
    /// # Errors
    ///
    /// Propagates [`CashFlow::has_occurred`] and [`CashFlow::amount`].
    pub fn next_cash_flow_amount(
        leg: &Leg,
        settings: &Settings<Date>,
        include_settlement_date_flows: Option<bool>,
        settlement_date: Option<Date>,
    ) -> QlResult<Option<Real>> {
        let Some(index) = Self::next_cash_flow(
            leg,
            settings,
            include_settlement_date_flows,
            settlement_date,
        )?
        else {
            return Ok(None);
        };
        Self::amount_on_payment_date(leg[index..].iter(), leg[index].date()).map(Some)
    }

    /// The NPV of the leg: every surviving flow discounted to `npv_date`.
    ///
    /// An empty leg is worth nothing. `settlement_date` defaults to the
    /// evaluation date and `npv_date` to `settlement_date`.
    ///
    /// # Errors
    ///
    /// Propagates the flow and curve lookups; without a `settlement_date` the
    /// evaluation date must be set.
    pub fn npv(
        leg: &Leg,
        discount_curve: &dyn YieldTermStructure,
        settings: &Settings<Date>,
        include_settlement_date_flows: Option<bool>,
        settlement_date: Option<Date>,
        npv_date: Option<Date>,
    ) -> QlResult<Real> {
        if leg.is_empty() {
            return Ok(0.0);
        }
        let (totals, discount) = Self::measure(
            leg,
            discount_curve,
            settings,
            include_settlement_date_flows,
            settlement_date,
            npv_date,
        )?;
        Ok(totals.npv / discount)
    }

    /// The change in [`npv`](Self::npv) for a uniform one-basis-point change in
    /// the rate the coupons pay. Flows that are not coupons contribute nothing.
    ///
    /// An empty leg has no sensitivity.
    ///
    /// # Errors
    ///
    /// As [`npv`](Self::npv).
    pub fn bps(
        leg: &Leg,
        discount_curve: &dyn YieldTermStructure,
        settings: &Settings<Date>,
        include_settlement_date_flows: Option<bool>,
        settlement_date: Option<Date>,
        npv_date: Option<Date>,
    ) -> QlResult<Real> {
        if leg.is_empty() {
            return Ok(0.0);
        }
        let (totals, discount) = Self::measure(
            leg,
            discount_curve,
            settings,
            include_settlement_date_flows,
            settlement_date,
            npv_date,
        )?;
        Ok(BASIS_POINT * totals.bps / discount)
    }

    /// The [`npv`](Self::npv) and the [`bps`](Self::bps), from one pass over the
    /// leg.
    ///
    /// # Errors
    ///
    /// As [`npv`](Self::npv).
    pub fn npvbps(
        leg: &Leg,
        discount_curve: &dyn YieldTermStructure,
        settings: &Settings<Date>,
        include_settlement_date_flows: Option<bool>,
        settlement_date: Option<Date>,
        npv_date: Option<Date>,
    ) -> QlResult<(Real, Real)> {
        if leg.is_empty() {
            return Ok((0.0, 0.0));
        }
        let (totals, discount) = Self::measure(
            leg,
            discount_curve,
            settings,
            include_settlement_date_flows,
            settlement_date,
            npv_date,
        )?;
        Ok((totals.npv / discount, BASIS_POINT * totals.bps / discount))
    }

    /// The fixed rate at which the leg's coupons would reach `target_npv`,
    /// taking the flows that are not coupons as given.
    ///
    /// `target_npv` is measured at `npv_date`, as [`npv`](Self::npv) is, and
    /// defaults to the leg's own NPV, which makes the result the rate that
    /// reprices the leg. An empty leg has no rate, and neither has a target of
    /// zero once the flows that do not accrue have paid for themselves.
    ///
    /// # Errors
    ///
    /// As [`npv`](Self::npv), and the leg must have some basis-point
    /// sensitivity for a rate to exist at all.
    pub fn atm_rate(
        leg: &Leg,
        discount_curve: &dyn YieldTermStructure,
        settings: &Settings<Date>,
        include_settlement_date_flows: Option<bool>,
        settlement_date: Option<Date>,
        npv_date: Option<Date>,
        target_npv: Option<Real>,
    ) -> QlResult<Rate> {
        if leg.is_empty() {
            return Ok(0.0);
        }
        let (totals, discount) = Self::measure(
            leg,
            discount_curve,
            settings,
            include_settlement_date_flows,
            settlement_date,
            npv_date,
        )?;

        let required = match target_npv {
            None => totals.npv,
            Some(target) => target * discount,
        };
        let target = required - totals.non_sens_npv;
        if target == 0.0 {
            return Ok(0.0);
        }
        require!(totals.bps != 0.0, "null bps: impossible atm rate");
        Ok(target / totals.bps)
    }

    /// The NPV of the leg discounted at one flat `yield_rate`, the internal
    /// rate of return [`solve_yield`](Self::solve_yield) inverts.
    ///
    /// Each surviving flow is discounted by the running product of the yield's
    /// discount factors over the steps between consecutive flows, rather than by
    /// its discount factor at the total time - the two agree for a compounded or
    /// continuous yield and part ways for a simple one. `settlement_date`
    /// defaults to the evaluation date and `npv_date` to `settlement_date`.
    ///
    /// A flow trading ex-coupon is discounted as a zero amount rather than
    /// dropped, so that it still advances the step the next flow discounts over.
    ///
    /// # Errors
    ///
    /// Propagates the flow lookups and the yield's compounding domain; without a
    /// `settlement_date` the evaluation date must be set.
    pub fn npv_at_yield(
        leg: &Leg,
        yield_rate: &InterestRate,
        settings: &Settings<Date>,
        include_settlement_date_flows: Option<bool>,
        settlement_date: Option<Date>,
        npv_date: Option<Date>,
    ) -> QlResult<Real> {
        if leg.is_empty() {
            return Ok(0.0);
        }
        let (settlement, npv_date) = Self::yield_dates(settings, settlement_date, npv_date)?;

        let mut npv = 0.0;
        let mut discount: DiscountFactor = 1.0;
        let mut last_date = npv_date;
        for flow in leg {
            if flow.has_occurred(settings, Some(settlement), include_settlement_date_flows)? {
                continue;
            }
            let amount = if flow.trading_ex_coupon(settings, Some(settlement))? {
                0.0
            } else {
                flow.amount()?
            };
            let step = stepwise_discount_time(
                flow.as_ref(),
                yield_rate.day_counter(),
                npv_date,
                last_date,
            );
            discount *= yield_rate.discount_factor(step)?;
            last_date = flow.date();
            npv += amount * discount;
        }
        Ok(npv)
    }

    /// The change in [`npv_at_yield`](Self::npv_at_yield) for a uniform
    /// one-basis-point change in the rate the coupons pay.
    ///
    /// As in C++, this is the discount-curve [`bps`](Self::bps) taken against a
    /// flat curve built from the yield, which discounts each flow at its own
    /// time rather than stepwise; the two paths need not agree to the last bit.
    ///
    /// # Errors
    ///
    /// As [`npv_at_yield`](Self::npv_at_yield).
    pub fn bps_at_yield(
        leg: &Leg,
        yield_rate: &InterestRate,
        settings: &Settings<Date>,
        include_settlement_date_flows: Option<bool>,
        settlement_date: Option<Date>,
        npv_date: Option<Date>,
    ) -> QlResult<Real> {
        if leg.is_empty() {
            return Ok(0.0);
        }
        let (settlement, npv_date) = Self::yield_dates(settings, settlement_date, npv_date)?;
        let flat_rate = FlatForward::with_rate(
            settlement,
            yield_rate.rate(),
            yield_rate.day_counter().clone(),
            yield_rate.compounding(),
            yield_rate.frequency(),
        );
        Self::bps(
            leg,
            &flat_rate,
            settings,
            include_settlement_date_flows,
            Some(settlement),
            Some(npv_date),
        )
    }

    /// The internal rate of return: the flat yield at which the leg is worth
    /// `npv`.
    ///
    /// Named for `CashFlows::yield`, which Rust reserves. Solved with
    /// [`NewtonSafe`] off [`npv_at_yield`](Self::npv_at_yield) and the analytic
    /// derivative `-D_modified * P`, as C++ does. `accuracy` defaults to `1e-10`,
    /// `max_iterations` to `100` and `guess` to `0.05`.
    ///
    /// # Errors
    ///
    /// As [`npv_at_yield`](Self::npv_at_yield). Also when the surviving flows
    /// never change sign against `-npv`, since then no rate reprices them, and
    /// when the solver fails to bracket or converge - in which case no rate is
    /// returned rather than the last iterate.
    #[allow(clippy::too_many_arguments)]
    pub fn solve_yield(
        leg: &Leg,
        npv: Real,
        day_counter: DayCounter,
        compounding: Compounding,
        frequency: Frequency,
        settings: &Settings<Date>,
        include_settlement_date_flows: Option<bool>,
        settlement_date: Option<Date>,
        npv_date: Option<Date>,
        accuracy: Option<Real>,
        max_iterations: Option<usize>,
        guess: Option<Rate>,
    ) -> QlResult<Rate> {
        let (settlement, npv_date) = Self::yield_dates(settings, settlement_date, npv_date)?;
        Self::check_sign(
            leg,
            npv,
            settings,
            include_settlement_date_flows,
            settlement,
        )?;

        let failure = RefCell::new(None);
        let finder = IrrFinder {
            leg,
            npv,
            day_counter,
            compounding,
            frequency,
            settings,
            include_settlement_date_flows,
            settlement,
            npv_date,
            failure: &failure,
        };
        let guess = guess.unwrap_or(0.05);
        let solver = NewtonSafe::new().with_max_evaluations(max_iterations.unwrap_or(100));
        let root = solver.solve(finder, accuracy.unwrap_or(1.0e-10), guess, guess / 10.0);

        match failure.into_inner() {
            Some(error) => Err(error),
            None => root,
        }
    }

    /// The `IrrFinder::checkSign` precondition: an IRR is nonsensical unless
    /// some surviving flow has the opposite sign to the price paid for them.
    fn check_sign(
        leg: &Leg,
        npv: Real,
        settings: &Settings<Date>,
        include_settlement_date_flows: Option<bool>,
        settlement: Date,
    ) -> QlResult<()> {
        let mut last_sign = sign(-npv);
        let mut sign_changes = 0;
        for flow in leg {
            if flow.has_occurred(settings, Some(settlement), include_settlement_date_flows)?
                || flow.trading_ex_coupon(settings, Some(settlement))?
            {
                continue;
            }
            let this_sign = sign(flow.amount()?);
            if last_sign * this_sign < 0 {
                sign_changes += 1;
            }
            if this_sign != 0 {
                last_sign = this_sign;
            }
        }
        require!(
            sign_changes > 0,
            "the given cash flows cannot result in the given market price ({npv}) due to their sign"
        );
        Ok(())
    }

    /// The duration of the leg under a flat `yield_rate`, in the convention
    /// `duration_type` names.
    ///
    /// An empty leg, and a leg whose surviving flows are worth nothing, have a
    /// duration of zero.
    ///
    /// # Errors
    ///
    /// As [`npv_at_yield`](Self::npv_at_yield), and a Macaulay duration needs a
    /// [`Compounded`](Compounding::Compounded) yield to divide the rate by a
    /// frequency.
    pub fn duration(
        leg: &Leg,
        yield_rate: &InterestRate,
        duration_type: Duration,
        settings: &Settings<Date>,
        include_settlement_date_flows: Option<bool>,
        settlement_date: Option<Date>,
        npv_date: Option<Date>,
    ) -> QlResult<Time> {
        if leg.is_empty() {
            return Ok(0.0);
        }
        let flows = Self::discounted_flows(
            leg,
            yield_rate,
            settings,
            include_settlement_date_flows,
            settlement_date,
            npv_date,
        )?;

        match duration_type {
            Duration::Simple => Ok(Self::simple_duration(&flows)),
            Duration::Modified => Ok(Self::modified_duration(&flows, yield_rate)),
            Duration::Macaulay => {
                require!(
                    yield_rate.compounding() == Compounding::Compounded,
                    "compounded rate required for a Macaulay duration"
                );
                let n = frequency_of(yield_rate);
                Ok((1.0 + yield_rate.rate() / n) * Self::modified_duration(&flows, yield_rate))
            }
        }
    }

    /// The convexity of the leg under a flat `yield_rate`: `(1 / P) d2P/dy2`.
    ///
    /// An empty leg, and a leg whose surviving flows are worth nothing, have a
    /// convexity of zero.
    ///
    /// # Errors
    ///
    /// As [`npv_at_yield`](Self::npv_at_yield).
    pub fn convexity(
        leg: &Leg,
        yield_rate: &InterestRate,
        settings: &Settings<Date>,
        include_settlement_date_flows: Option<bool>,
        settlement_date: Option<Date>,
        npv_date: Option<Date>,
    ) -> QlResult<Real> {
        if leg.is_empty() {
            return Ok(0.0);
        }
        let flows = Self::discounted_flows(
            leg,
            yield_rate,
            settings,
            include_settlement_date_flows,
            settlement_date,
            npv_date,
        )?;

        let (rate, n) = (yield_rate.rate(), frequency_of(yield_rate));
        let mut present_value = 0.0;
        let mut second_derivative = 0.0;
        for &(amount, t, discount) in &flows {
            present_value += amount * discount;
            second_derivative += amount
                * match effective_compounding(yield_rate.compounding(), t, n) {
                    Compounding::Simple => 2.0 * discount.powi(3) * t * t,
                    Compounding::Continuous => discount * t * t,
                    _ => discount * t * (n * t + 1.0) / (n * (1.0 + rate / n).powi(2)),
                };
        }
        if present_value == 0.0 {
            return Ok(0.0);
        }
        Ok(second_derivative / present_value)
    }

    /// The change in the leg's value for a one-basis-point rise in the yield,
    /// from the second-order Taylor expansion `dP = delta dy + gamma dy^2 / 2`.
    ///
    /// Negative for a leg that pays: a higher yield is worth less. The gamma
    /// term takes [`convexity`](Self::convexity) divided by 100, as C++ does -
    /// which makes it a hundredth of the second-order term the expansion asks
    /// for, and immaterial at a one-basis-point shift.
    ///
    /// # Errors
    ///
    /// As [`npv_at_yield`](Self::npv_at_yield).
    pub fn basis_point_value(
        leg: &Leg,
        yield_rate: &InterestRate,
        settings: &Settings<Date>,
        include_settlement_date_flows: Option<bool>,
        settlement_date: Option<Date>,
        npv_date: Option<Date>,
    ) -> QlResult<Real> {
        if leg.is_empty() {
            return Ok(0.0);
        }
        let npv = Self::npv_at_yield(
            leg,
            yield_rate,
            settings,
            include_settlement_date_flows,
            settlement_date,
            npv_date,
        )?;
        let modified = Self::duration(
            leg,
            yield_rate,
            Duration::Modified,
            settings,
            include_settlement_date_flows,
            settlement_date,
            npv_date,
        )?;
        let convexity = Self::convexity(
            leg,
            yield_rate,
            settings,
            include_settlement_date_flows,
            settlement_date,
            npv_date,
        )?;

        let shift = 0.0001;
        let delta = -modified * npv * shift;
        let gamma = (convexity / 100.0) * npv * shift * shift;
        Ok(delta + 0.5 * gamma)
    }

    /// The yield move a one-cent move in the leg's value implies:
    /// `(dy / dP) * 0.01`, to first order.
    ///
    /// # Errors
    ///
    /// As [`npv_at_yield`](Self::npv_at_yield).
    pub fn yield_value_basis_point(
        leg: &Leg,
        yield_rate: &InterestRate,
        settings: &Settings<Date>,
        include_settlement_date_flows: Option<bool>,
        settlement_date: Option<Date>,
        npv_date: Option<Date>,
    ) -> QlResult<Real> {
        if leg.is_empty() {
            return Ok(0.0);
        }
        let npv = Self::npv_at_yield(
            leg,
            yield_rate,
            settings,
            include_settlement_date_flows,
            settlement_date,
            npv_date,
        )?;
        let modified = Self::duration(
            leg,
            yield_rate,
            Duration::Modified,
            settings,
            include_settlement_date_flows,
            settlement_date,
            npv_date,
        )?;

        let shift = 0.01;
        Ok(shift / (-npv * modified))
    }

    /// `sum(t c B) / sum(c B)`, the discounted-time average of the flows.
    fn simple_duration(flows: &[(Real, Time, DiscountFactor)]) -> Time {
        let mut present_value = 0.0;
        let mut derivative = 0.0;
        for &(amount, t, discount) in flows {
            present_value += amount * discount;
            derivative += t * amount * discount;
        }
        if present_value == 0.0 {
            return 0.0;
        }
        derivative / present_value
    }

    /// `-(1 / P) dP/dy`, differentiating each flow's discount factor in the
    /// yield's own compounding convention.
    fn modified_duration(
        flows: &[(Real, Time, DiscountFactor)],
        yield_rate: &InterestRate,
    ) -> Time {
        let (rate, n) = (yield_rate.rate(), frequency_of(yield_rate));
        let mut present_value = 0.0;
        let mut derivative = 0.0;
        for &(amount, t, discount) in flows {
            present_value += amount * discount;
            derivative -= amount
                * match effective_compounding(yield_rate.compounding(), t, n) {
                    Compounding::Simple => discount * discount * t,
                    Compounding::Continuous => discount * t,
                    _ => t * discount / (1.0 + rate / n),
                };
        }
        if present_value == 0.0 {
            return 0.0;
        }
        -derivative / present_value
    }

    /// Every surviving flow as `(amount, time from the NPV date, discount
    /// factor)`, the one pass [`duration`](Self::duration) and
    /// [`convexity`](Self::convexity) differentiate.
    ///
    /// The times accumulate the same steps [`npv_at_yield`](Self::npv_at_yield)
    /// discounts over, but the discount factor is taken at the total time rather
    /// than chained - which is what C++ does, and what makes the two agree only
    /// for a compounded or continuous yield.
    fn discounted_flows(
        leg: &Leg,
        yield_rate: &InterestRate,
        settings: &Settings<Date>,
        include_settlement_date_flows: Option<bool>,
        settlement_date: Option<Date>,
        npv_date: Option<Date>,
    ) -> QlResult<Vec<(Real, Time, DiscountFactor)>> {
        let (settlement, npv_date) = Self::yield_dates(settings, settlement_date, npv_date)?;
        let mut flows = Vec::with_capacity(leg.len());
        let mut t = 0.0;
        let mut last_date = npv_date;
        for flow in leg {
            if flow.has_occurred(settings, Some(settlement), include_settlement_date_flows)? {
                continue;
            }
            let amount = if flow.trading_ex_coupon(settings, Some(settlement))? {
                0.0
            } else {
                flow.amount()?
            };
            t += stepwise_discount_time(
                flow.as_ref(),
                yield_rate.day_counter(),
                npv_date,
                last_date,
            );
            flows.push((amount, t, yield_rate.discount_factor(t)?));
            last_date = flow.date();
        }
        Ok(flows)
    }

    /// The settlement and NPV dates the yield analytics resolve up front, each
    /// defaulting to the one before it.
    fn yield_dates(
        settings: &Settings<Date>,
        settlement_date: Option<Date>,
        npv_date: Option<Date>,
    ) -> QlResult<(Date, Date)> {
        let settlement = reference_date(settings, settlement_date)?;
        Ok((settlement, npv_date.unwrap_or(settlement)))
    }

    /// The single pass the analytics share, and the discount factor at the NPV
    /// date they all divide by.
    fn measure(
        leg: &Leg,
        discount_curve: &dyn YieldTermStructure,
        settings: &Settings<Date>,
        include_settlement_date_flows: Option<bool>,
        settlement_date: Option<Date>,
        npv_date: Option<Date>,
    ) -> QlResult<(Totals, DiscountFactor)> {
        let settlement = reference_date(settings, settlement_date)?;
        let npv_date = npv_date.unwrap_or(settlement);
        let totals = Self::totals(
            leg,
            discount_curve,
            settings,
            include_settlement_date_flows,
            settlement,
        )?;
        Ok((totals, discount_curve.discount_date(npv_date, false)?))
    }

    fn totals(
        leg: &Leg,
        discount_curve: &dyn YieldTermStructure,
        settings: &Settings<Date>,
        include_settlement_date_flows: Option<bool>,
        settlement: Date,
    ) -> QlResult<Totals> {
        let settlement = Some(settlement);
        let mut totals = Totals::default();
        for flow in leg {
            if flow.has_occurred(settings, settlement, include_settlement_date_flows)?
                || flow.trading_ex_coupon(settings, settlement)?
            {
                continue;
            }
            let discount = discount_curve.discount_date(flow.date(), false)?;
            let amount = flow.amount()? * discount;
            totals.npv += amount;
            match flow.as_coupon() {
                Some(coupon) => totals.bps += coupon.nominal() * coupon.accrual_period() * discount,
                None => totals.non_sens_npv += amount,
            }
        }
        Ok(totals)
    }

    fn accrual_start_or_payment(flow: &dyn CashFlow) -> Date {
        match flow.as_coupon() {
            Some(coupon) => coupon.accrual_start_date(),
            None => flow.date(),
        }
    }

    fn accrual_end_or_payment(flow: &dyn CashFlow) -> Date {
        match flow.as_coupon() {
            Some(coupon) => coupon.accrual_end_date(),
            None => flow.date(),
        }
    }

    fn amount_on_payment_date<'a>(
        flows: impl Iterator<Item = &'a Shared<dyn CashFlow>>,
        payment_date: Date,
    ) -> QlResult<Real> {
        let mut total = 0.0;
        for flow in flows.take_while(|flow| flow.date() == payment_date) {
            total += flow.amount()?;
        }
        Ok(total)
    }
}

/// The yield's compounding frequency as the `Natural N` the derivative formulas
/// divide by.
///
/// A yield that compounds nothing reports `Frequency::NoFrequency`, whose `-1`
/// no formula may see; the branches that use `N` are exactly the ones whose
/// convention carries a frequency, so those never reach it.
fn frequency_of(yield_rate: &InterestRate) -> Real {
    yield_rate.frequency() as i16 as Real
}

/// The convention a hybrid yield actually applies at time `t`, so the
/// derivative formulas need only the three pure cases.
fn effective_compounding(compounding: Compounding, t: Time, n: Real) -> Compounding {
    match compounding {
        Compounding::SimpleThenCompounded if t <= 1.0 / n => Compounding::Simple,
        Compounding::CompoundedThenSimple if t > 1.0 / n => Compounding::Simple,
        Compounding::SimpleThenCompounded | Compounding::CompoundedThenSimple => {
            Compounding::Compounded
        }
        other => other,
    }
}

/// The time a flow's own discount step spans, from `last_date` to its payment
/// date (`getStepwiseDiscountTime` of `cashflows.cpp`).
///
/// A coupon measures the step in its own reference period, and as the remainder
/// of its accrual once `last_date` has moved off its accrual start - the shape
/// that makes a schedule-driven `ActualActual(ISMA)` come out right. Anything
/// else has no reference period, so the step gets one: the interval it is
/// discounted over, faked as the year before it when there is no previous flow
/// to start from.
fn stepwise_discount_time(
    flow: &dyn CashFlow,
    day_counter: &DayCounter,
    npv_date: Date,
    last_date: Date,
) -> Time {
    let payment_date = flow.date();
    let coupon = flow.as_coupon();
    let (ref_start, ref_end) = match coupon {
        Some(coupon) => (
            coupon.reference_period_start(),
            coupon.reference_period_end(),
        ),
        None if last_date == npv_date => {
            (payment_date - Period::new(1, TimeUnit::Years), payment_date)
        }
        None => (last_date, payment_date),
    };

    match coupon {
        Some(coupon) if last_date != coupon.accrual_start_date() => {
            let start = coupon.accrual_start_date();
            day_counter.year_fraction_ref(start, payment_date, ref_start, ref_end)
                - day_counter.year_fraction_ref(start, last_date, ref_start, ref_end)
        }
        _ => day_counter.year_fraction_ref(last_date, payment_date, ref_start, ref_end),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cashflows::fixedratecoupon::FixedRateCoupon;
    use crate::cashflows::simplecashflow::{Redemption, SimpleCashFlow};
    use crate::shared::shared;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;

    fn today() -> Date {
        Date::new(7, Month::July, 2026)
    }

    fn accrual_start() -> Date {
        Date::new(15, Month::January, 2026)
    }

    fn accrual_end() -> Date {
        Date::new(15, Month::July, 2026)
    }

    fn payment() -> Date {
        Date::new(20, Month::July, 2026)
    }

    fn early_payment() -> Date {
        Date::new(10, Month::January, 2026)
    }

    fn coupon_amount() -> Real {
        100.0 * 0.03 * 181.0 / 360.0
    }

    /// An early plain flow, a coupon accruing across the evaluation date, and a
    /// redemption sharing the coupon's payment date.
    fn leg() -> Leg {
        vec![
            shared(SimpleCashFlow::new(5.0, early_payment())) as Shared<dyn CashFlow>,
            shared(FixedRateCoupon::from_rate(
                payment(),
                100.0,
                0.03,
                Actual360::new(),
                accrual_start(),
                accrual_end(),
                None,
                None,
                None,
            )) as Shared<dyn CashFlow>,
            shared(Redemption::new(100.0, payment())) as Shared<dyn CashFlow>,
        ]
    }

    fn settings() -> Settings<Date> {
        let settings = Settings::new();
        settings.set_evaluation_date(today());
        settings
    }

    /// The coupon is read through its accrual dates and the plain flows through
    /// their payment dates, so the leg starts before its first payment and
    /// matures after its last accrual.
    #[test]
    fn the_leg_spans_the_accrual_dates_of_its_coupons() {
        let leg = leg();

        assert_eq!(CashFlows::start_date(&leg).unwrap(), early_payment());
        assert_eq!(CashFlows::maturity_date(&leg).unwrap(), payment());
    }

    #[test]
    fn an_empty_leg_has_no_dates() {
        let leg = Leg::new();

        assert!(CashFlows::start_date(&leg).is_err());
        assert!(CashFlows::maturity_date(&leg).is_err());
    }

    #[test]
    fn the_previous_and_next_flows_straddle_the_settlement_date() {
        let (leg, settings) = (leg(), settings());
        let at = |date| {
            (
                CashFlows::previous_cash_flow(&leg, &settings, None, date).unwrap(),
                CashFlows::next_cash_flow(&leg, &settings, None, date).unwrap(),
            )
        };

        assert_eq!(at(None), (Some(0), Some(1)));
        assert_eq!(at(Some(early_payment() - 1)), (None, Some(0)));
        assert_eq!(at(Some(payment() + 1)), (Some(2), None));
    }

    #[test]
    fn the_flow_dates_follow_the_flows_they_are_read_off() {
        let (leg, settings) = (leg(), settings());
        let previous = |date| CashFlows::previous_cash_flow_date(&leg, &settings, None, date);
        let next = |date| CashFlows::next_cash_flow_date(&leg, &settings, None, date);

        assert_eq!(previous(None).unwrap(), Some(early_payment()));
        assert_eq!(next(None).unwrap(), Some(payment()));
        assert_eq!(previous(Some(early_payment() - 1)).unwrap(), None);
        assert_eq!(next(Some(payment() + 1)).unwrap(), None);
    }

    /// Both amounts sum every flow sharing the payment date they land on: the
    /// coupon and the redemption pay together.
    #[test]
    fn the_amounts_sum_the_flows_sharing_a_payment_date() {
        let (leg, settings) = (leg(), settings());
        let previous = |date| CashFlows::previous_cash_flow_amount(&leg, &settings, None, date);
        let next = |date| CashFlows::next_cash_flow_amount(&leg, &settings, None, date);

        assert_eq!(previous(None).unwrap(), Some(5.0));
        assert_eq!(previous(Some(early_payment() - 1)).unwrap(), None);
        assert_eq!(next(Some(payment() + 1)).unwrap(), None);

        let both = next(None).unwrap().unwrap();
        assert!((both - (coupon_amount() + 100.0)).abs() < 1e-13);
    }

    /// Without a settlement date the flows are measured against the evaluation
    /// date, which the port refuses to invent (D10).
    #[test]
    fn an_unset_evaluation_date_is_an_error() {
        let (leg, settings) = (leg(), Settings::new());

        assert!(CashFlows::previous_cash_flow(&leg, &settings, None, None).is_err());
        assert!(CashFlows::next_cash_flow(&leg, &settings, None, None).is_err());
    }
}

#[cfg(test)]
mod analytics_tests {
    use super::*;
    use crate::cashflows::fixedratecoupon::FixedRateCoupon;
    use crate::cashflows::fixedrateleg::FixedRateLeg;
    use crate::cashflows::simplecashflow::{Redemption, SimpleCashFlow};
    use crate::interestrate::{Compounding, InterestRate};
    use crate::shared::shared;
    use crate::termstructures::yields::FlatForward;
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendars::nullcalendar::NullCalendar;
    use crate::time::date::{Month, Year};
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;
    use crate::time::frequency::Frequency;
    use crate::time::schedule::MakeSchedule;
    use crate::types::Rate;

    const NOMINAL: Real = 100.0;
    const RATE: Rate = 0.05;
    const FORWARD: Rate = 0.03;

    fn day(month: Month, year: Year) -> Date {
        Date::new(15, month, year)
    }

    fn today() -> Date {
        day(Month::January, 2026)
    }

    fn maturity() -> Date {
        day(Month::January, 2028)
    }

    fn settings() -> Settings<Date> {
        let settings = Settings::new();
        settings.set_evaluation_date(today());
        settings
    }

    /// A flat continuously-compounded curve on `Actual365Fixed`, so that
    /// `df(d) = exp(-FORWARD * (d - today) / 365)` by hand.
    fn curve(forward: Rate) -> FlatForward {
        FlatForward::with_rate(
            today(),
            forward,
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )
    }

    /// The discount factor of [`curve`], from the definition rather than off
    /// the curve.
    fn df(date: Date) -> Real {
        (-FORWARD * f64::from(date - today()) / 365.0).exp()
    }

    /// The `Actual360` accrual period of a coupon, by hand.
    fn accrual(start: Date, end: Date) -> Real {
        f64::from(end - start) / 360.0
    }

    fn simple(rate: Rate) -> InterestRate {
        InterestRate::new(
            rate,
            Actual360::new(),
            Compounding::Simple,
            Frequency::Annual,
        )
        .unwrap()
    }

    /// A simple yield on `Actual365Fixed`, the day counter [`df`] is written in.
    fn simple_365(rate: Rate) -> InterestRate {
        InterestRate::new(
            rate,
            Actual365Fixed::new(),
            Compounding::Simple,
            Frequency::NoFrequency,
        )
        .unwrap()
    }

    /// The accrual periods of [`fixed_leg`], which pays on its accrual ends.
    fn periods() -> [(Date, Date); 4] {
        [
            (today(), day(Month::July, 2026)),
            (day(Month::July, 2026), day(Month::January, 2027)),
            (day(Month::January, 2027), day(Month::July, 2027)),
            (day(Month::July, 2027), maturity()),
        ]
    }

    /// Four semiannual `Actual360` coupons on an unadjusted schedule.
    fn fixed_leg(rate: Rate) -> Leg {
        let schedule = MakeSchedule::new()
            .from(today())
            .to(maturity())
            .with_frequency(Frequency::Semiannual)
            .with_calendar(NullCalendar::new())
            .with_convention(BusinessDayConvention::Unadjusted)
            .backwards()
            .build();
        FixedRateLeg::new(schedule)
            .with_notional(NOMINAL)
            .with_interest_rate(simple(rate))
            .build()
            .unwrap()
    }

    /// The three unit flows the `CHECK_NPV` block of
    /// `cashflows.cpp::testSettings` (:157-179) prices, on today and the two
    /// days after.
    fn unit_leg() -> Leg {
        (0..3)
            .map(|i| shared(SimpleCashFlow::new(1.0, today() + i)) as Shared<dyn CashFlow>)
            .collect()
    }

    /// A direct port of the `CHECK_NPV` block of `cashflows.cpp::testSettings`
    /// (:157-179), which prices the leg off `InterestRate(0.0,
    /// Actual365Fixed(), Continuous, Annual)` - "no discount to make
    /// calculations easier" - and expects 2.0 / 3.0 / 2.0 / 2.0 across the
    /// `includeTodaysCashFlows` matrix.
    #[test]
    fn the_npv_at_yield_counts_the_flows_the_settlement_date_rule_admits() {
        let settings = settings();
        let leg = unit_leg();
        let no_discount = InterestRate::new(
            0.0,
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )
        .unwrap();
        let npv = |include| {
            CashFlows::npv_at_yield(
                &leg,
                &no_discount,
                &settings,
                Some(include),
                Some(today()),
                None,
            )
            .unwrap()
        };

        settings.set_include_todays_cash_flows(None);
        assert_eq!(npv(false), 2.0);
        assert_eq!(npv(true), 3.0);

        settings.set_include_todays_cash_flows(Some(false));
        assert_eq!(npv(false), 2.0);
        assert_eq!(npv(true), 2.0);
    }

    /// The same `CHECK_NPV` block of `cashflows.cpp::testSettings` (:157-179),
    /// adapted onto the `YieldTermStructure` overload, which C++ does not call
    /// there. Every discount factor of the C++ `InterestRate(0.0, ...,
    /// Continuous, Annual)` is 1.0, and a flat 0% `FlatForward` gives the same
    /// factors, so the expected values carry over unchanged. The direct port
    /// lives in `the_npv_at_yield_counts_the_flows_the_settlement_date_rule_admits`.
    #[test]
    fn the_npv_counts_the_flows_the_settlement_date_rule_admits() {
        let (settings, curve) = (settings(), curve(0.0));
        let leg = unit_leg();
        let npv = |include| {
            CashFlows::npv(&leg, &curve, &settings, Some(include), Some(today()), None).unwrap()
        };

        settings.set_include_todays_cash_flows(None);
        assert_eq!(npv(false), 2.0);
        assert_eq!(npv(true), 3.0);

        settings.set_include_todays_cash_flows(Some(false));
        assert_eq!(npv(false), 2.0);
        assert_eq!(npv(true), 2.0);
    }

    /// `npv = sum(amount * df(date)) / df(npv_date)`, with the amounts and the
    /// discount factors hand-computed from the coupon and curve definitions.
    #[test]
    fn the_npv_discounts_every_flow_and_then_the_npv_date() {
        let (settings, curve, leg) = (settings(), curve(FORWARD), fixed_leg(RATE));
        let expected: Real = periods()
            .iter()
            .map(|&(start, end)| NOMINAL * RATE * accrual(start, end) * df(end))
            .sum();
        let npv = |npv_date| CashFlows::npv(&leg, &curve, &settings, None, None, npv_date).unwrap();

        assert!((npv(None) - expected).abs() < 1e-12);
        assert!((npv(Some(maturity())) - expected / df(maturity())).abs() < 1e-12);
    }

    /// `bps = 1e-4 * sum(nominal * accrual_period * df(date)) / df(npv_date)`,
    /// hand-computed from the same definitions.
    #[test]
    fn the_bps_sums_the_discounted_accruals_of_the_coupons() {
        let (settings, curve, leg) = (settings(), curve(FORWARD), fixed_leg(RATE));
        let expected: Real = 1.0e-4
            * periods()
                .iter()
                .map(|&(start, end)| NOMINAL * accrual(start, end) * df(end))
                .sum::<Real>();

        let bps = CashFlows::bps(&leg, &curve, &settings, None, None, None).unwrap();
        assert!((bps - expected).abs() < 1e-14);
    }

    /// The definition of a basis-point sensitivity: the change in NPV when the
    /// coupons pay one basis point more. A `Simple` coupon is linear in its
    /// rate, so the finite difference is exact but for the cancellation in the
    /// `(1 + rate * accrual) - 1` its compound factor goes through.
    #[test]
    fn the_bps_is_the_npv_change_for_a_one_basis_point_coupon_spread() {
        let (settings, curve) = (settings(), curve(FORWARD));
        let npv =
            |rate| CashFlows::npv(&fixed_leg(rate), &curve, &settings, None, None, None).unwrap();

        let bumped = npv(RATE + 1.0e-4) - npv(RATE);
        let bps = CashFlows::bps(&fixed_leg(RATE), &curve, &settings, None, None, None).unwrap();
        assert!((bumped - bps).abs() < 1e-12);
    }

    /// A coupon trading ex-coupon is dropped though it has not been paid: the
    /// analytics filter on `has_occurred` *and* on `trading_ex_coupon`. The
    /// same coupon without an ex-coupon date is the control, without which a
    /// filter that dropped every flow would pass this.
    #[test]
    fn a_flow_trading_ex_coupon_is_left_out() {
        let (settings, curve) = (settings(), curve(FORWARD));
        let (start, end) = periods()[0];
        let leg = |ex_coupon_date| {
            vec![shared(FixedRateCoupon::new(
                end,
                NOMINAL,
                simple(RATE),
                start,
                end,
                None,
                None,
                ex_coupon_date,
            )) as Shared<dyn CashFlow>]
        };
        let npv = |leg: &Leg| CashFlows::npv(leg, &curve, &settings, None, None, None).unwrap();
        let bps = |leg: &Leg| CashFlows::bps(leg, &curve, &settings, None, None, None).unwrap();

        let paying = leg(None);
        assert!((npv(&paying) - NOMINAL * RATE * accrual(start, end) * df(end)).abs() < 1e-13);
        assert!(bps(&paying) > 0.0);

        let ex_coupon = leg(Some(today()));
        assert_eq!(npv(&ex_coupon), 0.0);
        assert_eq!(bps(&ex_coupon), 0.0);
    }

    /// The empty-leg short circuit comes before the settlement date is
    /// resolved, so it stands with no evaluation date set.
    #[test]
    fn an_empty_leg_is_worth_nothing() {
        let (settings, curve, leg) = (Settings::new(), curve(FORWARD), Leg::new());

        assert_eq!(
            CashFlows::npv(&leg, &curve, &settings, None, None, None).unwrap(),
            0.0
        );
        assert_eq!(
            CashFlows::bps(&leg, &curve, &settings, None, None, None).unwrap(),
            0.0
        );
    }

    #[test]
    fn npvbps_returns_the_npv_and_the_bps() {
        let (settings, curve, leg) = (settings(), curve(FORWARD), fixed_leg(RATE));
        let at = Some(day(Month::January, 2027));

        let (npv, bps) = CashFlows::npvbps(&leg, &curve, &settings, None, None, at).unwrap();
        assert_eq!(
            npv,
            CashFlows::npv(&leg, &curve, &settings, None, None, at).unwrap()
        );
        assert_eq!(
            bps,
            CashFlows::bps(&leg, &curve, &settings, None, None, at).unwrap()
        );
    }

    /// With no target the ATM rate reprices the leg, so a leg of coupons all
    /// paying one rate is already at the money. The redemption is not a coupon:
    /// it lands in `nonSensNPV`, is subtracted from the target, and so leaves
    /// the rate alone. Passing the leg's own NPV as the target is that same
    /// computation with the `df(npv_date)` division undone.
    #[test]
    fn the_atm_rate_reprices_the_leg_and_ignores_the_flows_that_do_not_accrue() {
        let (settings, curve) = (settings(), curve(FORWARD));
        let mut leg = fixed_leg(RATE);
        let atm = |leg: &Leg, target| {
            CashFlows::atm_rate(leg, &curve, &settings, None, None, None, target).unwrap()
        };

        assert!((atm(&leg, None) - RATE).abs() < 1e-14);

        leg.push(shared(Redemption::new(NOMINAL, maturity())) as Shared<dyn CashFlow>);
        assert!((atm(&leg, None) - RATE).abs() < 1e-14);

        let npv = CashFlows::npv(&leg, &curve, &settings, None, None, None).unwrap();
        assert!((atm(&leg, Some(npv)) - RATE).abs() < 1e-14);
    }

    /// A target NPV is quoted at the NPV date, so `atm_rate` scales it back up
    /// by `df(npv_date)` before dividing by the sensitivity. Every other test
    /// leaves `npv_date` at the settlement date, where that factor is 1.0 and a
    /// missing multiplication would go unseen.
    #[test]
    fn a_target_npv_is_scaled_by_the_discount_at_the_npv_date() {
        let (settings, curve) = (settings(), curve(FORWARD));
        let leg = fixed_leg(RATE);
        let npv_date = maturity();
        let discount = curve.discount_date(npv_date, false).unwrap();
        assert!((discount - 1.0).abs() > 0.05);

        let npv = CashFlows::npv(&leg, &curve, &settings, None, None, Some(npv_date)).unwrap();
        let atm = CashFlows::atm_rate(
            &leg,
            &curve,
            &settings,
            None,
            None,
            Some(npv_date),
            Some(npv),
        )
        .unwrap();

        assert!((atm - RATE).abs() < 1e-14);
    }

    /// A target NPV of zero short-circuits before the sensitivity is consulted;
    /// a leg with no coupon at all has none to consult.
    #[test]
    fn the_atm_rate_needs_a_target_and_a_sensitivity() {
        let (settings, curve) = (settings(), curve(FORWARD));
        let leg = fixed_leg(RATE);
        let bare: Leg = vec![shared(Redemption::new(NOMINAL, maturity())) as Shared<dyn CashFlow>];
        let atm = |leg: &Leg, target| {
            CashFlows::atm_rate(leg, &curve, &settings, None, None, None, target)
        };

        assert_eq!(atm(&leg, Some(0.0)).unwrap(), 0.0);
        assert_eq!(atm(&bare, None).unwrap(), 0.0);
        assert!(atm(&bare, Some(0.0)).is_err());
    }

    /// An `Actual365Fixed` yield on the `Actual360` leg. Every step of
    /// [`fixed_leg`] runs from one coupon's accrual start to its payment date,
    /// so the times compound to `A365(npv_date, payment_date)` and the discount
    /// factors are `exp(-y * t)` by hand. The NPV date is a month past the
    /// settlement date, where that factor is not 1.0.
    #[test]
    fn the_npv_at_yield_discounts_each_flow_over_its_own_step() {
        let (settings, leg) = (settings(), fixed_leg(RATE));
        let npv_date = today() + 30;
        let yield_rate = InterestRate::new(
            FORWARD,
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )
        .unwrap();
        let expected: Real = periods()
            .iter()
            .map(|&(start, end)| {
                let t = f64::from(end - npv_date) / 365.0;
                NOMINAL * RATE * accrual(start, end) * (-FORWARD * t).exp()
            })
            .sum();

        let npv = CashFlows::npv_at_yield(&leg, &yield_rate, &settings, None, None, Some(npv_date))
            .unwrap();
        assert!((npv - expected).abs() < 1e-12);
        assert!(
            (npv - CashFlows::npv_at_yield(&leg, &yield_rate, &settings, None, None, None)
                .unwrap())
            .abs()
                > 1e-3
        );
    }

    /// The discount is the running product of the per-step factors, not the
    /// factor at the total time. A simple yield tells the two apart: chaining
    /// its factors compounds them, so `prod (1 + y * step_i)` outgrows
    /// `1 + y * sum(step_i)` and the stepwise NPV comes out the smaller.
    #[test]
    fn the_npv_at_yield_compounds_the_steps_rather_than_the_total_time() {
        let (settings, leg) = (settings(), fixed_leg(RATE));
        let yield_rate = simple_365(FORWARD);
        let mut stepwise = 0.0;
        let mut flat = 0.0;
        let mut discount = 1.0;
        let mut last = today();
        for (start, end) in periods() {
            let amount = NOMINAL * RATE * accrual(start, end);
            discount /= 1.0 + FORWARD * f64::from(end - last) / 365.0;
            stepwise += amount * discount;
            flat += amount / (1.0 + FORWARD * f64::from(end - today()) / 365.0);
            last = end;
        }
        assert!(flat - stepwise > 1e-3);

        let npv = CashFlows::npv_at_yield(&leg, &yield_rate, &settings, None, None, None).unwrap();
        assert!((npv - stepwise).abs() < 1e-12);
    }

    /// C++ routes `bps(leg, InterestRate)` through a `FlatForward` built from
    /// the yield at the settlement date, so the yield form is the curve form on
    /// that curve, to the bit.
    #[test]
    fn the_bps_at_yield_is_the_bps_off_a_flat_curve_of_the_yield() {
        let (settings, leg) = (settings(), fixed_leg(RATE));
        let yield_rate = simple_365(FORWARD);
        let flat = FlatForward::with_rate(
            today(),
            FORWARD,
            Actual365Fixed::new(),
            Compounding::Simple,
            Frequency::NoFrequency,
        );

        let at_yield =
            CashFlows::bps_at_yield(&leg, &yield_rate, &settings, None, None, None).unwrap();
        assert_eq!(
            at_yield,
            CashFlows::bps(&leg, &flat, &settings, None, None, None).unwrap()
        );
        assert!(at_yield > 0.0);
    }

    /// Both yield analytics short-circuit an empty leg before resolving the
    /// settlement date, so they stand with no evaluation date set.
    #[test]
    fn an_empty_leg_has_no_yield_analytics() {
        let (settings, leg) = (Settings::new(), Leg::new());
        let yield_rate = simple_365(FORWARD);

        assert_eq!(
            CashFlows::npv_at_yield(&leg, &yield_rate, &settings, None, None, None).unwrap(),
            0.0
        );
        assert_eq!(
            CashFlows::bps_at_yield(&leg, &yield_rate, &settings, None, None, None).unwrap(),
            0.0
        );
        assert_eq!(
            CashFlows::duration(
                &leg,
                &yield_rate,
                Duration::Simple,
                &settings,
                None,
                None,
                None
            )
            .unwrap(),
            0.0
        );
        assert_eq!(
            CashFlows::convexity(&leg, &yield_rate, &settings, None, None, None).unwrap(),
            0.0
        );
    }

    /// A semiannually compounded yield, the convention `bonds.cpp` quotes and
    /// the only one a Macaulay duration is defined for.
    fn compounded(rate: Rate) -> InterestRate {
        InterestRate::new(
            rate,
            Actual365Fixed::new(),
            Compounding::Compounded,
            Frequency::Semiannual,
        )
        .unwrap()
    }

    /// The modified duration is `-(1 / P) dP/dy` by definition, so it must agree
    /// with a central finite difference of [`CashFlows::npv_at_yield`] in the
    /// yield. The two code paths share nothing but the flow times: `npv_at_yield`
    /// chains per-step discount factors, `duration` differentiates a closed form.
    #[test]
    fn the_modified_duration_is_the_finite_difference_of_the_npv() {
        let (settings, leg) = (settings(), fixed_leg(RATE));
        let h = 1.0e-6;
        let npv =
            |y| CashFlows::npv_at_yield(&leg, &compounded(y), &settings, None, None, None).unwrap();
        let expected = -(npv(FORWARD + h) - npv(FORWARD - h)) / (2.0 * h) / npv(FORWARD);

        let modified = CashFlows::duration(
            &leg,
            &compounded(FORWARD),
            Duration::Modified,
            &settings,
            None,
            None,
            None,
        )
        .unwrap();
        assert!(modified > 0.9);
        assert!((modified - expected).abs() < 1e-7);
    }

    /// The convexity is `(1 / P) d2P/dy2`, so it must agree with a central
    /// second difference of the NPV. The step is loose because that difference
    /// loses half its digits to cancellation.
    #[test]
    fn the_convexity_is_the_second_finite_difference_of_the_npv() {
        let (settings, leg) = (settings(), fixed_leg(RATE));
        let h = 1.0e-4;
        let npv =
            |y| CashFlows::npv_at_yield(&leg, &compounded(y), &settings, None, None, None).unwrap();
        let expected =
            (npv(FORWARD + h) - 2.0 * npv(FORWARD) + npv(FORWARD - h)) / (h * h) / npv(FORWARD);

        let convexity =
            CashFlows::convexity(&leg, &compounded(FORWARD), &settings, None, None, None).unwrap();
        assert!(convexity > 1.0);
        assert!((convexity - expected).abs() < 1e-5);
    }

    /// `D_Macaulay = (1 + y / N) D_modified`, and it is defined for no other
    /// compounding convention.
    #[test]
    fn the_macaulay_duration_scales_the_modified_one_and_needs_a_compounded_yield() {
        let (settings, leg) = (settings(), fixed_leg(RATE));
        let duration = |rate: &InterestRate, kind| {
            CashFlows::duration(&leg, rate, kind, &settings, None, None, None)
        };
        let (compounded, simple) = (compounded(FORWARD), simple_365(FORWARD));

        let modified = duration(&compounded, Duration::Modified).unwrap();
        let macaulay = duration(&compounded, Duration::Macaulay).unwrap();
        assert!((macaulay - (1.0 + FORWARD / 2.0) * modified).abs() < 1e-15);
        assert!(macaulay > modified);

        assert!(duration(&simple, Duration::Macaulay).is_err());
        assert!(duration(&simple, Duration::Modified).is_ok());
    }

    /// A leg of coupons and a redemption, whose flows the settlement date does
    /// not outlive, so an IRR exists against a positive price.
    fn redeeming_leg() -> Leg {
        let mut leg = fixed_leg(RATE);
        leg.push(shared(Redemption::new(NOMINAL, maturity())) as Shared<dyn CashFlow>);
        leg
    }

    /// The IRR inverts [`CashFlows::npv_at_yield`]: solving for the NPV the leg
    /// has at a known yield gives that yield back, and the solved yield reprices
    /// the leg. The NPV date is a month past the settlement date, so the
    /// objective and its derivative must agree on where time starts.
    #[test]
    fn the_solved_yield_reprices_the_leg() {
        let (settings, leg) = (settings(), redeeming_leg());
        let npv_date = Some(today() + 30);
        let rate = compounded(FORWARD);
        let target = CashFlows::npv_at_yield(&leg, &rate, &settings, None, None, npv_date).unwrap();

        let irr = CashFlows::solve_yield(
            &leg,
            target,
            Actual365Fixed::new(),
            Compounding::Compounded,
            Frequency::Semiannual,
            &settings,
            None,
            None,
            npv_date,
            None,
            None,
            None,
        )
        .unwrap();
        assert!((irr - FORWARD).abs() < 1e-10);

        let repriced =
            CashFlows::npv_at_yield(&leg, &compounded(irr), &settings, None, None, npv_date)
                .unwrap();
        assert!((repriced - target).abs() < 1e-9);
    }

    /// Every way the solve can fail returns an error, never a number: an NPV no
    /// sign change can reach, an evaluation budget too small to bracket the
    /// root, and a yield convention [`InterestRate::new`] rejects, whose failure
    /// is parked mid-solve and reported in place of the solver's own verdict.
    ///
    /// Each failure asserts on the message, because each of these legs also
    /// fails to bracket a root: `is_err()` alone would still hold with the sign
    /// check and the parked failure both deleted.
    #[test]
    fn the_yield_solver_errors_rather_than_returning_a_partial_answer() {
        let (settings, leg) = (settings(), redeeming_leg());
        let solve = |npv, frequency, max_iterations| {
            CashFlows::solve_yield(
                &leg,
                npv,
                Actual365Fixed::new(),
                Compounding::Compounded,
                frequency,
                &settings,
                None,
                None,
                None,
                None,
                max_iterations,
                None,
            )
        };

        let message = |npv, frequency, max_iterations| {
            solve(npv, frequency, max_iterations)
                .expect_err("the solve was meant to fail")
                .to_string()
        };

        assert!(solve(100.0, Frequency::Semiannual, None).is_ok());
        assert!(message(-100.0, Frequency::Semiannual, None).contains("due to their sign"));
        assert!(message(100.0, Frequency::Semiannual, Some(1)).contains("unable to bracket"));
        assert!(message(100.0, Frequency::Once, None).contains("frequency"));
    }

    /// **Hand-derived.** The C++ test-suite reaches `CashFlows::basisPointValue`
    /// only through `BondFunctions::basisPointValue`, which adds a tradability
    /// guard and delegates at `bondfunctions.cpp:449`;
    /// `bonds.cpp::testBasisPointValue` (:1759) is the case that calls it. The
    /// numbers below are not from that case: they come from the definition at
    /// `cashflows.cpp:1055-1080` and from the leg itself.
    ///
    /// The delta and gamma terms are the ones the doc comment names. The last
    /// two assertions pin the `convexity / 100.0` that C++ writes into gamma:
    /// undo the division and the expansion reproduces the leg's actual price
    /// change to nine decimals; leave it in and the value is short by a
    /// hundredth of the second-order term.
    #[test]
    fn the_basis_point_value_is_the_taylor_expansion_with_a_hundredth_of_the_gamma() {
        let (settings, leg) = (settings(), redeeming_leg());
        let rate = compounded(FORWARD);
        let npv =
            |y| CashFlows::npv_at_yield(&leg, &compounded(y), &settings, None, None, None).unwrap();
        let modified =
            CashFlows::duration(&leg, &rate, Duration::Modified, &settings, None, None, None)
                .unwrap();
        let convexity = CashFlows::convexity(&leg, &rate, &settings, None, None, None).unwrap();

        let shift = 1.0e-4;
        let delta = -modified * npv(FORWARD) * shift;
        let gamma = 0.5 * (convexity / 100.0) * npv(FORWARD) * shift * shift;

        let bpv = CashFlows::basis_point_value(&leg, &rate, &settings, None, None, None).unwrap();
        assert!(bpv < -0.01);
        assert!((bpv - (delta + gamma)).abs() < 1e-16);

        let actual = npv(FORWARD + shift) - npv(FORWARD);
        assert!(((delta + 100.0 * gamma) - actual).abs() < 1e-9);
        assert!((bpv - actual).abs() > 1e-6);
    }

    /// **Hand-derived.** As for the basis-point value, the C++ test-suite reaches
    /// `CashFlows::yieldValueBasisPoint` only through
    /// `BondFunctions::yieldValueBasisPoint`, which delegates at
    /// `bondfunctions.cpp:474`. This is the definition at
    /// `cashflows.cpp:1106-1130`, checked against the leg by re-solving.
    ///
    /// It is `dy/dP * 0.01`, so moving the target NPV up by one cent must move
    /// the solved yield down by it.
    #[test]
    fn the_yield_value_of_a_basis_point_is_the_yield_move_for_a_one_cent_price_move() {
        let (settings, leg) = (settings(), redeeming_leg());
        let rate = compounded(FORWARD);
        let npv = CashFlows::npv_at_yield(&leg, &rate, &settings, None, None, None).unwrap();
        let modified =
            CashFlows::duration(&leg, &rate, Duration::Modified, &settings, None, None, None)
                .unwrap();

        let yvbp =
            CashFlows::yield_value_basis_point(&leg, &rate, &settings, None, None, None).unwrap();
        assert!((yvbp - 0.01 / (-npv * modified)).abs() < 1e-18);
        assert!(yvbp < 0.0);

        let irr = |target| {
            CashFlows::solve_yield(
                &leg,
                target,
                Actual365Fixed::new(),
                Compounding::Compounded,
                Frequency::Semiannual,
                &settings,
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .unwrap()
        };
        assert!((irr(npv + 0.01) - irr(npv) - yvbp).abs() < 1e-8);
    }

    /// Both Taylor-expansion analytics short-circuit an empty leg, which spares
    /// [`CashFlows::yield_value_basis_point`] a division by zero.
    #[test]
    fn an_empty_leg_has_no_basis_point_values() {
        let (settings, leg) = (Settings::new(), Leg::new());
        let rate = simple_365(FORWARD);

        assert_eq!(
            CashFlows::basis_point_value(&leg, &rate, &settings, None, None, None).unwrap(),
            0.0
        );
        assert_eq!(
            CashFlows::yield_value_basis_point(&leg, &rate, &settings, None, None, None).unwrap(),
            0.0
        );
    }

    /// The times every flow is discounted over run from the NPV date, not the
    /// settlement date. Under a continuous yield the discount factors pick up a
    /// common `exp(y * shift)` that cancels out of the weights, so moving the
    /// NPV date forward moves the simple duration back by exactly the shift.
    #[test]
    fn the_durations_measure_time_from_the_npv_date() {
        let (settings, leg) = (settings(), fixed_leg(RATE));
        let yield_rate = InterestRate::new(
            FORWARD,
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )
        .unwrap();
        let duration = |npv_date| {
            CashFlows::duration(
                &leg,
                &yield_rate,
                Duration::Simple,
                &settings,
                None,
                None,
                npv_date,
            )
            .unwrap()
        };

        let shift = 30.0 / 365.0;
        assert!((duration(Some(today() + 30)) - (duration(None) - shift)).abs() < 1e-14);
        assert!(duration(None) > 1.0);
    }

    /// The empty-leg short circuit comes before the settlement date is
    /// resolved, so it stands with no evaluation date set.
    #[test]
    fn an_empty_leg_has_no_atm_rate() {
        let (settings, curve, leg) = (Settings::new(), curve(FORWARD), Leg::new());

        assert_eq!(
            CashFlows::npvbps(&leg, &curve, &settings, None, None, None).unwrap(),
            (0.0, 0.0)
        );
        assert_eq!(
            CashFlows::atm_rate(&leg, &curve, &settings, None, None, None, None).unwrap(),
            0.0
        );
    }
}

/// The two ex-coupon bond cases of `bonds.cpp`, the only test-suite calls to
/// `CashFlows::yield`, `duration` and `convexity`.
///
/// C++ prices a `FixedRateBond`, whose `cashflows_` is exactly the leg built
/// here (`fixedratebond.cpp:53-63` plus the single `Redemption` that
/// `bond.cpp:311-323` appends), and reaches the NPV as `testPrice + accrued`.
/// The tables pin that sum directly, so the port needs neither the instrument
/// nor `accruedAmount`: it feeds the tabulated NPV to
/// [`CashFlows::solve_yield`] and checks the yield, duration and convexity that
/// come back. The C++ round trip back to the price is the same check with the
/// accrued amount added to both sides.
#[cfg(test)]
mod bonds_tests {
    use super::*;
    use crate::cashflows::fixedrateleg::FixedRateLeg;
    use crate::cashflows::simplecashflow::Redemption;
    use crate::shared::shared;
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendar::Calendar;
    use crate::time::calendars::australia::{self, Australia};
    use crate::time::calendars::nullcalendar::NullCalendar;
    use crate::time::calendars::unitedkingdom::{self, UnitedKingdom};
    use crate::time::date::Month;
    use crate::time::dategenerationrule::DateGeneration;
    use crate::time::daycounters::actualactual::{ActualActual, Convention};
    use crate::time::schedule::Schedule;

    /// One row of a `bonds.cpp` expectation table. The NPV is its `NPV` column,
    /// which C++ asserts equals `testPrice + accruedAmount`.
    struct Case {
        settlement: Date,
        npv: Real,
        irr: Rate,
        duration: Real,
        convexity: Real,
    }

    /// The `FixedRateBond` cash flows of `bonds.cpp`: a semiannual `FixedRateLeg`
    /// on a schedule-driven `ActualActual(ISMA)`, then the redemption.
    fn ex_coupon_leg(
        start: Date,
        first_coupon: Date,
        maturity: Date,
        coupon: Rate,
        ex_coupon_period: Period,
        payment_calendar: Calendar,
        ex_coupon_calendar: Calendar,
    ) -> (Leg, DayCounter) {
        let unadjusted = BusinessDayConvention::Unadjusted;
        let schedule = Schedule::new(
            start,
            maturity,
            Period::new(6, TimeUnit::Months),
            NullCalendar::new(),
            unadjusted,
            unadjusted,
            DateGeneration::Forward,
            true,
            first_coupon,
            Date::null(),
        );
        let day_counter = ActualActual::with_schedule(Convention::ISMA, schedule.clone());
        let mut leg = FixedRateLeg::new(schedule)
            .with_notional(100.0)
            .with_coupon_rate(
                coupon,
                day_counter.clone(),
                Compounding::Simple,
                Frequency::Annual,
            )
            .unwrap()
            .with_payment_calendar(payment_calendar)
            .with_payment_adjustment(unadjusted)
            .with_ex_coupon_period(ex_coupon_period, ex_coupon_calendar, unadjusted, false)
            .build()
            .unwrap();
        leg.push(shared(Redemption::new(100.0, maturity)) as Shared<dyn CashFlow>);
        (leg, day_counter)
    }

    /// The `yield -> duration -> convexity -> npv` round trip each table row
    /// drives, at the `(yield and duration, convexity, npv)` tolerances the row
    /// is asserted to in C++.
    fn check(leg: &Leg, day_counter: &DayCounter, case: &Case, tolerance: (Real, Real, Real)) {
        let settings = Settings::new();
        let settlement = Some(case.settlement);
        let (comp, freq) = (Compounding::Compounded, Frequency::Semiannual);

        let irr = CashFlows::solve_yield(
            leg,
            case.npv,
            day_counter.clone(),
            comp,
            freq,
            &settings,
            Some(false),
            settlement,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        assert!((irr - case.irr).abs() < tolerance.0, "yield {irr}");

        let rate = InterestRate::new(irr, day_counter.clone(), comp, freq).unwrap();
        let duration = CashFlows::duration(
            leg,
            &rate,
            Duration::Modified,
            &settings,
            Some(false),
            settlement,
            None,
        )
        .unwrap();
        assert!(
            (duration - case.duration).abs() < tolerance.0,
            "duration {duration}"
        );

        let convexity =
            CashFlows::convexity(leg, &rate, &settings, Some(false), settlement, None).unwrap();
        assert!(
            (convexity - case.convexity).abs() < tolerance.1,
            "convexity {convexity}"
        );

        let npv =
            CashFlows::npv_at_yield(leg, &rate, &settings, Some(false), settlement, None).unwrap();
        assert!((npv - case.npv).abs() < tolerance.2, "npv {npv}");
    }

    /// `bonds.cpp::testExCouponGilt` (:1155), whose table (:1246-1256) is
    /// verified against Bloomberg. The gilt's ex-coupon date is six *business*
    /// days before the coupon, hence the UK calendar on the ex-coupon period.
    #[test]
    fn the_uk_gilt_reproduces_its_bloomberg_yield_duration_and_convexity() {
        let calendar = UnitedKingdom::new(unitedkingdom::Market::Settlement);
        let (leg, day_counter) = ex_coupon_leg(
            Date::new(29, Month::February, 1996),
            Date::new(7, Month::June, 1996),
            Date::new(7, Month::June, 2021),
            0.08,
            Period::new(6, TimeUnit::Days),
            calendar.clone(),
            calendar,
        );
        let cases = [
            Case {
                settlement: Date::new(29, Month::May, 2013),
                npv: 106.8021978,
                irr: 0.0749518,
                duration: 5.6760445,
                convexity: 42.1531486,
            },
            Case {
                settlement: Date::new(30, Month::May, 2013),
                npv: 102.8241758,
                irr: 0.0749618,
                duration: 5.8928163,
                convexity: 43.7562186,
            },
            Case {
                settlement: Date::new(31, Month::May, 2013),
                npv: 102.8461538,
                irr: 0.0749599,
                duration: 5.8901860,
                convexity: 43.7239438,
            },
        ];

        for case in &cases {
            check(&leg, &day_counter, case, (1e-6, 1e-6, 1e-6));
        }
    }

    /// `bonds.cpp::testExCouponAustralianBond` (:1283). The ex-coupon date is
    /// seven *calendar* days before the coupon, so the ex-coupon calendar is the
    /// null one while the payments follow Australia.
    #[test]
    fn the_australian_bond_reproduces_its_bloomberg_yield_duration_and_convexity() {
        let (leg, day_counter) = ex_coupon_leg(
            Date::new(15, Month::February, 2004),
            Date::new(15, Month::August, 2004),
            Date::new(15, Month::February, 2017),
            0.06,
            Period::new(7, TimeUnit::Days),
            Australia::new(australia::Market::Settlement),
            NullCalendar::new(),
        );
        let cases = [
            Case {
                settlement: Date::new(7, Month::August, 2014),
                npv: 105.867,
                irr: 0.04723,
                duration: 2.26276,
                convexity: 6.54870,
            },
            Case {
                settlement: Date::new(8, Month::August, 2014),
                npv: 102.884,
                irr: 0.047235,
                duration: 2.32536,
                convexity: 6.72531,
            },
            Case {
                settlement: Date::new(11, Month::August, 2014),
                npv: 102.934,
                irr: 0.047190,
                duration: 2.31732,
                convexity: 6.68407,
            },
        ];

        for case in &cases {
            check(&leg, &day_counter, case, (1e-5, 1e-4, 1e-3));
        }
    }
}
