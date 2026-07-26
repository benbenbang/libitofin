//! Concrete SABR swaption volatility cube.
//!
//! Port of the `SabrSwaptionVolatilityCube` typedef of
//! `ql/termstructures/volatility/swaption/sabrswaptionvolatilitycube.hpp`
//! (`XabrSwaptionVolatilityCube<SwaptionVolCubeSabrModel>`, hpp:1277). Per D10
//! the generic `XabrSwaptionVolatilityCube<Model>` template is not ported; this
//! is the concrete 4-parameter SABR instantiation, wired to
//! [`SABRInterpolation`](crate::math::interpolations::sabrinterpolation::SABRInterpolation)
//! (#583) as its smile calibrator.
//!
//! It embeds the [`SwaptionVolatilityCube`] framework (#594) and adds the SABR
//! machinery: a per-node parameter-guess store, the observer wiring that reacts
//! to guess-quote bumps, and (from #602's second commit) the sparse per-node
//! calibration.
//!
//! ## Guess store and the PrivateObserver (D1 re-entrancy)
//!
//! C++ holds `parametersGuess_`, a [`Cube`] of the per-node `[alpha, beta, nu,
//! rho]` guesses read from `parametersGuessQuotes_`, rebuilt by
//! `setParameterGuess()`. A `PrivateObserver` registered with every guess quote
//! runs `setParameterGuess(); update();` on a bump (hpp:268-281): rebuild the
//! guess cube, then invalidate the lazy state so the next query recalibrates.
//! Rebuilding inside the notification is a D1 re-entrancy hazard (a `borrow_mut`
//! held across the notify chain).
//!
//! This port takes the C++-blessed, behaviourally identical alternative: the
//! guess cube is rebuilt inside [`perform_calculations`] (from the current quote
//! values) rather than inside the observer, and the guess quotes are registered
//! on the #594 base's updater chain exactly as the vol-spread quotes are. A
//! [`SabrCubeUpdater`] on the base observable does `invalidate_silently` on any
//! bump (the same pattern the interpolated cube #595 uses). A guess bump thus
//! (a) notifies downstream observers through the base observable and
//! (b) invalidates the lazy state; the next [`calculate`](Self::calculate)
//! rebuilds the guess cube from the bumped quotes and recalibrates. No borrow is
//! held across notification.
//!
//! ## What works after #602 T3b, and what defers
//!
//! - Construction, the guess store, the observer wiring, and the sparse per-node
//!   SABR calibration work.
//! - `smileSectionImpl` (the volatility query) returns `Err` naming #604: the
//!   [`SabrSmileSection`](crate::termstructures::volatility::sabrsmilesection)
//!   bridge is that ticket.
//! - The `isAtmCalibrated` dense arm (`fillVolatilityCube` + `denseParameters_`)
//!   returns `Err` naming #603 rather than stubbing silently.
//! - A non-zero ATM shift returns `Err` naming #586 (displaced SABR); the oracle
//!   fixtures are shift-0.
//! - `backwardFlat = true` returns `Err` naming #606, as the #601 [`Cube`] does.
//! - `sabrCalibrationSection` and `recalibration` (the dense/section-recalibration
//!   API) are not ported; they defer with the dense arm.
#![allow(dead_code)]
#![allow(unused_imports)]

use std::cell::RefCell;

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::math::optimization::endcriteria::EndCriteria;
use crate::math::optimization::levenbergmarquardt::LevenbergMarquardt;
use crate::math::optimization::method::OptimizationMethod;
use crate::patterns::lazyobject::LazyObject;
use crate::patterns::observable::{AsObservable, Observable, Observer};
use crate::quotes::Quote;
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut, shared_mut};
use crate::termstructures::volatility::{SmileSection, VolatilityTermStructure, VolatilityType};
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::date::Date;
use crate::time::period::Period;
use crate::types::{Rate, Real, Time, Volatility};
use crate::{fail, require};

