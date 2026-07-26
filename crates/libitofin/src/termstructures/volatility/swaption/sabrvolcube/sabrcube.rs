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

use std::cell::RefCell;

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::math::interpolations::sabrinterpolation::SABRInterpolation;
use crate::math::matrix::Matrix;
use crate::math::optimization::endcriteria::{EndCriteria, EndCriteriaType};
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

        let market = self.build_market_vol_cube(n_options, n_swaps)?;
        let sparse = self.sabr_calibration(&market)?;
        *self.market_vol_cube.borrow_mut() = market;
        *self.sparse_parameters.borrow_mut() = sparse;

        if self.is_atm_calibrated {
            fail!(
                "SABR cube ATM-calibrated (dense) arm - fillVolatilityCube + denseParameters - is \
                 not yet ported; deferred to #603"
            );
        }
        Ok(())
    }

    /// Assembles `marketVolCube_` (hpp:370-386): one layer per strike spread,
    /// each node the ATM vol plus that strike's vol-spread quote.
    #[allow(clippy::needless_range_loop)]
    fn build_market_vol_cube(&self, n_options: usize, n_swaps: usize) -> QlResult<Cube> {
        let atm = self.cube.atm_vol().current_link()?;
        let strike_spreads = self.cube.strike_spreads();
        let n_strikes = strike_spreads.len();
        let (dates, tenors, times, lengths) = cube_grid(&self.cube)?;
        let mut market = Cube::new(
            dates.clone(),
            tenors.clone(),
            times,
            lengths.clone(),
            n_strikes,
            true,
            self.backward_flat,
        )?;
        for j in 0..n_options {
            for k in 0..n_swaps {
                let atm_forward = self.cube.atm_strike(dates[j], tenors[k])?;
                let atm_vol = atm.volatility(dates[j], lengths[k], atm_forward, false)?;
                for i in 0..n_strikes {
                    let spread = self.cube.vol_spreads()[j * n_swaps + k][i]
                        .current_link()?
                        .value()?;
                    market.set_element(i, j, k, atm_vol + spread)?;
                }
            }
        }
        market.update_interpolators()?;
        Ok(market)
    }

    /// The sparse per-node SABR calibration (`sabrCalibration`, hpp:411-535).
    /// For each `(option, swap)` node it fits the four SABR parameters to the
    /// node's `(atmForward + spread, marketVol)` smile with
    /// [`SABRInterpolation`], honouring the per-node guess, the fixed flags and
    /// vega weighting, and fails loudly (mapping C++'s `QL_ENSURE`s to `Err`)
    /// when a node hits `MaxIterations` or exceeds the error tolerance.
    ///
    /// The returned [`Cube`] carries eight layers: the four parameters
    /// (alpha, beta, nu, rho), then the forward, rms error, max error and an
    /// end-criteria code (hpp:515-531).
    ///
    /// A non-zero ATM shift returns `Err` naming #586 (displaced SABR).
    #[allow(clippy::needless_range_loop)]
    fn sabr_calibration(&self, market: &Cube) -> QlResult<Cube> {
        let option_times = market.option_times().to_vec();
        let swap_lengths = market.swap_lengths().to_vec();
        let option_dates = market.option_dates().to_vec();
        let swap_tenors = market.swap_tenors().to_vec();
        let n_options = option_times.len();
        let n_swaps = swap_lengths.len();
        let strike_spreads = self.cube.strike_spreads();
        let n_strikes = strike_spreads.len();
        let vega_weighted = self.cube.vega_weighted_smile_fit();
        let atm = self.cube.atm_vol().current_link()?;
        let guess = self.parameters_guess.borrow();
        let mut method = self.opt_method.borrow_mut();

        let n_layers = N_SABR_PARAMS + N_METADATA_LAYERS;
        let mut layers: Vec<Matrix> = (0..n_layers)
            .map(|_| Matrix::with_size(n_options, n_swaps))
            .collect();

        for j in 0..n_options {
            for k in 0..n_swaps {
                let atm_forward = self.cube.atm_strike(option_dates[j], swap_tenors[k])?;
                let shift = atm.shift_time(option_times[j], swap_lengths[k], false)?;
                if shift != 0.0 {
                    fail!(
                        "SABR cube with non-zero ATM shift ({shift}) - displaced SABR - is not yet \
                         ported; deferred to #586"
                    );
                }

                let mut strikes = Vec::with_capacity(n_strikes);
                let mut vols = Vec::with_capacity(n_strikes);
                for i in 0..n_strikes {
                    let strike = atm_forward + strike_spreads[i];
                    if strike + shift >= self.cutoff_strike {
                        strikes.push(strike);
                        vols.push(market.points()[i][(j, k)]);
                    }
                }

                let node_guess = guess.value(option_times[j], swap_lengths[k])?;
                let mut interp = SABRInterpolation::new(
                    strikes,
                    vols,
                    option_times[j],
                    atm_forward,
                    node_guess[0],
                    node_guess[1],
                    node_guess[2],
                    node_guess[3],
                    self.is_parameter_fixed[0],
                    self.is_parameter_fixed[1],
                    self.is_parameter_fixed[2],
                    self.is_parameter_fixed[3],
                    vega_weighted,
                    self.end_criteria,
                    self.error_accept,
                    self.max_guesses,
                    self.volatility_type,
                )?;
                interp.update(&mut **method)?;

                if interp.end_criteria() == EndCriteriaType::MaxIterations {
                    fail!(
                        "global swaptions calibration failed: MaxIterations reached at option \
                         maturity {}, swap tenor {} (rms error {}, max error {})",
                        option_dates[j],
                        swap_tenors[k],
                        interp.rms_error(),
                        interp.max_error()
                    );
                }
                let error_metric = if self.use_max_error {
                    interp.max_error()
                } else {
                    interp.rms_error()
                };
                if error_metric >= self.max_error_tolerance {
                    fail!(
                        "global swaptions calibration failed: error tolerance {} exceeded at \
                         option maturity {}, swap tenor {} (rms error {}, max error {})",
                        self.max_error_tolerance,
                        option_dates[j],
                        swap_tenors[k],
                        interp.rms_error(),
                        interp.max_error()
                    );
                }

                layers[0][(j, k)] = interp.alpha();
                layers[1][(j, k)] = interp.beta();
                layers[2][(j, k)] = interp.nu();
                layers[3][(j, k)] = interp.rho();
                layers[4][(j, k)] = atm_forward;
                layers[5][(j, k)] = interp.rms_error();
                layers[6][(j, k)] = interp.max_error();
                layers[7][(j, k)] = end_criteria_code(interp.end_criteria());
            }
        }
        drop(guess);
        drop(method);

        let mut sparse = Cube::new(
            option_dates,
            swap_tenors,
            option_times,
            swap_lengths,
            n_layers,
            true,
            self.backward_flat,
        )?;
        for (layer, matrix) in layers.into_iter().enumerate() {
            sparse.set_layer(layer, matrix)?;
        }
        sparse.update_interpolators()?;
        Ok(sparse)
    }

    /// The interpolated guess parameters `[alpha, beta, nu, rho]` at
    /// `(option_time, swap_length)` (recomputed after any quote bump). At a grid
    /// node it returns that node's guess exactly.
    ///
    /// # Errors
    ///
    /// Propagates the lazy recomputation.
    pub fn parameters_guess_value(
        &self,
        option_time: Time,
        swap_length: Time,
    ) -> QlResult<Vec<Real>> {
        self.calculate()?;
        self.parameters_guess
            .borrow()
            .value(option_time, swap_length)
    }

    /// The calibrated sparse SABR parameters at `(option_time, swap_length)`:
    /// `[alpha, beta, nu, rho, forward, rms_error, max_error, end_criteria]`
    /// (the layers of `sparseParameters_`). At a grid node it returns that node's
    /// fitted values exactly.
    ///
    /// # Errors
    ///
    /// Propagates the calibration (including the #586/#603 deferrals).
    pub fn sparse_parameter_values(
        &self,
        option_time: Time,
        swap_length: Time,
    ) -> QlResult<Vec<Real>> {
        self.calculate()?;
        self.sparse_parameters
            .borrow()
            .value(option_time, swap_length)
    }
}

