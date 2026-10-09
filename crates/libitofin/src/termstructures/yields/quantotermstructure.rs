//! Live quanto-adjusted dividend zero yields.
//!
//! All five inputs are evaluated at the same numerical time with internal
//! extrapolation, matching QuantLib. Input reference dates and day counters
//! are not aligned automatically. The public curve has its own range checks.

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::interestrate::Compounding;
use crate::patterns::observable::{AsObservable, Observable, Observer};
use crate::require;
use crate::shared::SharedMut;
use crate::termstructures::volatility::BlackVolTermStructure;
use crate::termstructures::yields::ZeroYieldStructure;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::types::{DiscountFactor, Natural, Rate, Real, Time};

/// Dividend yield adjusted for domestic/foreign rates and correlated FX risk.
///
/// Its zero yield is `q + r_domestic - r_foreign + rho * vol_asset * vol_fx`.
/// The dividend curve supplies live metadata. The maximum date is the minimum
/// of all five inputs' calendar dates, not their numerical maximum times.
/// Extrapolation starts disabled and is independent of the input flags.
pub struct QuantoTermStructure {
    base: TermStructureBase,
    dividend: Handle<dyn YieldTermStructure>,
    domestic: Handle<dyn YieldTermStructure>,
    foreign: Handle<dyn YieldTermStructure>,
    asset_vol: Handle<dyn BlackVolTermStructure>,
    fx_vol: Handle<dyn BlackVolTermStructure>,
    strike: Real,
    fx_atm: Real,
    correlation: Real,
}

impl QuantoTermStructure {
    /// Create a live five-handle quanto adjustment in QuantLib argument order.
    ///
    /// Finite zero/negative strike levels are allowed, as input volatility
    /// domains can support them. Internal volatility queries extrapolate both
    /// time and strike. Empty handles may be linked later; inspectors use
    /// empty fallbacks and queries requiring a missing input return errors.
    ///
    /// # Errors
    /// Returns an error for nonfinite strike/FX levels or a nonfinite
    /// correlation outside `[-1, 1]`. These are checked Rust additions to the
    /// original constructor, which does not validate these scalar inputs.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        dividend: Handle<dyn YieldTermStructure>,
        domestic: Handle<dyn YieldTermStructure>,
        foreign: Handle<dyn YieldTermStructure>,
        asset_vol: Handle<dyn BlackVolTermStructure>,
        strike: Real,
        fx_vol: Handle<dyn BlackVolTermStructure>,
        fx_atm: Real,
        correlation: Real,
    ) -> QlResult<Self> {
        require!(strike.is_finite(), "nonfinite underlying strike");
        require!(fx_atm.is_finite(), "nonfinite FX ATM level");
        require!(
            correlation.is_finite() && (-1.0..=1.0).contains(&correlation),
            "correlation must be finite and within [-1, 1]"
        );
        let base = TermStructureBase::new(None);
        let observer = base.updater();
        dividend.register_observer(&observer);
        domestic.register_observer(&observer);
        foreign.register_observer(&observer);
        asset_vol.register_observer(&observer);
        fx_vol.register_observer(&observer);
        Ok(Self {
            base,
            dividend,
            domestic,
            foreign,
            asset_vol,
            fx_vol,
            strike,
            fx_atm,
            correlation,
        })
    }
}

impl AsObservable for QuantoTermStructure {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl TermStructure for QuantoTermStructure {
    fn base(&self) -> &TermStructureBase {
        &self.base
    }

    fn register_upstream(&self, observer: &SharedMut<dyn Observer>) {
        self.dividend.register_observer(observer);
        self.domestic.register_observer(observer);
        self.foreign.register_observer(observer);
        self.asset_vol.register_observer(observer);
        self.fx_vol.register_observer(observer);
    }

    fn max_date(&self) -> Date {
        let dates = (|| -> QlResult<[Date; 5]> {
            Ok([
                self.dividend.current_link()?.max_date(),
                self.domestic.current_link()?.max_date(),
                self.foreign.current_link()?.max_date(),
                self.asset_vol.current_link()?.max_date(),
                self.fx_vol.current_link()?.max_date(),
            ])
        })();
        dates
            .map(|dates| dates.into_iter().min().unwrap_or_else(Date::null))
            .unwrap_or_else(|_| Date::null())
    }

    fn day_counter(&self) -> Option<DayCounter> {
        self.dividend
            .current_link()
            .ok()
            .and_then(|curve| curve.day_counter())
    }

    fn calendar(&self) -> Option<Calendar> {
        self.dividend
            .current_link()
            .ok()
            .and_then(|curve| curve.calendar())
    }

    fn settlement_days(&self) -> QlResult<Natural> {
        self.dividend.current_link()?.settlement_days()
    }

    fn reference_date(&self) -> QlResult<Date> {
        self.dividend.current_link()?.reference_date()
    }
}

impl ZeroYieldStructure for QuantoTermStructure {
    fn zero_yield_impl(&self, t: Time) -> QlResult<Rate> {
        self.check_range_time(t, true)?;
        let zero = |handle: &Handle<dyn YieldTermStructure>| -> QlResult<Rate> {
            let rate = handle
                .current_link()?
                .zero_rate(t, Compounding::Continuous, Frequency::NoFrequency, true)?
                .rate();
            require!(rate.is_finite(), "nonfinite input zero yield");
            Ok(rate)
        };
        let dividend = zero(&self.dividend)?;
        let domestic = zero(&self.domestic)?;
        let foreign = zero(&self.foreign)?;
        let asset_vol = self
            .asset_vol
            .current_link()?
            .black_vol(t, self.strike, true)?;
        let fx_vol = self
            .fx_vol
            .current_link()?
            .black_vol(t, self.fx_atm, true)?;
        require!(
            asset_vol.is_finite() && asset_vol >= 0.0 && fx_vol.is_finite() && fx_vol >= 0.0,
            "input Black volatilities must be finite and nonnegative"
        );
        let result = dividend + domestic - foreign + self.correlation * asset_vol * fx_vol;
        require!(result.is_finite(), "nonfinite quanto zero yield");
        Ok(result)
    }
}

impl YieldTermStructure for QuantoTermStructure {
    fn discount_impl(&self, t: Time) -> QlResult<DiscountFactor> {
        self.check_range_time(t, true)?;
        let discount = self.discount_from_zero_yield(t)?;
        require!(
            discount.is_finite() && discount > 0.0,
            "quanto discount must be finite and positive"
        );
        Ok(discount)
    }
}
