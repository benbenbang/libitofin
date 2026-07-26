//! Interpolated (spread) swaption volatility cube.
//!
//! Port of `ql/termstructures/volatility/swaption/interpolatedswaptionvolatilitycube.{hpp,cpp}`:
//! `class InterpolatedSwaptionVolatilityCube : public SwaptionVolatilityCube`.
//! The simpler of the two concrete cubes: it interpolates the market
//! volatility spreads directly (no SABR fit). It embeds the
//! [`SwaptionVolatilityCube`] framework (#594) and holds one
//! [`BilinearInterpolation`] per strike spread, each over the
//! option-time by swap-length grid of that strike's spreads.
//!
//! ## Composition, not inheritance
//!
//! C++ derives from `SwaptionVolatilityCube`; Rust embeds it. The concrete cube
//! supplies the [`SwaptionCubeSmileSection`] smile hook and implements the
//! [`SwaptionVolatilityStructure`] trait, routing `volatility_impl` through the
//! base's provided [`cube_volatility_impl`](SwaptionCubeSmileSection::cube_volatility_impl).
//!
//! ## Lazy refresh
//!
//! C++ rebuilds `volSpreadsInterpolator_` in `performCalculations`, aliasing the
//! spread matrices by reference so a quote bump is seen for free. The Rust
//! [`BilinearInterpolation`] owns its `z`, so - exactly as
//! [`SwaptionVolatilityMatrix`](super::SwaptionVolatilityMatrix) does - the
//! per-strike interpolators live behind a [`RefCell`] and every query routes
//! through [`calculate`](InterpolatedSwaptionVolatilityCube::calculate) first;
//! `perform_calculations` re-reads the vol-spread quotes and rebuilds all
//! interpolators. A [`CubeUpdater`] registered on the base observable invalidates
//! the lazy state on a quote bump (the #594 base already registers every
//! vol-spread quote into the discrete base updater chain) or an evaluation-date
//! move.
//!
//! ## The smile section
//!
//! `smileSectionImpl` has two layers (interpolatedswaptionvolatilitycube.cpp:68-108):
//! the time-based hook converts `(option_time, swap_length)` back to
//! `(option_date, swap_tenor)`, adjusting the option date to a valid fixing date
//! on the relevant base index's calendar; the date-based body reads the ATM
//! forward and vol from the ATM surface, adds each strike's interpolated spread,
//! and returns an [`InterpolatedSmileSection`] (#593) over the resulting
//! `(strike, std_dev)` grid.
//!
//! ## Divergences from QuantLib
//!
//! - The `smileSection`/`volSpreads(i)` matrix inspectors are not exposed; the
//!   only consumer is the smile hook, which reads the interpolators directly.
//! - `referenceDate()` follows the #594 base: the Rust cube grid anchors to the
//!   moving reference (evaluation date + 0 settlement), whereas C++ delegates to
//!   `atmVol_->referenceDate()`. They agree for the expected settlement-0 moving
//!   ATM surface, which the oracle asserts.

use std::cell::RefCell;

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::indexes::{Index, InterestRateIndex, SwapIndex};
use crate::math::interpolations::Interpolation2D;
use crate::math::interpolations::bilinear::BilinearInterpolation;
use crate::patterns::lazyobject::LazyObject;
use crate::patterns::observable::{AsObservable, Observable, Observer};
use crate::quotes::Quote;
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut, shared, shared_mut};
use crate::termstructures::volatility::interpolatedsmilesection::InterpolatedSmileSection;
use crate::termstructures::volatility::{SmileSection, VolatilityTermStructure, VolatilityType};
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::date::Date;
use crate::time::daycounters::actual365fixed::Actual365Fixed;
use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;
use crate::types::{Integer, Rate, Real, Time, Volatility};

use super::{SwaptionCubeSmileSection, SwaptionVolatilityCube, SwaptionVolatilityStructure};

/// Invalidates the cube's lazy state when a vol-spread quote bumps or the
/// reference date moves, so the next
/// [`calculate`](InterpolatedSwaptionVolatilityCube::calculate) rebuilds the
/// per-strike interpolators.
struct CubeUpdater {
    lazy: SharedMut<LazyObject>,
}

impl Observer for CubeUpdater {
    fn update(&mut self) {
        self.lazy.borrow_mut().invalidate_silently();
    }
}

/// Interpolated swaption volatility cube: bilinear per strike over the vol
/// spreads, added to the ATM surface's vol.
pub struct InterpolatedSwaptionVolatilityCube {
    cube: SwaptionVolatilityCube,
    interpolators: RefCell<Vec<BilinearInterpolation>>,
    lazy: SharedMut<LazyObject>,
    _updater: SharedMut<CubeUpdater>,
}

