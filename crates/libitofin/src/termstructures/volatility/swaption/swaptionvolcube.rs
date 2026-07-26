//! Swaption volatility cube base.
//!
//! Port of `ql/termstructures/volatility/swaption/swaptionvolcube.{hpp,cpp}`:
//! `class SwaptionVolatilityCube : public SwaptionVolatilityDiscrete`. This is
//! the framework the two concrete cubes extend - the interpolated cube (#595)
//! and the SABR cube (#596). It holds the at-the-money surface as a
//! [`Handle`], the strike spreads relative to the ATM level, a grid of
//! per-node volatility-spread quotes, and the long and short base
//! [`SwapIndex`]es from whose conventions [`atm_strike`](SwaptionVolatilityCube::atm_strike)
//! rebuilds an on-the-fly swap rate.
//!
//! ## Composition, not a trait
//!
//! C++ derives from `SwaptionVolatilityDiscrete`; Rust has no inheritance, so -
//! exactly as [`SwaptionVolatilityMatrix`](super::SwaptionVolatilityMatrix)
//! does - this is a reusable struct embedding [`SwaptionVolatilityDiscrete`] for
//! the shared tenor/date/time grid. A concrete cube embeds this base in turn and
//! implements the [`SwaptionVolatilityStructure`] trait, routing its
//! `volatility_impl` through the [`SwaptionCubeSmileSection`] smile hook.
//!
//! ## The smile seam
//!
//! C++'s `volatilityImpl` calls the pure-virtual `smileSectionImpl` and takes
//! its `volatility(strike)`. The base struct cannot call up into the concrete
//! that embeds it, so the hook is the [`SwaptionCubeSmileSection`] trait: the
//! concrete supplies [`smile_section_impl`](SwaptionCubeSmileSection::smile_section_impl)
//! and the provided [`cube_volatility_impl`](SwaptionCubeSmileSection::cube_volatility_impl)
//! is the faithful port of the routing. The base is therefore complete except
//! for that one hook.
//!
//! ## Divergences from QuantLib
//!
//! - The embedded discrete grid is built through
//!   [`SwaptionVolatilityDiscrete::moving`], which per D5 takes the shared
//!   [`Settings`] handle explicitly (C++ reads the global singleton). This
//!   follows [`SwaptionVolatilityMatrix::moving`]'s signature; the cube
//!   constructor threads the same handle.
//! - The `nStrikes >= requiredNumberOfStrikes()` check is C++'s
//!   `performCalculations` guard (it defers because the required count is
//!   virtual). Here the base count is a fixed constant, so the check runs at
//!   construction and returns `Err` per D4, rather than on first use.
//! - `atmStrike` calls the reconstructed index's `.fixing(optionDate, false)`,
//!   as C++ does (`.fixing(optionD)`, whose default `forecastTodaysFixing` is
//!   false): a strictly-future date forecasts the underlying swap's fair rate, a
//!   past or today date routes to the D11 fixing store.

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::indexes::{Index, InterestRateIndex, SwapIndex};
use crate::patterns::observable::Observable;
use crate::quotes::Quote;
use crate::settings::Settings;
use crate::shared::Shared;
use crate::termstructures::TermStructureBase;
use crate::termstructures::volatility::{SmileSection, VolatilityType};
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::date::Date;
use crate::time::period::Period;
use crate::types::{Rate, Real, Time, Volatility};
use crate::{fail, require};

use super::{SwaptionVolatilityDiscrete, SwaptionVolatilityStructure};

/// The smallest number of strikes the base cube accepts, C++'s
/// `requiredNumberOfStrikes()` base return (swaptionvolcube.hpp:97). A concrete
/// cube that needs more enforces the stronger bound itself.
const REQUIRED_NUMBER_OF_STRIKES: usize = 2;

