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
//! Every signature below is taken verbatim from the crate's own tests:
//! `midpointcdsengine.rs` (mod `oracle`) and `piecewisedefaultcurve.rs`
//! (mod `tests`). Notes on D5 (explicit `Settings`, no global singleton) and
//! D2 (`Handle`) inline.

use libitofin::errors::QlResult;
use libitofin::handle::Handle;
use libitofin::instrument::Instrument; // brings npv(), base_mut(), recalculate()
use libitofin::instruments::{CdsTerms, CreditDefaultSwap, ProtectionSide};
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
    println!("  maturity              : {:?}", cds.maturity());
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
        println!("    {date:?}  h = {hazard:.10}");
    }

    // Survival probability at each pillar (the helper's latest/pillar date).
    println!("  survival probabilities at pillars:");
    for (helper, n) in helpers.iter().zip(tenors) {
        let pillar = helper.latest_date();
        let survival = curve.survival_probability_date(pillar, false)?;
        println!("    {n}Y  pillar {pillar:?}  S = {survival:.10}");
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
