//! Calibration-helper base and the Black76 market instrument.
//!
//! Port of `ql/models/calibrationhelper.{hpp,cpp}`. [`CalibrationHelper`] is the
//! abstract base whose sole method a model's calibration cost function calls;
//! [`BlackCalibrationHelper`] is the Black76 liquid-instrument helper that
//! compares a market price (implied by a quoted volatility) against a model
//! price.
//!
//! The C++ `BlackCalibrationHelper` multiply-inherits `LazyObject` and
//! `CalibrationHelper` and mixes concrete state with pure-virtual hooks. The
//! port mirrors the [`Instrument`](crate::instrument::Instrument) split:
//! [`BlackCalibrationHelperBase`] is the embedded state (the lazy market-value
//! cache, the volatility handle, the model engine slot) and the
//! [`BlackCalibrationHelper`] trait carries the pure-virtual `model_value` /
//! `black_price` plus default methods reproducing the C++ base behaviour. A
//! blanket impl wires every [`BlackCalibrationHelper`] up as a
//! [`CalibrationHelper`].
//!
//! ## Divergences from QuantLib
//!
//! - `calibration_error` and `market_value` take `&mut self` where C++'s are
//!   conceptually const (mutating a `mutable marketValue_` cache). The cache
//!   lives in the shared `LazyObject`, driven through the same `&mut self` seam
//!   [`Instrument`](crate::instrument::Instrument) uses; `model_value`,
//!   `black_price` and `implied_volatility` stay `&self`, matching C++'s
//!   const-ness. C++ `calibrationError()` is itself non-const (hpp:44,79).
//! - `black_price` and `model_value` return [`QlResult`] where C++'s return
//!   `Real`: the Black/Bachelier formula and the model engine are fallible in
//!   this crate (D4). Inside the [`implied_volatility`] solver closure a
//!   `black_price` failure surfaces as a non-finite value the solver rejects, so
//!   the solve returns an error rather than a wrong root.
//!
//! ## Deferred
//!
//! - `addTimesTo(std::list<Time>&)` (hpp:81) serves only the tree/lattice
//!   pricing path (not ported), so it is omitted from the trait surface rather
//!   than stubbed. A helper that later needs it (`SwaptionHelper` on the lattice
//!   engine) adds it then.

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::math::solver1d::Solver1D;
use crate::math::solvers1d::brent::Brent;
use crate::patterns::lazyobject::LazyObject;
use crate::patterns::observable::Observer;
use crate::pricingengine::PricingEngine;
use crate::quotes::Quote;
use crate::shared::{SharedMut, shared_mut};
use crate::termstructures::volatility::VolatilityType;
use crate::types::Real;

/// Abstract base class for calibration helpers (`calibrationhelper.hpp:40`).
///
/// The only method a model's calibration cost function calls.
pub trait CalibrationHelper {
    /// The error resulting from the model valuation (`calibrationError`,
    /// hpp:44).
    ///
    /// # Errors
    ///
    /// Propagates a failure of the market or model valuation, or of the implied
    /// volatility solve.
    fn calibration_error(&mut self) -> QlResult<Real>;
}

/// How the market and model prices are compared during calibration
/// (`calibrationhelper.hpp:50`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalibrationErrorType {
    /// `|market - model| / market`.
    RelativePriceError,
    /// `market - model`.
    PriceError,
    /// `impliedVol(model) - impliedVol(market)`.
    ImpliedVolError,
}

/// Observer half of a helper: feeds volatility-quote notifications into the
/// lazy core (mirrors [`Instrument`](crate::instrument)'s `Updater`).
struct Updater {
    lazy: SharedMut<LazyObject>,
}

impl Observer for Updater {
    fn update(&mut self) {
        if let Some(update) = LazyObject::deferred_update(&self.lazy) {
            update.notify_observers();
        }
    }

    fn defer_reentrant_update(&self) -> bool {
        false
    }
}

/// State embedded by every Black76 calibration helper: the lazy market-value
/// cache, the quoted volatility, and the model pricing engine slot (the
/// concrete half of the C++ `BlackCalibrationHelper`).
pub struct BlackCalibrationHelperBase {
    lazy: SharedMut<LazyObject>,
    updater: SharedMut<Updater>,
    volatility: Handle<dyn Quote>,
    volatility_type: VolatilityType,
    shift: Real,
    calibration_error_type: CalibrationErrorType,
    market_value: Real,
    engine: Option<SharedMut<dyn PricingEngine>>,
}