use super::cube::Cube;
use crate::termstructures::volatility::swaption::{
    SwaptionCubeSmileSection, SwaptionVolatilityCube, SwaptionVolatilityStructure,
};

/// The four SABR model parameters (alpha, beta, nu, rho).
const N_SABR_PARAMS: usize = 4;

/// The metadata layers the sparse parameter cube carries after the four model
/// parameters: forward, rms error, max error, end-criteria code (hpp:515-531).
const N_METADATA_LAYERS: usize = 4;

/// Default max-error tolerance when `maxErrorTolerance` is unset and the fit is
/// not vega-weighted (`SWAPTIONVOLCUBE_TOL`, hpp:49-51).
const SWAPTIONVOLCUBE_TOL: Real = 100.0e-4;

/// Default max-error tolerance when `maxErrorTolerance` is unset and the fit is
/// vega-weighted (`SWAPTIONVOLCUBE_VEGAWEIGHTED_TOL`, hpp:46-48).
const SWAPTIONVOLCUBE_VEGAWEIGHTED_TOL: Real = 15.0e-4;

/// Invalidates the cube's lazy state when a guess or vol-spread quote bumps or
/// the reference date moves, so the next [`SabrSwaptionVolatilityCube::calculate`]
/// rebuilds the guess cube and recalibrates. Mirrors the interpolated cube's
/// updater and, together with registering the guess quotes on the base updater,
/// stands in for C++'s `PrivateObserver`.
struct SabrCubeUpdater {
    lazy: SharedMut<LazyObject>,
}

impl Observer for SabrCubeUpdater {
    fn update(&mut self) {
        self.lazy.borrow_mut().invalidate_silently();
    }
}

/// SABR volatility cube for swaptions: fits the four SABR parameters per
/// (option, swap) node to the ATM-plus-spread smile.
pub struct SabrSwaptionVolatilityCube {
    cube: SwaptionVolatilityCube,
    parameters_guess_quotes: Vec<Vec<Handle<dyn Quote>>>,
    parameters_guess: RefCell<Cube>,
    market_vol_cube: RefCell<Cube>,
    sparse_parameters: RefCell<Cube>,
    is_parameter_fixed: [bool; N_SABR_PARAMS],
    is_atm_calibrated: bool,
    end_criteria: EndCriteria,
    max_error_tolerance: Real,
    opt_method: RefCell<Box<dyn OptimizationMethod>>,
    error_accept: Real,
    use_max_error: bool,
    max_guesses: usize,
    backward_flat: bool,
    cutoff_strike: Real,
    volatility_type: VolatilityType,
    lazy: SharedMut<LazyObject>,
    _updater: SharedMut<SabrCubeUpdater>,
}