impl InterpolatedSwaptionVolatilityCube {
    /// Builds the interpolated cube off the ATM surface, the strike/vol-spread
    /// grid and the two base swap indexes (C++'s single constructor,
    /// interpolatedswaptionvolatilitycube.cpp:31-46).
    ///
    /// `vol_spreads` is row-major over the `(option tenor, swap tenor)` nodes:
    /// row `i*nSwapTenors + j` holds one quote per strike spread. `settings` is
    /// the D5 handle the moving discrete grid needs.
    ///
    /// # Errors
    ///
    /// Propagates [`SwaptionVolatilityCube::new`] (empty ATM handle, missing
    /// calendar or day counter, too few or non-increasing strike spreads, a
    /// mis-shaped vol-spread grid, or a short index longer than the long one) and
    /// the initial interpolator build.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        atm_vol: Handle<dyn SwaptionVolatilityStructure>,
        option_tenors: Vec<Period>,
        swap_tenors: Vec<Period>,
        strike_spreads: Vec<Real>,
        vol_spreads: Vec<Vec<Handle<dyn Quote>>>,
        swap_index_base: Shared<SwapIndex>,
        short_swap_index_base: Shared<SwapIndex>,
        vega_weighted_smile_fit: bool,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<InterpolatedSwaptionVolatilityCube> {
        let cube = SwaptionVolatilityCube::new(
            atm_vol,
            option_tenors,
            swap_tenors,
            strike_spreads,
            vol_spreads,
            swap_index_base,
            short_swap_index_base,
            vega_weighted_smile_fit,
            settings,
        )?;
        let interpolators = build_interpolators(&cube)?;
        let lazy = shared_mut(LazyObject::new(true));
        let updater = shared_mut(CubeUpdater {
            lazy: SharedMut::clone(&lazy),
        });
        cube.observable()
            .register_observer(&(SharedMut::clone(&updater) as SharedMut<dyn Observer>));
        Ok(InterpolatedSwaptionVolatilityCube {
            cube,
            interpolators: RefCell::new(interpolators),
            lazy,
            _updater: updater,
        })
    }

    /// The embedded cube framework, for the ATM surface, strike spreads and base
    /// swap indexes.
    pub fn cube(&self) -> &SwaptionVolatilityCube {
        &self.cube
    }

    /// Rebuilds the per-strike interpolators if a vol-spread quote or the
    /// reference date has changed since they were last computed. Every query
    /// calls this first, as C++'s `smileSectionImpl` calls `calculate()`.
    ///
    /// # Errors
    ///
    /// Propagates the interpolator rebuild.
    pub fn calculate(&self) -> QlResult<()> {
        if !self.lazy.borrow_mut().start_calculation() {
            return Ok(());
        }
        let result = self.perform_calculations();
        self.lazy.borrow_mut().finish_calculation(&result);
        result
    }

    fn perform_calculations(&self) -> QlResult<()> {
        self.cube.calculate()?;
        let interpolators = build_interpolators(&self.cube)?;
        *self.interpolators.borrow_mut() = interpolators;
        Ok(())
    }

    /// The smile section for an option date and swap tenor (C++'s
    /// `smileSectionImpl(const Date&, const Period&)`,
    /// interpolatedswaptionvolatilitycube.cpp:85-108).
    fn smile_section_from_date(
        &self,
        option_date: Date,
        swap_tenor: Period,
    ) -> QlResult<Shared<dyn SmileSection>> {
        let atm = self.cube.atm_vol().current_link()?;
        let atm_forward = self.cube.atm_strike(option_date, swap_tenor)?;
        let length = self.swap_length_tenor(swap_tenor)?;
        let atm_vol = atm.volatility(option_date, length, atm_forward, false)?;
        let option_time = self.time_from_reference(option_date)?;
        let exercise_time_sqrt = option_time.sqrt();

        let strike_spreads = self.cube.strike_spreads();
        let interpolators = self.interpolators.borrow();
        let mut strikes = Vec::with_capacity(strike_spreads.len());
        let mut std_devs = Vec::with_capacity(strike_spreads.len());
        for (i, &spread) in strike_spreads.iter().enumerate() {
            strikes.push(atm_forward + spread);
            let vol_spread = interpolators[i].value(length, option_time)?;
            std_devs.push(exercise_time_sqrt * (atm_vol + vol_spread));
        }

        let shift = atm.shift_time(option_time, length, false)?;
        let section = InterpolatedSmileSection::with_exercise_time(
            option_time,
            strikes,
            std_devs,
            atm_forward,
            Actual365Fixed::new(),
            self.cube.volatility_type()?,
            shift,
        )?;
        Ok(shared(section) as Shared<dyn SmileSection>)
    }
}