/// The smile-section hook a swaption vol cube's concrete surfaces provide.
///
/// Port of `SwaptionVolatilityCube::smileSectionImpl` (pure virtual,
/// swaptionvolcube.hpp:98-100). The interpolated cube (#595) and the SABR cube
/// (#596) each build a [`SmileSection`] for an (option time, swap length) node;
/// the base's volatility routing (`volatilityImpl`, hpp:118-123) is the provided
/// [`cube_volatility_impl`](Self::cube_volatility_impl), the port of
/// `smileSectionImpl(optionTime, swapLength)->volatility(strike)`.
pub trait SwaptionCubeSmileSection {
    /// The smile section at `option_time` and `swap_length`.
    fn smile_section_impl(
        &self,
        option_time: Time,
        swap_length: Time,
    ) -> QlResult<Shared<dyn SmileSection>>;

    /// The cube volatility at `strike`: the smile section's volatility there.
    fn cube_volatility_impl(
        &self,
        option_time: Time,
        swap_length: Time,
        strike: Rate,
    ) -> QlResult<Volatility> {
        self.smile_section_impl(option_time, swap_length)?
            .volatility(strike)
    }
}

/// Swaption volatility cube base, embedding the discrete tenor/date/time grid.
pub struct SwaptionVolatilityCube {
    discrete: SwaptionVolatilityDiscrete,
    atm_vol: Handle<dyn SwaptionVolatilityStructure>,
    strike_spreads: Vec<Real>,
    vol_spreads: Vec<Vec<Handle<dyn Quote>>>,
    swap_index_base: Shared<SwapIndex>,
    short_swap_index_base: Shared<SwapIndex>,
    vega_weighted_smile_fit: bool,
}