impl SabrSwaptionVolatilityCube {
    /// Builds the SABR cube off the ATM surface, the strike/vol-spread grid, the
    /// two base swap indexes, and the per-node parameter guesses (C++'s single
    /// constructor, hpp:290-339).
    ///
    /// `parameters_guess` is row-major over the `(option tenor, swap tenor)`
    /// nodes: row `j*nSwapTenors + k` holds the four guess quotes
    /// `[alpha, beta, nu, rho]` for that node. `is_parameter_fixed` pins a
    /// parameter at its guess across every node.
    ///
    /// The C++-defaulted knobs are `Option`s here (Rust has no default args),
    /// resolved to the C++ null-defaults:
    /// - `end_criteria` unset -> `EndCriteria(60000, 100, 1e-8, 1e-8, 1e-8)`
    ///   (xabrinterpolation.hpp:130-133);
    /// - `opt_method` unset -> `LevenbergMarquardt(1e-8, 1e-8, 1e-8)`
    ///   (xabrinterpolation.hpp:125-127);
    /// - `max_error_tolerance` unset -> `SWAPTIONVOLCUBE_TOL` (100bp), or the
    ///   vega-weighted 15bp when `vega_weighted_smile_fit` (hpp:324-329);
    /// - `error_accept` unset -> `max_error_tolerance / 5` (hpp:330-334).
    ///
    /// `settings` is the D5 handle the moving discrete grid needs.
    ///
    /// # Errors
    ///
    /// Returns `Err` when `backward_flat` is `true` (deferred to #606), when
    /// `parameters_guess` is not `nOptionTenors * nSwapTenors` rows of four
    /// quotes each, or from [`SwaptionVolatilityCube::new`] (empty ATM handle,
    /// missing calendar or day counter, too few or non-increasing strike
    /// spreads, a mis-shaped vol-spread grid, or a short index longer than the
    /// long one), plus the initial guess-cube build.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        atm_vol: Handle<dyn SwaptionVolatilityStructure>,
        option_tenors: Vec<Period>,
        swap_tenors: Vec<Period>,
        strike_spreads: Vec<Real>,
        vol_spreads: Vec<Vec<Handle<dyn Quote>>>,
        swap_index_base: Shared<crate::indexes::SwapIndex>,
        short_swap_index_base: Shared<crate::indexes::SwapIndex>,
        vega_weighted_smile_fit: bool,
        parameters_guess: Vec<Vec<Handle<dyn Quote>>>,
        is_parameter_fixed: [bool; N_SABR_PARAMS],
        is_atm_calibrated: bool,
        end_criteria: Option<EndCriteria>,
        max_error_tolerance: Option<Real>,
        opt_method: Option<Box<dyn OptimizationMethod>>,
        error_accept: Option<Real>,
        use_max_error: bool,
        max_guesses: usize,
        backward_flat: bool,
        cutoff_strike: Real,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<SabrSwaptionVolatilityCube> {
        if backward_flat {
            fail!(
                "SABR cube backward-flat interpolation is not ported; deferred under #606. \
                 The SABR oracle grids have >= 2 nodes per axis."
            );
        }

        let n_strikes = strike_spreads.len();
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

        let n_options = cube.discrete().option_tenors().len();
        let n_swaps = cube.discrete().swap_tenors().len();
        require!(
            parameters_guess.len() == n_options * n_swaps,
            "parameters guess has {} rows, expected {} (nOptionTenors * nSwapTenors)",
            parameters_guess.len(),
            n_options * n_swaps
        );
        for (row, guesses) in parameters_guess.iter().enumerate() {
            require!(
                guesses.len() == N_SABR_PARAMS,
                "parameters guess row {} has {} entries, expected {N_SABR_PARAMS} (alpha, beta, nu, rho)",
                row + 1,
                guesses.len()
            );
        }

        let volatility_type = cube.volatility_type()?;
        let max_error_tolerance = max_error_tolerance.unwrap_or({
            if vega_weighted_smile_fit {
                SWAPTIONVOLCUBE_VEGAWEIGHTED_TOL
            } else {
                SWAPTIONVOLCUBE_TOL
            }
        });
        let error_accept = error_accept.unwrap_or(max_error_tolerance / 5.0);
        let end_criteria = match end_criteria {
            Some(criteria) => criteria,
            None => EndCriteria::new(60000, Some(100), 1e-8, 1e-8, Some(1e-8))?,
        };
        let opt_method: Box<dyn OptimizationMethod> = match opt_method {
            Some(method) => method,
            None => Box::new(LevenbergMarquardt::new(1e-8, 1e-8, 1e-8, false)),
        };

        let parameters_guess_cube =
            build_guess_cube(&cube, &parameters_guess, backward_flat, n_options, n_swaps)?;
        let market_vol_cube = zero_cube(&cube, n_strikes, backward_flat)?;
        let sparse_parameters = zero_cube(&cube, N_SABR_PARAMS + N_METADATA_LAYERS, backward_flat)?;

        let lazy = shared_mut(LazyObject::new(true));
        let updater = shared_mut(SabrCubeUpdater {
            lazy: SharedMut::clone(&lazy),
        });
        cube.observable()
            .register_observer(&(SharedMut::clone(&updater) as SharedMut<dyn Observer>));

        let base_updater = cube.base().updater();
        for row in &parameters_guess {
            for handle in row {
                handle.register_observer(&base_updater);
            }
        }

        Ok(SabrSwaptionVolatilityCube {
            cube,
            parameters_guess_quotes: parameters_guess,
            parameters_guess: RefCell::new(parameters_guess_cube),
            market_vol_cube: RefCell::new(market_vol_cube),
            sparse_parameters: RefCell::new(sparse_parameters),
            is_parameter_fixed,
            is_atm_calibrated,
            end_criteria,
            max_error_tolerance,
            opt_method: RefCell::new(opt_method),
            error_accept,
            use_max_error,
            max_guesses,
            backward_flat,
            cutoff_strike,
            volatility_type,
            lazy,
            _updater: updater,
        })
    }

