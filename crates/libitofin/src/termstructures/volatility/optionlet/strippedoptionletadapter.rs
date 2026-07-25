//! Stripped-optionlet adapter (`StrippedOptionletAdapter`).
//!
//! Port of `ql/termstructures/volatility/optionlet/strippedoptionletadapter.{hpp,cpp}`:
//! `class StrippedOptionletAdapter : public OptionletVolatilityStructure, public
//! LazyObject`. It turns a [`StrippedOptionletBase`] (the grid of stripped
//! caplet/floorlet volatilities produced by #575) into a queryable
//! [`OptionletVolatilityStructure`]. For each optionlet maturity it holds a
//! [`LinearInterpolation`] across strikes (`performCalculations`,
//! `strippedoptionletadapter.cpp:82-118`); a volatility query interpolates each
//! maturity's strike curve at the strike, then interpolates those values across
//! the optionlet fixing times at the requested option time
//! (`volatilityImpl`, `strippedoptionletadapter.cpp:66-80`). Both interpolation
//! layers extrapolate, matching the C++ `(..., true)` evaluations.
//!
//! ## Divergences from QuantLib
//!
//! - `smileSectionImpl` (`strippedoptionletadapter.cpp:44-64`) is omitted: the
//!   smile-section layer is unported and the Rust
//!   [`OptionletVolatilityStructure`] base defers it (as
//!   [`ConstantOptionletVolatility`](super::ConstantOptionletVolatility) does).
//! - The constructor takes an explicit [`Settings`] handle and returns a
//!   [`QlResult`] (D5): the adapter is a moving term structure whose reference
//!   date tracks the evaluation date, and the [`StrippedOptionletBase`] interface
//!   carries no settings to thread through, so the caller supplies it.
//! - The infallible base accessors `min_strike`/`max_strike`/`max_date` read the
//!   stripped strikes and fixing dates, whose data path runs through the
//!   stripper's fallible strip. They are snapshotted once in
//!   [`new`](StrippedOptionletAdapter::new), which therefore strips eagerly
//!   rather than lazily as C++ does. This is harmless (the adapter is always
//!   priced immediately) but means the snapshot does not track a later
//!   evaluation-date change.
//! - Change propagation from the stripper to the adapter is not wired: the #575
//!   [`OptionletStripper1`](super::OptionletStripper1) invalidates its lazy state
//!   silently and exposes no observable, so a surface or index change is not
//!   forwarded here. The adapter still observes nothing upstream and recomputes
//!   its interpolations on first use, which is what the strip-and-reprice oracle
//!   needs; full propagation is deferred with the stripper's own silent
//!   invalidation (#577).

use std::cell::RefCell;

use crate::errors::QlResult;
use crate::fail;
use crate::math::interpolations::Interpolation;
use crate::math::interpolations::linear::LinearInterpolation;
use crate::patterns::lazyobject::LazyObject;
use crate::patterns::observable::{AsObservable, Observable};
use crate::settings::Settings;
use crate::shared::Shared;
use crate::termstructures::volatility::{VolatilityTermStructure, VolatilityType};
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::date::Date;
use crate::types::{Rate, Real, Time, Volatility};

use super::{OptionletVolatilityStructure, StrippedOptionletBase};

/// Adapts a [`StrippedOptionletBase`] into an [`OptionletVolatilityStructure`].
pub struct StrippedOptionletAdapter {
    base: TermStructureBase,
    stripper: Shared<dyn StrippedOptionletBase>,
    n_interpolations: usize,
    min_strike: Rate,
    max_strike: Rate,
    max_date: Date,
    strike_interpolations: RefCell<Vec<LinearInterpolation>>,
    lazy: RefCell<LazyObject>,
}

