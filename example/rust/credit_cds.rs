//! Credit-default swap pricing and credit-curve bootstrapping with `libitofin`.
//!
//! Part A prices a 5Y CDS with `MidPointCdsEngine` off a flat hazard-rate
//! default curve, printing NPV and fair spread.
//!
//! Part B bootstraps a `PiecewiseDefaultCurve` from four `SpreadCdsHelper`
//! CDS quotes, then prints the bootstrapped hazard-rate nodes, the survival
//! probabilities at each pillar, and the fair spreads a contract rebuilt off
//! the curve reproduces (each returns its input quote to ~1e-6).
//!
//! Part C settles the protection leg against a `FaceValueAccrualClaim` on a
//! reference bond instead of the default `FaceValueClaim`: the accrual the
//! protection buyer no longer collects is surrendered out of the claim, so the
//! protection leg pays less.
//!
//! Every signature below is taken verbatim from the crate's own tests:
//! `midpointcdsengine.rs` (mod `oracle`), `piecewisedefaultcurve.rs`
//! (mod `tests`) and `claim.rs` (mod `tests`). Notes on D5 (explicit `Settings`,
//! no global singleton) and D2 (`Handle`) inline. See `isda_cds.rs` for the ISDA
//! standard-model engine and the Markit reconciliation flow.

use libitofin::cashflow::{CashFlow, Leg};
use libitofin::cashflows::FixedRateCoupon;
use libitofin::errors::QlResult;
use libitofin::handle::Handle;
use libitofin::instrument::Instrument; // brings npv(), base_mut(), recalculate()
use libitofin::instruments::{
    Bond, CdsTerms, Claim, CreditDefaultSwap, FaceValueAccrualClaim, FaceValueClaim,
    MakeCreditDefaultSwap, ProtectionSide,
};
use libitofin::interestrate::Compounding;
use libitofin::math::interpolations::flat::BackwardFlat;
use libitofin::pricingengine::PricingEngine; // needed for the `as SharedMut<dyn PricingEngine>` cast
use libitofin::pricingengines::credit::MidPointCdsEngine;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::credit::defaultprobabilityhelpers::{
    DefaultProbabilityHelper, SpreadCdsHelper,
};
use libitofin::termstructures::credit::defaulttermstructure::DefaultProbabilityTermStructure;
use libitofin::termstructures::credit::flathazardrate::FlatHazardRate;
use libitofin::termstructures::credit::piecewisedefaultcurve::PiecewiseDefaultCurve;
use libitofin::termstructures::credit::probabilitytraits::HazardRate;
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::businessdayconvention::BusinessDayConvention;
use libitofin::time::calendars::target::Target;
use libitofin::time::date::{Date, Month};
use libitofin::time::dategenerationrule::DateGeneration;
use libitofin::time::daycounters::actual360::Actual360;
use libitofin::time::daycounters::thirty360::{Convention, Thirty360};
use libitofin::time::frequency::Frequency;
use libitofin::time::period::Period;
use libitofin::time::schedule::{MakeSchedule, Schedule};
use libitofin::time::timeunit::TimeUnit;
use libitofin::types::{Integer, Real};

const RECOVERY: Real = 0.4;
const SETTLEMENT_DAYS: Integer = 1;

