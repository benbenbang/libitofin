//! Year-on-year inflation cap/floor engines.
//!
//! Port of `ql/pricingengines/inflation/inflationcapfloorengines.{hpp,cpp}`:
//! [`YoYInflationCapFloorEngine`] prices each optionlet of a
//! [`YoYInflationCapFloor`](crate::instruments::YoYInflationCapFloor) against a
//! [`YoYOptionletVolatilitySurface`], discounting on a nominal
//! [`YieldTermStructure`].
//!
//! The engine prices the optionlets *standalone*: it reads each forward off the
//! index's own year-on-year curve rather than through a coupon pricer. C++ says
//! why (`.cpp:74-80`) - the fixing is natural, so there is no convexity
//! adjustment to make, and a convexity correction would need nominal vols and
//! hence a different engine altogether.
//!
//! That is also what makes cap - floor == swap exact rather than approximate.
//! [`yoy_rate_date`](crate::termstructures::inflation::inflationtermstructure::YoYInflationTermStructure::yoy_rate_date)
//! quantizes the date it is given to the start of its inflation period, and the
//! swap coupon's own forecast quantizes to the same period, so both read the
//! curve at one identical point and the intra-period drift cancels.
//!
//! ## Divergences from QuantLib
//!
//! - C++ spells three engine classes differing only in `optionletImpl`
//!   (`.cpp:142-184`); nothing dispatches on the concrete type, so the port
//!   folds them into one engine carrying a
//!   [`YoYOptionletDistribution`], with a constructor apiece - the same shape
//!   `#838` gave the coupon pricers. The distribution switch itself lives once,
//!   in [`yoy_optionlet_price`], which both this engine and that pricer call.
//! - The past-fixing guard is `fixing_date > base_date` where C++ writes
//!   `sqrt(volatility_->timeFromBase(fixingDate)) > 0.0` (`.cpp:86-89`). The
//!   two coincide, and the port's `dyn YoYOptionletVolatilitySurface` carries no
//!   `timeFromBase` - it serves the stripping hierarchy, not the pricing.

use std::any::Any;

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::indexes::Index;
use crate::indexes::inflationindex::YoYInflationIndex;
use crate::instrument::InstrumentResults;
use crate::instruments::{CapFloorType, YoYInflationCapFloorArguments};
use crate::option::OptionType;
use crate::patterns::observable::{AsObservable, Observable};
use crate::pricingengine::{Arguments, GenericEngine, PricingEngine, Results};
use crate::pricingengines::blackformula::{bachelier_black_formula, black_formula};
use crate::shared::{Shared, shared};
use crate::termstructures::volatility::YoYOptionletVolatilitySurface;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;
use crate::types::{Rate, Real};
use crate::{cashflows::YoYOptionletDistribution, fail};

/// The value of one year-on-year optionlet under `distribution`
/// (the three C++ `optionletImpl` overrides, `.cpp:142-184`).
///
/// `discount` carries whatever the caller wants folded in: the engine passes
/// nominal times gearing times discount factor times accrual time, while the
/// coupon pricer, which wants a *rate* rather than a price, passes `1.0`.
///
/// # Errors
///
/// As the underlying formula: a negative std-dev or discount, or a strike the
/// lognormal cases cannot take.
pub fn yoy_optionlet_price(
    distribution: YoYOptionletDistribution,
    option_type: OptionType,
    strike: Rate,
    forward: Rate,
    std_dev: Real,
    discount: Real,
) -> QlResult<Real> {
    match distribution {
        YoYOptionletDistribution::Black => {
            black_formula(option_type, strike, forward, std_dev, discount, 0.0)
        }
        // C++ writes `blackFormula(type, strike + 1, forward + 1, stdDev)`
        // (`.cpp:166-167`); a displacement of 1.0 adds the same 1 to both
        // (`blackformula.rs:115-116`).
        YoYOptionletDistribution::UnitDisplaced => {
            black_formula(option_type, strike, forward, std_dev, discount, 1.0)
        }
        YoYOptionletDistribution::Bachelier => {
            bachelier_black_formula(option_type, strike, forward, std_dev, discount)
        }
    }
}

