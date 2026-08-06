//! Vanilla fixed-vs-float interest-rate swap priced with `DiscountingSwapEngine`.
//!
//! Built through `MakeVanillaSwap` (the QuantLib `MakeVanillaSwap` port), which
//! derives both schedules, calls `VanillaSwap::new`, and attaches a
//! `DiscountingSwapEngine` over the index's forwarding curve. A 5Y EUR swap:
//! annual Thirty360(BondBasis) fixed leg (EUR currency default) vs semiannual
//! Euribor 6M / Actual360 floating leg, both forecasting and discounting off a
//! flat 2% continuously-compounded curve.
//!
//! D5: `Settings` is an explicit `Shared` handle carrying the evaluation date
//! (no global singleton). D11: because the swap starts on a future effective
//! date, every floating fixing lies in the future and is forecast off the curve,
//! so no past fixing needs to be seeded into `Settings`' fixing store.

use libitofin::handle::Handle;
use libitofin::indexes::IborIndex;
use libitofin::indexes::ibor::Euribor;
use libitofin::instrument::Instrument; // brings the `npv()` method into scope
use libitofin::instruments::MakeVanillaSwap;
use libitofin::interestrate::Compounding;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual360::Actual360;
use libitofin::time::frequency::Frequency;
use libitofin::time::period::Period;
use libitofin::time::timeunit::TimeUnit;

fn main() -> libitofin::errors::QlResult<()> {
    // --- D5: explicit Settings with an evaluation date (no global clock) ---
    let today = Date::new(7, Month::July, 2026);
    let settings: Shared<Settings<Date>> = shared(Settings::<Date>::new());
    settings.set_evaluation_date(today);

    // --- Flat 2% forecasting/discounting curve, wrapped in a Handle ---
    let curve: Handle<dyn YieldTermStructure> = Handle::new(shared(FlatForward::with_rate(
        today,
        0.02,
        Actual360::new(),
        Compounding::Continuous,
        Frequency::Annual,
    ))
        as Shared<dyn YieldTermStructure>);

    // --- 6-month Euribor forecasting off that curve ---
    let index: Shared<IborIndex> = shared(Euribor::six_months(curve, Shared::clone(&settings)));

    // --- Build + price a 5Y payer swap at a fixed 3% via MakeVanillaSwap ---
    // Effective date after the evaluation date => all fixings are forecast
    // (no stored past fixing required). build() attaches DiscountingSwapEngine.
    let mut swap = MakeVanillaSwap::new(
        Period::new(5, TimeUnit::Years),
        Shared::clone(&index),
        Some(0.03),                     // fixed rate; pass None to fill the fair rate
        Period::new(0, TimeUnit::Days), // no forward start
        Shared::clone(&settings),
    )
    .with_effective_date(Date::new(9, Month::July, 2026))
    .build()?;

    // npv() (Instrument trait) prices via the attached DiscountingSwapEngine.
    let npv = swap.npv()?;
    // fair (par) swap rate is on the FixedVsFloatingSwap base, needs &mut.
    let fair_rate = swap.fixed_vs_floating_mut().fair_rate()?;

    println!("Swap NPV:  {npv:.6}");
    println!("Fair rate: {:.6}%", fair_rate * 100.0);
    Ok(())
}
