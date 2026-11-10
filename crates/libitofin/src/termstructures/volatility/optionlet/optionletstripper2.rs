//! QuantLib's ATM-cap volatility correction layered over OptionletStripper1.
use super::{
    OptionletStripper1, OptionletVolatilityStructure, StrippedOptionletAdapter,
    StrippedOptionletBase,
};
use crate::cashflows::Coupon;
use crate::errors::QlResult;
use crate::event::Event;
use crate::indexes::interestrateindex::InterestRateIndex;
use crate::instrument::Instrument;
use crate::instruments::{CapFloorType, MakeCapFloor};
use crate::math::comparison::close;
use crate::math::solver1d::Solver1D;
use crate::math::solvers1d::brent::Brent;
use crate::option::OptionType;
use crate::patterns::lazyobject::LazyObject;
use crate::patterns::observable::{AsObservable, Observable, Observer};
use crate::pricingengine::PricingEngine;
use crate::pricingengines::blackformula::bachelier_black_formula;
use crate::pricingengines::{BachelierCapFloorEngine, BlackCapFloorEngine, black_formula};
use crate::quotes::make_quote_handle;
use crate::shared::{Shared, SharedMut, shared, shared_mut};
use crate::termstructures::TermStructure;
use crate::termstructures::volatility::{
    CapFloorTermVolCurve, CapFloorTermVolatilityStructure, VolatilityType,
};
use crate::time::{
    businessdayconvention::BusinessDayConvention, calendar::Calendar, date::Date,
    daycounter::DayCounter, period::Period, timeunit::TimeUnit,
};
use crate::types::{Natural, Rate, Real, Time, Volatility};
use std::cell::RefCell;

const MAX_SPREAD: Volatility = 0.1;

#[derive(Default)]
struct Correction {
    strikes: Vec<Vec<Rate>>,
    vols: Vec<Vec<Volatility>>,
    atm_strikes: Vec<Rate>,
    atm_prices: Vec<Real>,
    spreads: Vec<Volatility>,
}

struct Updater {
    lazy: SharedMut<LazyObject>,
    observable: Shared<Observable>,
}
impl Observer for Updater {
    fn update(&mut self) {
        self.lazy.borrow_mut().invalidate_silently();
        self.observable.notify_observers();
    }
}

/// Inserts ATM cap strikes with implied additive volatility corrections.
///
/// ATM caps are priced, and the spreads solved, with the source's volatility type:
/// shifted-lognormal Black with the source displacement, or normal Bachelier.
pub struct OptionletStripper2 {
    source: Shared<OptionletStripper1>,
    atm_curve: Shared<CapFloorTermVolCurve>,
    state: RefCell<Correction>,
    lazy: SharedMut<LazyObject>,
    observable: Shared<Observable>,
    _updater: SharedMut<Updater>,
}

impl OptionletStripper2 {
    /// Builds a correction layer retaining its source and live ATM quote curve.
    pub fn new(
        source: Shared<OptionletStripper1>,
        atm_curve: Shared<CapFloorTermVolCurve>,
    ) -> QlResult<Self> {
        crate::require!(
            source.day_counter() == atm_curve.day_counter(),
            "different day counters provided"
        );
        let lazy = shared_mut(LazyObject::new(true));
        let observable = shared(Observable::new());
        let updater = shared_mut(Updater {
            lazy: lazy.clone(),
            observable: observable.clone(),
        });
        let observer = updater.clone() as SharedMut<dyn Observer>;
        AsObservable::observable(source.as_ref()).register_observer(&observer);
        atm_curve.observable().register_observer(&observer);
        Ok(Self {
            source,
            atm_curve,
            state: RefCell::new(Correction::default()),
            lazy,
            observable,
            _updater: updater,
        })
    }