impl StrippedOptionletAdapter {
    /// Builds an adapter over `stripper`, its reference date moving off the
    /// evaluation date carried by `settings`.
    ///
    /// The settlement days, calendar, business-day convention and day counter are
    /// taken from the stripper (`strippedoptionletadapter.cpp:32-42`). The strikes
    /// and fixing dates are read once here to snapshot the strike domain and
    /// maximum date, which strips the surface eagerly.
    ///
    /// # Errors
    ///
    /// Fails when the stripper exposes no calendar, and propagates the eager
    /// strip triggered by reading the optionlet strikes and fixing dates.
    pub fn new(
        stripper: Shared<dyn StrippedOptionletBase>,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<StrippedOptionletAdapter> {
        let settlement_days = stripper.settlement_days()?;
        let Some(calendar) = stripper.calendar() else {
            fail!("stripped-optionlet adapter needs a calendar from the stripper");
        };
        let day_counter = stripper.day_counter();
        let n_interpolations = stripper.optionlet_maturities();

        let first_strikes = stripper.optionlet_strikes(0)?;
        let (Some(&min_strike), Some(&max_strike)) = (first_strikes.first(), first_strikes.last())
        else {
            fail!("stripped-optionlet adapter needs at least one strike");
        };
        let fixing_dates = stripper.optionlet_fixing_dates()?;
        let Some(&max_date) = fixing_dates.last() else {
            fail!("stripped-optionlet adapter needs at least one fixing date");
        };

        let base = TermStructureBase::moving(settlement_days, calendar, day_counter, settings);

        Ok(StrippedOptionletAdapter {
            base,
            stripper,
            n_interpolations,
            min_strike,
            max_strike,
            max_date,
            strike_interpolations: RefCell::new(Vec::new()),
            lazy: RefCell::new(LazyObject::new(true)),
        })
    }

    /// Rebuilds the per-maturity strike interpolations from the stripped
    /// volatilities, unless already cached (the C++ `performCalculations` guarded
    /// by `LazyObject::calculate`).
    pub fn calculate(&self) -> QlResult<()> {
        if !self.lazy.borrow_mut().start_calculation() {
            return Ok(());
        }
        let result = self.perform_calculations();
        self.lazy.borrow_mut().finish_calculation(&result);
        result
    }

    fn perform_calculations(&self) -> QlResult<()> {
        let mut interpolations = Vec::with_capacity(self.n_interpolations);
        for i in 0..self.n_interpolations {
            let strikes = self.stripper.optionlet_strikes(i)?;
            let vols = self.stripper.optionlet_volatilities(i)?;
            interpolations.push(LinearInterpolation::new(strikes, vols)?.with_extrapolation(true));
        }
        *self.strike_interpolations.borrow_mut() = interpolations;
        Ok(())
    }
}

impl AsObservable for StrippedOptionletAdapter {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl TermStructure for StrippedOptionletAdapter {
    fn base(&self) -> &TermStructureBase {
        &self.base
    }

    fn max_date(&self) -> Date {
        self.max_date
    }
}

impl VolatilityTermStructure for StrippedOptionletAdapter {
    fn business_day_convention(&self) -> BusinessDayConvention {
        self.stripper.business_day_convention()
    }

    fn min_strike(&self) -> Rate {
        self.min_strike
    }

    fn max_strike(&self) -> Rate {
        self.max_strike
    }
}

impl OptionletVolatilityStructure for StrippedOptionletAdapter {
    fn volatility_impl(&self, option_time: Time, strike: Rate) -> QlResult<Volatility> {
        self.calculate()?;

        let mut vols = Vec::with_capacity(self.n_interpolations);
        {
            let interpolations = self.strike_interpolations.borrow();
            for interpolation in interpolations.iter() {
                vols.push(interpolation.value(strike)?);
            }
        }

        let times = self.stripper.optionlet_fixing_times()?;
        let time_interpolation = LinearInterpolation::new(times, vols)?.with_extrapolation(true);
        time_interpolation.value(option_time)
    }

    fn volatility_type(&self) -> VolatilityType {
        self.stripper.volatility_type()
    }

