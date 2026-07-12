//! Discount-curve and yield bond analytics.
//!
//! Port of the `YieldTermStructure` and yield (IRR) subsets of
//! `ql/pricingengines/bond/bondfunctions.{hpp,cpp}`: the price, accrued, rate
//! and yield/duration/convexity wrappers that add a tradability guard and
//! delegate to the matching [`CashFlows`] overload (`bondfunctions.cpp:224-486`).
//! Each reads the bond's own cash flows and settings rather than a global
//! evaluation date (D5).
//!
//! Deviations, all by existing design decisions:
//! - `accruedAmount` is the port's [`Bond::accrued_amount`], which already
//!   carries the tradability guard and the `100 / notional` scaling
//!   (`bond.cpp` delegates the same call to `BondFunctions` in reverse); the
//!   wrapper here is the free-function surface over it.
//! - `atmRate` takes no [`BondPrice`]: the type lands with the yield wrappers
//!   below (#290), but the price-fed curve round trip of `bonds.cpp:241` stays
//!   a discount-curve follow-up and is unreachable here. The
//!   port ports the discount-curve `atmRate` at its `price = {}` default,
//!   whose target NPV is the leg's own; with one coupon rate that recovers the
//!   coupon exactly for any curve (a curve-independent invariant, not a curve
//!   oracle - the clean-price tests are the curve oracles).

use crate::cashflows::{CashFlows, Duration};
use crate::errors::QlResult;
use crate::instruments::{Bond, BondPrice};
use crate::interestrate::{Compounding, InterestRate};
use crate::require;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::types::{Rate, Real, Time};

/// Free-function bond analytics over a discount curve.
pub struct BondFunctions;

impl BondFunctions {
    /// Whether the bond can be traded at `settlement` (the settlement date when
    /// `None`): its notional has not yet been redeemed (`bondfunctions.cpp:39`).
    ///
    /// # Errors
    ///
    /// Propagates the settlement-date and notional lookups.
    pub fn is_tradable(bond: &Bond, settlement: Option<Date>) -> QlResult<bool> {
        let settlement = Self::settlement_or_eval(bond, settlement)?;
        Ok(bond.notional(Some(settlement))? != 0.0)
    }

    /// The interest accrued at `settlement` (the settlement date when `None`),
    /// per 100 of notional (`bondfunctions.cpp:224`).
    ///
    /// # Errors
    ///
    /// Propagates the accrual lookup.
    pub fn accrued_amount(bond: &Bond, settlement: Option<Date>) -> QlResult<Real> {
        bond.accrued_amount(settlement)
    }

    /// The clean price per 100 of notional on `discount_curve`
    /// (`dirtyPrice - accruedAmount`, `bondfunctions.cpp:239`).
    ///
    /// # Errors
    ///
    /// The bond must be tradable at the settlement date.
    pub fn clean_price(
        bond: &Bond,
        discount_curve: &dyn YieldTermStructure,
        settlement: Option<Date>,
    ) -> QlResult<Real> {
        let settlement = Self::settlement_or_eval(bond, settlement)?;
        let dirty = Self::dirty_price(bond, discount_curve, Some(settlement))?;
        Ok(dirty - bond.accrued_amount(Some(settlement))?)
    }

    /// The dirty price per 100 of notional on `discount_curve`
    /// (`CashFlows::npv * 100 / notional`, `bondfunctions.cpp:248`).
    ///
    /// # Errors
    ///
    /// The bond must be tradable at the settlement date.
    pub fn dirty_price(
        bond: &Bond,
        discount_curve: &dyn YieldTermStructure,
        settlement: Option<Date>,
    ) -> QlResult<Real> {
        let settlement = Self::settlement_or_eval(bond, settlement)?;
        let notional = Self::require_tradable(bond, settlement)?;
        let npv = CashFlows::npv(
            bond.cashflows(),
            discount_curve,
            bond.settings(),
            Some(false),
            Some(settlement),
            None,
        )?;
        Ok(npv * 100.0 / notional)
    }