impl BlackCalibrationHelperBase {
    /// `BlackCalibrationHelper(volatility, calibrationErrorType, type, shift)`
    /// (hpp:53-60): registers with the volatility handle so a quote change
    /// invalidates the cached market value (D1, C++ `registerWith(volatility_)`).
    pub fn new(
        volatility: Handle<dyn Quote>,
        calibration_error_type: CalibrationErrorType,
        volatility_type: VolatilityType,
        shift: Real,
    ) -> Self {
        let lazy = shared_mut(LazyObject::new(true));
        let updater = shared_mut(Updater {
            lazy: SharedMut::clone(&lazy),
        });
        volatility.register_observer(&(SharedMut::clone(&updater) as SharedMut<dyn Observer>));
        BlackCalibrationHelperBase {
            lazy,
            updater,
            volatility,
            volatility_type,
            shift,
            calibration_error_type,
            market_value: 0.0,
            engine: None,
        }
    }

    /// The volatility handle (`volatility()`, hpp:67).
    pub fn volatility(&self) -> &Handle<dyn Quote> {
        &self.volatility
    }

    /// The volatility type (`volatilityType()`, hpp:70).
    pub fn volatility_type(&self) -> VolatilityType {
        self.volatility_type
    }

    /// The lognormal shift (C++ `shift_`, hpp:102).
    pub fn shift(&self) -> Real {
        self.shift
    }

    /// Stores the model pricing engine (`setPricingEngine`, hpp:93-95).
    ///
    /// A bare store: unlike an instrument's, it neither observes the engine nor
    /// invalidates the cache (the C++ body is just `engine_ = engine`).
    pub fn set_pricing_engine(&mut self, engine: SharedMut<dyn PricingEngine>) {
        self.engine = Some(engine);
    }

    /// The stored model pricing engine, if any.
    pub fn pricing_engine(&self) -> Option<&SharedMut<dyn PricingEngine>> {
        self.engine.as_ref()
    }

    /// Whether the cached market value is currently valid.
    pub fn is_calculated(&self) -> bool {
        self.lazy.borrow().is_calculated()
    }

    /// The helper's observer half, for wiring extra inputs (mirrors
    /// [`InstrumentBase::observer`](crate::instrument::InstrumentBase::observer)).
    pub fn observer(&self) -> SharedMut<dyn Observer> {
        SharedMut::clone(&self.updater) as SharedMut<dyn Observer>
    }

    /// Registers a downstream observer of the helper's own notifications.
    pub fn register_observer(&self, observer: &SharedMut<dyn Observer>) -> bool {
        self.lazy.borrow().register_observer(observer)
    }
}

/// The Black76 calibration-helper seam (the abstract half of C++
/// `BlackCalibrationHelper`).
///
/// A concrete helper embeds a [`BlackCalibrationHelperBase`], exposes it through
/// [`base`](Self::base)/[`base_mut`](Self::base_mut), and provides
/// [`model_value`](Self::model_value) and [`black_price`](Self::black_price).
/// The default methods reproduce the C++ base behaviour, and the blanket impl
/// makes every implementor a [`CalibrationHelper`].
pub trait BlackCalibrationHelper {
    /// The embedded base state.
    fn base(&self) -> &BlackCalibrationHelperBase;

    /// Mutable access to the embedded base state.
    fn base_mut(&mut self) -> &mut BlackCalibrationHelperBase;

    /// The price of the instrument according to the model (`modelValue`,
    /// hpp:76).
    ///
    /// # Errors
    ///
    /// Propagates a failure of the model valuation.
    fn model_value(&self) -> QlResult<Real>;

    /// The Black or Bachelier price for a given volatility (`blackPrice`,
    /// hpp:91).
    ///
    /// # Errors
    ///
    /// Propagates a failure of the underlying price formula.
    fn black_price(&self, volatility: Real) -> QlResult<Real>;