    fn displacement(&self) -> Real {
        self.stripper.displacement()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handle::Handle;
    use crate::indexes::IborIndex;
    use crate::interestrate::Compounding;
    use crate::quotes::{Quote, SimpleQuote};
    use crate::shared::shared;
    use crate::termstructures::volatility::CapFloorTermVolSurface;
    use crate::termstructures::volatility::optionlet::OptionletStripper1;
    use crate::termstructures::yields::FlatForward;
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendars::target::Target;
    use crate::time::date::Month;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;
    use crate::time::frequency::Frequency;
    use crate::time::period::Period;
    use crate::time::timeunit::TimeUnit;

    fn eval_date() -> Date {
        Date::new(15, Month::June, 2026)
    }

    fn settings() -> Shared<Settings<Date>> {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(eval_date());
        settings
    }

    fn strikes() -> Vec<Rate> {
        vec![0.02, 0.03, 0.04, 0.05, 0.06]
    }

    fn flat_surface(settings: Shared<Settings<Date>>) -> Shared<CapFloorTermVolSurface> {
        let option_tenors: Vec<Period> = (1..=5).map(|n| Period::new(n, TimeUnit::Years)).collect();
        let vols: Vec<Vec<Handle<dyn Quote>>> = option_tenors
            .iter()
            .map(|_| {
                strikes()
                    .iter()
                    .map(|_| Handle::new(shared(SimpleQuote::new(Some(0.20))) as Shared<dyn Quote>))
                    .collect()
            })
            .collect();
        shared(
            CapFloorTermVolSurface::moving(
                0,
                Target::new(),
                BusinessDayConvention::Following,
                option_tenors,
                strikes(),
                vols,
                Actual365Fixed::new(),
                settings,
            )
            .unwrap(),
        )
    }

    fn adapter() -> (StrippedOptionletAdapter, Shared<Settings<Date>>) {
        let settings = settings();
        let curve: Handle<dyn YieldTermStructure> =
            Handle::new(shared(FlatForward::moving_with_rate(
                0,
                Target::new(),
                0.04,
                Actual365Fixed::new(),
                Compounding::Continuous,
                Frequency::Annual,
                Shared::clone(&settings),
            )) as Shared<dyn YieldTermStructure>);
        let index: Shared<IborIndex> = shared(crate::indexes::ibor::Euribor::six_months(
            curve,
            Shared::clone(&settings),
        ));
        let stripper = shared(
            OptionletStripper1::new(
                flat_surface(Shared::clone(&settings)),
                index,
                Handle::<dyn YieldTermStructure>::empty(),
                1e-6,
                100,
                VolatilityType::ShiftedLognormal,
                0.0,
                None,
            )
            .unwrap(),
        ) as Shared<dyn StrippedOptionletBase>;
        let adapter = StrippedOptionletAdapter::new(stripper, Shared::clone(&settings)).unwrap();
        (adapter, settings)
    }

    /// The strike domain is snapshotted from the stripped strikes (the surface
    /// strikes) and the maximum date from the last optionlet fixing date
    /// (`strippedoptionletadapter.cpp:120-131`).
    #[test]
    fn snapshots_the_strike_domain_and_max_date() {
        let (adapter, _settings) = adapter();
        assert_eq!(adapter.min_strike(), 0.02);
        assert_eq!(adapter.max_strike(), 0.06);
        assert!(adapter.max_date() > adapter.reference_date().unwrap());
        assert_eq!(adapter.volatility_type(), VolatilityType::ShiftedLognormal);
        assert_eq!(adapter.displacement(), 0.0);
    }

    /// A flat 20% term-vol surface strips and re-interpolates to optionlet
    /// volatilities near 20% at an interior time and strike, queried without
    /// extrapolation. The exact reprice identity is the discriminating oracle.
    #[test]
    fn interpolates_a_flat_surface_near_the_flat_input() {
        let (adapter, _settings) = adapter();
        let vol = adapter.volatility(2.0, 0.04, false).unwrap();
        assert!(
            (vol - 0.20).abs() < 0.02,
            "interpolated optionlet vol {vol}"
        );
    }

    use crate::instrument::Instrument;
    use crate::instruments::{CapFloorType, MakeCapFloor};
    use crate::math::matrix::Matrix;
    use crate::pricingengine::PricingEngine;
    use crate::pricingengines::BlackCapFloorEngine;
    use crate::quotes::make_quote_handle;
    use crate::shared::{SharedMut, shared_mut};

    /// `optionletstripper.cpp` `testFlatTermVolatilityStripping1` (`:489-548`):
    /// eval date 28-Oct-2013, a flat 18% cap/floor term-vol surface (10 tenors x
    /// 10 strikes) over a flat 4% Actual365Fixed curve. Stripping into optionlet
    /// volatilities, adapting them into an interpolated surface and repricing each
    /// cap on that surface must reproduce the flat-vol cap price to 2.5e-8. This
    /// is the whole B2 pipeline: strip -> per-maturity strike interpolation ->
    /// time interpolation -> reprice, and it is not tautological (the flat term
    /// vol must survive the round trip back to the same cap price).
    #[test]
    fn flat_term_vol_round_trips_through_the_stripped_optionlets() {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(Date::new(28, Month::October, 2013));

        let curve: Handle<dyn YieldTermStructure> =
            Handle::new(shared(FlatForward::moving_with_rate(
                0,
                Target::new(),
                0.04,
                Actual365Fixed::new(),
                Compounding::Continuous,
                Frequency::Annual,
                Shared::clone(&settings),
            )) as Shared<dyn YieldTermStructure>);

        let option_tenors: Vec<Period> =
            (1..=10).map(|n| Period::new(n, TimeUnit::Years)).collect();
        let strikes: Vec<Rate> = (1..=10).map(|j| j as Real / 100.0).collect();
        let flat_vol = 0.18;
        let vols = Matrix::filled(option_tenors.len(), strikes.len(), flat_vol);
        let surface = shared(
            CapFloorTermVolSurface::moving_from_matrix(
                0,
                Target::new(),
                BusinessDayConvention::Following,
                option_tenors.clone(),
                strikes.clone(),
                &vols,
                Actual365Fixed::new(),
                Shared::clone(&settings),
            )
            .unwrap(),
        );

        let index: Shared<IborIndex> = shared(crate::indexes::ibor::Euribor::six_months(
            curve.clone(),
            Shared::clone(&settings),
        ));

        let stripper = shared(
            OptionletStripper1::new(
                surface,
                Shared::clone(&index),
                Handle::<dyn YieldTermStructure>::empty(),
                1e-6,
                100,
                VolatilityType::ShiftedLognormal,
                0.0,
                None,
            )
            .unwrap(),
        );
        let adapter = shared(
            StrippedOptionletAdapter::new(
                Shared::clone(&stripper) as Shared<dyn StrippedOptionletBase>,
                Shared::clone(&settings),
            )
            .unwrap(),
        );
        adapter.enable_extrapolation();

        let stripped_engine = shared_mut(
            BlackCapFloorEngine::new(
                curve.clone(),
                Handle::new(Shared::clone(&adapter) as Shared<dyn OptionletVolatilityStructure>),
                None,
            )
            .unwrap(),
        ) as SharedMut<dyn PricingEngine>;

        let tolerance = 2.5e-8;
        let mut max_error: Real = 0.0;
        for tenor in &option_tenors {
            for &strike in &strikes {
                let mut cap = MakeCapFloor::new(
                    CapFloorType::Cap,
                    *tenor,
                    Shared::clone(&index),
                    strike,
                    Period::new(0, TimeUnit::Days),
                    Shared::clone(&settings),
                )
                .with_pricing_engine(SharedMut::clone(&stripped_engine))
                .build()
                .unwrap();
                let price_stripped = cap.npv().unwrap();

                let constant_engine = shared_mut(
                    BlackCapFloorEngine::with_flat_vol(
                        curve.clone(),
                        make_quote_handle(flat_vol).handle(),
                        Actual365Fixed::new(),
                        0.0,
                        Shared::clone(&settings),
                    )
                    .unwrap(),
                ) as SharedMut<dyn PricingEngine>;
                cap.base_mut().set_pricing_engine(constant_engine);
                let price_constant = cap.npv().unwrap();

                let error = (price_stripped - price_constant).abs();
                max_error = max_error.max(error);
                assert!(
                    error < tolerance,
                    "tenor {tenor} strike {strike}: stripped {price_stripped} vs \
                     constant {price_constant}, error {error} > tolerance {tolerance}"
                );
            }
        }
        assert!(max_error < tolerance, "max round-trip error {max_error}");
    }
}