impl SwaptionVolatilityCube {
    /// Builds the cube framework off the ATM surface, the strike/vol-spread grid
    /// and the two base swap indexes (C++'s single constructor,
    /// swaptionvolcube.cpp:28-79).
    ///
    /// The discrete grid is built moving with zero settlement days from the ATM
    /// surface's calendar, business-day convention and day counter, mirroring
    /// C++'s `SwaptionVolatilityDiscrete(optionTenors, swapTenors, 0,
    /// atmVol->calendar(), atmVol->businessDayConvention(),
    /// atmVol->dayCounter())`; `settings` is the D5 handle that constructor
    /// needs.
    ///
    /// `vol_spreads` is row-major over the `(option tenor, swap tenor)` nodes:
    /// row `i*nSwapTenors + j` is the option-`i`, swap-`j` node, and every row
    /// holds one quote per strike spread.
    ///
    /// # Errors
    ///
    /// Returns `Err` when the ATM handle is empty or lacks a calendar or day
    /// counter, when there are fewer than [`REQUIRED_NUMBER_OF_STRIKES`] strike
    /// spreads or they are not strictly increasing, when the vol-spread grid does
    /// not match `nOptionTenors * nSwapTenors` rows by `nStrikes` columns, or
    /// when the short index tenor exceeds the long index tenor.
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
    ) -> QlResult<SwaptionVolatilityCube> {
        let atm = atm_vol.current_link()?;
        let Some(calendar) = atm.calendar() else {
            fail!("atm vol structure has no calendar");
        };
        let business_day_convention = atm.business_day_convention();
        let Some(day_counter) = atm.day_counter() else {
            fail!("atm vol structure has no day counter");
        };

        let discrete = SwaptionVolatilityDiscrete::moving(
            option_tenors,
            swap_tenors,
            0,
            calendar,
            business_day_convention,
            day_counter,
            settings,
        )?;

        let n_strikes = strike_spreads.len();
        require!(
            n_strikes >= REQUIRED_NUMBER_OF_STRIKES,
            "too few strikes ({n_strikes}): at least {REQUIRED_NUMBER_OF_STRIKES} are required"
        );
        for i in 1..n_strikes {
            let increasing = strike_spreads[i - 1] < strike_spreads[i];
            require!(
                increasing,
                "non increasing strike spreads: {} is {}, {} is {}",
                i,
                strike_spreads[i - 1],
                i + 1,
                strike_spreads[i]
            );
        }

        require!(!vol_spreads.is_empty(), "empty vol spreads matrix");
        let n_options = discrete.option_tenors().len();
        let n_swaps = discrete.swap_tenors().len();
        require!(
            n_options * n_swaps == vol_spreads.len(),
            "mismatch between number of option tenors * swap tenors ({}) and number of rows ({})",
            n_options * n_swaps,
            vol_spreads.len()
        );
        for (i, row) in vol_spreads.iter().enumerate() {
            require!(
                row.len() == n_strikes,
                "mismatch between number of strikes ({n_strikes}) and number of columns ({}) in the {} row",
                row.len(),
                i + 1
            );
        }

        let updater = discrete.base().updater();
        atm_vol.register_observer(&updater);
        atm.enable_extrapolation();

        swap_index_base
            .base()
            .observable()
            .register_observer(&updater);
        short_swap_index_base
            .base()
            .observable()
            .register_observer(&updater);

        let short_not_longer = short_swap_index_base.tenor() <= swap_index_base.tenor();
        require!(
            short_not_longer,
            "short index tenor ({}) is not less or equal than index tenor ({})",
            short_swap_index_base.tenor(),
            swap_index_base.tenor()
        );

        for row in &vol_spreads {
            for handle in row {
                handle.register_observer(&updater);
            }
        }

        Ok(SwaptionVolatilityCube {
            discrete,
            atm_vol,
            strike_spreads,
            vol_spreads,
            swap_index_base,
            short_swap_index_base,
            vega_weighted_smile_fit,
        })
    }

    /// The embedded discrete grid, for a concrete cube to read its option
    /// times/dates and swap lengths and to route its `TermStructure` impl.
    pub fn discrete(&self) -> &SwaptionVolatilityDiscrete {
        &self.discrete
    }

    /// The embedded term-structure holder, for the concrete's `TermStructure`
    /// impl to route through.
    pub fn base(&self) -> &TermStructureBase {
        self.discrete.base()
    }

    /// The observable the cube notifies downstream observers through.
    pub fn observable(&self) -> &Observable {
        self.discrete.observable()
    }

    /// Rebuilds the discrete grid if the reference date has moved. A concrete
    /// cube calls this before a grid-dependent query.
    pub fn calculate(&self) -> QlResult<()> {
        self.discrete.calculate()
    }

    /// The business-day convention used in tenor-to-date conversion.
    pub fn business_day_convention(&self) -> BusinessDayConvention {
        self.discrete.business_day_convention()
    }

    /// The at-the-money surface (`atmVol()`).
    pub fn atm_vol(&self) -> Handle<dyn SwaptionVolatilityStructure> {
        self.atm_vol.clone()
    }

    /// The strike spreads relative to the ATM level (`strikeSpreads()`).
    pub fn strike_spreads(&self) -> &[Real] {
        &self.strike_spreads
    }

    /// The per-node volatility-spread quotes, row-major over the
    /// `(option tenor, swap tenor)` nodes (`volSpreads()`).
    pub fn vol_spreads(&self) -> &[Vec<Handle<dyn Quote>>] {
        &self.vol_spreads
    }

    /// The long base swap index (`swapIndexBase()`).
    pub fn swap_index_base(&self) -> Shared<SwapIndex> {
        Shared::clone(&self.swap_index_base)
    }

    /// The short base swap index (`shortSwapIndexBase()`).
    pub fn short_swap_index_base(&self) -> Shared<SwapIndex> {
        Shared::clone(&self.short_swap_index_base)
    }

    /// Whether the smiles are fitted with vega weighting (`vegaWeightedSmileFit()`).
    pub fn vega_weighted_smile_fit(&self) -> bool {
        self.vega_weighted_smile_fit
    }

    /// The volatility type the cube quotes, taken from the ATM surface
    /// (`volatilityType()`).
    ///
    /// # Errors
    ///
    /// Returns `Err` when the ATM handle is empty.
    pub fn volatility_type(&self) -> QlResult<VolatilityType> {
        Ok(self.atm_vol.current_link()?.volatility_type())
    }

    /// The at-the-money strike for an option date and swap tenor
    /// (`atmStrike`, swaptionvolcube.cpp:89-144).
    ///
    /// Reconstructs a [`SwapIndex`] with `swap_tenor` from the conventions of the
    /// long base index when `swap_tenor` exceeds the short index tenor, else the
    /// short base index, and takes its `.fixing(option_date, false)`: the fair
    /// rate of the underlying swap for a strictly-future date, or the stored
    /// fixing (D11) for a past or today date.
    ///
    /// # Errors
    ///
    /// Propagates the reconstructed index's fixing.
    pub fn atm_strike(&self, option_date: Date, swap_tenor: Period) -> QlResult<Rate> {
        let chosen = if swap_tenor > self.short_swap_index_base.tenor() {
            &self.swap_index_base
        } else {
            &self.short_swap_index_base
        };
        let settings = chosen.base().settings().clone();
        let index = if chosen.exogenous_discount() {
            SwapIndex::with_exogenous_discount(
                chosen.family_name().to_string(),
                swap_tenor,
                chosen.fixing_days(),
                chosen.currency().clone(),
                chosen.fixing_calendar(),
                chosen.fixed_leg_tenor(),
                chosen.fixed_leg_convention(),
                chosen.day_counter().clone(),
                chosen.ibor_index(),
                chosen.discounting_term_structure(),
                settings,
            )
        } else {
            SwapIndex::new(
                chosen.family_name().to_string(),
                swap_tenor,
                chosen.fixing_days(),
                chosen.currency().clone(),
                chosen.fixing_calendar(),
                chosen.fixed_leg_tenor(),
                chosen.fixed_leg_convention(),
                chosen.day_counter().clone(),
                chosen.ibor_index(),
                settings,
            )
        };
        index.fixing(option_date, false)
    }

    /// The at-the-money strike for an option tenor and swap tenor (C++'s inline
    /// `atmStrike(optionTenor, swapTenor)`, swaptionvolcube.hpp:71-75): resolves
    /// the option date off the reference date, then delegates to
    /// [`atm_strike`](Self::atm_strike).
    ///
    /// # Errors
    ///
    /// Returns `Err` when the grid has no calendar or reference date, or
    /// propagates [`atm_strike`](Self::atm_strike).
    pub fn atm_strike_from_tenor(
        &self,
        option_tenor: Period,
        swap_tenor: Period,
    ) -> QlResult<Rate> {
        let Some(calendar) = self.discrete.base().calendar() else {
            fail!("no calendar for swaption vol cube");
        };
        let reference = self.discrete.base().reference_date()?;
        let option_date = calendar.advance_by_period(
            reference,
            option_tenor,
            self.business_day_convention(),
            false,
        );
        self.atm_strike(option_date, swap_tenor)
    }
}

