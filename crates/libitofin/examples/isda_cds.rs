//! Credit-default swaps under the ISDA standard model, and the Markit
//! reconciliation flow.
//!
//! Part A reproduces the twenty-case ISDA/Markit upfront grid of
//! `test-suite/creditdefaultswap.cpp` `testIsdaEngine` (`:567-722`): a USD
//! ISDA-convention discount curve, a flat hazard rate implied off a quoted trade
//! through `implied_hazard_rate`, and a 1 % conventional trade of the same
//! maturity repriced on it. Each case also checks that both sides are worth
//! nothing once the fair upfront is paid.
//!
//! Part B reproduces the two single-record reconciliations (`:759-861` and
//! `:863-960`): the same Markit record traded today, whose value carries an
//! unsettled accrual rebate of a thousand, and traded two years ago, whose
//! rebate settled long before today so the value drops by exactly that thousand.
//!
//! Part C converts a trade quoted with an upfront into a running spread with
//! `conventional_spread`.
//!
//! Every construction step is copied from the port's `markit_oracle` module in
//! `crates/libitofin/src/pricingengines/credit/isdacdsengine.rs`. See
//! `credit_cds.rs` for the mid-point engine and default-curve bootstrapping.

use libitofin::cashflow::CashFlow; // brings amount() on the rebate flow into scope
use libitofin::currency::Currency;
use libitofin::errors::QlResult;
use libitofin::event::Event;
use libitofin::handle::Handle;
use libitofin::indexes::IborIndex;
use libitofin::instrument::Instrument;
use libitofin::instruments::{MakeCreditDefaultSwap, PricingModel, ProtectionSide};
use libitofin::math::interpolations::loglinear::LogLinear;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::credit::{
    AccrualBias, ForwardsInCouponPeriod, IsdaCdsEngine, NumericalFix,
};
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::bootstraphelper::RateHelper;
use libitofin::termstructures::bootstraptraits::Discount;
use libitofin::termstructures::credit::defaulttermstructure::DefaultProbabilityTermStructure;
use libitofin::termstructures::credit::flathazardrate::FlatHazardRate;
use libitofin::termstructures::yields::{DepositRateHelper, PiecewiseYieldCurve, SwapRateHelper};
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::businessdayconvention::BusinessDayConvention;
use libitofin::time::calendars::weekendsonly::WeekendsOnly;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual360::Actual360;
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use libitofin::time::daycounters::thirty360::{Convention, Thirty360};
use libitofin::time::frequency::Frequency;
use libitofin::time::period::Period;
use libitofin::time::timeunit::TimeUnit;
use libitofin::types::{Integer, Rate, Real};

/// `creditdefaultswap.cpp:583-588`: the Markit deposit quotes, in months.
const USD_DEPOSITS: [(Integer, Real); 6] = [
    (1, 0.003081),
    (2, 0.005525),
    (3, 0.007163),
    (6, 0.012413),
    (9, 0.014),
    (12, 0.015488),
];

/// `creditdefaultswap.cpp:598-611`: the Markit swap quotes, in years.
const USD_SWAPS: [(Integer, Real); 14] = [
    (2, 0.011907),
    (3, 0.01699),
    (4, 0.021198),
    (5, 0.02444),
    (6, 0.026937),
    (7, 0.028967),
    (8, 0.030504),
    (9, 0.031719),
    (10, 0.03279),
    (12, 0.034535),
    (15, 0.036217),
    (20, 0.036981),
    (25, 0.037246),
    (30, 0.037605),
];

/// `creditdefaultswap.cpp:643-664`: the ISDA-model upfronts on a ten-million
/// notional, in the term-date / spread / recovery order the loop below visits.
const MARKIT_VALUES: [Real; 20] = [
    -97798.29358,
    -97776.11889,
    914971.5977,
    894985.6298,
    -186921.3594,
    -186839.8148,
    1646623.672,
    1579803.626,
    -274298.9203,
    -274122.4725,
    2279730.93,
    2147972.527,
    -592420.2297,
    -591571.2294,
    3993550.206,
    3545843.418,
    -797501.1422,
    -795915.9787,
    4702034.688,
    4042340.999,
];

/// `creditdefaultswap.cpp:770-771` and `:875-876`: the EUR deposit quotes, in
/// months. Both reconciliation records build the same curve.
const EUR_DEPOSITS: [(Integer, Real); 4] = [
    (1, -0.0056),
    (3, -0.005440),
    (6, -0.005190),
    (12, -0.004930),
];

