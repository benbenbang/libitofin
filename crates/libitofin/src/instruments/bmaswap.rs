//! Municipal BMA versus a fraction of Ibor, from QuantLib's `bmaswap.cpp`.
use crate::cashflows::{AverageBMALeg, IborLeg};
use crate::errors::QlResult;
use crate::indexes::{BMAIndex, IborIndex, InterestRateIndex};
use crate::instrument::{Instrument, InstrumentBase};
use crate::instruments::{Swap, swap::SwapType};
use crate::pricingengine::{Arguments, Results};
use crate::settings::Settings;
use crate::shared::Shared;
use crate::time::{date::Date, daycounter::DayCounter, schedule::Schedule};

/// Two-leg municipal swap. Payer pays BMA and receives the Ibor leg.
pub struct BMASwap {
    swap: Swap,
    swap_type: SwapType,
    nominal: f64,
    libor_fraction: f64,
    libor_spread: f64,
}
impl BMASwap {
    /// Constructs the two observed legs, retaining indexes, history and settings.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        swap_type: SwapType,
        nominal: f64,
        libor_schedule: Schedule,
        libor_fraction: f64,
        libor_spread: f64,
        libor_index: Shared<IborIndex>,
        libor_day_count: DayCounter,
        bma_schedule: Schedule,
        bma_index: Shared<BMAIndex>,
        bma_day_count: DayCounter,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<Self> {
        crate::require!(
            nominal.is_finite() && libor_fraction.is_finite() && libor_spread.is_finite(),
            "BMA swap inputs must be finite"
        );
        crate::require!(
            Shared::ptr_eq(libor_index.base().settings(), &settings)
                && Shared::ptr_eq(bma_index.base().settings(), &settings),
            "BMA swap indexes must share settings"
        );
        let lc = libor_schedule.business_day_convention();
        let bc = bma_schedule.business_day_convention();
        let libor = IborLeg::new(libor_schedule, libor_index)
            .with_notional(nominal)
            .with_payment_day_counter(libor_day_count)
            .with_payment_adjustment(lc)
            .with_gearing(libor_fraction)
            .with_spread(libor_spread)
            .build()?;
        let bma = AverageBMALeg::new(bma_schedule, bma_index)
            .with_notional(nominal)
            .with_payment_day_counter(bma_day_count)
            .with_payment_adjustment(bc)
            .build()?;
        let payer = swap_type == SwapType::Payer;
        Ok(Self {
            swap: Swap::new(vec![libor, bma], vec![!payer, payer], settings)?,
            swap_type,
            nominal,
            libor_fraction,
            libor_spread,
        })
    }
    /// Base swap, including generic leg results and dates.
    pub fn swap(&self) -> &Swap {
        &self.swap
    }
    /// Mutable base swap for pricing and leg results.
    pub fn swap_mut(&mut self) -> &mut Swap {
        &mut self.swap
    }
    /// Payer/receiver orientation.
    pub fn swap_type(&self) -> SwapType {
        self.swap_type
    }
    /// Leg notional.
    pub fn nominal(&self) -> f64 {
        self.nominal
    }
    /// Contractual Ibor fraction.
    pub fn libor_fraction(&self) -> f64 {
        self.libor_fraction
    }
    /// Contractual Ibor spread.
    pub fn libor_spread(&self) -> f64 {
        self.libor_spread
    }
    /// Fraction of Ibor producing zero NPV at the contractual spread.
    pub fn fair_libor_fraction(&mut self) -> QlResult<f64> {
        let spread_npv = self.libor_spread / 1e-4 * self.swap.leg_bps(0)?;
        let pure = self.swap.leg_npv(0)? - spread_npv;
        crate::require!(pure != 0.0, "null Ibor NPV");
        Ok(-self.libor_fraction * (self.swap.leg_npv(1)? + spread_npv) / pure)
    }
    /// Additive Ibor spread producing zero NPV at the contractual fraction.
    pub fn fair_libor_spread(&mut self) -> QlResult<f64> {
        let bps = self.swap.leg_bps(0)?;
        crate::require!(bps != 0.0, "null Ibor BPS");
        Ok(self.libor_spread - self.swap.npv()? / (bps / 1e-4))
    }
}
impl Instrument for BMASwap {
    fn base(&self) -> &InstrumentBase {
        self.swap.base()
    }
    fn base_mut(&mut self) -> &mut InstrumentBase {
        self.swap.base_mut()
    }
    fn is_expired(&self) -> QlResult<bool> {
        self.swap.is_expired()
    }
    fn setup_arguments(&self, args: &mut dyn Arguments) -> QlResult<()> {
        self.swap.setup_arguments(args)
    }
    fn fetch_results(&mut self, results: &dyn Results) -> QlResult<()> {
        self.swap.fetch_results(results)
    }
    fn setup_expired(&mut self) {
        self.swap.setup_expired();
    }
}