/// These tests pin the cube framework's construction guards and provide the
/// fixtures the rest of the suite builds on: a flat ATM surface, the long
/// (5Y, annual, exogenous-discount) and short (1Y, semiannual, plain) base swap
/// indexes, and the cube builder.
#[cfg(test)]
mod tests {
    use super::*;

    use crate::currency::Currency;
    use crate::indexes::{Euribor, IborIndex};
    use crate::interestrate::Compounding;
    use crate::patterns::observable::AsObservable;
    use crate::quotes::make_quote_handle;
    use crate::shared::shared;
    use crate::termstructures::yields::FlatForward;
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::termstructures::{TermStructure, volatility::VolatilityTermStructure};
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

    /// A flat swaption vol surface standing in for the ATM matrix: the cube reads
    /// its calendar, business-day convention and day counter and enables its
    /// extrapolation. Its own reference date is unused by the cube's grid, which
    /// is moving off `Settings`.
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

    /// The long base index: 5Y, annual fixed leg, discounting off a separate
    /// curve. The exogenous discount and the annual leg are what make the
    /// long-branch oracle discriminating against the short base.
    fn long_index(
        euribor6m: &Shared<IborIndex>,
        discount: &Handle<dyn YieldTermStructure>,
        settings: &Shared<Settings<Date>>,
    ) -> SwapIndex {
        SwapIndex::with_exogenous_discount(
            "LongSwap".into(),
            Period::new(5, TimeUnit::Years),
            2,
            Currency::eur(),
            Target::new(),
            Period::new(1, TimeUnit::Years),
            BDC,
            Thirty360::with_convention(Convention::BondBasis),
            Shared::clone(euribor6m),
            discount.clone(),
            Shared::clone(settings),
        )
    }