    /// `performCalculations` (hpp:62-64): caches the market value as the Black
    /// price at the quoted volatility.
    ///
    /// # Errors
    ///
    /// Fails if the volatility handle is empty or its quote holds no value, or
    /// if [`black_price`](Self::black_price) fails.
    fn perform_calculations(&mut self) -> QlResult<()> {
        let volatility = self.base().volatility.current_link()?.value()?;
        let market_value = self.black_price(volatility)?;
        self.base_mut().market_value = market_value;
        Ok(())
    }

    /// Recomputes the cached market value if stale (the `LazyObject::calculate`
    /// path, mirroring [`Instrument::calculate`](crate::instrument::Instrument)).
    ///
    /// # Errors
    ///
    /// Propagates a [`perform_calculations`](Self::perform_calculations) failure.
    fn calculate(&mut self) -> QlResult<()> {
        let lazy = SharedMut::clone(&self.base().lazy);
        if !lazy.borrow_mut().start_calculation() {
            return Ok(());
        }
        let result = self.perform_calculations();
        lazy.borrow_mut().finish_calculation(&result);
        result
    }

    /// The actual price of the instrument, from the quoted volatility
    /// (`marketValue`, hpp:73).
    ///
    /// # Errors
    ///
    /// Propagates a [`calculate`](Self::calculate) failure.
    fn market_value(&mut self) -> QlResult<Real> {
        self.calculate()?;
        Ok(self.base().market_value)
    }

    /// The Black volatility implied by a target price
    /// (`impliedVolatility`, calibrationhelper.cpp:26-36): a Brent solve of
    /// `target - blackPrice(x)`, started from the quoted volatility.
    ///
    /// # Errors
    ///
    /// Fails if the volatility handle is empty, or if the solve does not
    /// converge in the bracket - including a [`black_price`](Self::black_price)
    /// failure, which surfaces as a non-finite value the solver rejects.
    fn implied_volatility(
        &self,
        target_value: Real,
        accuracy: Real,
        max_evaluations: usize,
        min_vol: Real,
        max_vol: Real,
    ) -> QlResult<Real> {
        let guess = self.base().volatility.current_link()?.value()?;
        let mut solver = Brent::new().with_max_evaluations(max_evaluations);
        solver.solve_bracketed(
            |x| target_value - self.black_price(x).unwrap_or(Real::NAN),
            accuracy,
            guess,
            min_vol,
            max_vol,
        )
    }
}