fn main() -> QlResult<()> {
    // A TARGET business day; the evaluation date every curve/contract shares (D5).
    let today = Date::new(9, Month::June, 2006);

    part_a_price_a_cds(today)?;
    println!();
    part_b_bootstrap_a_default_curve(today)?;
    println!();
    part_c_settle_against_a_reference_bond(today)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Part A: price a CDS with MidPointCdsEngine off a FlatHazardRate curve.
// ---------------------------------------------------------------------------
fn part_a_price_a_cds(today: Date) -> QlResult<()> {
    println!("== Part A: MidPointCdsEngine over a flat hazard-rate curve ==");

    // One explicit Settings drives the curves, the contract and the engine (D5).
    let settings = shared(Settings::<Date>::new());
    settings.set_evaluation_date(today);

    // Flat 3% discount curve (continuous / annual are the C++ FlatForward defaults),
    // wrapped in a Handle<dyn YieldTermStructure> (D2).
    let discount: Handle<dyn YieldTermStructure> = Handle::new(shared(FlatForward::with_rate(
        today,
        0.03,
        Actual360::new(),
        Compounding::Continuous,
        Frequency::Annual,
    ))
        as Shared<dyn YieldTermStructure>);

    // Flat 2% hazard rate as the default-probability curve (survival = exp(-h t)).
    let hazard: Handle<dyn DefaultProbabilityTermStructure> = Handle::new(shared(
        FlatHazardRate::with_rate(today, 0.02, Actual360::new()),
    )
        as Shared<dyn DefaultProbabilityTermStructure>);

    // A 5Y semiannual premium schedule. Backward generation keeps it pre-Big-Bang,
    // so the default trade date is protection_start - 1 and the accrual rebate
    // (a zero-amount flow here) is allowed.
    let schedule = MakeSchedule::new()
        .from(today)
        .to(today + Period::new(5, TimeUnit::Years))
        .with_frequency(Frequency::Semiannual)
        .with_calendar(Target::new())
        .with_convention(BusinessDayConvention::Following)
        .with_termination_date_convention(BusinessDayConvention::Unadjusted)
        .backwards()
        .build();

    // Protection-buyer CDS, notional 10mm, 1% running spread, ACT/360 premium accrual.
    // The final two bools are settles_accrual and pays_at_default_time (C++ defaults).
    let mut cds = CreditDefaultSwap::new(
        ProtectionSide::Buyer,
        10_000_000.0,
        0.01,
        schedule,
        BusinessDayConvention::Following,
        Actual360::new(),
        true,
        true,
        Shared::clone(&settings),
    )?;

    // MidPointCdsEngine::new(default-prob handle, recovery, discount handle,
    // include_settlement_date_flows override, settings).
    let engine = MidPointCdsEngine::new(
        hazard.clone(),
        RECOVERY,
        discount.clone(),
        None,
        Shared::clone(&settings),
    );
    cds.base_mut()
        .set_pricing_engine(shared_mut(engine) as SharedMut<dyn PricingEngine>);

    let npv = cds.npv()?;
    let fair_spread = cds.fair_spread()?;
    println!("  maturity              : {}", cds.maturity());
    println!("  NPV (buyer)           : {npv:.4}");
    println!("  fair spread           : {fair_spread:.10}");

    // The default curve can be queried directly through its handle.
    let survival_to_maturity = hazard
        .current_link()?
        .survival_probability_date(cds.maturity(), false)?;
    println!("  survival to maturity  : {survival_to_maturity:.10}");
    Ok(())
}

// ---------------------------------------------------------------------------
// Part B: bootstrap a PiecewiseDefaultCurve from SpreadCdsHelper quotes.
// ---------------------------------------------------------------------------
fn part_b_bootstrap_a_default_curve(today: Date) -> QlResult<()> {
    println!("== Part B: PiecewiseDefaultCurve bootstrapped from CDS spreads ==");

    let quotes: [Real; 4] = [0.005, 0.006, 0.007, 0.009];
    let tenors: [i32; 4] = [1, 2, 3, 5];

    let settings = shared(Settings::<Date>::new());
    settings.set_evaluation_date(today);
    // The helpers price their own contracts inside the bootstrap; this flag must
    // be set before the first read triggers it.
    settings.set_include_todays_cash_flows(Some(true));

    // 30/360 bond-basis premium accrual; flat 6% discount curve.
    let day_counter = Thirty360::with_convention(Convention::BondBasis);
    let discount: Handle<dyn YieldTermStructure> = Handle::new(shared(FlatForward::with_rate(
        today,
        0.06,
        Actual360::new(),
        Compounding::Continuous,
        Frequency::Annual,
    ))
        as Shared<dyn YieldTermStructure>);

    // One SpreadCdsHelper per quoted CDS spread. Signature:
    //   SpreadCdsHelper::new(running_spread_handle, tenor, settlement_days,
    //     calendar, frequency, payment_convention, date_generation_rule,
    //     day_counter, recovery_rate, discount_handle, settings) -> Shared<SpreadCdsHelper>
    // TwentiethIMM is the accepted (pre-Big-Bang) rule; the three CDS rules are
    // rejected (they need the unported cdsMaturity).
    let helpers: Vec<Shared<dyn DefaultProbabilityHelper>> = quotes
        .iter()
        .zip(tenors)
        .map(|(quote, n)| {
            SpreadCdsHelper::new(
                Handle::new(shared(SimpleQuote::new(*quote)) as Shared<dyn Quote>),
                Period::new(n, TimeUnit::Years),
                SETTLEMENT_DAYS,
                Target::new(),
                Frequency::Quarterly,
                BusinessDayConvention::Following,
                DateGeneration::TwentiethIMM,
                day_counter.clone(),
                RECOVERY,
                discount.clone(),
                Shared::clone(&settings),
            )
            .expect("TwentiethIMM is an accepted date-generation rule")
                as Shared<dyn DefaultProbabilityHelper>
        })
        .collect();

    // Bootstrap is lazy: construction lays down no nodes; the first read solves.
    // Only HazardRate x BackwardFlat is wired (see the module's Scope note).
    let curve = PiecewiseDefaultCurve::<HazardRate, BackwardFlat>::new(
        today,
        helpers.clone(),
        day_counter.clone(),
        BackwardFlat,
    )?;

    // Bootstrapped (date, hazard-rate) nodes. nodes() triggers the bootstrap.
    println!("  bootstrapped hazard-rate nodes:");
    for (date, hazard) in curve.nodes()? {
        println!("    {date}  h = {hazard:.10}");
    }

    // Survival probability at each pillar (the helper's latest/pillar date).
    println!("  survival probabilities at pillars:");
    for (helper, n) in helpers.iter().zip(tenors) {
        let pillar = helper.latest_date();
        let survival = curve.survival_probability_date(pillar, false)?;
        println!("    {n}Y  pillar {pillar}  S = {survival:.10}");
    }

    // Round trip: rebuild each pillar's CDS and reprice it off the bootstrapped
    // curve; the fair spread returns the input quote to ~1e-6.
    let curve_handle: Handle<dyn DefaultProbabilityTermStructure> =
        Handle::new(Shared::clone(&curve) as Shared<dyn DefaultProbabilityTermStructure>);
    println!("  fair spreads reproduced off the curve:");
    for (quote, n) in quotes.iter().zip(tenors) {
        let tenor = Period::new(n, TimeUnit::Years);
        let protection_start = today + SETTLEMENT_DAYS;
        let mut cds = CreditDefaultSwap::with_terms(
            ProtectionSide::Buyer,
            1.0,
            *quote,
            round_trip_schedule(today, tenor),
            BusinessDayConvention::Following,
            day_counter.clone(),
            CdsTerms {
                protection_start: Some(protection_start),
                ..CdsTerms::default()
            },
            Shared::clone(&settings),
        )?;
        let engine = MidPointCdsEngine::new(
            curve_handle.clone(),
            RECOVERY,
            discount.clone(),
            None,
            Shared::clone(&settings),
        );
        cds.base_mut()
            .set_pricing_engine(shared_mut(engine) as SharedMut<dyn PricingEngine>);

        let fair = cds.fair_spread()?;
        println!("    {n}Y  input {quote:.6}  ->  fair {fair:.10}");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Part C: settle the protection leg against a FaceValueAccrualClaim.
// ---------------------------------------------------------------------------
fn part_c_settle_against_a_reference_bond(today: Date) -> QlResult<()> {
    println!("== Part C: FaceValueClaim against FaceValueAccrualClaim ==");

    let settings = shared(Settings::<Date>::new());
    settings.set_evaluation_date(today);

    // The reference security: two annual 5% coupons on a notional of 100,
    // running `today` to `today` + 2Y (the `claim.rs` par-bond fixture).
    let first_period_end = today + Period::new(1, TimeUnit::Years);
    let redemption = today + Period::new(2, TimeUnit::Years);
    let coupons: Leg = vec![
        shared(FixedRateCoupon::from_rate(
            first_period_end,
            100.0,
            0.05,
            Actual360::new(),
            today,
            first_period_end,
            None,
            None,
            None,
        )) as Shared<dyn CashFlow>,
        shared(FixedRateCoupon::from_rate(
            redemption,
            100.0,
            0.05,
            Actual360::new(),
            first_period_end,
            redemption,
            None,
            None,
            None,
        )) as Shared<dyn CashFlow>,
    ];
    let reference_security = shared(Bond::from_coupons(
        2,
        Target::new(),
        Some(today),
        coupons,
        Shared::clone(&settings),
    )?);

    // The two claims side by side at a default half way through the first
    // coupon period: the 5% Actual/360 coupon accrued there, normalised by the
    // reference notional, is what the accrual claim surrenders.
    let mid_first_period = today + Period::new(6, TimeUnit::Months);
    let accrual_claim = FaceValueAccrualClaim::new(Shared::clone(&reference_security));
    println!(
        "  face value         claim on 100 at R=0.4 : {:.10}",
        FaceValueClaim.amount(&mid_first_period, 100.0, RECOVERY)?
    );
    println!(
        "  face value accrual claim on 100 at R=0.4 : {:.10}",
        accrual_claim.amount(&mid_first_period, 100.0, RECOVERY)?
    );

    // The same contract priced with each claim. `with_claim` overrides the
    // default FaceValueClaim the builder fills in.
    let discount: Handle<dyn YieldTermStructure> = Handle::new(shared(FlatForward::with_rate(
        today,
        0.03,
        Actual360::new(),
        Compounding::Continuous,
        Frequency::Annual,
    ))
        as Shared<dyn YieldTermStructure>);
    let hazard: Handle<dyn DefaultProbabilityTermStructure> = Handle::new(shared(
        FlatHazardRate::with_rate(today, 0.02, Actual360::new()),
    )
        as Shared<dyn DefaultProbabilityTermStructure>);
    let term_date = today + Period::new(18, TimeUnit::Months);

    for (label, claim) in [
        ("face value        ", None),
        (
            "face value accrual",
            Some(shared(accrual_claim) as Shared<dyn Claim>),
        ),
    ] {
        let mut builder =
            MakeCreditDefaultSwap::from_term_date(term_date, 0.01, Shared::clone(&settings))
                .with_nominal(10_000_000.0);
        if let Some(claim) = claim {
            builder = builder.with_claim(claim);
        }
        let mut cds = builder.build()?;
        cds.base_mut()
            .set_pricing_engine(shared_mut(MidPointCdsEngine::new(
                hazard.clone(),
                RECOVERY,
                discount.clone(),
                None,
                Shared::clone(&settings),
            )) as SharedMut<dyn PricingEngine>);
        println!(
            "  {label} : protection leg {:>14.4}   NPV {:>12.4}",
            cds.default_leg_npv()?,
            cds.npv()?
        );
    }
    Ok(())
}

/// The round-trip contract's schedule: quarterly TwentiethIMM, starting at the
/// rolled protection start and ending a tenor past `today`.
fn round_trip_schedule(today: Date, tenor: Period) -> Schedule {
    let calendar = Target::new();
    let start_date = calendar.adjust(today + SETTLEMENT_DAYS, BusinessDayConvention::Following);
    Schedule::new(
        start_date,
        today + tenor,
        Period::try_from(Frequency::Quarterly).expect("a quarterly period"),
        calendar,
        BusinessDayConvention::Following,
        BusinessDayConvention::Unadjusted,
        DateGeneration::TwentiethIMM,
        false,
        Date::null(),
        Date::null(),
    )
}