    /// The basis-point value per 100 of notional on `discount_curve`
    /// (`CashFlows::bps * 100 / notional`, `bondfunctions.cpp:265`).
    ///
    /// # Errors
    ///
    /// The bond must be tradable at the settlement date.
    pub fn bps(
        bond: &Bond,
        discount_curve: &dyn YieldTermStructure,
        settlement: Option<Date>,
    ) -> QlResult<Real> {
        let settlement = Self::settlement_or_eval(bond, settlement)?;
        let notional = Self::require_tradable(bond, settlement)?;
        let bps = CashFlows::bps(
            bond.cashflows(),
            discount_curve,
            bond.settings(),
            Some(false),
            Some(settlement),
            None,
        )?;
        Ok(bps * 100.0 / notional)
    }

    /// The at-the-money coupon rate that reprices the bond on `discount_curve`
    /// (`bondfunctions.cpp:280`, at the `price = {}` default).
    ///
    /// # Errors
    ///
    /// The bond must be tradable at the settlement date, and the leg must have
    /// some basis-point sensitivity.
    pub fn atm_rate(
        bond: &Bond,
        discount_curve: &dyn YieldTermStructure,
        settlement: Option<Date>,
    ) -> QlResult<Rate> {
        let settlement = Self::settlement_or_eval(bond, settlement)?;
        Self::require_tradable(bond, settlement)?;
        CashFlows::atm_rate(
            bond.cashflows(),
            discount_curve,
            bond.settings(),
            Some(false),
            Some(settlement),
            Some(settlement),
            None,
        )
    }

    /// The yield (internal rate of return) that reprices the bond at `price`,
    /// solved off its own cash flows (`bondfunctions.hpp:167`).
    ///
    /// `settlement` defaults to the bond's settlement date, `accuracy` to
    /// `1e-10`, `max_evaluations` to `100` and `guess` to `0.05`. As C++, a
    /// clean price is grossed up by the accrued amount and the quote is scaled
    /// from per-100 to the bond's notional before the solve.
    ///
    /// # Errors
    ///
    /// The bond must be tradable at the settlement date, and the solve must
    /// converge (as [`CashFlows::solve_yield`]).
    #[allow(clippy::too_many_arguments)]
    pub fn yield_rate(
        bond: &Bond,
        price: BondPrice,
        day_counter: DayCounter,
        compounding: Compounding,
        frequency: Frequency,
        settlement: Option<Date>,
        accuracy: Option<Real>,
        max_evaluations: Option<usize>,
        guess: Option<Rate>,
    ) -> QlResult<Rate> {
        let settlement = Self::settlement_or_eval(bond, settlement)?;
        let notional = Self::require_tradable(bond, settlement)?;
        let mut amount = price.amount();
        if matches!(price, BondPrice::Clean(_)) {
            amount += bond.accrued_amount(Some(settlement))?;
        }
        amount *= notional / 100.0;
        CashFlows::solve_yield(
            bond.cashflows(),
            amount,
            day_counter,
            compounding,
            frequency,
            bond.settings(),
            Some(false),
            Some(settlement),
            Some(settlement),
            accuracy,
            max_evaluations,
            guess,
        )
    }

    /// The bond's [`Duration`] under a flat `yield_rate` (`bondfunctions.cpp:389`).
    ///
    /// # Errors
    ///
    /// The bond must be tradable at the settlement date.
    pub fn duration(
        bond: &Bond,
        yield_rate: &InterestRate,
        duration_type: Duration,
        settlement: Option<Date>,
    ) -> QlResult<Time> {
        let settlement = Self::settlement_or_eval(bond, settlement)?;
        Self::require_tradable(bond, settlement)?;
        CashFlows::duration(
            bond.cashflows(),
            yield_rate,
            duration_type,
            bond.settings(),
            Some(false),
            Some(settlement),
            None,
        )
    }

    /// The bond's convexity under a flat `yield_rate` (`bondfunctions.cpp:416`).
    ///
    /// # Errors
    ///
    /// The bond must be tradable at the settlement date.
    pub fn convexity(
        bond: &Bond,
        yield_rate: &InterestRate,
        settlement: Option<Date>,
    ) -> QlResult<Real> {
        let settlement = Self::settlement_or_eval(bond, settlement)?;
        Self::require_tradable(bond, settlement)?;
        CashFlows::convexity(
            bond.cashflows(),
            yield_rate,
            bond.settings(),
            Some(false),
            Some(settlement),
            None,
        )
    }