    /// The embedded cube framework, for the ATM surface, strike spreads and base
    /// swap indexes.
    pub fn cube(&self) -> &SwaptionVolatilityCube {
        &self.cube
    }

    /// Rebuilds the guess cube (and, from #602's second commit, recalibrates) if
    /// a quote or the reference date has changed since the last computation.
    /// Every query calls this first, as C++'s `smileSectionImpl` calls
    /// `calculate()`.
    ///
    /// # Errors
    ///
    /// Propagates [`perform_calculations`](Self::perform_calculations).
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
        let n_options = self.cube.discrete().option_tenors().len();
        let n_swaps = self.cube.discrete().swap_tenors().len();
        *self.parameters_guess.borrow_mut() = build_guess_cube(
            &self.cube,
            &self.parameters_guess_quotes,
            self.backward_flat,
            n_options,
            n_swaps,
        )?;
        Ok(())
    }
}

/// The discrete grid as `(option_dates, swap_tenors, option_times, swap_lengths)`.
type CubeGrid = (Vec<Date>, Vec<Period>, Vec<Time>, Vec<Time>);

/// Reads the current discrete grid (calculating first so a moved reference date
/// is picked up).
fn cube_grid(cube: &SwaptionVolatilityCube) -> QlResult<CubeGrid> {
    cube.calculate()?;
    let discrete = cube.discrete();
    Ok((
        discrete.option_dates()?,
        discrete.swap_tenors().to_vec(),
        discrete.option_times()?,
        discrete.swap_lengths()?,
    ))
}

/// Builds the guess [`Cube`] (`setParameterGuess`, hpp:349-364): four layers
/// (alpha, beta, nu, rho) over the grid, each node read from its guess quote.
#[allow(clippy::needless_range_loop)]
fn build_guess_cube(
    cube: &SwaptionVolatilityCube,
    guess_quotes: &[Vec<Handle<dyn Quote>>],
    backward_flat: bool,
    n_options: usize,
    n_swaps: usize,
) -> QlResult<Cube> {
    let (dates, tenors, times, lengths) = cube_grid(cube)?;
    let mut guess = Cube::new(
        dates,
        tenors,
        times,
        lengths,
        N_SABR_PARAMS,
        true,
        backward_flat,
    )?;
    for i in 0..N_SABR_PARAMS {
        for j in 0..n_options {
            for k in 0..n_swaps {
                let value = guess_quotes[j * n_swaps + k][i].current_link()?.value()?;
                guess.set_element(i, j, k, value)?;
            }
        }
    }
    guess.update_interpolators()?;
    Ok(guess)
}

/// Builds a zero-filled [`Cube`] with `n_layers` layers over the current grid,
/// the placeholder the market and sparse-parameter stores hold until
/// `perform_calculations` fills them.
fn zero_cube(
    cube: &SwaptionVolatilityCube,
    n_layers: usize,
    backward_flat: bool,
) -> QlResult<Cube> {
    let (dates, tenors, times, lengths) = cube_grid(cube)?;
    Cube::new(dates, tenors, times, lengths, n_layers, true, backward_flat)
}
