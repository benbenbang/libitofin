//! Fixed-date overnight and exchange-convention SOFR futures helpers.

use std::cell::RefCell;

use crate::cashflows::RateAveraging;
use crate::errors::{QlError, QlResult};
use crate::handle::{Handle, RelinkableHandle};
use crate::indexes::ibor::sofr::Sofr;
use crate::indexes::iborindex::OvernightIndex;
use crate::indexes::index::Index;
use crate::instrument::Instrument;
use crate::instruments::overnightindexfuture::OvernightIndexFuture;
use crate::patterns::observable::{AsObservable, Observable};
use crate::quotes::Quote;
use crate::settings::Settings;
use crate::shared::{Shared, shared};
use crate::termstructures::bootstraphelper::{BootstrapHelperBase, RateHelper};
use crate::termstructures::yields::ratehelpers::Pillar;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::date::{Date, Month};
use crate::time::frequency::Frequency;
use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;
use crate::time::weekday::Weekday;
use crate::types::Real;

/// Fits a discount curve to an overnight future with fixed reference dates.
///
/// The internal forecast handle is weak and does not observe the fitted curve.
/// Each implied-quote query forces the future to recalculate.
pub struct OvernightIndexFutureRateHelper {
    base: BootstrapHelperBase,
    future: RefCell<OvernightIndexFuture>,
    forecast: RelinkableHandle<dyn YieldTermStructure>,
}

impl OvernightIndexFutureRateHelper {
    /// Creates a helper, cloning the index onto a private forecast handle.
    ///
    /// # Errors
    /// Rejects an empty price handle, invalid reference dates, or a custom pillar
    /// outside the inclusive reference period.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        price: Handle<dyn Quote>,
        value_date: Date,
        maturity_date: Date,
        index: &Shared<OvernightIndex>,
        convexity: Handle<dyn Quote>,
        averaging: RateAveraging,
        pillar: Pillar,
    ) -> QlResult<Shared<Self>> {
        crate::require!(!price.is_empty(), "futures price handle is empty");
        let pillar_date = match pillar {
            Pillar::MaturityDate | Pillar::LastRelevantDate => maturity_date,
            Pillar::CustomDate(date) => {
                crate::require!(date != Date::null(), "custom pillar date must be provided");
                crate::require!(
                    date >= value_date,
                    "custom pillar date before start of reference period"
                );
                crate::require!(
                    date <= maturity_date,
                    "custom pillar date after end of reference period"
                );
                date
            }
        };
        let base = BootstrapHelperBase::new(price);
        convexity.register_observer(&base.observer());
        let forecast = RelinkableHandle::empty();
        let future = OvernightIndexFuture::new(
            index.clone_with(forecast.handle()),
            value_date,
            maturity_date,
            convexity,
            averaging,
        )?;
        let retained_index = future.overnight_index();
        retained_index
            .settings()
            .register_fixing_observer(&retained_index.name(), &base.observer());
        retained_index
            .settings()
            .register_eval_date_observer(&base.observer());
        base.set_earliest_date(value_date);
        base.set_maturity_date(maturity_date);
        base.set_latest_relevant_date(maturity_date);
        base.set_latest_date(maturity_date);
        base.set_pillar_date(pillar_date);
        Ok(shared(Self {
            base,
            future: RefCell::new(future),
            forecast,
        }))
    }

    /// Current convexity adjustment, zero when no quote was supplied.
    pub fn convexity_adjustment(&self) -> QlResult<Real> {
        self.future.borrow().convexity_adjustment()
    }
}

impl AsObservable for OvernightIndexFutureRateHelper {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl RateHelper for OvernightIndexFutureRateHelper {
    fn base(&self) -> &BootstrapHelperBase {
        &self.base
    }
    fn implied_quote(&self) -> QlResult<Real> {
        self.base.term_structure()?;
        let mut future = self.future.borrow_mut();
        future.recalculate()?;
        future.npv()
    }
    fn set_term_structure(&self, curve: &Shared<dyn YieldTermStructure>) {
        self.forecast.link_to_weak(Shared::downgrade(curve));
        self.base.set_term_structure(curve);
    }
}

/// Constructor namespace for monthly arithmetic and quarterly compounded SOFR futures.
pub struct SofrFutureRateHelper;

impl SofrFutureRateHelper {
    /// Builds an exchange-convention helper with a first-of-month or third-Wednesday start.
    ///
    /// # Errors
    /// Rejects frequencies other than monthly/quarterly, unsupported years, and
    /// invalid custom pillars or missing price handles.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(
        price: Handle<dyn Quote>,
        month: Month,
        year: i32,
        frequency: Frequency,
        convexity: Handle<dyn Quote>,
        pillar: Pillar,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<Shared<OvernightIndexFutureRateHelper>> {
        crate::require!(
            matches!(frequency, Frequency::Monthly | Frequency::Quarterly),
            "only monthly and quarterly SOFR futures accepted"
        );
        let (start, end) = std::panic::catch_unwind(|| {
            if frequency == Frequency::Monthly {
                let start = Date::new(1, month, year);
                (start, start + Period::new(1, TimeUnit::Months))
            } else {
                let start = Date::nth_weekday(3, Weekday::Wednesday, month, year);
                let next = start + Period::new(3, TimeUnit::Months);
                (
                    start,
                    Date::nth_weekday(3, Weekday::Wednesday, next.month(), next.year()),
                )
            }
        })
        .map_err(|_| {
            QlError::new(
                "SOFR reference period outside supported date range",
                file!(),
                line!(),
            )
        })?;
        let averaging = if frequency == Frequency::Quarterly {
            RateAveraging::Compound
        } else {
            RateAveraging::Simple
        };
        OvernightIndexFutureRateHelper::new(
            price,
            start,
            end,
            &shared(Sofr::new(Handle::empty(), settings)),
            convexity,
            averaging,
            pillar,
        )
    }
}