    /// The bond's basis-point value under a flat `yield_rate`, in the leg's own
    /// currency (unscaled, as C++, `bondfunctions.cpp:440`).
    ///
    /// # Errors
    ///
    /// The bond must be tradable at the settlement date.
    pub fn basis_point_value(
        bond: &Bond,
        yield_rate: &InterestRate,
        settlement: Option<Date>,
    ) -> QlResult<Real> {
        let settlement = Self::settlement_or_eval(bond, settlement)?;
        Self::require_tradable(bond, settlement)?;
        CashFlows::basis_point_value(
            bond.cashflows(),
            yield_rate,
            bond.settings(),
            Some(false),
            Some(settlement),
            None,
        )
    }

    /// The yield move a one-cent change in the bond's value implies, under a
    /// flat `yield_rate` (unscaled, as C++, `bondfunctions.cpp:464`).
    ///
    /// # Errors
    ///
    /// The bond must be tradable at the settlement date.
    pub fn yield_value_basis_point(
        bond: &Bond,
        yield_rate: &InterestRate,
        settlement: Option<Date>,
    ) -> QlResult<Real> {
        let settlement = Self::settlement_or_eval(bond, settlement)?;
        Self::require_tradable(bond, settlement)?;
        CashFlows::yield_value_basis_point(
            bond.cashflows(),
            yield_rate,
            bond.settings(),
            Some(false),
            Some(settlement),
            None,
        )
    }

    /// The basis-point value per 100 of notional under a flat `yield_rate`
    /// (`CashFlows::bps * 100 / notional`, `bondfunctions.cpp:348`).
    ///
    /// # Errors
    ///
    /// The bond must be tradable at the settlement date.
    pub fn bps_at_yield(
        bond: &Bond,
        yield_rate: &InterestRate,
        settlement: Option<Date>,
    ) -> QlResult<Real> {
        let settlement = Self::settlement_or_eval(bond, settlement)?;
        let notional = Self::require_tradable(bond, settlement)?;
        let bps = CashFlows::bps_at_yield(
            bond.cashflows(),
            yield_rate,
            bond.settings(),
            Some(false),
            Some(settlement),
            None,
        )?;
        Ok(bps * 100.0 / notional)
    }

    fn settlement_or_eval(bond: &Bond, settlement: Option<Date>) -> QlResult<Date> {
        match settlement {
            Some(date) => Ok(date),
            None => bond.settlement_date(None),
        }
    }