impl<T: BlackCalibrationHelper> CalibrationHelper for T {
    /// `calibrationError()` (calibrationhelper.cpp:38-72): compares the market
    /// and model prices per the configured error type.
    fn calibration_error(&mut self) -> QlResult<Real> {
        match self.base().calibration_error_type {
            CalibrationErrorType::RelativePriceError => {
                let market = self.market_value()?;
                let model = self.model_value()?;
                Ok((market - model).abs() / market)
            }
            CalibrationErrorType::PriceError => {
                let market = self.market_value()?;
                let model = self.model_value()?;
                Ok(market - model)
            }
            CalibrationErrorType::ImpliedVolError => {
                let (min_vol, max_vol) = match self.base().volatility_type {
                    VolatilityType::ShiftedLognormal => (0.0010, 10.0),
                    VolatilityType::Normal => (0.00005, 0.50),
                };
                let lower_price = self.black_price(min_vol)?;
                let upper_price = self.black_price(max_vol)?;
                let model_price = self.model_value()?;
                let implied = if model_price <= lower_price {
                    min_vol
                } else if model_price >= upper_price {
                    max_vol
                } else {
                    self.implied_volatility(model_price, 1e-12, 5000, min_vol, max_vol)?
                };
                let market_vol = self.base().volatility.current_link()?.value()?;
                Ok(implied - market_vol)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quotes::SimpleQuote;
    use crate::shared::{Shared, shared};
    use std::cell::Cell;

    /// Minimal Black76 helper with an analytic identity `black_price(v) = v`, so
    /// an implied volatility round-trips trivially, and a settable model value.
    struct StubHelper {
        base: BlackCalibrationHelperBase,
        model: Cell<Real>,
    }

    impl StubHelper {
        fn new(
            vol: Shared<SimpleQuote>,
            error_type: CalibrationErrorType,
            model: Real,
        ) -> StubHelper {
            let handle: Handle<dyn Quote> = Handle::new(vol as Shared<dyn Quote>);
            StubHelper {
                base: BlackCalibrationHelperBase::new(
                    handle,
                    error_type,
                    VolatilityType::ShiftedLognormal,
                    0.0,
                ),
                model: Cell::new(model),
            }
        }
    }

    impl BlackCalibrationHelper for StubHelper {
        fn base(&self) -> &BlackCalibrationHelperBase {
            &self.base
        }
        fn base_mut(&mut self) -> &mut BlackCalibrationHelperBase {
            &mut self.base
        }
        fn model_value(&self) -> QlResult<Real> {
            Ok(self.model.get())
        }
        fn black_price(&self, volatility: Real) -> QlResult<Real> {
            Ok(volatility)
        }
    }

    #[test]
    fn relative_price_error_is_the_relative_gap() {
        let vol = shared(SimpleQuote::new(0.20));
        let mut helper = StubHelper::new(vol, CalibrationErrorType::RelativePriceError, 0.18);
        let error = helper.calibration_error().unwrap();
        assert!((error - (0.20 - 0.18) / 0.20).abs() < 1e-12);
    }

    #[test]
    fn price_error_is_the_signed_gap() {
        let vol = shared(SimpleQuote::new(0.20));
        let mut helper = StubHelper::new(vol, CalibrationErrorType::PriceError, 0.18);
        let error = helper.calibration_error().unwrap();
        assert!((error - (0.20 - 0.18)).abs() < 1e-12);
    }

    #[test]
    fn implied_vol_error_brent_recovers_the_interior_vol() {
        let vol = shared(SimpleQuote::new(0.20));
        // model = 0.18 lies inside the lognormal bracket (0.0010, 10.0); under
        // the identity black_price the implied vol is 0.18, so the error is the
        // gap to the 0.20 market vol.
        let mut helper = StubHelper::new(vol, CalibrationErrorType::ImpliedVolError, 0.18);
        let error = helper.calibration_error().unwrap();
        assert!((error - (0.18 - 0.20)).abs() < 1e-9);
    }

    #[test]
    fn implied_vol_error_clamps_below_the_lower_price() {
        let vol = shared(SimpleQuote::new(0.20));
        // model 0.0005 <= blackPrice(minVol=0.0010): implied clamps to minVol.
        let mut helper = StubHelper::new(vol, CalibrationErrorType::ImpliedVolError, 0.0005);
        let error = helper.calibration_error().unwrap();
        assert!((error - (0.0010 - 0.20)).abs() < 1e-12);
    }

    #[test]
    fn implied_vol_error_clamps_above_the_upper_price() {
        let vol = shared(SimpleQuote::new(0.20));
        // model 20.0 >= blackPrice(maxVol=10.0): implied clamps to maxVol.
        let mut helper = StubHelper::new(vol, CalibrationErrorType::ImpliedVolError, 20.0);
        let error = helper.calibration_error().unwrap();
        assert!((error - (10.0 - 0.20)).abs() < 1e-12);
    }

    #[test]
    fn market_value_recomputes_when_the_vol_quote_changes() {
        let vol = shared(SimpleQuote::new(0.20));
        let mut helper =
            StubHelper::new(Shared::clone(&vol), CalibrationErrorType::PriceError, 0.0);

        assert_eq!(helper.market_value().unwrap(), 0.20);
        assert!(helper.base().is_calculated());

        vol.set_value(0.30);
        assert!(
            !helper.base().is_calculated(),
            "a quote change must invalidate the cached market value"
        );
        assert_eq!(helper.market_value().unwrap(), 0.30);
    }

    #[test]
    fn errors_vanish_when_model_matches_market_and_move_when_perturbed() {
        for error_type in [
            CalibrationErrorType::RelativePriceError,
            CalibrationErrorType::PriceError,
            CalibrationErrorType::ImpliedVolError,
        ] {
            let vol = shared(SimpleQuote::new(0.20));
            // model == the 0.20 market vol (== identity black_price of the vol):
            // every arm collapses to zero.
            let mut helper = StubHelper::new(Shared::clone(&vol), error_type, 0.20);
            assert!(
                helper.calibration_error().unwrap().abs() < 1e-9,
                "{error_type:?} must vanish when the model matches the market"
            );

            helper.model.set(0.25);
            assert!(
                helper.calibration_error().unwrap().abs() > 1e-6,
                "{error_type:?} must move once the model is perturbed"
            );
        }
    }
}