    /// Rebuilds the correction transactionally following upstream changes.
    pub fn calculate(&self) -> QlResult<()> {
        if !self.lazy.borrow_mut().start_calculation() {
            return Ok(());
        }
        let result = self.perform_calculations();
        self.lazy.borrow_mut().finish_calculation(&result);
        result
    }

    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    fn perform_calculations(&self) -> QlResult<()> {
        let index = self.source.base().ibor_index();
        let settings = index.base().settings().clone();
        let curve_handle = index.forwarding_term_structure().clone();
        let curve = curve_handle.current_link()?;
        let day_counter = self
            .source
            .day_counter()
            .ok_or_else(|| crate::errors::QlError::new("missing day counter", file!(), line!()))?;
        let adapter = StrippedOptionletAdapter::new(self.source.clone(), settings.clone())?;
        adapter.enable_extrapolation();
        let times = self.source.optionlet_fixing_times()?;
        let first_optionlet_date = *self
            .source
            .optionlet_fixing_dates()?
            .first()
            .ok_or_else(|| crate::errors::QlError::new("empty optionlet grid", file!(), line!()))?;
        let mut state = Correction::default();
        for i in 0..self.source.optionlet_maturities() {
            state.strikes.push(self.source.optionlet_strikes(i)?);
            state.vols.push(self.source.optionlet_volatilities(i)?);
        }
        for &tenor in self.atm_curve.option_tenors() {
            let atm_vol = self.atm_curve.volatility_tenor(tenor, 33.3333, false)?;
            let cap = MakeCapFloor::new(
                CapFloorType::Cap,
                tenor,
                index.clone(),
                0.0,
                Period::new(0, TimeUnit::Days),
                settings.clone(),
            )
            .build()?;
            crate::require!(
                !cap.coupons().is_empty(),
                "cap/floor length {tenor} does not contain any optionlets"
            );
            let strike = cap.atm_rate(curve.as_ref())?;
            let normal = self.volatility_type() == VolatilityType::Normal;
            let engine: SharedMut<dyn PricingEngine> = if normal {
                shared_mut(BachelierCapFloorEngine::with_flat_vol(
                    curve_handle.clone(),
                    make_quote_handle(atm_vol).handle(),
                    day_counter.clone(),
                    settings.clone(),
                )?)
            } else {
                shared_mut(BlackCapFloorEngine::with_flat_vol(
                    curve_handle.clone(),
                    make_quote_handle(atm_vol).handle(),
                    day_counter.clone(),
                    self.displacement(),
                    settings.clone(),
                )?)
            };
            let mut cap = MakeCapFloor::new(
                CapFloorType::Cap,
                tenor,
                index.clone(),
                strike,
                Period::new(0, TimeUnit::Days),
                settings.clone(),
            )
            .with_pricing_engine(engine)
            .build()?;
            let price = cap.npv()?;
            let mut count = cap.coupons().len().min(times.len());
            if cap.coupons()[0].fixing_date() < first_optionlet_date {
                crate::require!(
                    times.len() >= 2,
                    "cannot extrapolate an ATM cap/floor fixing before the first optionlet date from fewer than two optionlet nodes"
                );
                count = count.max(2);
            }
            let inputs = cap
                .coupons()
                .iter()
                .map(|coupon| {
                    let time = adapter.time_from_reference(coupon.fixing_date())?;
                    let vol = adapter.volatility(time, strike, true)?;
                    let annuity = coupon.nominal()
                        * coupon.accrual_period()
                        * curve.discount_date(coupon.date(), false)?;
                    Ok((coupon.index_fixing()?, time.sqrt(), vol, annuity))
                })
                .collect::<QlResult<Vec<_>>>()?;
            let objective = |spread: Real| {
                inputs
                    .iter()
                    .map(|&(forward, root_time, vol, annuity)| {
                        let std_dev = (vol + spread) * root_time;
                        if normal {
                            bachelier_black_formula(
                                OptionType::Call,
                                strike,
                                forward,
                                std_dev,
                                annuity,
                            )
                        } else {
                            black_formula(
                                OptionType::Call,
                                strike,
                                forward,
                                std_dev,
                                annuity,
                                self.displacement(),
                            )
                        }
                    })
                    .sum::<QlResult<Real>>()
                    .map(|npv| npv - price)
            };
            let min_base_vol = inputs
                .iter()
                .map(|&(_, _, vol, _)| vol)
                .fold(Real::MAX, Real::min);
            crate::require!(
                min_base_vol >= 0.0,
                "negative optionlet volatility at ATM strike {strike}"
            );
            let mut solver = Brent::new().with_max_evaluations(10000);
            let value_at_zero = objective(0.0)?;
            let spread = if close(value_at_zero, 0.0) {
                0.0
            } else if value_at_zero > 0.0 {
                let min_spread = -min_base_vol;
                crate::require!(
                    min_base_vol > 0.0,
                    "ATM cap/floor price for option expiry {tenor} is below the minimum attainable price"
                );
                crate::require!(
                    objective(min_spread)? <= 0.0,
                    "ATM cap/floor price for option expiry {tenor} is below the minimum attainable price"
                );
                solver.solve_bracketed(
                    |spread| objective(spread).unwrap_or(Real::NAN),
                    1e-6,
                    0.5 * min_spread,
                    min_spread,
                    0.0,
                )?
            } else {
                crate::require!(
                    objective(MAX_SPREAD)? >= 0.0,
                    "ATM cap/floor price for option expiry {tenor} requires a volatility spread greater than {MAX_SPREAD}"
                );
                solver.solve_bracketed(
                    |spread| objective(spread).unwrap_or(Real::NAN),
                    1e-6,
                    0.5 * MAX_SPREAD,
                    0.0,
                    MAX_SPREAD,
                )?
            };
            for (i, &time) in times.iter().enumerate().take(count) {
                let vol = adapter.volatility(time, strike, true)? + spread;
                let position = state.strikes[i].partition_point(|&value| value < strike);
                state.strikes[i].insert(position, strike);
                state.vols[i].insert(position, vol);
            }
            state.atm_strikes.push(strike);
            state.atm_prices.push(price);
            state.spreads.push(spread);
        }
        *self.state.borrow_mut() = state;
        Ok(())
    }