    fn require_tradable(bond: &Bond, settlement: Date) -> QlResult<Real> {
        let notional = bond.notional(Some(settlement))?;
        require!(
            notional != 0.0,
            "non tradable at {settlement} settlement date (maturity being {})",
            bond.maturity_date()?
        );
        Ok(notional)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cashflows::FixedRateLeg;
    use crate::instruments::Bond;
    use crate::interestrate::Compounding;
    use crate::settings::Settings;
    use crate::shared::{Shared, shared};
    use crate::termstructures::yields::FlatForward;
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendar::Calendar;
    use crate::time::calendars::unitedstates::{self, UnitedStates};
    use crate::time::date::Month;
    use crate::time::dategenerationrule::DateGeneration;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::daycounters::actualactual::{ActualActual, Convention};
    use crate::time::frequency::Frequency;
    use crate::time::period::Period;
    use crate::time::schedule::Schedule;
    use crate::time::timeunit::TimeUnit;
    use crate::types::Rate;

    fn today() -> Date {
        Date::new(22, Month::November, 2004)
    }

    fn settings() -> Shared<Settings<Date>> {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(today());
        settings
    }

    fn us_gov() -> Calendar {
        UnitedStates::new(unitedstates::Market::GovernmentBond)
    }

    /// `bonds.cpp::testCachedFixed`'s `flatRate(today, 0.03, Actual360())`,
    /// whose default compounding is continuous and annual.
    fn discount_curve() -> FlatForward {
        FlatForward::with_rate(
            today(),
            0.03,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )
    }

    /// The `FixedRateBond` of `bonds.cpp::testCachedFixed`: a semiannual
    /// schedule-driven `ActualActual(ISMA)` leg, 30 Nov 2004 to 30 Nov 2008,
    /// on the US government-bond calendar with a modified-following payment
    /// convention, then the par redemption `Bond::from_coupons` appends.
    fn bond_with_coupons(rates: Vec<Rate>) -> Bond {
        let unadjusted = BusinessDayConvention::Unadjusted;
        let schedule = Schedule::new(
            Date::new(30, Month::November, 2004),
            Date::new(30, Month::November, 2008),
            Period::new(6, TimeUnit::Months),
            us_gov(),
            unadjusted,
            unadjusted,
            DateGeneration::Backward,
            false,
            Date::null(),
            Date::null(),
        );
        let day_counter = ActualActual::with_schedule(Convention::ISMA, schedule.clone());
        let leg = FixedRateLeg::new(schedule)
            .with_notional(100.0)
            .with_coupon_rates(rates, day_counter, Compounding::Simple, Frequency::Annual)
            .unwrap()
            .with_payment_calendar(us_gov())
            .with_payment_adjustment(BusinessDayConvention::ModifiedFollowing)
            .build()
            .unwrap();
        Bond::from_coupons(
            1,
            us_gov(),
            Some(Date::new(30, Month::November, 2004)),
            leg,
            settings(),
        )
        .unwrap()
    }

    fn plain_bond() -> Bond {
        bond_with_coupons(vec![0.02875])
    }

    /// The plain bond's clean price reproduces the cached `bonds.cpp` value
    /// (`:867`, 99.298100 at 1e-6), oracling `CashFlows::npv` through the
    /// discount-curve `dirtyPrice`. Settlement is the 30 Nov 2004 dated date, so
    /// the accrued interest is zero and the clean and dirty prices agree.
    #[test]
    fn the_plain_bond_reproduces_its_cached_clean_price() {
        let bond = plain_bond();
        let curve = discount_curve();
        let clean = BondFunctions::clean_price(&bond, &curve, None).unwrap();
        let dirty = BondFunctions::dirty_price(&bond, &curve, None).unwrap();
        assert!((clean - 99.298100).abs() < 1e-6, "clean price {clean}");
        assert!((dirty - 99.298100).abs() < 1e-6, "dirty price {dirty}");
    }

    /// The varying-coupon bond's clean price reproduces the second cached
    /// `bonds.cpp` value (`:892`, 100.334149 at 1e-6), exercising the
    /// multiple-rate leg through the same discount-curve wrapper.
    #[test]
    fn the_varying_coupon_bond_reproduces_its_cached_clean_price() {
        let bond = bond_with_coupons(vec![0.02875, 0.03, 0.03125, 0.0325]);
        let curve = discount_curve();
        let clean = BondFunctions::clean_price(&bond, &curve, None).unwrap();
        assert!((clean - 100.334149).abs() < 1e-6, "clean price {clean}");
    }

    /// A single-rate bond's at-the-money rate is its coupon, and the
    /// default-price `atmRate` recovers it regardless of the discount curve:
    /// the target NPV is the leg's own, so the numerator and denominator
    /// discount factors cancel and the rate is `coupon-leg PV / bps = coupon`
    /// for any curve. This is a coupon-recovery invariant, not a curve oracle -
    /// the curve oracles are the two clean-price tests above. The price-fed
    /// `atmRate` round trip of `bonds.cpp:241` needs the `Bond::Price` argument
    /// deferred to #290.
    #[test]
    fn the_atm_rate_recovers_the_single_coupon_rate() {
        let bond = plain_bond();
        let curve = discount_curve();
        let atm = BondFunctions::atm_rate(&bond, &curve, None).unwrap();
        assert!((atm - 0.02875).abs() < 1e-10, "atm rate {atm}");
    }

    /// The basis-point value equals the dirty-price change for a one-basis-point
    /// coupon bump: the price is linear in the coupon rate on a fixed curve, so
    /// the finite difference is exact.
    #[test]
    fn the_bps_matches_a_one_basis_point_coupon_bump() {
        let curve = discount_curve();
        let base = plain_bond();
        let bumped = bond_with_coupons(vec![0.02875 + 1.0e-4]);
        let bps = BondFunctions::bps(&base, &curve, None).unwrap();
        let bump = BondFunctions::dirty_price(&bumped, &curve, None).unwrap()
            - BondFunctions::dirty_price(&base, &curve, None).unwrap();
        assert!((bps - bump).abs() < 1e-10, "bps {bps} vs bump {bump}");
    }

    /// Away from a coupon date the clean price nets off a positive accrued
    /// amount, and the three wrappers stay consistent.
    #[test]
    fn the_clean_price_nets_the_accrued_interest_off_the_dirty_price() {
        let bond = plain_bond();
        let curve = discount_curve();
        let mid = Date::new(15, Month::January, 2005);
        let dirty = BondFunctions::dirty_price(&bond, &curve, Some(mid)).unwrap();
        let clean = BondFunctions::clean_price(&bond, &curve, Some(mid)).unwrap();
        let accrued = BondFunctions::accrued_amount(&bond, Some(mid)).unwrap();
        assert!(accrued > 0.0, "accrued {accrued}");
        assert!((dirty - clean - accrued).abs() < 1e-12);
    }

    /// Once the notional has been redeemed the bond is no longer tradable: the
    /// price wrappers error and the accrued amount is zero.
    #[test]
    fn a_redeemed_bond_is_not_tradable() {
        let bond = plain_bond();
        let curve = discount_curve();
        let after = Date::new(1, Month::December, 2008);
        assert!(!BondFunctions::is_tradable(&bond, Some(after)).unwrap());
        let err = BondFunctions::dirty_price(&bond, &curve, Some(after)).unwrap_err();
        assert!(err.message().contains("non tradable"));
        assert_eq!(
            BondFunctions::accrued_amount(&bond, Some(after)).unwrap(),
            0.0
        );
    }
}

/// The yield-side wrappers reproduced at the bond-instrument level, oracling the
/// leg analytics of #251 through `BondFunctions`.
///
/// The cases chosen to oracle the wrappers at the bond level: the gilt and
/// Australian ex-coupon bonds (`bonds.cpp::testExCouponGilt` :1155,
/// `testExCouponAustralianBond` :1283), `testBasisPointValue` (:1759) and
/// `testThirty360BondWithSettlementOn31st` (:1715). In C++ only the last two
/// call the `BondFunctions` wrappers directly; the ex-coupon tests call
/// `CashFlows::` on the bare leg (`:1265`, `:1393`). The Rust cases route every
/// number through the bond wrappers regardless, which is the bond-level oracle
/// this ticket wants. Each builds the `FixedRateBond` cash flows, feeds the
/// tabulated price to [`BondFunctions::yield_rate`] (or [`Bond::yield_rate`]),
/// and checks the analytics that come back; the redemption is the one
/// `Bond::from_coupons` appends, so the leg matches #251's exactly.
#[cfg(test)]
mod yield_tests {
    use super::*;
    use crate::cashflows::FixedRateLeg;
    use crate::instruments::BondPrice;
    use crate::interestrate::{Compounding, InterestRate};
    use crate::settings::Settings;
    use crate::shared::{Shared, shared};
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendar::Calendar;
    use crate::time::calendars::australia::{self, Australia};
    use crate::time::calendars::nullcalendar::NullCalendar;
    use crate::time::calendars::unitedkingdom::{self, UnitedKingdom};
    use crate::time::calendars::unitedstates::{self, UnitedStates};
    use crate::time::date::Month;
    use crate::time::dategenerationrule::DateGeneration;
    use crate::time::daycounters::actualactual::{ActualActual, Convention};
    use crate::time::daycounters::thirty360::{Convention as Thirty360Convention, Thirty360};
    use crate::time::frequency::Frequency;
    use crate::time::period::Period;
    use crate::time::schedule::Schedule;
    use crate::time::timeunit::TimeUnit;

