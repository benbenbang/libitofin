//! Temporary-engine inversion of a single European callable-bond quote.

use std::any::Any;

use super::{CallableFixedRateBond, CallableZeroCouponBond};
use crate::errors::{QlError, QlResult};
use crate::handle::Handle;
use crate::instrument::Instrument;
use crate::instruments::{BondPrice, BondResults};
use crate::math::solver1d::Solver1D;
use crate::math::solvers1d::brent::Brent;
use crate::pricingengine::PricingEngine;
use crate::pricingengines::bond::BlackCallableFixedRateBondEngine;
use crate::quotes::{Quote, SimpleQuote};
use crate::settings::Settings;
use crate::shared::{Shared, shared};
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::date::Date;
use crate::types::{Real, Size, Volatility};
use crate::{fail, require};

impl CallableFixedRateBond {
    /// Recovers constant forward-yield volatility from a settlement price per 100.
    ///
    /// Supports one European hard call or put with the Black callable engine.
    /// Clean targets include settlement accrued interest before inversion. The
    /// installed engine, cached results, freeze state and observers are untouched.
    ///
    /// # Errors
    /// Rejects expired instruments, invalid targets, solver controls, unsupported
    /// exercise schedules, Black pricing errors and targets outside the bracket.
    #[allow(clippy::too_many_arguments)]
    pub fn implied_volatility(
        &self,
        target_price: BondPrice,
        discount_curve: Handle<dyn YieldTermStructure>,
        accuracy: Real,
        max_evaluations: Size,
        min_vol: Volatility,
        max_vol: Volatility,
    ) -> QlResult<Volatility> {
        let settlement = self.checked_settlement_date()?;
        let accrued = match target_price {
            BondPrice::Clean(_) => self.bond().accrued_amount(Some(settlement))?,
            BondPrice::Dirty(_) => 0.0,
        };
        implied_volatility(
            self,
            target_price.amount() + accrued,
            self.face_amount,
            discount_curve,
            Shared::clone(&self.settings),
            accuracy,
            max_evaluations,
            min_vol,
            max_vol,
        )
    }
}

impl CallableZeroCouponBond {
    /// Recovers constant forward-yield volatility from a settlement price per 100.
    ///
    /// Supports one European hard call or put. Clean and dirty targets coincide.
    /// No installed engine is required; its cache and freeze state are untouched.
    ///
    /// # Examples
    ///
    /// ```
    /// use libitofin::{handle::Handle, instruments::{BondPrice, Callability,
    ///     CallabilityType, CallableZeroCouponBond}, interestrate::Compounding,
    ///     settings::Settings, shared::shared, termstructures::{yields::FlatForward,
    ///     yieldtermstructure::YieldTermStructure}, time::{businessdayconvention::BusinessDayConvention,
    ///     calendars::nullcalendar::NullCalendar, date::{Date, Month},
    ///     daycounters::actual365fixed::Actual365Fixed, frequency::Frequency}};
    /// let today = Date::new(12, Month::October, 2026);
    /// let settings = shared(Settings::new());
    /// settings.set_evaluation_date(today);
    /// let right = Callability::new(BondPrice::Clean(85.0), CallabilityType::Call,
    ///     Date::new(15, Month::April, 2029))?;
    /// let bond = CallableZeroCouponBond::new(0, 100.0, NullCalendar::new(),
    ///     Date::new(15, Month::January, 2033), Actual365Fixed::new(),
    ///     BusinessDayConvention::Unadjusted, 100.0, None, vec![right], settings)?;
    /// let term: std::rc::Rc<dyn YieldTermStructure> = shared(FlatForward::with_rate(
    ///     today, 0.035, Actual365Fixed::new(), Compounding::Continuous, Frequency::Annual));
    /// let vol = bond.implied_volatility(BondPrice::Clean(76.90104184226928),
    ///     Handle::new(term), 1e-10, 300, 0.0, 1.0)?;
    /// assert!((vol - 0.3).abs() < 2e-8);
    /// # Ok::<(), libitofin::errors::QlError>(())
    /// ```
    ///
    /// # Errors
    /// Rejects expired instruments, invalid targets, solver controls, unsupported
    /// exercise schedules, Black pricing errors and targets outside the bracket.
    #[allow(clippy::too_many_arguments)]
    pub fn implied_volatility(
        &self,
        target_price: BondPrice,
        discount_curve: Handle<dyn YieldTermStructure>,
        accuracy: Real,
        max_evaluations: Size,
        min_vol: Volatility,
        max_vol: Volatility,
    ) -> QlResult<Volatility> {
        implied_volatility(
            self,
            target_price.amount(),
            self.face_amount,
            discount_curve,
            Shared::clone(&self.settings),
            accuracy,
            max_evaluations,
            min_vol,
            max_vol,
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn implied_volatility(
    instrument: &dyn Instrument,
    dirty_target: Real,
    face_amount: Real,
    discount_curve: Handle<dyn YieldTermStructure>,
    settings: Shared<Settings<Date>>,
    accuracy: Real,
    max_evaluations: Size,
    min_vol: Volatility,
    max_vol: Volatility,
) -> QlResult<Volatility> {
    require!(!instrument.is_expired()?, "instrument expired");
    require!(
        dirty_target.is_finite() && dirty_target >= 0.0,
        "invalid dirty target price"
    );
    require!(
        accuracy.is_finite() && accuracy > 0.0,
        "accuracy must be finite and positive"
    );
    require!(max_evaluations > 0, "maximum evaluations must be positive");
    require!(
        min_vol.is_finite() && min_vol >= 0.0,
        "invalid minimum volatility"
    );
    require!(
        max_vol.is_finite() && max_vol > min_vol,
        "invalid maximum volatility"
    );
    let target = dirty_target / 100.0 * face_amount;
    require!(target.is_finite(), "target settlement value overflow");
    let quote = shared(SimpleQuote::new(min_vol));
    let mut engine = BlackCallableFixedRateBondEngine::new(
        Handle::new(Shared::clone(&quote) as Shared<dyn Quote>),
        discount_curve,
        settings,
    );
    instrument.setup_arguments(engine.arguments_mut())?;
    engine.arguments_mut().validate()?;
    let mut failure = None;
    let mut evaluations = 0;
    let mut objective = |vol| -> Real {
        if failure.is_some() {
            return Real::NAN;
        }
        let value = (|| -> QlResult<Real> {
            require!(
                evaluations < max_evaluations,
                "maximum number of function evaluations ({max_evaluations}) exceeded"
            );
            evaluations += 1;
            quote.set_value(vol);
            engine.calculate()?;
            let Some(results) = (engine.results() as &dyn Any).downcast_ref::<BondResults>() else {
                fail!("wrong callable bond result type");
            };
            let value = results
                .settlement_value
                .ok_or_else(|| QlError::new("missing settlement value", file!(), line!()))?;
            let residual = value / face_amount * 100.0 - dirty_target;
            require!(residual.is_finite(), "nonfinite settlement price residual");
            Ok(residual)
        })();
        match value {
            Ok(residual) => residual,
            Err(error) => {
                failure = Some(error);
                Real::NAN
            }
        }
    };
    let guess = min_vol + (max_vol - min_vol) / 2.0;
    let root = Brent::new()
        .with_max_evaluations(max_evaluations)
        .solve_bracketed(&mut objective, accuracy, guess, min_vol, max_vol);
    match failure {
        Some(error) => Err(error),
        None => root,
    }
}