    /// Additive implied-volatility spreads, ordered by ATM curve tenor.
    pub fn spreads_vol(&self) -> QlResult<Vec<Volatility>> {
        self.calculate()?;
        Ok(self.state.borrow().spreads.clone())
    }
    /// ATM cap strikes, ordered by ATM curve tenor.
    pub fn atm_cap_floor_strikes(&self) -> QlResult<Vec<Rate>> {
        self.calculate()?;
        Ok(self.state.borrow().atm_strikes.clone())
    }
    /// Target ATM cap prices, ordered by ATM curve tenor.
    pub fn atm_cap_floor_prices(&self) -> QlResult<Vec<Real>> {
        self.calculate()?;
        Ok(self.state.borrow().atm_prices.clone())
    }
}

impl AsObservable for OptionletStripper2 {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}
impl StrippedOptionletBase for OptionletStripper2 {
    fn observable(&self) -> Option<&Observable> {
        Some(&self.observable)
    }
    fn optionlet_strikes(&self, i: usize) -> QlResult<Vec<Rate>> {
        self.calculate()?;
        self.state.borrow().strikes.get(i).cloned().ok_or_else(|| {
            crate::errors::QlError::new("optionlet index out of range", file!(), line!())
        })
    }
    fn optionlet_volatilities(&self, i: usize) -> QlResult<Vec<Volatility>> {
        self.calculate()?;
        self.state.borrow().vols.get(i).cloned().ok_or_else(|| {
            crate::errors::QlError::new("optionlet index out of range", file!(), line!())
        })
    }
    fn optionlet_fixing_dates(&self) -> QlResult<Vec<Date>> {
        self.calculate()?;
        self.source.optionlet_fixing_dates()
    }
    fn optionlet_fixing_times(&self) -> QlResult<Vec<Time>> {
        self.calculate()?;
        self.source.optionlet_fixing_times()
    }
    fn optionlet_maturities(&self) -> usize {
        self.source.optionlet_maturities()
    }
    fn atm_optionlet_rates(&self) -> QlResult<Vec<Rate>> {
        self.calculate()?;
        self.source.atm_optionlet_rates()
    }
    fn day_counter(&self) -> Option<DayCounter> {
        self.source.day_counter()
    }
    fn calendar(&self) -> Option<Calendar> {
        self.source.calendar()
    }
    fn settlement_days(&self) -> QlResult<Natural> {
        self.source.settlement_days()
    }
    fn business_day_convention(&self) -> BusinessDayConvention {
        self.source.business_day_convention()
    }
    fn volatility_type(&self) -> VolatilityType {
        self.source.volatility_type()
    }
    fn displacement(&self) -> Real {
        self.source.displacement()
    }
}