    const FACE_AMOUNT: Real = 1_000_000.0;

    /// One row of a `bonds.cpp` expectation table: the settlement date, the
    /// dirty price (`testPrice + accrued`, per 100 of notional) and the
    /// Bloomberg yield, duration and convexity it drives.
    struct Case {
        settlement: Date,
        dirty_price: Real,
        irr: Rate,
        duration: Real,
        convexity: Real,
    }

    fn us_gov() -> Calendar {
        UnitedStates::new(unitedstates::Market::GovernmentBond)
    }

    /// The `FixedRateBond` of the ex-coupon cases: a semiannual `ActualActual
    /// (ISMA)` leg on a 100 notional with an ex-coupon period, wrapped in a
    /// `Bond` whose appended par redemption completes the leg.
    fn ex_coupon_bond(
        start: Date,
        first_coupon: Date,
        maturity: Date,
        coupon: Rate,
        ex_coupon_period: Period,
        payment_calendar: Calendar,
        ex_coupon_calendar: Calendar,
    ) -> (Bond, DayCounter) {
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
        let leg = FixedRateLeg::new(schedule)
            .with_notional(100.0)
            .with_coupon_rate(
                coupon,
                day_counter.clone(),
                Compounding::Simple,
                Frequency::Annual,
            )
            .unwrap()
            .with_payment_calendar(payment_calendar.clone())
            .with_payment_adjustment(unadjusted)
            .with_ex_coupon_period(ex_coupon_period, ex_coupon_calendar, unadjusted, false)
            .build()
            .unwrap();
        let bond =
            Bond::from_coupons(1, payment_calendar, None, leg, shared(Settings::new())).unwrap();
        (bond, day_counter)
    }

