//! Analytic cap/floor engine for the Hull-White model.
//!
//! Port of `ql/pricingengines/capfloor/analyticcapfloorengine.{hpp,cpp}`:
//! [`AnalyticCapFloorEngine`] prices a
//! [`CapFloor`](crate::instruments::CapFloor) under the one-factor
//! [`HullWhite`] model as a portfolio of options on the individual coupon
//! discount bonds. Each caplet is a put on the accrual-period zero-coupon bond,
//! each floorlet a call, priced by the model's `discount_bond_option`
//! (`analyticcapfloorengine.cpp:94-113`). A coupon whose fixing has already set
//! contributes its deterministic intrinsic value instead
//! (`analyticcapfloorengine.cpp:78-93`).
//!
//! ## Model binding (documented deferral)
//!
//! C++'s engine is `GenericModelEngine<AffineModel, ...>`
//! (`analyticcapfloorengine.hpp:36`), generic over any affine model.
//! `discount_bond_option` lives on no trait on main, and [`HullWhite`] (#391) is
//! the only model that provides it, so the port binds concretely to a
//! [`SharedMut<HullWhite>`] - the same choice, for the same reason, that the
//! [`JamshidianSwaptionEngine`](crate::pricingengines::swaption::JamshidianSwaptionEngine)
//! documents. A generic engine over every affine model waits for a
//! `DiscountBondOption` trait, a later ticket.
//!
//! ## Deferred / collapsed
//!
//! - **The non-term-structure-consistent-model fallback**
//!   (`analyticcapfloorengine.cpp:40-48`, the `dynamic_pointer_cast` `else`
//!   branch, and the engine-level `termStructure_`): Hull-White is always
//!   term-structure consistent, so only the `tsmodel` branch is live. The
//!   reference date and day counter are read straight off
//!   `model.term_structure()`. The fallback and its ctor overload are not ported.
//! - **The model-present guard** (`:35`, `QL_REQUIRE(!model_.empty())`): the ctor
//!   takes the model by value, so an absent model is structurally impossible.
//!
//! ## Divergences from QuantLib
//!
//! - **Explicit [`Settings`] (D5).** C++ reads the global `Settings::instance()`
//!   for `includeReferenceDateEvents`/`includeTodaysCashFlows` and the evaluation
//!   date (`:54-61`); the port threads an explicit handle, as every other engine
//!   does under D5.
//! - **The intrinsic-branch discount reads the curve directly.** C++ calls
//!   `model_->discount(paymentTime)` (`:80`); [`HullWhite::discount`] is private,
//!   but its value equals `term_structure()->discount(paymentTime)` by
//!   construction (`hullwhite.cpp:76-78`), so the port reads the curve handle.
//! - **`Option`-typed forwards and strikes are checked, not unwrapped.** A
//!   still-live coupon always has a forward (`setup_arguments` fills it whenever
//!   `end_date >= today`, `capfloor.cpp:245`, and a coupon reaching the pricing
//!   branch has `payment_time >= 0`), and a cap/collar always has a cap rate (a
//!   floor/collar a floor rate); the port `Err`s rather than `unwrap`s if any is
//!   absent, mirroring C++'s implicit invariants without a panic path.

use crate::errors::QlResult;
use crate::fail;
use crate::instrument::InstrumentResults;
use crate::instruments::{CapFloorArguments, CapFloorType};
use crate::models::model::CalibratedModelHolder;
use crate::models::shortrate::hullwhite::HullWhite;
use crate::option::OptionType;
use crate::patterns::observable::{AsObservable, Observable};
use crate::pricingengine::{Arguments, GenericEngine, PricingEngine, Results};
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut};
use crate::time::date::Date;

/// Analytic cap/floor engine under [`HullWhite`]
/// (`analyticcapfloorengine.hpp:35`).
pub struct AnalyticCapFloorEngine {
    base: GenericEngine<CapFloorArguments, InstrumentResults>,
    model: SharedMut<HullWhite>,
    settings: Shared<Settings<Date>>,
}

