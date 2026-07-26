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
//! - Construction, the guess store, the observer wiring, the sparse per-node
//!   SABR calibration, and (from #603) the `isAtmCalibrated` dense arm work.
//! - The dense arm (`fillVolatilityCube` + `spreadVolInterpolation` +
//!   `denseParameters_`, hpp:394-398/646-857) widens the cube with ATM-anchored
//!   points and re-fits SABR over the widened grid; the private
//!   `smileSection(paramCube)` helper (hpp:861-874) lands here too, building the
//!   per-node [`SabrSmileSection`](crate::termstructures::volatility::sabrsmilesection).
//!   The ATM structure's grid is read through
//!   [`SwaptionVolatilityStructure::discrete_grid`], the downcast-equivalent for
//!   C++'s `dynamic_pointer_cast<SwaptionVolatilityDiscrete>`; a non-grid ATM
//!   handle returns `Err` there, exactly where the C++ cast would null-deref.
//! - `smileSectionImpl` (the volatility query) serves the calibrated smile
//!   (this ticket, #604): it [`calculate`](SabrSwaptionVolatilityCube::calculate)s,
//!   then builds the [`SabrSmileSection`] from the dense parameter cube when
//!   `isAtmCalibrated`, else the sparse cube (hpp:878-884). `volatilityImpl`
//!   routes through it via the #594 seam, and the public
//!   [`smile_section`](SabrSwaptionVolatilityCube::smile_section) exposes the
//!   concrete smile for an option/swap tenor.
//! - A non-zero ATM shift returns `Err` naming #586 (displaced SABR); the oracle
//!   fixtures are shift-0.
//! - `backwardFlat = true` returns `Err` naming #606, as the #601 [`Cube`] does.
//! - `updateAfterRecalibration`, `sabrCalibrationSection` and `recalibration`
//!   (the section-recalibration API) are not ported; they defer as their own
//!   issue.
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
use crate::shared::{Shared, SharedMut, shared, shared_mut};
use crate::termstructures::volatility::sabrsmilesection::SabrSmileSection;
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
    dense_parameters: RefCell<Cube>,
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
        let dense_parameters = zero_cube(&cube, N_SABR_PARAMS + N_METADATA_LAYERS, backward_flat)?;

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
            dense_parameters: RefCell::new(dense_parameters),
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

        if self.is_atm_calibrated {
            let sparse_smiles = self.create_sparse_smiles(&sparse)?;
            let mut vol_cube_atm_calibrated = self.build_market_vol_cube(n_options, n_swaps)?;
            self.fill_volatility_cube(&mut vol_cube_atm_calibrated, &sparse, &sparse_smiles)?;
            *self.dense_parameters.borrow_mut() =
                self.sabr_calibration(&vol_cube_atm_calibrated)?;
        }

        *self.market_vol_cube.borrow_mut() = market;
        *self.sparse_parameters.borrow_mut() = sparse;
        Ok(())
    }

    /// Widens `vol_cube_atm_calibrated` with ATM-anchored points
    /// (`fillVolatilityCube`, hpp:646-715). It merges the ATM structure's own
    /// option/swap grid (read through
    /// [`SwaptionVolatilityStructure::discrete_grid`], the downcast-equivalent for
    /// C++'s `dynamic_pointer_cast<SwaptionVolatilityDiscrete>`) with the cube's,
    /// and for every merged node NOT already present in the cube sets the ATM vol
    /// plus the interpolated per-strike vol spread ([`spread_vol_interpolation`]).
    /// The four merged axes are index-aligned exactly as C++ leaves them: the
    /// binary-search checks read the cube's ORIGINAL axes, captured before the
    /// widening loop mutates the cube.
    #[allow(clippy::needless_range_loop)]
    fn fill_volatility_cube(
        &self,
        vol_cube_atm_calibrated: &mut Cube,
        sparse: &Cube,
        sparse_smiles: &[Vec<SabrSmileSection>],
    ) -> QlResult<()> {
        let atm = self.cube.atm_vol().current_link()?;
        let grid = atm.discrete_grid()?;
        let n_strikes = self.cube.strike_spreads().len();

        let cube_option_times = vol_cube_atm_calibrated.option_times().to_vec();
        let cube_swap_lengths = vol_cube_atm_calibrated.swap_lengths().to_vec();

        let atm_option_times = union_sorted_times(&grid.option_times, &cube_option_times);
        let atm_swap_lengths = union_sorted_times(&grid.swap_lengths, &cube_swap_lengths);
        let atm_option_dates =
            union_sorted_dates(&grid.option_dates, vol_cube_atm_calibrated.option_dates());
        let atm_swap_tenors =
            union_sorted_tenors(&grid.swap_tenors, vol_cube_atm_calibrated.swap_tenors());

        for j in 0..atm_option_times.len() {
            for k in 0..atm_swap_lengths.len() {
                let expand_option_times = !sorted_contains(&cube_option_times, atm_option_times[j]);
                let expand_swap_lengths = !sorted_contains(&cube_swap_lengths, atm_swap_lengths[k]);
                if !(expand_option_times || expand_swap_lengths) {
                    continue;
                }
                let atm_forward = self
                    .cube
                    .atm_strike(atm_option_dates[j], atm_swap_tenors[k])?;
                let atm_vol =
                    atm.volatility(atm_option_dates[j], atm_swap_lengths[k], atm_forward, false)?;
                let spread_vols = self.spread_vol_interpolation(
                    atm_option_dates[j],
                    atm_swap_tenors[k],
                    sparse,
                    sparse_smiles,
                )?;
                let mut vol_atm_calibrated = Vec::with_capacity(n_strikes);
                for i in 0..n_strikes {
                    vol_atm_calibrated.push(atm_vol + spread_vols[i]);
                }
                vol_cube_atm_calibrated.set_point(
                    atm_option_dates[j],
                    atm_swap_tenors[k],
                    atm_option_times[j],
                    atm_swap_lengths[k],
                    vol_atm_calibrated,
                )?;
            }
        }
        vol_cube_atm_calibrated.update_interpolators()?;
        Ok(())
    }

    /// The sparse smile sections (`createSparseSmiles`, hpp:719-734): for each
    /// sparse `(optionTime, swapLength)` node, the [`SabrSmileSection`] built from
    /// the sparse parameter cube. Row-major over the sparse option axis, each row
    /// over the sparse swap axis, mirroring C++'s `sparseSmiles_`.
    fn create_sparse_smiles(&self, sparse: &Cube) -> QlResult<Vec<Vec<SabrSmileSection>>> {
        let option_times = sparse.option_times().to_vec();
        let swap_lengths = sparse.swap_lengths().to_vec();
        let mut smiles = Vec::with_capacity(option_times.len());
        for &option_time in &option_times {
            let mut row = Vec::with_capacity(swap_lengths.len());
            for &swap_length in &swap_lengths {
                row.push(self.smile_section_from_cube(option_time, swap_length, sparse)?);
            }
            smiles.push(row);
        }
        Ok(smiles)
    }

    /// The private `smileSection(optionTime, swapLength, paramCube)` helper
    /// (hpp:861-874): reads `[alpha, beta, nu, rho, forward, ...]` at the node from
    /// a parameter cube and builds the (shift-0) [`SabrSmileSection`]. The forward
    /// is stored at index [`N_SABR_PARAMS`], after the model parameters. Unlike
    /// C++ it does not call `calculate()`: it runs only from inside
    /// [`perform_calculations`](Self::perform_calculations), where the lazy guard
    /// is already held; the public smile hook (#604) will call `calculate()` before
    /// delegating here.
    fn smile_section_from_cube(
        &self,
        option_time: Time,
        swap_length: Time,
        param_cube: &Cube,
    ) -> QlResult<SabrSmileSection> {
        let params = param_cube.value(option_time, swap_length)?;
        let forward = params[N_SABR_PARAMS];
        let shift =
            self.cube
                .atm_vol()
                .current_link()?
                .shift_time(option_time, swap_length, false)?;
        SabrSmileSection::with_exercise_time(
            option_time,
            forward,
            params[0],
            params[1],
            params[2],
            params[3],
            shift,
            self.volatility_type,
        )
    }

    /// The served SABR smile at `(option_time, swap_length)` (`smileSectionImpl`,
    /// hpp:878-884): [`calculate`](Self::calculate)s, then reads the DENSE
    /// parameter cube when `is_atm_calibrated`, else the SPARSE cube. Unlike the
    /// private [`smile_section_from_cube`](Self::smile_section_from_cube) it runs
    /// the lazy calibration first, as C++'s `smileSection` does before indexing
    /// the parameter cube.
    fn served_smile_section(
        &self,
        option_time: Time,
        swap_length: Time,
    ) -> QlResult<SabrSmileSection> {
        self.calculate()?;
        if self.is_atm_calibrated {
            let dense = self.dense_parameters.borrow();
            self.smile_section_from_cube(option_time, swap_length, &dense)
        } else {
            let sparse = self.sparse_parameters.borrow();
            self.smile_section_from_cube(option_time, swap_length, &sparse)
        }
    }

    /// The calibrated SABR smile section for an option tenor and swap tenor
    /// (C++'s inherited `smileSection(optionTenor, swapTenor)`,
    /// swaptionvolstructure.hpp:277/457): resolves the option date off the
    /// reference date and the swap length off the tenor, then serves the smile.
    ///
    /// Returns the concrete [`SabrSmileSection`] so its `alpha`/`beta`/`nu`/`rho`
    /// and `atm_level` accessors are reachable; Rust cannot downcast the
    /// `dyn SmileSection` the
    /// [`smile_section_impl`](SwaptionCubeSmileSection::smile_section_impl) seam
    /// returns, so this is the concrete-typed public entry the C++
    /// `dynamic_pointer_cast<SabrSmileSection>` stands in for.
    ///
    /// # Errors
    ///
    /// Propagates the tenor-to-date/length conversions and the calibration.
    pub fn smile_section(
        &self,
        option_tenor: Period,
        swap_tenor: Period,
    ) -> QlResult<SabrSmileSection> {
        let option_date = self.option_date_from_tenor(option_tenor)?;
        let option_time = self.time_from_reference(option_date)?;
        let swap_length = self.swap_length_tenor(swap_tenor)?;
        self.served_smile_section(option_time, swap_length)
    }

    /// Interpolates the per-strike vol spreads onto an ATM node
    /// (`spreadVolInterpolation`, hpp:737-857). It brackets the ATM node between
    /// the four surrounding sparse nodes (a `lower_bound` then step back on each
    /// axis, clamped at 0), then for each strike spread: rescales the strike to
    /// each corner's forward through the shared moneyness, reads the corner smile
    /// vol minus that corner's ATM vol, and bilinearly blends the four corner
    /// spreads in `(optionTime, swapLength)` via a local one-layer [`Cube`]. This
    /// carries the documented small ATM-fit-error the C++ comment (hpp:818-832)
    /// describes.
    #[allow(clippy::needless_range_loop)]
    fn spread_vol_interpolation(
        &self,
        atm_option_date: Date,
        atm_swap_tenor: Period,
        sparse: &Cube,
        sparse_smiles: &[Vec<SabrSmileSection>],
    ) -> QlResult<Vec<Real>> {
        let atm_option_time = self.time_from_reference(atm_option_date)?;
        let atm_time_length = self.swap_length_tenor(atm_swap_tenor)?;

        let option_times = sparse.option_times();
        let swap_lengths = sparse.swap_lengths();
        let option_dates = sparse.option_dates();
        let swap_tenors = sparse.swap_tenors();

        let opt_prev = lower_bound(option_times, atm_option_time).saturating_sub(1);
        let swp_prev = lower_bound(swap_lengths, atm_time_length).saturating_sub(1);

        require!(
            opt_prev + 1 < sparse_smiles.len(),
            "optionTimesPreviousIndex+1 >= sparseSmiles length"
        );
        require!(
            opt_prev + 1 < option_times.len() && swp_prev + 1 < swap_lengths.len(),
            "sparse bracket index out of range"
        );
        require!(
            swp_prev + 1 < sparse_smiles[0].len(),
            "swapLengthsPreviousIndex+1 >= sparseSmiles[0] length"
        );

        let smiles = [
            [
                &sparse_smiles[opt_prev][swp_prev],
                &sparse_smiles[opt_prev][swp_prev + 1],
            ],
            [
                &sparse_smiles[opt_prev + 1][swp_prev],
                &sparse_smiles[opt_prev + 1][swp_prev + 1],
            ],
        ];
        let options_nodes = [option_times[opt_prev], option_times[opt_prev + 1]];
        let options_date_nodes = [option_dates[opt_prev], option_dates[opt_prev + 1]];
        let swap_lengths_nodes = [swap_lengths[swp_prev], swap_lengths[swp_prev + 1]];
        let swap_tenor_nodes = [swap_tenors[swp_prev], swap_tenors[swp_prev + 1]];

        let atm_forward = self.cube.atm_strike(atm_option_date, atm_swap_tenor)?;
        let atm = self.cube.atm_vol().current_link()?;
        let shift = atm.shift_time(atm_option_time, atm_time_length, false)?;

        let mut atm_forwards = [[0.0; 2]; 2];
        let mut atm_shifts = [[0.0; 2]; 2];
        let mut atm_vols = [[0.0; 2]; 2];
        for i in 0..2 {
            for j in 0..2 {
                atm_forwards[i][j] = self
                    .cube
                    .atm_strike(options_date_nodes[i], swap_tenor_nodes[j])?;
                atm_shifts[i][j] =
                    atm.shift_time(options_nodes[i], swap_lengths_nodes[j], false)?;
                atm_vols[i][j] = atm.volatility(
                    options_date_nodes[i],
                    swap_lengths_nodes[j],
                    atm_forwards[i][j],
                    false,
                )?;
            }
        }

        let strike_spreads = self.cube.strike_spreads();
        let n_strikes = strike_spreads.len();
        let mut result = Vec::with_capacity(n_strikes);
        for k in 0..n_strikes {
            let strike = (atm_forward + strike_spreads[k]).max(self.cutoff_strike - shift);
            let moneyness = (atm_forward + shift) / (strike + shift);
            let mut spread_vols = Matrix::with_size(2, 2);
            for i in 0..2 {
                for j in 0..2 {
                    let node_strike =
                        (atm_forwards[i][j] + atm_shifts[i][j]) / moneyness - atm_shifts[i][j];
                    spread_vols[(i, j)] = smiles[i][j].volatility(node_strike)? - atm_vols[i][j];
                }
            }
            let mut local = Cube::new(
                options_date_nodes.to_vec(),
                swap_tenor_nodes.to_vec(),
                options_nodes.to_vec(),
                swap_lengths_nodes.to_vec(),
                1,
                true,
                self.backward_flat,
            )?;
            local.set_layer(0, spread_vols)?;
            local.update_interpolators()?;
            result.push(local.value(atm_option_time, atm_time_length)?[0]);
        }
        Ok(result)
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

    /// The calibrated DENSE SABR parameters at `(option_time, swap_length)`:
    /// `[alpha, beta, nu, rho, forward, rms_error, max_error, end_criteria]` over
    /// the ATM-widened grid (`denseParameters_`). Only populated when
    /// `is_atm_calibrated`; on a sparse-only cube it stays the zero placeholder.
    ///
    /// # Errors
    ///
    /// Propagates the calibration (including the #586 shift deferral).
    pub fn dense_parameter_values(
        &self,
        option_time: Time,
        swap_length: Time,
    ) -> QlResult<Vec<Real>> {
        self.calculate()?;
        self.dense_parameters
            .borrow()
            .value(option_time, swap_length)
    }
}