    /// The `yield -> duration -> convexity` round trip each table row drives
    /// through `BondFunctions`, at the row's C++ tolerances.
    fn check(bond: &Bond, day_counter: &DayCounter, case: &Case, tolerance: (Real, Real, Real)) {
        let (comp, freq) = (Compounding::Compounded, Frequency::Semiannual);
        let settlement = Some(case.settlement);

        let irr = BondFunctions::yield_rate(
            bond,
            BondPrice::Dirty(case.dirty_price),
            day_counter.clone(),
            comp,
            freq,
            settlement,
            None,
            None,
            None,
        )
        .unwrap();
        assert!((irr - case.irr).abs() < tolerance.0, "yield {irr}");

        let rate = InterestRate::new(irr, day_counter.clone(), comp, freq).unwrap();
        let duration =
            BondFunctions::duration(bond, &rate, Duration::Modified, settlement).unwrap();
        assert!(
            (duration - case.duration).abs() < tolerance.0,
            "duration {duration}"
        );

        let convexity = BondFunctions::convexity(bond, &rate, settlement).unwrap();
        assert!(
            (convexity - case.convexity).abs() < tolerance.1,
            "convexity {convexity}"
        );
    }

    /// `bonds.cpp::testExCouponGilt` (:1155), verified against Bloomberg.
    #[test]
    fn the_uk_gilt_reproduces_its_yield_duration_and_convexity_through_the_bond() {
        let calendar = UnitedKingdom::new(unitedkingdom::Market::Settlement);
        let (bond, day_counter) = ex_coupon_bond(
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
                dirty_price: 106.8021978,
                irr: 0.0749518,
                duration: 5.6760445,
                convexity: 42.1531486,
            },
            Case {
                settlement: Date::new(30, Month::May, 2013),
                dirty_price: 102.8241758,
                irr: 0.0749618,
                duration: 5.8928163,
                convexity: 43.7562186,
            },
            Case {
                settlement: Date::new(31, Month::May, 2013),
                dirty_price: 102.8461538,
                irr: 0.0749599,
                duration: 5.8901860,
                convexity: 43.7239438,
            },
        ];
        for case in &cases {
            check(&bond, &day_counter, case, (1e-6, 1e-6, 1e-6));
        }
    }