    /// The short base index: 1Y, semiannual fixed leg, no exogenous discount.
    fn short_index(euribor6m: &Shared<IborIndex>, settings: &Shared<Settings<Date>>) -> SwapIndex {
        SwapIndex::new(
            "ShortSwap".into(),
            Period::new(1, TimeUnit::Years),
            2,
            Currency::eur(),
            Target::new(),
            Period::new(6, TimeUnit::Months),
            BDC,
            Thirty360::with_convention(Convention::BondBasis),
            Shared::clone(euribor6m),
            Shared::clone(settings),
        )
    }

    struct Parts {
        settings: Shared<Settings<Date>>,
        long: Shared<SwapIndex>,
        short: Shared<SwapIndex>,
    }

    fn parts() -> Parts {
        let settings = settings_today();
        let euribor6m = shared(Euribor::six_months(
            flat_curve(0.05),
            Shared::clone(&settings),
        ));
        let discount = flat_curve(0.03);
        let long = shared(long_index(&euribor6m, &discount, &settings));
        let short = shared(short_index(&euribor6m, &settings));
        Parts {
            settings,
            long,
            short,
        }
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

    fn vol_spreads(n_rows: usize, n_strikes: usize) -> Vec<Vec<Handle<dyn Quote>>> {
        (0..n_rows)
            .map(|_| {
                (0..n_strikes)
                    .map(|_| make_quote_handle(0.001).handle())
                    .collect()
            })
            .collect()
    }

    fn build_cube(
        p: &Parts,
        strike_spreads: Vec<Real>,
        vol_spreads: Vec<Vec<Handle<dyn Quote>>>,
    ) -> QlResult<SwaptionVolatilityCube> {
        SwaptionVolatilityCube::new(
            atm_handle(0.2),
            option_tenors(),
            swap_tenors(),
            strike_spreads,
            vol_spreads,
            Shared::clone(&p.long),
            Shared::clone(&p.short),
            false,
            Shared::clone(&p.settings),
        )
    }

    #[test]
    fn too_few_strikes_is_rejected() {
        let p = parts();
        assert!(build_cube(&p, vec![0.0], vol_spreads(4, 1)).is_err());
    }

    #[test]
    fn non_increasing_strike_spreads_are_rejected() {
        let p = parts();
        assert!(build_cube(&p, vec![0.01, 0.0, -0.01], vol_spreads(4, 3)).is_err());
    }

    #[test]
    fn wrong_shaped_vol_spreads_are_rejected() {
        let p = parts();
        assert!(
            build_cube(&p, vec![-0.01, 0.0, 0.01], vol_spreads(3, 3)).is_err(),
            "row count must equal option tenors * swap tenors"
        );
        assert!(
            build_cube(&p, vec![-0.01, 0.0, 0.01], vol_spreads(4, 2)).is_err(),
            "each row must hold one quote per strike"
        );
    }

    #[test]
    fn short_tenor_longer_than_long_tenor_is_rejected() {
        let p = parts();
        let swapped = SwaptionVolatilityCube::new(
            atm_handle(0.2),
            option_tenors(),
            swap_tenors(),
            vec![-0.01, 0.0, 0.01],
            vol_spreads(4, 3),
            Shared::clone(&p.short),
            Shared::clone(&p.long),
            false,
            Shared::clone(&p.settings),
        );
        assert!(
            swapped.is_err(),
            "short (5Y) longer than long (1Y) must be rejected"
        );
    }
}