/// Rebuilds one [`BilinearInterpolation`] per strike spread. Matches the base
/// matrix axis convention: `z[option_row][swap_col]`, built with `x =
/// swap_lengths`, `y = option_times`, so evaluation is `value(swap_length,
/// option_time)`.
fn build_interpolators(cube: &SwaptionVolatilityCube) -> QlResult<Vec<BilinearInterpolation>> {
    cube.calculate()?;
    let discrete = cube.discrete();
    let swap_lengths = discrete.swap_lengths()?;
    let option_times = discrete.option_times()?;
    let n_swaps = swap_lengths.len();
    let n_options = option_times.len();
    let n_strikes = cube.strike_spreads().len();
    let vol_spreads = cube.vol_spreads();

    let mut interpolators = Vec::with_capacity(n_strikes);
    #[allow(clippy::needless_range_loop)]
    for i in 0..n_strikes {
        let mut z = Vec::with_capacity(n_options);
        for j in 0..n_options {
            let mut row = Vec::with_capacity(n_swaps);
            for k in 0..n_swaps {
                let handle = &vol_spreads[j * n_swaps + k][i];
                row.push(handle.current_link()?.value()?);
            }
            z.push(row);
        }
        let interpolator =
            BilinearInterpolation::new(swap_lengths.clone(), option_times.clone(), z)?
                .with_extrapolation(true);
        interpolators.push(interpolator);
    }
    Ok(interpolators)
}

impl SwaptionCubeSmileSection for InterpolatedSwaptionVolatilityCube {
    fn smile_section_impl(
        &self,
        option_time: Time,
        swap_length: Time,
    ) -> QlResult<Shared<dyn SmileSection>> {
        self.calculate()?;
        let option_date = self.cube.discrete().option_date_from_time(option_time)?;
        let months = (swap_length * 12.0).round() as Integer;
        let swap_tenor = Period::new(months, TimeUnit::Months);
        let short_tenor = self.cube.short_swap_index_base().tenor();
        let branch_calendar = if swap_tenor > short_tenor {
            self.cube.swap_index_base().fixing_calendar()
        } else {
            self.cube.short_swap_index_base().fixing_calendar()
        };
        let option_date = branch_calendar.adjust(option_date, BusinessDayConvention::Following);
        self.smile_section_from_date(option_date, swap_tenor)
    }
}

impl AsObservable for InterpolatedSwaptionVolatilityCube {
    fn observable(&self) -> &Observable {
        self.cube.observable()
    }
}

impl TermStructure for InterpolatedSwaptionVolatilityCube {
    fn base(&self) -> &TermStructureBase {
        self.cube.base()
    }

    fn max_date(&self) -> Date {
        self.cube
            .atm_vol()
            .current_link()
            .map(|atm| atm.max_date())
            .unwrap_or_else(|_| Date::max_date())
    }
}

impl VolatilityTermStructure for InterpolatedSwaptionVolatilityCube {
    fn business_day_convention(&self) -> BusinessDayConvention {
        self.cube.business_day_convention()
    }

    fn min_strike(&self) -> Rate {
        Rate::MIN
    }

    fn max_strike(&self) -> Rate {
        Rate::MAX
    }
}

impl SwaptionVolatilityStructure for InterpolatedSwaptionVolatilityCube {
    fn volatility_impl(
        &self,
        option_time: Time,
        swap_length: Time,
        strike: Rate,
    ) -> QlResult<Volatility> {
        self.cube_volatility_impl(option_time, swap_length, strike)
    }

    fn max_swap_tenor(&self) -> Period {
        self.cube
            .atm_vol()
            .current_link()
            .map(|atm| atm.max_swap_tenor())
            .unwrap_or_else(|_| {
                self.cube
                    .discrete()
                    .swap_tenors()
                    .last()
                    .copied()
                    .expect("swap tenors are non-empty by construction")
            })
    }

    fn volatility_type(&self) -> VolatilityType {
        self.cube
            .volatility_type()
            .unwrap_or(VolatilityType::ShiftedLognormal)
    }

    fn shift_impl(&self, option_time: Time, swap_length: Time) -> QlResult<Real> {
        self.cube
            .atm_vol()
            .current_link()?
            .shift_time(option_time, swap_length, false)
    }
}