/// The sorted-unique union of two finite time axes (C++'s concat + `std::sort` +
/// `std::unique`). Panics only on a non-finite axis value, which the cube grid
/// never holds.
fn union_sorted_times(a: &[Time], b: &[Time]) -> Vec<Time> {
    let mut v: Vec<Time> = a.iter().chain(b.iter()).copied().collect();
    v.sort_by(|x, y| x.partial_cmp(y).expect("cube axis times are finite"));
    v.dedup();
    v
}

/// The sorted-unique union of two option-date axes.
fn union_sorted_dates(a: &[Date], b: &[Date]) -> Vec<Date> {
    let mut v: Vec<Date> = a.iter().chain(b.iter()).copied().collect();
    v.sort();
    v.dedup();
    v
}

/// The sorted-unique union of two swap-tenor axes. Panics only on an
/// incomparable tenor pair (mixed sub-month units), which the year/month cube
/// axes never hold.
fn union_sorted_tenors(a: &[Period], b: &[Period]) -> Vec<Period> {
    let mut v: Vec<Period> = a.iter().chain(b.iter()).copied().collect();
    v.sort_by(|x, y| x.partial_cmp(y).expect("cube swap tenors are comparable"));
    v.dedup();
    v
}

/// Whether the strictly-increasing finite axis `sorted` already contains `v`
/// (C++'s `std::binary_search`).
fn sorted_contains(sorted: &[Time], v: Time) -> bool {
    sorted
        .binary_search_by(|probe| {
            probe
                .partial_cmp(&v)
                .expect("cube axis times are finite and comparable")
        })
        .is_ok()
}