/// Engine pricing a year-on-year cap, floor or collar optionlet by optionlet.
pub struct YoYInflationCapFloorEngine {
    base: GenericEngine<YoYInflationCapFloorArguments, InstrumentResults>,
    distribution: YoYOptionletDistribution,
    index: Shared<YoYInflationIndex>,
    volatility: Handle<dyn YoYOptionletVolatilitySurface>,
    nominal_term_structure: Handle<dyn YieldTermStructure>,
}

impl YoYInflationCapFloorEngine {
    /// Builds the engine over `index`, `volatility` and a nominal discount
    /// curve, registering for changes in all three (`.cpp:29-38`).
    fn new(
        distribution: YoYOptionletDistribution,
        index: Shared<YoYInflationIndex>,
        volatility: Handle<dyn YoYOptionletVolatilitySurface>,
        nominal_term_structure: Handle<dyn YieldTermStructure>,
    ) -> YoYInflationCapFloorEngine {
        let base = GenericEngine::new(
            YoYInflationCapFloorArguments::default(),
            InstrumentResults::default(),
        );
        base.register_with(index.observable());
        volatility.register_observer(&base.observer());
        nominal_term_structure.register_observer(&base.observer());
        YoYInflationCapFloorEngine {
            base,
            distribution,
            index,
            volatility,
            nominal_term_structure,
        }
    }

    /// Optionlets under the lognormal model
    /// (`YoYInflationBlackCapFloorEngine`). See [`new`](Self::new).
    pub fn black(
        index: Shared<YoYInflationIndex>,
        volatility: Handle<dyn YoYOptionletVolatilitySurface>,
        nominal_term_structure: Handle<dyn YieldTermStructure>,
    ) -> YoYInflationCapFloorEngine {
        Self::new(
            YoYOptionletDistribution::Black,
            index,
            volatility,
            nominal_term_structure,
        )
    }

    /// Optionlets under the unit-displaced lognormal model
    /// (`YoYInflationUnitDisplacedBlackCapFloorEngine`). See [`new`](Self::new).
    pub fn unit_displaced(
        index: Shared<YoYInflationIndex>,
        volatility: Handle<dyn YoYOptionletVolatilitySurface>,
        nominal_term_structure: Handle<dyn YieldTermStructure>,
    ) -> YoYInflationCapFloorEngine {
        Self::new(
            YoYOptionletDistribution::UnitDisplaced,
            index,
            volatility,
            nominal_term_structure,
        )
    }

    /// Optionlets under the normal model
    /// (`YoYInflationBachelierCapFloorEngine`). See [`new`](Self::new).
    pub fn bachelier(
        index: Shared<YoYInflationIndex>,
        volatility: Handle<dyn YoYOptionletVolatilitySurface>,
        nominal_term_structure: Handle<dyn YieldTermStructure>,
    ) -> YoYInflationCapFloorEngine {
        Self::new(
            YoYOptionletDistribution::Bachelier,
            index,
            volatility,
            nominal_term_structure,
        )
    }

    /// The distribution optionlets are valued under.
    pub fn distribution(&self) -> YoYOptionletDistribution {
        self.distribution
    }

    /// The index the forwards are read off.
    pub fn index(&self) -> &Shared<YoYInflationIndex> {
        &self.index
    }

    /// The optionlet volatility surface.
    pub fn volatility(&self) -> &Handle<dyn YoYOptionletVolatilitySurface> {
        &self.volatility
    }

    /// The nominal curve the optionlets are discounted on.
    pub fn nominal_term_structure(&self) -> &Handle<dyn YieldTermStructure> {
        &self.nominal_term_structure
    }
}

impl AsObservable for YoYInflationCapFloorEngine {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl PricingEngine for YoYInflationCapFloorEngine {
    fn arguments_mut(&mut self) -> &mut dyn Arguments {
        self.base.arguments_mut()
    }

    fn results(&self) -> &dyn Results {
        self.base.results()
    }