/// Maps an [`EndCriteriaType`] to the numeric code stored in the sparse cube's
/// end-criteria layer, matching the Rust variant order (the analogue of C++'s
/// `Integer(EndCriteria::Type)`; the Rust enum has an extra variant so codes >= 6
/// need not coincide). Cosmetic in this ticket's scope: the fail-loud checks read
/// the enum directly, and the consumers #603/#604 read the parameter and forward
/// layers.
fn end_criteria_code(criteria: EndCriteriaType) -> Real {
    match criteria {
        EndCriteriaType::None => 0.0,
        EndCriteriaType::MaxIterations => 1.0,
        EndCriteriaType::StationaryPoint => 2.0,
        EndCriteriaType::StationaryFunctionValue => 3.0,
        EndCriteriaType::StationaryFunctionAccuracy => 4.0,
        EndCriteriaType::ZeroGradientNorm => 5.0,
        EndCriteriaType::FunctionEpsilonTooSmall => 6.0,
        EndCriteriaType::Unknown => 7.0,
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

impl SwaptionCubeSmileSection for SabrSwaptionVolatilityCube {
    fn smile_section_impl(
        &self,
        _option_time: Time,
        _swap_length: Time,
    ) -> QlResult<Shared<dyn SmileSection>> {
        fail!(
            "SABR cube smile section (SabrSmileSection bridge) is not yet ported; deferred to #604"
        )
    }
}

impl AsObservable for SabrSwaptionVolatilityCube {
    fn observable(&self) -> &Observable {
        self.cube.observable()
    }
}

impl TermStructure for SabrSwaptionVolatilityCube {
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

impl VolatilityTermStructure for SabrSwaptionVolatilityCube {
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

impl SwaptionVolatilityStructure for SabrSwaptionVolatilityCube {
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
        self.volatility_type
    }

    fn shift_impl(&self, option_time: Time, swap_length: Time) -> QlResult<Real> {
        self.cube
            .atm_vol()
            .current_link()?
            .shift_time(option_time, swap_length, false)
    }
}

/// A 2x2-node SABR cube fixture over a flat ATM surface and two hand-built swap
/// indexes. The guess and vol-spread quotes are held as [`SimpleQuote`]s so the
/// observer arm can bump them; [`atm_strike`](SwaptionVolatilityCube::atm_strike)
/// forecasts a positive swap rate off the 5% forwarding curve, and the flat ATM
/// vol is shift-0.
#[cfg(test)]
mod tests {
    use super::*;

    use crate::currency::Currency;
    use crate::indexes::{Euribor, IborIndex, SwapIndex};
    use crate::interestrate::Compounding;
    use crate::patterns::observable::AsObservable;
    use crate::quotes::SimpleQuote;
    use crate::shared::{shared, shared_mut};
    use crate::termstructures::volatility::sabr::sabr_volatility;
    use crate::termstructures::volatility::swaption::SwaptionCubeSmileSection;
    use crate::termstructures::yields::FlatForward;
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::calendars::target::Target;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;
    use crate::time::daycounters::thirty360::{Convention, Thirty360};
    use crate::time::frequency::Frequency;
    use crate::time::timeunit::TimeUnit;

    const BDC: BusinessDayConvention = BusinessDayConvention::ModifiedFollowing;

    fn today() -> Date {
        Date::new(15, Month::June, 2026)
    }

    fn settings_today() -> Shared<Settings<Date>> {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today());
        settings
    }

    fn flat_curve(rate: Rate) -> Handle<dyn YieldTermStructure> {
        Handle::new(shared(FlatForward::with_rate(
            today(),
            rate,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>)
    }

    /// A flat, shift-0 swaption vol surface standing in for the ATM matrix.
    struct MockAtmVol {
        base: TermStructureBase,
        vol: Volatility,
    }

    impl AsObservable for MockAtmVol {
        fn observable(&self) -> &Observable {
            self.base.observable()
        }
    }

    impl TermStructure for MockAtmVol {
        fn base(&self) -> &TermStructureBase {
            &self.base
        }
        fn max_date(&self) -> Date {
            Date::max_date()
        }
    }

    impl VolatilityTermStructure for MockAtmVol {
        fn business_day_convention(&self) -> BusinessDayConvention {
            BDC
        }
        fn min_strike(&self) -> Rate {
            Rate::MIN
        }
        fn max_strike(&self) -> Rate {
            Rate::MAX
        }
    }

    impl SwaptionVolatilityStructure for MockAtmVol {
        fn volatility_impl(&self, _t: Time, _l: Time, _strike: Rate) -> QlResult<Volatility> {
            Ok(self.vol)
        }
        fn max_swap_tenor(&self) -> Period {
            Period::new(100, TimeUnit::Years)
        }
    }

    fn atm_handle(vol: Volatility) -> Handle<dyn SwaptionVolatilityStructure> {
        Handle::new(shared(MockAtmVol {
            base: TermStructureBase::with_reference_date(
                today(),
                Some(Target::new()),
                Some(Actual365Fixed::new()),
            ),
            vol,
        }) as Shared<dyn SwaptionVolatilityStructure>)
    }

    fn long_index(
        euribor6m: &Shared<IborIndex>,
        settings: &Shared<Settings<Date>>,
    ) -> Shared<SwapIndex> {
        shared(SwapIndex::new(
            "LongSwap".into(),
            Period::new(5, TimeUnit::Years),
            2,
            Currency::eur(),
            Target::new(),
            Period::new(1, TimeUnit::Years),
            BDC,
            Thirty360::with_convention(Convention::BondBasis),
            Shared::clone(euribor6m),
            Shared::clone(settings),
        ))
    }

    fn short_index(
        euribor6m: &Shared<IborIndex>,
        settings: &Shared<Settings<Date>>,
    ) -> Shared<SwapIndex> {
        shared(SwapIndex::new(
            "ShortSwap".into(),
            Period::new(1, TimeUnit::Years),
            2,
            Currency::eur(),
            Target::new(),
            Period::new(1, TimeUnit::Years),
            BDC,
            Thirty360::with_convention(Convention::BondBasis),
            Shared::clone(euribor6m),
            Shared::clone(settings),
        ))
    }

    fn option_tenors() -> Vec<Period> {
        vec![
            Period::new(1, TimeUnit::Years),
            Period::new(2, TimeUnit::Years),
        ]
    }

    fn swap_tenors() -> Vec<Period> {
        vec![
            Period::new(1, TimeUnit::Years),
            Period::new(5, TimeUnit::Years),
        ]
    }

    /// Seven strike spreads around the ATM level, all keeping strikes positive
    /// off a ~5% forward, enough points to identify the SABR parameters.
    fn strike_spreads() -> Vec<Real> {
        vec![-0.02, -0.01, -0.005, 0.0, 0.005, 0.01, 0.02]
    }

    const N_NODES: usize = 4;
    const ATM_VOL: Volatility = 0.20;

    /// Records whether it was notified (the QuantLib test-suite `Flag`).
    #[derive(Default)]
    struct Flag {
        up: bool,
    }

    impl Observer for Flag {
        fn update(&mut self) {
            self.up = true;
        }
    }

    struct Fixture {
        settings: Shared<Settings<Date>>,
        cube: SabrSwaptionVolatilityCube,
        guess_quotes: Vec<Vec<Shared<SimpleQuote>>>,
        spread_quotes: Vec<Vec<Shared<SimpleQuote>>>,
    }

    /// Builds the fixture with per-node guesses `guess[node] = [alpha, beta, nu,
    /// rho]`, the given fixed flags, tolerance, and vega weighting, over the flat
    /// shift-0 ATM surface with `is_atm_calibrated = false`.
    fn fixture(
        guess: Vec<[Real; N_SABR_PARAMS]>,
        is_fixed: [bool; N_SABR_PARAMS],
        max_error_tolerance: Option<Real>,
        vega: bool,
    ) -> Fixture {
        fixture_full(
            atm_handle(ATM_VOL),
            guess,
            is_fixed,
            max_error_tolerance,
            vega,
            false,
        )
    }

    /// The general fixture builder: takes the ATM handle and `is_atm_calibrated`
    /// so the shift and dense-arm seams can be exercised. Vol-spread quotes start
    /// at zero; a calibration test sets them to a synthetic smile.
    fn fixture_full(
        atm: Handle<dyn SwaptionVolatilityStructure>,
        guess: Vec<[Real; N_SABR_PARAMS]>,
        is_fixed: [bool; N_SABR_PARAMS],
        max_error_tolerance: Option<Real>,
        vega: bool,
        is_atm_calibrated: bool,
    ) -> Fixture {
        let settings = settings_today();
        let euribor6m = shared(Euribor::six_months(
            flat_curve(0.05),
            Shared::clone(&settings),
        ));
        let long = long_index(&euribor6m, &settings);
        let short = short_index(&euribor6m, &settings);

        let n_strikes = strike_spreads().len();
        let spread_quotes: Vec<Vec<Shared<SimpleQuote>>> = (0..N_NODES)
            .map(|_| {
                (0..n_strikes)
                    .map(|_| shared(SimpleQuote::new(0.0)))
                    .collect()
            })
            .collect();
        let vol_spreads: Vec<Vec<Handle<dyn Quote>>> = spread_quotes
            .iter()
            .map(|row| {
                row.iter()
                    .map(|q| Handle::new(Shared::clone(q) as Shared<dyn Quote>))
                    .collect()
            })
            .collect();

        let guess_quotes: Vec<Vec<Shared<SimpleQuote>>> = guess
            .iter()
            .map(|node| node.iter().map(|&v| shared(SimpleQuote::new(v))).collect())
            .collect();
        let parameters_guess: Vec<Vec<Handle<dyn Quote>>> = guess_quotes
            .iter()
            .map(|row| {
                row.iter()
                    .map(|q| Handle::new(Shared::clone(q) as Shared<dyn Quote>))
                    .collect()
            })
            .collect();

        let cube = SabrSwaptionVolatilityCube::new(
            atm,
            option_tenors(),
            swap_tenors(),
            strike_spreads(),
            vol_spreads,
            long,
            short,
            vega,
            parameters_guess,
            is_fixed,
            is_atm_calibrated,
            None,
            max_error_tolerance,
            None,
            None,
            false,
            50,
            false,
            0.0001,
            Shared::clone(&settings),
        )
        .unwrap();

        Fixture {
            settings,
            cube,
            guess_quotes,
            spread_quotes,
        }
    }

    /// The full node coordinates in row-major `j*nSwaps + k` order:
    /// `(option_date, swap_tenor, option_time, swap_length)`.
    fn node_info(cube: &SabrSwaptionVolatilityCube) -> Vec<(Date, Period, Time, Time)> {
        let discrete = cube.cube().discrete();
        let dates = discrete.option_dates().unwrap();
        let tenors = discrete.swap_tenors().to_vec();
        let times = discrete.option_times().unwrap();
        let lengths = discrete.swap_lengths().unwrap();
        let mut nodes = Vec::with_capacity(times.len() * lengths.len());
        for j in 0..times.len() {
            for k in 0..lengths.len() {
                nodes.push((dates[j], tenors[k], times[j], lengths[k]));
            }
        }
        nodes
    }

    /// Overwrites every vol-spread quote so each node's market smile IS the
    /// `params` SABR smile: at node `n`, strike `atm_forward_n + spread_i` gets
    /// `sabr_volatility(...) - ATM_VOL` so `marketVolCube = ATM_VOL + spread`
    /// reconstructs the SABR vol exactly. `atm_forward_n` and the expiry are read
    /// off the constructed cube - the same values the calibration feeds
    /// `SABRInterpolation`.
    fn seed_sabr_smile(f: &Fixture, params: [Real; N_SABR_PARAMS]) {
        let spreads = strike_spreads();
        for (n, &(od, st, ot, _sl)) in node_info(&f.cube).iter().enumerate() {
            let forward = f.cube.cube().atm_strike(od, st).unwrap();
            for (i, &spread) in spreads.iter().enumerate() {
                let strike = forward + spread;
                let vol = sabr_volatility(
                    strike,
                    forward,
                    ot,
                    params[0],
                    params[1],
                    params[2],
                    params[3],
                    VolatilityType::ShiftedLognormal,
                )
                .unwrap();
                f.spread_quotes[n][i].set_value(vol - ATM_VOL);
            }
        }
    }

    /// The grid nodes as `(option_time, swap_length)`, node-index order matching
    /// the row-major `j*nSwaps + k` layout of the quote grid.
    fn grid_nodes(cube: &SabrSwaptionVolatilityCube) -> Vec<(Time, Time)> {
        let discrete = cube.cube().discrete();
        let times = discrete.option_times().unwrap();
        let lengths = discrete.swap_lengths().unwrap();
        let mut nodes = Vec::with_capacity(times.len() * lengths.len());
        for &t in &times {
            for &l in &lengths {
                nodes.push((t, l));
            }
        }
        nodes
    }

    fn uniform_guess(g: [Real; N_SABR_PARAMS]) -> Vec<[Real; N_SABR_PARAMS]> {
        vec![g; N_NODES]
    }

    #[test]
    fn backward_flat_defers_to_606() {
        let settings = settings_today();
        let euribor6m = shared(Euribor::six_months(
            flat_curve(0.05),
            Shared::clone(&settings),
        ));
        let long = long_index(&euribor6m, &settings);
        let short = short_index(&euribor6m, &settings);
        let n_strikes = strike_spreads().len();
        let vol_spreads: Vec<Vec<Handle<dyn Quote>>> = (0..N_NODES)
            .map(|_| {
                (0..n_strikes)
                    .map(|_| Handle::new(shared(SimpleQuote::new(0.0)) as Shared<dyn Quote>))
                    .collect()
            })
            .collect();
        let parameters_guess: Vec<Vec<Handle<dyn Quote>>> = (0..N_NODES)
            .map(|_| {
                (0..N_SABR_PARAMS)
                    .map(|_| Handle::new(shared(SimpleQuote::new(0.2)) as Shared<dyn Quote>))
                    .collect()
            })
            .collect();
        let err = SabrSwaptionVolatilityCube::new(
            atm_handle(ATM_VOL),
            option_tenors(),
            swap_tenors(),
            strike_spreads(),
            vol_spreads,
            long,
            short,
            false,
            parameters_guess,
            [false; N_SABR_PARAMS],
            false,
            None,
            None,
            None,
            None,
            false,
            50,
            true,
            0.0001,
            settings,
        )
        .err()
        .expect("backward_flat = true must be rejected");
        assert!(err.to_string().contains("606"), "err was: {err}");
    }

    #[test]
    fn wrong_guess_shape_is_rejected() {
        let settings = settings_today();
        let euribor6m = shared(Euribor::six_months(
            flat_curve(0.05),
            Shared::clone(&settings),
        ));
        let long = long_index(&euribor6m, &settings);
        let short = short_index(&euribor6m, &settings);
        let n_strikes = strike_spreads().len();
        let build = |guess: Vec<Vec<Handle<dyn Quote>>>| {
            let vol_spreads: Vec<Vec<Handle<dyn Quote>>> = (0..N_NODES)
                .map(|_| {
                    (0..n_strikes)
                        .map(|_| Handle::new(shared(SimpleQuote::new(0.0)) as Shared<dyn Quote>))
                        .collect()
                })
                .collect();
            SabrSwaptionVolatilityCube::new(
                atm_handle(ATM_VOL),
                option_tenors(),
                swap_tenors(),
                strike_spreads(),
                vol_spreads,
                Shared::clone(&long),
                Shared::clone(&short),
                false,
                guess,
                [false; N_SABR_PARAMS],
                false,
                None,
                None,
                None,
                None,
                false,
                50,
                false,
                0.0001,
                Shared::clone(&settings),
            )
        };
        let too_few_rows: Vec<Vec<Handle<dyn Quote>>> = (0..N_NODES - 1)
            .map(|_| {
                (0..N_SABR_PARAMS)
                    .map(|_| Handle::new(shared(SimpleQuote::new(0.2)) as Shared<dyn Quote>))
                    .collect()
            })
            .collect();
        assert!(
            build(too_few_rows).is_err(),
            "row count must be nOptions*nSwaps"
        );

        let wrong_cols: Vec<Vec<Handle<dyn Quote>>> = (0..N_NODES)
            .map(|_| {
                (0..N_SABR_PARAMS - 1)
                    .map(|_| Handle::new(shared(SimpleQuote::new(0.2)) as Shared<dyn Quote>))
                    .collect()
            })
            .collect();
        assert!(
            build(wrong_cols).is_err(),
            "each row must hold four guesses"
        );
    }

    #[test]
    fn guess_cube_recovers_the_quote_values_at_each_node() {
        let guess: Vec<[Real; N_SABR_PARAMS]> = (0..N_NODES)
            .map(|n| {
                let base = n as Real;
                [0.20 + base, 0.60 + base, 0.30 + base, -0.10 + 0.01 * base]
            })
            .collect();
        let f = fixture(guess.clone(), [false; N_SABR_PARAMS], None, false);
        let nodes = grid_nodes(&f.cube);
        for (node, &(t, l)) in nodes.iter().enumerate() {
            let got = f.cube.parameters_guess_value(t, l).unwrap();
            for p in 0..N_SABR_PARAMS {
                assert!(
                    (got[p] - guess[node][p]).abs() < 1e-14,
                    "node {node} param {p}: got {}, want {}",
                    got[p],
                    guess[node][p]
                );
            }
        }
        let _ = &f.settings;
        let _ = &f.spread_quotes;
    }

    #[test]
    fn guess_quote_bump_rebuilds_the_guess_cube_and_notifies() {
        let f = fixture(
            uniform_guess([0.20, 0.60, 0.30, -0.10]),
            [false; N_SABR_PARAMS],
            None,
            false,
        );
        let flag = shared_mut(Flag::default());
        f.cube
            .observable()
            .register_observer(&(Shared::clone(&flag) as SharedMut<dyn Observer>));

        let nodes = grid_nodes(&f.cube);
        let (t0, l0) = nodes[0];
        let before = f.cube.parameters_guess_value(t0, l0).unwrap();
        assert!((before[0] - 0.20).abs() < 1e-14);

        flag.borrow_mut().up = false;
        f.guess_quotes[0][0].set_value(0.35);
        assert!(
            flag.borrow().up,
            "a guess-quote bump must notify the cube's observers"
        );

        let after = f.cube.parameters_guess_value(t0, l0).unwrap();
        assert!(
            (after[0] - 0.35).abs() < 1e-14,
            "guess cube must rebuild from the bumped quote: got {}",
            after[0]
        );
    }

    const TRUE_PARAMS: [Real; N_SABR_PARAMS] = [0.20, 0.60, 0.30, -0.10];

    /// A flat swaption vol surface carrying a constant non-zero lognormal shift,
    /// to drive the displaced-SABR (#586) seam.
    struct MockShiftedAtmVol {
        base: TermStructureBase,
        vol: Volatility,
        shift: Real,
    }

    impl AsObservable for MockShiftedAtmVol {
        fn observable(&self) -> &Observable {
            self.base.observable()
        }
    }

    impl TermStructure for MockShiftedAtmVol {
        fn base(&self) -> &TermStructureBase {
            &self.base
        }
        fn max_date(&self) -> Date {
            Date::max_date()
        }
    }

    impl VolatilityTermStructure for MockShiftedAtmVol {
        fn business_day_convention(&self) -> BusinessDayConvention {
            BDC
        }
        fn min_strike(&self) -> Rate {
            Rate::MIN
        }
        fn max_strike(&self) -> Rate {
            Rate::MAX
        }
    }

    impl SwaptionVolatilityStructure for MockShiftedAtmVol {
        fn volatility_impl(&self, _t: Time, _l: Time, _strike: Rate) -> QlResult<Volatility> {
            Ok(self.vol)
        }
        fn max_swap_tenor(&self) -> Period {
            Period::new(100, TimeUnit::Years)
        }
        fn shift_impl(&self, _option_time: Time, _swap_length: Time) -> QlResult<Real> {
            Ok(self.shift)
        }
    }

    #[test]
    fn sparse_calibration_recovers_the_true_params_at_every_node() {
        let guess = uniform_guess([0.15, TRUE_PARAMS[1], 0.20, 0.0]);
        let is_fixed = [false, true, false, false];
        let f = fixture(guess, is_fixed, None, false);
        seed_sabr_smile(&f, TRUE_PARAMS);

        let mut worst_param = 0.0_f64;
        let mut worst_error = 0.0_f64;
        for &(_od, _st, ot, sl) in node_info(&f.cube).iter() {
            let p = f.cube.sparse_parameter_values(ot, sl).unwrap();
            for (idx, &want) in TRUE_PARAMS.iter().enumerate() {
                worst_param = worst_param.max((p[idx] - want).abs());
            }
            assert!(
                (p[1] - TRUE_PARAMS[1]).abs() < 1e-15,
                "fixed beta must stay at its guess exactly: got {}",
                p[1]
            );
            worst_error = worst_error.max(p[5]);
        }
        assert!(
            worst_param < 1e-8,
            "worst SABR parameter recovery error {worst_param}"
        );
        assert!(
            worst_error < 1e-6,
            "rms fit error should be tiny for exact synthetic data, worst {worst_error}"
        );
    }

    #[test]
    fn fixed_parameter_is_held_at_a_wrong_value() {
        let wrong_beta = 0.30;
        let guess = uniform_guess([0.15, wrong_beta, 0.20, 0.0]);
        let is_fixed = [false, true, false, false];
        let f = fixture(guess, is_fixed, Some(1.0), false);
        seed_sabr_smile(&f, TRUE_PARAMS);

        for &(_od, _st, ot, sl) in node_info(&f.cube).iter() {
            let p = f.cube.sparse_parameter_values(ot, sl).unwrap();
            assert!(
                (p[1] - wrong_beta).abs() < 1e-15,
                "wrongly-fixed beta must be held exactly at {wrong_beta}, got {}",
                p[1]
            );
        }
    }

    #[test]
    fn recalibrates_when_a_fixed_guess_is_bumped() {
        let fixed_alpha = 0.25;
        let guess = uniform_guess([fixed_alpha, TRUE_PARAMS[1], TRUE_PARAMS[2], TRUE_PARAMS[3]]);
        let is_fixed = [true, false, false, false];
        let f = fixture(guess, is_fixed, Some(1.0), false);
        seed_sabr_smile(&f, TRUE_PARAMS);

        let (_od, _st, ot0, sl0) = node_info(&f.cube)[0];
        let before = f.cube.sparse_parameter_values(ot0, sl0).unwrap();
        assert!(
            (before[0] - fixed_alpha).abs() < 1e-15,
            "fixed alpha must be pinned at its guess, got {}",
            before[0]
        );

        f.guess_quotes[0][0].set_value(0.18);
        let after = f.cube.sparse_parameter_values(ot0, sl0).unwrap();
        assert!(
            (after[0] - 0.18).abs() < 1e-15,
            "bumping the fixed alpha-guess must rebuild the guess and recalibrate: got {}",
            after[0]
        );
    }

    #[test]
    fn non_zero_atm_shift_defers_to_586() {
        let atm = Handle::new(shared(MockShiftedAtmVol {
            base: TermStructureBase::with_reference_date(
                today(),
                Some(Target::new()),
                Some(Actual365Fixed::new()),
            ),
            vol: ATM_VOL,
            shift: 0.01,
        }) as Shared<dyn SwaptionVolatilityStructure>);
        let f = fixture_full(
            atm,
            uniform_guess([0.15, TRUE_PARAMS[1], 0.20, 0.0]),
            [false, true, false, false],
            None,
            false,
            false,
        );
        let (_od, _st, ot, sl) = node_info(&f.cube)[0];
        let err = f
            .cube
            .sparse_parameter_values(ot, sl)
            .expect_err("non-zero shift must defer");
        assert!(err.to_string().contains("586"), "err was: {err}");
    }

    #[test]
    fn smile_section_and_volatility_defer_to_604() {
        let f = fixture(
            uniform_guess([0.15, TRUE_PARAMS[1], 0.20, 0.0]),
            [false, true, false, false],
            None,
            false,
        );
        seed_sabr_smile(&f, TRUE_PARAMS);
        let (_od, _st, ot, sl) = node_info(&f.cube)[0];
        f.cube
            .sparse_parameter_values(ot, sl)
            .expect("sparse calibration itself succeeds");

        match f.cube.smile_section_impl(ot, sl) {
            Ok(_) => panic!("smile section must defer to #604"),
            Err(e) => assert!(e.to_string().contains("604"), "err was: {e}"),
        }

        let vol_err = f
            .cube
            .volatility_impl(ot, sl, 0.05)
            .expect_err("volatility query routes through the smile hook and defers to #604");
        assert!(vol_err.to_string().contains("604"), "err was: {vol_err}");
    }

    #[test]
    fn atm_calibrated_dense_arm_defers_to_603() {
        let f = fixture_full(
            atm_handle(ATM_VOL),
            uniform_guess([0.15, TRUE_PARAMS[1], 0.20, 0.0]),
            [false, true, false, false],
            None,
            false,
            true,
        );
        seed_sabr_smile(&f, TRUE_PARAMS);
        let (_od, _st, ot, sl) = node_info(&f.cube)[0];
        let err = f
            .cube
            .sparse_parameter_values(ot, sl)
            .expect_err("is_atm_calibrated dense arm must defer");
        assert!(err.to_string().contains("603"), "err was: {err}");
    }
}