/// `creditdefaultswap.cpp:781-793` and `:886-898`: the EUR swap quotes, in years.
const EUR_SWAPS: [(Integer, Real); 13] = [
    (2, -0.004820),
    (3, -0.004420),
    (4, -0.003990),
    (5, -0.003520),
    (6, -0.002970),
    (7, -0.002370),
    (8, -0.001760),
    (9, -0.001140),
    (10, -0.000540),
    (12, 0.000570),
    (15, 0.001880),
    (20, 0.002940),
    (30, 0.002820),
];

const NOTIONAL: Real = 10_000_000.0;
const RECONCILE_NOMINAL: Real = 1.0e6;
const RECONCILE_RECOVERY: Real = 0.4;
/// The spread the reconciliation records are quoted at (`:764-826`).
const CONVENTIONAL_SPREAD: Rate = 0.006713;

/// The ISDA-convention forecasting index the C++ fixture builds inline
/// (`creditdefaultswap.cpp:616-618` and `:796-798`).
///
/// The forwarding handle is left empty: both helper families re-point their own
/// clone of the index at the curve being bootstrapped.
fn isda_ibor(tenor: Period, currency: Currency, settings: &Shared<Settings<Date>>) -> IborIndex {
    IborIndex::new(
        "IsdaIbor".to_string(),
        tenor,
        2, // settlement days
        currency,
        WeekendsOnly::new(),
        BusinessDayConvention::ModifiedFollowing,
        false, // end of month
        Actual360::new(),
        Handle::empty(),
        Shared::clone(settings),
    )
}

/// The ISDA-compliant discount curve: deposits in months, swaps in years,
/// bootstrapped log-linearly on discount factors over Act/365F
/// (`creditdefaultswap.cpp:628-632`).
fn isda_curve(
    reference: Date,
    deposits: &[(Integer, Real)],
    swaps: &[(Integer, Real)],
    float_tenor: Period,
    fixed_frequency: Frequency,
    currency: Currency,
    settings: &Shared<Settings<Date>>,
) -> QlResult<Handle<dyn YieldTermStructure>> {
    let mut helpers: Vec<Shared<dyn RateHelper>> = Vec::new();
    for (months, quote) in deposits {
        let index = isda_ibor(
            Period::new(*months, TimeUnit::Months),
            currency.clone(),
            settings,
        );
        helpers.push(DepositRateHelper::from_rate(*quote, &index) as Shared<dyn RateHelper>);
    }
    let float_index = isda_ibor(float_tenor, currency, settings);
    for (years, quote) in swaps {
        helpers.push(SwapRateHelper::from_rate(
            *quote,
            Period::new(*years, TimeUnit::Years),
            WeekendsOnly::new(),
            fixed_frequency,
            BusinessDayConvention::ModifiedFollowing,
            Thirty360::with_convention(Convention::BondBasis),
            &float_index,
        ) as Shared<dyn RateHelper>);
    }
    let curve = PiecewiseYieldCurve::<Discount, LogLinear>::new(
        reference,
        helpers,
        Actual365Fixed::new(),
        LogLinear,
    )?;
    Ok(Handle::new(curve as Shared<dyn YieldTermStructure>))
}

/// The engine over a flat hazard rate and the three fidelity flags C++ spells
/// out (`creditdefaultswap.cpp:686-688`). They select which of the standard
/// model's known approximations the engine reproduces, so it can be graded
/// against the model's own C code rather than against the theory.
fn isda_engine(
    hazard_rate: Rate,
    recovery: Real,
    discount: &Handle<dyn YieldTermStructure>,
    settings: &Shared<Settings<Date>>,
) -> SharedMut<dyn PricingEngine> {
    let probability = Handle::new(shared(FlatHazardRate::moving_with_rate(
        0, // settlement days
        WeekendsOnly::new(),
        hazard_rate,
        Actual365Fixed::new(),
        Shared::clone(settings),
    )) as Shared<dyn DefaultProbabilityTermStructure>);
    shared_mut(
        IsdaCdsEngine::new(
            probability,
            recovery,
            discount.clone(),
            None, // include_settlement_date_flows override
            Shared::clone(settings),
        )
        .with_fidelity(
            NumericalFix::Taylor,
            AccrualBias::HalfDayBias,
            ForwardsInCouponPeriod::Piecewise,
        ),
    ) as SharedMut<dyn PricingEngine>
}