/// The index of the first element of `sorted` not less than `v` (C++'s
/// `std::lower_bound`).
fn lower_bound(sorted: &[Time], v: Time) -> usize {
    sorted.partition_point(|&probe| probe < v)
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
        option_time: Time,
        swap_length: Time,
    ) -> QlResult<Shared<dyn SmileSection>> {
        let section = self.served_smile_section(option_time, swap_length)?;
        Ok(shared(section) as Shared<dyn SmileSection>)
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
    use crate::indexes::{Euribor, IborIndex, Index, SwapIndex};
    use crate::interestrate::Compounding;
    use crate::patterns::observable::AsObservable;
    use crate::quotes::SimpleQuote;
    use crate::shared::{shared, shared_mut};
    use crate::termstructures::volatility::sabr::sabr_volatility;
    use crate::termstructures::volatility::swaption::{
        SwaptionCubeSmileSection, SwaptionVolatilityMatrix,
    };
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
    fn smile_section_serves_the_calibrated_sparse_smile() {
        let f = fixture(
            uniform_guess([0.15, TRUE_PARAMS[1], 0.20, 0.0]),
            [false, true, false, false],
            None,
            false,
        );
        seed_sabr_smile(&f, TRUE_PARAMS);
        let (od, st, ot, sl) = node_info(&f.cube)[0];
        let forward = f.cube.cube().atm_strike(od, st).unwrap();

        let section = f
            .cube
            .smile_section_impl(ot, sl)
            .expect("the sparse SABR smile now serves");
        for spread in [-0.01, 0.0, 0.01] {
            let strike = forward + spread;
            let expected = sabr_volatility(
                strike,
                forward,
                ot,
                TRUE_PARAMS[0],
                TRUE_PARAMS[1],
                TRUE_PARAMS[2],
                TRUE_PARAMS[3],
                VolatilityType::ShiftedLognormal,
            )
            .unwrap();
            let served = section.volatility(strike).unwrap();
            assert!(
                (served - expected).abs() < 1e-6,
                "served smile vol {served} vs true SABR {expected} at strike {strike}"
            );
            let via_impl = f.cube.volatility_impl(ot, sl, strike).unwrap();
            assert!(
                (via_impl - served).abs() < 1e-12,
                "volatility_impl must route through the served smile: {via_impl} vs {served}"
            );
        }
    }

    #[test]
    fn atm_calibrated_with_non_grid_atm_surface_errors() {
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
            .expect_err("dense arm over a non-grid ATM surface must error at discrete_grid");
        let msg = err.to_string();
        assert!(
            msg.contains("grid-backed") || msg.contains("discrete"),
            "err was: {err}"
        );
    }

    // --- Dense ATM-calibration oracle (#603) ---
    //
    // The ATM surface is a real SwaptionVolatilityMatrix on a grid STRICTLY denser
    // than the cube's (option {1Y,2Y,3Y} x swap {2Y,5Y,10Y} vs cube {1Y,3Y} x
    // {2Y,10Y}), so fillVolatilityCube genuinely inserts the 2Y-option and 5Y-swap
    // nodes. The matrix ATM vols are SABR-consistent (each is the DENSE_PARAMS SABR
    // ATM vol at that node's forward), and the cube smiles are seeded so the market
    // smile IS an exact SABR smile at every cube node. Serving the dense params at
    // the ATM strike then recovers the matrix's ATM vol.

    const DENSE_PARAMS: [Real; N_SABR_PARAMS] = [0.20, 0.60, 0.30, -0.10];

    fn matrix_option_tenors() -> Vec<Period> {
        vec![
            Period::new(1, TimeUnit::Years),
            Period::new(2, TimeUnit::Years),
            Period::new(3, TimeUnit::Years),
        ]
    }

    fn matrix_swap_tenors() -> Vec<Period> {
        vec![
            Period::new(2, TimeUnit::Years),
            Period::new(5, TimeUnit::Years),
            Period::new(10, TimeUnit::Years),
        ]
    }

    fn dense_cube_option_tenors() -> Vec<Period> {
        vec![
            Period::new(1, TimeUnit::Years),
            Period::new(3, TimeUnit::Years),
        ]
    }

    fn dense_cube_swap_tenors() -> Vec<Period> {
        vec![
            Period::new(2, TimeUnit::Years),
            Period::new(10, TimeUnit::Years),
        ]
    }

    /// A long-base swap index of arbitrary tenor, matching the conventions
    /// [`SwaptionVolatilityCube::atm_strike`] reconstructs on its long branch, so
    /// its `.fixing` reproduces the node forward the cube uses.
    fn long_swap_index(
        euribor6m: &Shared<IborIndex>,
        settings: &Shared<Settings<Date>>,
        tenor: Period,
    ) -> SwapIndex {
        SwapIndex::new(
            "LongSwap".into(),
            tenor,
            2,
            Currency::eur(),
            Target::new(),
            Period::new(1, TimeUnit::Years),
            BDC,
            Thirty360::with_convention(Convention::BondBasis),
            Shared::clone(euribor6m),
            Shared::clone(settings),
        )
    }

    /// The SABR-consistent ATM matrix: `M[i][j] = sabr ATM vol` at the forward of
    /// (option tenor i, swap tenor j), fixed reference at [`today`].
    fn sabr_consistent_matrix(
        euribor6m: &Shared<IborIndex>,
        settings: &Shared<Settings<Date>>,
    ) -> SwaptionVolatilityMatrix {
        let option_tenors = matrix_option_tenors();
        let swap_tenors = matrix_swap_tenors();
        let day_counter = Actual365Fixed::new();
        let mut vols = Matrix::with_size(option_tenors.len(), swap_tenors.len());
        for (i, &option_tenor) in option_tenors.iter().enumerate() {
            let option_date = Target::new().advance_by_period(today(), option_tenor, BDC, false);
            let option_time = day_counter.year_fraction(today(), option_date);
            for (j, &swap_tenor) in swap_tenors.iter().enumerate() {
                let forward = long_swap_index(euribor6m, settings, swap_tenor)
                    .fixing(option_date, false)
                    .unwrap();
                vols[(i, j)] = sabr_volatility(
                    forward,
                    forward,
                    option_time,
                    DENSE_PARAMS[0],
                    DENSE_PARAMS[1],
                    DENSE_PARAMS[2],
                    DENSE_PARAMS[3],
                    VolatilityType::ShiftedLognormal,
                )
                .unwrap();
            }
        }
        SwaptionVolatilityMatrix::new(
            today(),
            Target::new(),
            BDC,
            option_tenors,
            swap_tenors,
            &vols,
            Actual365Fixed::new(),
            VolatilityType::ShiftedLognormal,
            &Matrix::default(),
        )
        .unwrap()
    }

    #[test]
    fn dense_arm_recovers_matrix_atm_vol_over_the_widened_grid() {
        let settings = settings_today();
        let euribor6m = shared(Euribor::six_months(
            flat_curve(0.05),
            Shared::clone(&settings),
        ));
        let long = long_index(&euribor6m, &settings);
        let short = short_index(&euribor6m, &settings);
        let matrix = sabr_consistent_matrix(&euribor6m, &settings);
        let atm = Handle::new(shared(matrix) as Shared<dyn SwaptionVolatilityStructure>);

        let n_strikes = strike_spreads().len();
        let n_nodes = dense_cube_option_tenors().len() * dense_cube_swap_tenors().len();
        let spread_quotes: Vec<Vec<Shared<SimpleQuote>>> = (0..n_nodes)
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
        let parameters_guess: Vec<Vec<Handle<dyn Quote>>> = (0..n_nodes)
            .map(|_| {
                [0.15, DENSE_PARAMS[1], 0.20, 0.0]
                    .iter()
                    .map(|&v| Handle::new(shared(SimpleQuote::new(v)) as Shared<dyn Quote>))
                    .collect()
            })
            .collect();

        let cube = SabrSwaptionVolatilityCube::new(
            atm.clone(),
            dense_cube_option_tenors(),
            dense_cube_swap_tenors(),
            strike_spreads(),
            vol_spreads,
            long,
            short,
            false,
            parameters_guess,
            [false, true, false, false],
            true,
            None,
            Some(1.0),
            None,
            None,
            false,
            50,
            false,
            0.0001,
            Shared::clone(&settings),
        )
        .unwrap();

        let atm_link = atm.current_link().unwrap();
        for (n, &(od, st, ot, sl)) in node_info(&cube).iter().enumerate() {
            let forward = cube.cube().atm_strike(od, st).unwrap();
            let atm_node_vol = atm_link.volatility(od, sl, forward, false).unwrap();
            for (i, &spread) in strike_spreads().iter().enumerate() {
                let vol = sabr_volatility(
                    forward + spread,
                    forward,
                    ot,
                    DENSE_PARAMS[0],
                    DENSE_PARAMS[1],
                    DENSE_PARAMS[2],
                    DENSE_PARAMS[3],
                    VolatilityType::ShiftedLognormal,
                )
                .unwrap();
                spread_quotes[n][i].set_value(vol - atm_node_vol);
            }
        }

        for &(_od, _st, ot, sl) in node_info(&cube).iter() {
            let p = cube.sparse_parameter_values(ot, sl).unwrap();
            for (idx, &want) in DENSE_PARAMS.iter().enumerate() {
                assert!(
                    (p[idx] - want).abs() < 1e-6,
                    "sparse node param {idx}: got {}, want {want}",
                    p[idx]
                );
            }
        }

        let day_counter = Actual365Fixed::new();
        let mut worst = 0.0_f64;
        for &option_tenor in matrix_option_tenors().iter() {
            let od = Target::new().advance_by_period(today(), option_tenor, BDC, false);
            let ot = day_counter.year_fraction(today(), od);
            for &swap_tenor in matrix_swap_tenors().iter() {
                let sl = swap_tenor.length() as Time;
                let dense = cube.dense_parameter_values(ot, sl).unwrap();
                let forward = dense[N_SABR_PARAMS];
                assert!(
                    forward > 0.0,
                    "dense forward must be positive at ({ot},{sl})"
                );
                let served = sabr_volatility(
                    forward,
                    forward,
                    ot,
                    dense[0],
                    dense[1],
                    dense[2],
                    dense[3],
                    VolatilityType::ShiftedLognormal,
                )
                .unwrap();
                let matrix_vol = atm_link.volatility(od, sl, forward, false).unwrap();
                worst = worst.max((served - matrix_vol).abs());
            }
        }
        assert!(
            worst < 1e-6,
            "dense ATM recovery worst error {worst} over the widened grid"
        );
    }

    // --- CommonVars discriminating SABR oracle (swaptionvolatilitycube.cpp) ---
    //
    // The full end-to-end fixture from the C++ test suite's CommonVars +
    // swaptionvolstructuresutilities.hpp: a real 6x4 SwaptionVolatilityMatrix ATM
    // surface, a 3x3 cube with five strike spreads and nine market vol-spread
    // rows, two EuriborSwapIsdaFixA-style base swap indexes (2Y long, 1Y short),
    // and the C++ constructor defaults. testSabrVols runs the dense ATM-calibrated
    // cube and checks it reproduces the ATM matrix vols (3e-4) and the input smile
    // spreads (12e-4).

    /// The B1 6x4 ATM matrix option tenors {1M,6M,1Y,5Y,10Y,30Y}.
    fn atm_option_tenors() -> Vec<Period> {
        vec![
            Period::new(1, TimeUnit::Months),
            Period::new(6, TimeUnit::Months),
            Period::new(1, TimeUnit::Years),
            Period::new(5, TimeUnit::Years),
            Period::new(10, TimeUnit::Years),
            Period::new(30, TimeUnit::Years),
        ]
    }

    /// The B1 4-swap axis {1Y,5Y,10Y,30Y}.
    fn atm_swap_tenors() -> Vec<Period> {
        vec![
            Period::new(1, TimeUnit::Years),
            Period::new(5, TimeUnit::Years),
            Period::new(10, TimeUnit::Years),
            Period::new(30, TimeUnit::Years),
        ]
    }

    /// The B1 6x4 ATM lognormal vols (swaptionvolstructuresutilities.hpp,
    /// AtmVolatility::setMarketData); the same numbers the swaptionvolmatrix tests
    /// carry.
    fn atm_vols() -> [[Volatility; 4]; 6] {
        [
            [0.1300, 0.1560, 0.1390, 0.1220],
            [0.1440, 0.1580, 0.1460, 0.1260],
            [0.1600, 0.1590, 0.1470, 0.1290],
            [0.1640, 0.1470, 0.1370, 0.1220],
            [0.1400, 0.1300, 0.1250, 0.1100],
            [0.1130, 0.1090, 0.1070, 0.0930],
        ]
    }

    /// The moving 6x4 ATM surface (C++'s floating-reference
    /// `SwaptionVolatilityMatrix`), so the cube grid and the ATM grid share the
    /// moving reference date.
    fn common_atm_matrix(settings: &Shared<Settings<Date>>) -> Shared<SwaptionVolatilityMatrix> {
        let vols = atm_vols();
        let vols_handle: Vec<Vec<Handle<dyn Quote>>> = (0..6)
            .map(|i| {
                (0..4)
                    .map(|j| Handle::new(shared(SimpleQuote::new(vols[i][j])) as Shared<dyn Quote>))
                    .collect()
            })
            .collect();
        shared(
            SwaptionVolatilityMatrix::moving(
                Target::new(),
                BDC,
                atm_option_tenors(),
                atm_swap_tenors(),
                vols_handle,
                Actual365Fixed::new(),
                VolatilityType::ShiftedLognormal,
                Vec::new(),
                Shared::clone(settings),
            )
            .unwrap(),
        )
    }

    /// The cube's option axis {1Y,10Y,30Y}.
    fn common_cube_option_tenors() -> Vec<Period> {
        vec![
            Period::new(1, TimeUnit::Years),
            Period::new(10, TimeUnit::Years),
            Period::new(30, TimeUnit::Years),
        ]
    }

    /// The cube's swap axis {2Y,10Y,30Y}.
    fn common_cube_swap_tenors() -> Vec<Period> {
        vec![
            Period::new(2, TimeUnit::Years),
            Period::new(10, TimeUnit::Years),
            Period::new(30, TimeUnit::Years),
        ]
    }

    /// The cube's five strike spreads {-2%,-0.5%,0,+0.5%,+2%}.
    fn common_strike_spreads() -> Vec<Real> {
        vec![-0.020, -0.005, 0.000, 0.005, 0.020]
    }

    /// The nine 5-strike market vol-spread rows (row `i*3+j` = option-`i`,
    /// swap-`j`), transcribed from VolatilityCube::setMarketData.
    fn common_vol_spreads() -> [[Volatility; 5]; 9] {
        [
            [0.0599, 0.0049, 0.0000, -0.0001, 0.0127],
            [0.0729, 0.0086, 0.0000, -0.0024, 0.0098],
            [0.0738, 0.0102, 0.0000, -0.0039, 0.0065],
            [0.0465, 0.0063, 0.0000, -0.0032, -0.0010],
            [0.0558, 0.0084, 0.0000, -0.0050, -0.0057],
            [0.0576, 0.0083, 0.0000, -0.0043, -0.0014],
            [0.0437, 0.0059, 0.0000, -0.0030, -0.0006],
            [0.0533, 0.0078, 0.0000, -0.0045, -0.0046],
            [0.0545, 0.0079, 0.0000, -0.0042, -0.0020],
        ]
    }

    /// A base swap index with EuriborSwapIsdaFixA-style conventions (hand-built
    /// per the #594/#595 precedent, not the named family): annual Thirty360 fixed
    /// leg forecasting off `euribor6m`. `atm_strike` reconstructs from these
    /// conventions at the requested swap tenor.
    fn isda_swap_index(
        euribor6m: &Shared<IborIndex>,
        settings: &Shared<Settings<Date>>,
        tenor: Period,
    ) -> Shared<SwapIndex> {
        shared(SwapIndex::new(
            "EuriborSwapIsdaFixA".into(),
            tenor,
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

    /// Builds the CommonVars SABR cube (2Y long base, 1Y short base, guess
    /// [0.2,0.5,0.4,0.0], all parameters free, C++ constructor defaults).
    fn build_common_sabr_cube(
        settings: &Shared<Settings<Date>>,
        is_atm_calibrated: bool,
    ) -> SabrSwaptionVolatilityCube {
        let euribor6m = shared(Euribor::six_months(
            flat_curve(0.05),
            Shared::clone(settings),
        ));
        let long = isda_swap_index(&euribor6m, settings, Period::new(2, TimeUnit::Years));
        let short = isda_swap_index(&euribor6m, settings, Period::new(1, TimeUnit::Years));
        let atm =
            Handle::new(common_atm_matrix(settings) as Shared<dyn SwaptionVolatilityStructure>);

        let spreads = common_vol_spreads();
        let vol_spreads: Vec<Vec<Handle<dyn Quote>>> = (0..9)
            .map(|n| {
                (0..5)
                    .map(|k| {
                        Handle::new(shared(SimpleQuote::new(spreads[n][k])) as Shared<dyn Quote>)
                    })
                    .collect()
            })
            .collect();
        let parameters_guess: Vec<Vec<Handle<dyn Quote>>> = (0..9)
            .map(|_| {
                [0.2, 0.5, 0.4, 0.0]
                    .iter()
                    .map(|&v| Handle::new(shared(SimpleQuote::new(v)) as Shared<dyn Quote>))
                    .collect()
            })
            .collect();

        SabrSwaptionVolatilityCube::new(
            atm,
            common_cube_option_tenors(),
            common_cube_swap_tenors(),
            common_strike_spreads(),
            vol_spreads,
            long,
            short,
            false,
            parameters_guess,
            [false; N_SABR_PARAMS],
            is_atm_calibrated,
            None,
            None,
            None,
            None,
            false,
            50,
            false,
            0.0001,
            Shared::clone(settings),
        )
        .unwrap()
    }

    #[test]
    fn testsabrvols_recovers_atm_vols_and_smile_spreads() {
        let settings = settings_today();
        let cube = build_common_sabr_cube(&settings, true);
        let atm = cube.cube().atm_vol();
        let atm_link = atm.current_link().unwrap();

        let mut worst_atm = 0.0_f64;
        for &o in atm_option_tenors().iter() {
            for &s in atm_swap_tenors().iter() {
                let strike = cube.cube().atm_strike_from_tenor(o, s).unwrap();
                let exp = atm_link.volatility_tenors(o, s, strike, true).unwrap();
                let act = cube.volatility_tenors(o, s, strike, true).unwrap();
                worst_atm = worst_atm.max((exp - act).abs());
            }
        }
        assert!(
            worst_atm < 3.0e-4,
            "recovery of atm vols worst error {worst_atm} (tolerance 3e-4)"
        );

        let spreads_in = common_vol_spreads();
        let strike_spreads = common_strike_spreads();
        let mut worst_spread = 0.0_f64;
        for (i, &o) in common_cube_option_tenors().iter().enumerate() {
            for (j, &s) in common_cube_swap_tenors().iter().enumerate() {
                let atm_strike = cube.cube().atm_strike_from_tenor(o, s).unwrap();
                let atm_vol = atm_link.volatility_tenors(o, s, atm_strike, true).unwrap();
                for (k, &spread) in strike_spreads.iter().enumerate() {
                    let vol = cube
                        .volatility_tenors(o, s, atm_strike + spread, true)
                        .unwrap();
                    let got_spread = vol - atm_vol;
                    let exp_spread = spreads_in[i * 3 + j][k];
                    worst_spread = worst_spread.max((exp_spread - got_spread).abs());
                }
            }
        }
        assert!(
            worst_spread < 12.0e-4,
            "recovery of smile vol spreads worst error {worst_spread} (tolerance 12e-4)"
        );
    }
}