    fn reset(&mut self) {
        self.base.reset();
    }

    /// `calculate` (`.cpp:51-128`).
    fn calculate(&mut self) -> QlResult<()> {
        let nominal = self.nominal_term_structure.current_link()?;
        let settlement = nominal.reference_date()?;
        let surface = self.volatility.current_link()?;
        let base_date = surface.base_date()?;
        let yoy_curve = self.index.yoy_inflation_term_structure().current_link()?;
        let no_lag = Period::new(0, TimeUnit::Days);
        let distribution = self.distribution;

        let arguments = self.base.arguments();
        let cap_floor_type = match arguments.cap_floor_type {
            Some(cap_floor_type) => cap_floor_type,
            None => fail!("cap/floor type not set"),
        };
        let has_cap = matches!(cap_floor_type, CapFloorType::Cap | CapFloorType::Collar);
        let has_floor = matches!(cap_floor_type, CapFloorType::Floor | CapFloorType::Collar);

        let n = arguments.start_dates.len();
        let mut values = vec![0.0; n];
        let mut std_devs = vec![0.0; n];
        let mut forwards = vec![0.0; n];
        let mut value = 0.0;

        for i in 0..n {
            // Expired optionlets are discarded but keep their zero entry, so
            // every additional result spans the whole leg.
            if arguments.pay_dates[i] <= settlement {
                continue;
            }
            let discounted_accrual = arguments.nominals[i]
                * arguments.gearings[i]
                * nominal.discount_date(arguments.pay_dates[i], false)?
                * arguments.accrual_times[i];

            let fixing_date = arguments.fixing_dates[i];
            // The natural fixing: the curve's own year-on-year rate, with no
            // convexity adjustment and no extrapolation.
            let forward = yoy_curve.yoy_rate_date(fixing_date, false)?;
            forwards[i] = forward;
            let determined = fixing_date <= base_date;

            let mut optionlet = 0.0;
            if has_cap {
                let strike = arguments.cap_rates[i].expect("cap rate set for cap/collar");
                if !determined {
                    std_devs[i] = surface.total_variance(fixing_date, strike, no_lag)?.sqrt();
                }
                // A determined optionlet keeps std-dev 0, at which every
                // formula collapses to its intrinsic value times the discount.
                optionlet = yoy_optionlet_price(
                    distribution,
                    OptionType::Call,
                    strike,
                    forward,
                    std_devs[i],
                    discounted_accrual,
                )?;
            }
            if has_floor {
                let strike = arguments.floor_rates[i].expect("floor rate set for floor/collar");
                // Re-read at the floor strike: on a smiling surface the two
                // strikes carry different variances (`.cpp:105-108`).
                if !determined {
                    std_devs[i] = surface.total_variance(fixing_date, strike, no_lag)?.sqrt();
                }
                let floorlet = yoy_optionlet_price(
                    distribution,
                    OptionType::Put,
                    strike,
                    forward,
                    std_devs[i],
                    discounted_accrual,
                )?;
                if cap_floor_type == CapFloorType::Floor {
                    optionlet = floorlet;
                } else {
                    // A collar is long the cap and short the floor.
                    optionlet -= floorlet;
                }
            }

            values[i] = optionlet;
            value += optionlet;
        }

        drop(nominal);
        drop(surface);
        drop(yoy_curve);

        let results = self.base.results_mut();
        results.value = Some(value);
        results.error_estimate = None;
        results.valuation_date = None;
        results.additional_results.insert(
            "optionletsPrice".to_string(),
            shared(values) as Shared<dyn Any>,
        );
        results.additional_results.insert(
            "optionletsAtmForward".to_string(),
            shared(forwards) as Shared<dyn Any>,
        );
        // A collar overwrote each std-dev at its floor strike, so the vector no
        // longer describes one option; C++ withholds it for that case
        // (`.cpp:126-127`).
        if cap_floor_type != CapFloorType::Collar {
            results.additional_results.insert(
                "optionletsStdDev".to_string(),
                shared(std_devs) as Shared<dyn Any>,
            );
        }
        Ok(())
    }
}