fn main() -> QlResult<()> {
    part_a_the_markit_grid()?;
    println!();
    part_b_reconcile_a_single_record()?;
    println!();
    part_c_convert_an_upfront_to_a_running_spread()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Part A: the twenty-case ISDA/Markit upfront grid.
// ---------------------------------------------------------------------------
fn part_a_the_markit_grid() -> QlResult<()> {
    println!("== Part A: the ISDA/Markit upfront grid (notional 10mm) ==");

    let settings = shared(Settings::<Date>::new());
    let trade_date = Date::new(21, Month::May, 2009);
    settings.set_evaluation_date(trade_date);
    let discount = isda_curve(
        trade_date,
        &USD_DEPOSITS,
        &USD_SWAPS,
        Period::new(3, TimeUnit::Months),
        Frequency::Semiannual,
        Currency::usd(),
        &settings,
    )?;

    let term_dates = [
        Date::new(20, Month::June, 2010),
        Date::new(20, Month::June, 2011),
        Date::new(20, Month::June, 2012),
        Date::new(20, Month::June, 2016),
        Date::new(20, Month::June, 2019),
    ];
    println!("  term date   spread  recov  upfront            Markit             rel. error");
    let mut case = 0;
    for term_date in term_dates {
        for spread in [0.001, 0.1] {
            for recovery in [0.2, 0.4] {
                let trade = |running: Rate| {
                    MakeCreditDefaultSwap::from_term_date(
                        term_date,
                        running,
                        Shared::clone(&settings),
                    )
                    .with_nominal(NOTIONAL)
                };

                // The quoted trade fixes the credit: invert it for the flat
                // hazard rate that prices it to zero on the ISDA engine.
                let hazard_rate = trade(spread).build()?.implied_hazard_rate(
                    0.0, // target NPV
                    &discount,
                    Actual365Fixed::new(),
                    recovery,
                    1.0e-10, // accuracy
                    PricingModel::Isda,
                )?;
                let engine = isda_engine(hazard_rate, recovery, &discount, &settings);

                // The conventional 1 % trade of the same maturity, priced on it.
                let mut conventional = trade(0.01).build()?;
                conventional
                    .base_mut()
                    .set_pricing_engine(SharedMut::clone(&engine));
                let fair_upfront = conventional.fair_upfront()?;
                let upfront = conventional.notional() * fair_upfront;
                let expected = MARKIT_VALUES[case];
                println!(
                    "  {term_date}  {spread:>5}  {recovery:>4}  {upfront:>17.5}  {expected:>17.5}  \
                     {:.2e}",
                    (upfront - expected).abs() / expected.abs()
                );

                // Both sides of the same trade are worth nothing once that
                // upfront is paid.
                for side in [ProtectionSide::Buyer, ProtectionSide::Seller] {
                    let mut at_fair = trade(0.01)
                        .with_upfront_rate(fair_upfront)
                        .with_side(side)
                        .build()?;
                    at_fair
                        .base_mut()
                        .set_pricing_engine(SharedMut::clone(&engine));
                    assert!(at_fair.npv()?.abs() <= 1.0e-6);
                }
                case += 1;
            }
        }
    }
    Ok(())
}

/// The EUR curve both reconciliation records share, and the engine over the flat
/// hazard rate implied off a trade quoted at the conventional spread
/// (`creditdefaultswap.cpp:764-826`). Returns the engine and the term date.
///
/// The evaluation date is set before the helpers are built, since they date
/// themselves off it (D5).
fn reconcile_engine(
    settings: &Shared<Settings<Date>>,
) -> QlResult<(SharedMut<dyn PricingEngine>, Date)> {
    let value_date = Date::new(26, Month::July, 2021);
    settings.set_evaluation_date(value_date);
    let discount = isda_curve(
        value_date,
        &EUR_DEPOSITS,
        &EUR_SWAPS,
        Period::new(6, TimeUnit::Months),
        Frequency::Annual,
        Currency::eur(),
        settings,
    )?;
    let maturity = Date::new(20, Month::June, 2026);
    let hazard_rate = MakeCreditDefaultSwap::from_term_date(
        maturity,
        CONVENTIONAL_SPREAD,
        Shared::clone(settings),
    )
    .with_nominal(RECONCILE_NOMINAL)
    .build()?
    .implied_hazard_rate(
        0.0,
        &discount,
        Actual365Fixed::new(),
        RECONCILE_RECOVERY,
        1.0e-10,
        PricingModel::Isda,
    )?;
    Ok((
        isda_engine(hazard_rate, RECONCILE_RECOVERY, &discount, settings),
        maturity,
    ))
}

// ---------------------------------------------------------------------------
// Part B: the two single-record Markit reconciliations.
// ---------------------------------------------------------------------------
fn part_b_reconcile_a_single_record() -> QlResult<()> {
    println!("== Part B: one Markit record, traded today and traded in the past ==");

    // Traded today: the rebate is still unsettled at the value date, so it is
    // part of what the trade is worth.
    let settings = shared(Settings::<Date>::new());
    let (engine, maturity) = reconcile_engine(&settings)?;
    let mut today_trade =
        MakeCreditDefaultSwap::from_term_date(maturity, 0.01, Shared::clone(&settings))
            .with_nominal(RECONCILE_NOMINAL)
            .build()?;
    today_trade.base_mut().set_pricing_engine(engine);

    let npv = today_trade.npv()?;
    let upfront = today_trade.notional() * today_trade.fair_upfront()?;
    // C++'s own discount to cash settlement: the ratio of the upfront to the
    // value, which the derived accrual then divides back out.
    let df = upfront / npv;
    let derived_accrual =
        df * (npv - today_trade.default_leg_npv()? - today_trade.coupon_leg_npv()?);
    let rebate = today_trade
        .accrual_rebate()
        .expect("a rebating trade carries the flow");

    println!("  traded today     value    = {npv:.4}   (Markit -16070.7)");
    println!("  traded today     upfront  = {upfront:.4}");
    println!("  accrual off the legs      = {derived_accrual:.6}   (expected 1000)");
    println!("  accrual on the rebate     = {:.6}", rebate.amount()?);
    println!(
        "  rebate settles           : {}",
        Event::date(rebate.as_ref())
    );

    // The same record traded two years ago: its rebate settled long before
    // today, so the value drops by exactly the thousand the rebate carried.
    let settings = shared(Settings::<Date>::new());
    let (engine, maturity) = reconcile_engine(&settings)?;
    let mut past_trade =
        MakeCreditDefaultSwap::from_term_date(maturity, 0.01, Shared::clone(&settings))
            .with_nominal(RECONCILE_NOMINAL)
            .with_trade_date(Date::new(20, Month::July, 2019))
            .build()?;
    past_trade.base_mut().set_pricing_engine(engine);

    let npv = past_trade.npv()?;
    let residual = npv - past_trade.default_leg_npv()? - past_trade.coupon_leg_npv()?;
    println!("  traded 2019-07-20 value   = {npv:.4}   (Markit -17070.77)");
    println!("  accrual off the legs      = {residual:.6}   (expected 0)");
    Ok(())
}

// ---------------------------------------------------------------------------
// Part C: an upfront quotation converted into a running spread.
// ---------------------------------------------------------------------------
fn part_c_convert_an_upfront_to_a_running_spread() -> QlResult<()> {
    println!("== Part C: conventional_spread on an upfront-quoted trade ==");

    let settings = shared(Settings::<Date>::new());
    let trade_date = Date::new(21, Month::May, 2009);
    settings.set_evaluation_date(trade_date);
    let discount = isda_curve(
        trade_date,
        &USD_DEPOSITS,
        &USD_SWAPS,
        Period::new(3, TimeUnit::Months),
        Frequency::Semiannual,
        Currency::usd(),
        &settings,
    )?;
    let term_date = Date::new(20, Month::June, 2016);

    // A five-year trade paying 1 % running plus a 5 % upfront, still unsettled
    // at the evaluation date - which is what stops the conversion collapsing
    // back onto the running spread.
    let quoted = MakeCreditDefaultSwap::from_term_date(term_date, 0.01, Shared::clone(&settings))
        .with_nominal(NOTIONAL)
        .with_upfront_rate(0.05)
        .build()?;

    let hazard_rate = quoted.implied_hazard_rate(
        0.0,
        &discount,
        Actual365Fixed::new(),
        RECONCILE_RECOVERY,
        1.0e-9,
        PricingModel::Isda,
    )?;
    let conventional = quoted.conventional_spread(
        RECONCILE_RECOVERY,
        &discount,
        Actual365Fixed::new(),
        PricingModel::Isda,
    )?;

    println!("  quoted            : 1 % running + 5 % upfront to {term_date}");
    println!("  implied hazard    : {hazard_rate:.10}");
    println!("  conventional spread: {:.10}", conventional);
    Ok(())
}