impl AnalyticCapFloorEngine {
    /// Builds the engine over a Hull-White `model`
    /// (`analyticcapfloorengine.hpp:43`). The engine observes the model's
    /// observable, so a model or curve change invalidates a cap/floor priced by
    /// it. `settings` supplies the evaluation date and the reference-date-event
    /// gates the C++ engine reads from the global singleton.
    pub fn new(
        model: SharedMut<HullWhite>,
        settings: Shared<Settings<Date>>,
    ) -> AnalyticCapFloorEngine {
        let base = GenericEngine::new(CapFloorArguments::default(), InstrumentResults::default());
        base.register_with(model.borrow().calibrated_model().observable());
        AnalyticCapFloorEngine {
            base,
            model,
            settings,
        }
    }
}

impl AsObservable for AnalyticCapFloorEngine {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl PricingEngine for AnalyticCapFloorEngine {
    fn arguments_mut(&mut self) -> &mut dyn Arguments {
        self.base.arguments_mut()
    }

    fn results(&self) -> &dyn Results {
        self.base.results()
    }

    fn reset(&mut self) {
        self.base.reset();
    }

    fn calculate(&mut self) -> QlResult<()> {
        let model = self.model.borrow();
        let (reference_date, day_counter) = {
            let curve = model.term_structure().current_link()?;
            (curve.reference_date()?, curve.require_day_counter()?)
        };

        // includeRefDatePayments, overridden by includeTodaysCashFlows only when
        // the reference date is the evaluation date (`:54-61`).
        let mut include_ref_date_payments = self.settings.include_reference_date_events();
        if Some(reference_date) == self.settings.evaluation_date()
            && let Some(include_todays) = self.settings.include_todays_cash_flows()
        {
            include_ref_date_payments = include_todays;
        }

        let arguments = self.base.arguments();
        let Some(cap_floor_type) = arguments.cap_floor_type else {
            fail!("cap/floor type not set");
        };
        let has_cap = matches!(cap_floor_type, CapFloorType::Cap | CapFloorType::Collar);
        let has_floor = matches!(cap_floor_type, CapFloorType::Floor | CapFloorType::Collar);
        let floor_mult = if cap_floor_type == CapFloorType::Floor {
            1.0
        } else {
            -1.0
        };

        let mut value = 0.0;
        for i in 0..arguments.end_dates.len() {
            let fixing_time = day_counter.year_fraction(reference_date, arguments.fixing_dates[i]);
            let payment_time = day_counter.year_fraction(reference_date, arguments.end_dates[i]);
            let not_expired = if include_ref_date_payments {
                payment_time >= 0.0
            } else {
                payment_time > 0.0
            };
            if !not_expired {
                continue;
            }

            let tenor = arguments.accrual_times[i];
            let nominal = arguments.nominals[i];
            let gearing = arguments.gearings[i];

            if fixing_time <= 0.0 {
                let Some(fixing) = arguments.forwards[i] else {
                    fail!("a still-live cap/floor coupon has no forward set");
                };
                let discount = model
                    .term_structure()
                    .current_link()?
                    .discount(payment_time, false)?;
                if has_cap {
                    let Some(strike) = arguments.cap_rates[i] else {
                        fail!("cap rate not set for a cap/collar");
                    };
                    value += discount * nominal * tenor * gearing * (fixing - strike).max(0.0);
                }
                if has_floor {
                    let Some(strike) = arguments.floor_rates[i] else {
                        fail!("floor rate not set for a floor/collar");
                    };
                    value += discount
                        * nominal
                        * tenor
                        * floor_mult
                        * gearing
                        * (strike - fixing).max(0.0);
                }
            } else {
                let maturity = day_counter.year_fraction(reference_date, arguments.start_dates[i]);
                if has_cap {
                    let Some(cap_rate) = arguments.cap_rates[i] else {
                        fail!("cap rate not set for a cap/collar");
                    };
                    let temp = 1.0 + cap_rate * tenor;
                    value += nominal
                        * gearing
                        * temp
                        * model.discount_bond_option(
                            OptionType::Put,
                            1.0 / temp,
                            maturity,
                            payment_time,
                        )?;
                }
                if has_floor {
                    let Some(floor_rate) = arguments.floor_rates[i] else {
                        fail!("floor rate not set for a floor/collar");
                    };
                    let temp = 1.0 + floor_rate * tenor;
                    value += nominal
                        * gearing
                        * temp
                        * floor_mult
                        * model.discount_bond_option(
                            OptionType::Call,
                            1.0 / temp,
                            maturity,
                            payment_time,
                        )?;
                }
            }
        }
        drop(model);

        self.base.results_mut().value = Some(value);
        Ok(())
    }
}