    /// `bonds.cpp::testExCouponAustralianBond` (:1283), Bloomberg-verified.
    #[test]
    fn the_australian_bond_reproduces_its_yield_duration_and_convexity_through_the_bond() {
        let (bond, day_counter) = ex_coupon_bond(
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
                dirty_price: 105.867,
                irr: 0.04723,
                duration: 2.26276,
                convexity: 6.54870,
            },
            Case {
                settlement: Date::new(8, Month::August, 2014),
                dirty_price: 102.884,
                irr: 0.047235,
                duration: 2.32536,
                convexity: 6.72531,
            },
            Case {
                settlement: Date::new(11, Month::August, 2014),
                dirty_price: 102.934,
                irr: 0.047190,
                duration: 2.31732,
                convexity: 6.68407,
            },
        ];
        for case in &cases {
            check(&bond, &day_counter, case, (1e-5, 1e-4, 1e-3));
        }
    }

    fn treasury_today() -> Date {
        Date::new(29, Month::January, 2024)
    }

    /// The `FixedRateBond` cash flows of `testBasisPointValue`: a semiannual
    /// `Thirty360(USA)` leg on the million-face notional at `coupon`, with the
    /// par redemption `Bond::from_coupons` appends.
    fn treasury_bond(coupon: Rate, settings: Shared<Settings<Date>>) -> (Bond, DayCounter) {
        let unadjusted = BusinessDayConvention::Unadjusted;
        let day_counter = Thirty360::with_convention(Thirty360Convention::USA);
        let schedule = Schedule::new(
            Date::new(15, Month::November, 2023),
            Date::new(15, Month::August, 2033),
            Period::new(6, TimeUnit::Months),
            us_gov(),
            unadjusted,
            unadjusted,
            DateGeneration::Forward,
            false,
            Date::null(),
            Date::null(),
        );
        let leg = FixedRateLeg::new(schedule)
            .with_notional(FACE_AMOUNT)
            .with_coupon_rate(
                coupon,
                day_counter.clone(),
                Compounding::Simple,
                Frequency::Annual,
            )
            .unwrap()
            .with_payment_calendar(us_gov())
            .with_payment_adjustment(unadjusted)
            .build()
            .unwrap();
        let bond = Bond::from_coupons(1, us_gov(), None, leg, settings).unwrap();
        (bond, day_counter)
    }

    /// `bonds.cpp::testBasisPointValue` (:1759): the clean price yields
    /// 0.041301, and the basis-point value and yield value reproduce the table
    /// at `:1798-1806` at both settlement dates. The yield is solved once and
    /// reused, as C++ does, since `d(bpv)/dy` here is of order 1e4.
    #[test]
    fn the_treasury_bond_reproduces_its_basis_point_value_through_the_bond() {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(treasury_today());
        let (bond, day_counter) = treasury_bond(0.045, settings);
        let (comp, freq) = (Compounding::Compounded, Frequency::Semiannual);
        let settlement = Date::new(30, Month::January, 2024);

        let irr = BondFunctions::yield_rate(
            &bond,
            BondPrice::Clean(102.890625),
            day_counter.clone(),
            comp,
            freq,
            Some(settlement),
            None,
            None,
            None,
        )
        .unwrap();
        assert!((irr - 0.041301).abs() < 1e-6, "yield {irr}");

        let rate = InterestRate::new(irr, day_counter, comp, freq).unwrap();
        let cases = [
            (settlement, -795.459834, -0.0012571287),
            (
                Date::new(12, Month::February, 2024),
                -793.149033,
                -0.0012607913,
            ),
        ];
        for (settlement, expected_bpv, expected_yvbp) in cases {
            let settlement = Some(settlement);
            let bpv = BondFunctions::basis_point_value(&bond, &rate, settlement).unwrap();
            assert!((bpv - expected_bpv).abs() < 1e-6, "bpv {bpv}");

            let yvbp = BondFunctions::yield_value_basis_point(&bond, &rate, settlement).unwrap()
                * FACE_AMOUNT;
            assert!((yvbp - expected_yvbp).abs() < 1e-9, "yvbp {yvbp}");
        }
    }

    /// [`Bond::yield_rate`] resolves the default settlement date off the
    /// evaluation date and reproduces the treasury yield, closing the instrument
    /// round trip.
    #[test]
    fn the_bond_yield_method_reproduces_the_treasury_yield() {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(treasury_today());
        let (bond, day_counter) = treasury_bond(0.045, settings);
        let irr = bond
            .yield_rate(
                BondPrice::Clean(102.890625),
                day_counter,
                Compounding::Compounded,
                Frequency::Semiannual,
                None,
                None,
                None,
                None,
            )
            .unwrap();
        assert!((irr - 0.041301).abs() < 1e-6, "yield {irr}");
    }

    /// The basis-point value per 100 of notional is the change in the leg's
    /// present value for a one-basis-point bump in the coupon at the same flat
    /// yield: an independent finite difference of the analytic `bps_at_yield`.
    #[test]
    fn the_bps_at_yield_matches_a_one_basis_point_coupon_bump() {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(treasury_today());
        let settlement = Some(Date::new(30, Month::January, 2024));
        let rate = InterestRate::new(
            0.041301,
            Thirty360::with_convention(Thirty360Convention::USA),
            Compounding::Compounded,
            Frequency::Semiannual,
        )
        .unwrap();

        let (base, _) = treasury_bond(0.045, settings.clone());
        let (bumped, _) = treasury_bond(0.045 + 1.0e-4, settings.clone());
        let base_npv = CashFlows::npv_at_yield(
            base.cashflows(),
            &rate,
            settings.as_ref(),
            Some(false),
            settlement,
            None,
        )
        .unwrap();
        let bumped_npv = CashFlows::npv_at_yield(
            bumped.cashflows(),
            &rate,
            settings.as_ref(),
            Some(false),
            settlement,
            None,
        )
        .unwrap();

        let bps = BondFunctions::bps_at_yield(&base, &rate, settlement).unwrap();
        let bump = (bumped_npv - base_npv) * 100.0 / FACE_AMOUNT;
        assert!((bps - bump).abs() < 1e-6, "bps {bps} vs bump {bump}");
    }

    /// A redeemed bond is not tradable, so the yield wrappers guard, while
    /// [`Bond::yield_rate`] short-circuits to a zero yield.
    #[test]
    fn a_redeemed_bond_has_no_yield_analytics() {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(treasury_today());
        let (bond, day_counter) = treasury_bond(0.045, settings);
        let after = Date::new(16, Month::August, 2033);
        let rate = InterestRate::new(
            0.041301,
            day_counter.clone(),
            Compounding::Compounded,
            Frequency::Semiannual,
        )
        .unwrap();

        let err =
            BondFunctions::duration(&bond, &rate, Duration::Modified, Some(after)).unwrap_err();
        assert!(err.message().contains("non tradable"));
        let zero = bond
            .yield_rate(
                BondPrice::Clean(100.0),
                day_counter,
                Compounding::Compounded,
                Frequency::Semiannual,
                Some(after),
                None,
                None,
                None,
            )
            .unwrap();
        assert_eq!(zero, 0.0);
    }

    /// `bonds.cpp::testThirty360BondWithSettlementOn31st` (:1715): a real bond
    /// (CUSIP 3130A0X70, Bloomberg data) whose settlement falls on the 31st,
    /// driving yield, Macaulay duration, convexity and accrued straight through
    /// the `BondFunctions` wrappers at the C++ tolerances.
    #[test]
    fn the_thirty360_bond_reproduces_its_yield_macaulay_duration_and_convexity() {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(Date::new(28, Month::July, 2017));
        let unadjusted = BusinessDayConvention::Unadjusted;
        let day_counter = Thirty360::with_convention(Thirty360Convention::USA);
        let schedule = Schedule::new(
            Date::new(13, Month::February, 2014),
            Date::new(13, Month::August, 2018),
            Period::new(6, TimeUnit::Months),
            us_gov(),
            unadjusted,
            unadjusted,
            DateGeneration::Forward,
            false,
            Date::null(),
            Date::null(),
        );
        let leg = FixedRateLeg::new(schedule)
            .with_notional(100.0)
            .with_coupon_rate(
                0.015,
                day_counter.clone(),
                Compounding::Simple,
                Frequency::Annual,
            )
            .unwrap()
            .with_payment_calendar(us_gov())
            .with_payment_adjustment(unadjusted)
            .build()
            .unwrap();
        let bond = Bond::from_coupons(1, us_gov(), None, leg, settings).unwrap();

        let (comp, freq) = (Compounding::Compounded, Frequency::Semiannual);
        let settlement = Some(Date::new(31, Month::July, 2017));

        let irr = BondFunctions::yield_rate(
            &bond,
            BondPrice::Clean(100.0),
            day_counter.clone(),
            comp,
            freq,
            settlement,
            None,
            None,
            None,
        )
        .unwrap();
        assert!((irr - 0.015).abs() < 1e-4, "yield {irr}");

        let rate = InterestRate::new(irr, day_counter, comp, freq).unwrap();
        let duration =
            BondFunctions::duration(&bond, &rate, Duration::Macaulay, settlement).unwrap();
        assert!((duration - 1.022).abs() < 1e-3, "duration {duration}");

        let convexity = BondFunctions::convexity(&bond, &rate, settlement).unwrap() / 100.0;
        assert!((convexity - 0.015).abs() < 1e-3, "convexity {convexity}");

        let accrued = BondFunctions::accrued_amount(&bond, settlement).unwrap();
        assert!((accrued - 0.7).abs() < 1e-6, "accrued {accrued}");
    }
}
