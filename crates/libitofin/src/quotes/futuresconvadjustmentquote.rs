//! Quote for the futures-convexity adjustment of an index.
//!
//! Port of `ql/quotes/futuresconvadjustmentquote.{hpp,cpp}`. The quote wraps
//! [`convexity_bias`](crate::models::shortrate::hullwhite::convexity_bias) over
//! a futures price, a Hull-White volatility and a mean reversion, and is what a
//! [`FuturesRateHelper`](crate::termstructures::yields::FuturesRateHelper) reads
//! as its convexity adjustment.
//!
//! The value is the BIAS itself, not a bias-adjusted rate: the helper adds it to
//! the forward (`ratehelpers.cpp:168`).
//!
//! ## The evaluation date is explicit (D5)
//!
//! C++ reads `Settings::instance().evaluationDate()` inside `value()` and
//! registers with it in the constructor. The core has no global settings
//! singleton, so the quote carries a [`Shared<Settings<Date>>`](Settings) - the
//! way the ibor indexes do - reads the evaluation date from it lazily in
//! `value()`, and registers with its evaluation-date observable. An unset
//! evaluation date is an error rather than a clock fallback (D10).

use std::cell::Cell;

use crate::ensure;
use crate::errors::QlResult;
use crate::handle::{AsObservable, Handle};
use crate::indexes::iborindex::IborIndex;
use crate::indexes::interestrateindex::InterestRateIndex;
use crate::models::shortrate::hullwhite::convexity_bias;
use crate::patterns::observable::{Observable, Observer, ResetThenNotify};
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut};
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::imm;
use crate::types::Real;

use super::{Quote, invalidator};

/// Quote for the futures-convexity adjustment of an index.
///
/// Mirrors QuantLib's `FuturesConvAdjustmentQuote`: the index maturity of the
/// futures date is resolved once at construction, the bias is computed lazily
/// and cached, and any notification from the three source handles or from the
/// evaluation date drops the cache and reaches this quote's observers.
pub struct FuturesConvAdjustmentQuote {
    day_counter: DayCounter,
    futures_date: Date,
    index_maturity_date: Date,
    futures_quote: Handle<dyn Quote>,
    volatility: Handle<dyn Quote>,
    mean_reversion: Handle<dyn Quote>,
    settings: Shared<Settings<Date>>,
    cache: Shared<Cell<Option<Real>>>,
    observable: Shared<Observable>,
    _listener: SharedMut<ResetThenNotify>,
}

impl FuturesConvAdjustmentQuote {
    /// The quote over an explicit futures date (C++ constructor #1).
    ///
    /// `index` is read only here, for its day counter and for the index
    /// maturity of `futures_date`.
    ///
    /// # Errors
    ///
    /// Fails if the index cannot resolve the maturity of `futures_date`.
    pub fn new(
        index: &IborIndex,
        futures_date: Date,
        futures_quote: Handle<dyn Quote>,
        volatility: Handle<dyn Quote>,
        mean_reversion: Handle<dyn Quote>,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<FuturesConvAdjustmentQuote> {
        let index_maturity_date = index.maturity_date(futures_date)?;
        let (cache, observable, listener) = invalidator();
        let observer = listener.clone() as SharedMut<dyn Observer>;
        futures_quote.register_observer(&observer);
        volatility.register_observer(&observer);
        mean_reversion.register_observer(&observer);
        settings.register_eval_date_observer(&observer);
        Ok(FuturesConvAdjustmentQuote {
            day_counter: index.day_counter().clone(),
            futures_date,
            index_maturity_date,
            futures_quote,
            volatility,
            mean_reversion,
            settings,
            cache,
            observable,
            _listener: listener,
        })
    }

    /// The same over an IMM code (C++ constructor #2), resolved as the first
    /// such IMM date on or after the evaluation date - C++ resolves it against
    /// the same default.
    ///
    /// # Errors
    ///
    /// Fails on an unset evaluation date, on a string that is not an IMM code,
    /// or as [`new`](Self::new) does.
    pub fn from_imm_code(
        index: &IborIndex,
        imm_code: &str,
        futures_quote: Handle<dyn Quote>,
        volatility: Handle<dyn Quote>,
        mean_reversion: Handle<dyn Quote>,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<FuturesConvAdjustmentQuote> {
        let Some(evaluation_date) = settings.evaluation_date() else {
            crate::fail!("no evaluation date set");
        };
        let futures_date = imm::date(imm_code, evaluation_date)?;
        Self::new(
            index,
            futures_date,
            futures_quote,
            volatility,
            mean_reversion,
            settings,
        )
    }

    /// The futures price the adjustment is computed at.
    ///
    /// # Errors
    ///
    /// Fails if the futures handle is empty or holds no value.
    pub fn futures_value(&self) -> QlResult<Real> {
        self.futures_quote.current_link()?.value()
    }

    /// The Hull-White volatility.
    ///
    /// # Errors
    ///
    /// Fails if the volatility handle is empty or holds no value.
    pub fn volatility(&self) -> QlResult<Real> {
        self.volatility.current_link()?.value()
    }

    /// The Hull-White mean reversion.
    ///
    /// # Errors
    ///
    /// Fails if the mean-reversion handle is empty or holds no value.
    pub fn mean_reversion(&self) -> QlResult<Real> {
        self.mean_reversion.current_link()?.value()
    }

    /// The futures date the adjustment is computed for.
    pub fn imm_date(&self) -> Date {
        self.futures_date
    }
}

impl AsObservable for FuturesConvAdjustmentQuote {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}

impl Quote for FuturesConvAdjustmentQuote {
    fn value(&self) -> QlResult<Real> {
        if let Some(cached) = self.cache.get() {
            return Ok(cached);
        }
        ensure!(self.is_valid(), "invalid FuturesConvAdjustmentQuote");
        let Some(settlement_date) = self.settings.evaluation_date() else {
            crate::fail!("no evaluation date set");
        };
        let start_time = self
            .day_counter
            .year_fraction(settlement_date, self.futures_date);
        let index_maturity = self
            .day_counter
            .year_fraction(settlement_date, self.index_maturity_date);
        let value = convexity_bias(
            self.futures_value()?,
            start_time,
            index_maturity,
            self.volatility()?,
            self.mean_reversion()?,
        )?;
        self.cache.set(Some(value));
        Ok(value)
    }

    fn is_valid(&self) -> bool {
        [&self.futures_quote, &self.volatility, &self.mean_reversion]
            .iter()
            .all(|handle| handle.current_link().is_ok_and(|quote| quote.is_valid()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexes::ibor::euribor::Euribor;
    use crate::quotes::{SimpleQuote, make_quote_handle};
    use crate::shared::shared;
    use crate::time::date::Month;
    use crate::time::imm;

    /// The fixture of `testGlobalBootstrapVariables`
    /// (`piecewiseyieldcurve.cpp:1486`), reduced to its first futures: a
    /// Euribor 3M index at an evaluation date of 25 September 2019, the first
    /// IMM date after it, and the 95.419 price of `immFutData[0]`.
    fn fixture() -> (
        Shared<Settings<Date>>,
        Shared<IborIndex>,
        Shared<SimpleQuote>,
        FuturesConvAdjustmentQuote,
    ) {
        let settings = shared(Settings::<Date>::new());
        let today = Date::new(25, Month::September, 2019);
        settings.set_evaluation_date(today);
        let index = shared(Euribor::three_months(Handle::empty(), settings.clone()));
        let futures_date = imm::next_date(today, true);
        let volatility = shared(SimpleQuote::new(1.0));
        let quote = FuturesConvAdjustmentQuote::new(
            &index,
            futures_date,
            Handle::new(shared(SimpleQuote::new(95.419)) as Shared<dyn Quote>),
            Handle::new(Shared::clone(&volatility) as Shared<dyn Quote>),
            make_quote_handle(0.03).handle(),
            settings.clone(),
        )
        .expect("the index resolves the maturity of an IMM date");
        (settings, index, volatility, quote)
    }

    /// The C++ value at the fixture's first futures, reproduced at full
    /// precision by a harness run against a locally built QuantLib dylib
    /// (`usingAtParCoupons()` set, which this quote does not depend on): the
    /// IMM date is 18 December 2019, its index maturity 18 March 2020, and the
    /// bias at vol 1.0 and mean reversion 0.03 is 0.085125076735947713.
    ///
    /// The year fractions are pinned too, because they are what fixes the
    /// evaluation date as the reference: the SETTLEMENT date of that fixture
    /// (27 September 2019) would give 82/360 rather than 84/360 and would still
    /// leave a plausible-looking bias.
    #[test]
    fn futures_conv_adjustment_quote_reproduces_the_cpp_bias() {
        let (_settings, index, _volatility, quote) = fixture();
        let today = Date::new(25, Month::September, 2019);

        assert_eq!(quote.imm_date(), Date::new(18, Month::December, 2019));
        assert_eq!(
            index.maturity_date(quote.imm_date()).unwrap(),
            Date::new(18, Month::March, 2020)
        );
        let start_time = index.day_counter().year_fraction(today, quote.imm_date());
        let index_maturity = index
            .day_counter()
            .year_fraction(today, index.maturity_date(quote.imm_date()).unwrap());
        assert!((start_time - 0.233_333_333_333_333_34).abs() < 1.0e-15);
        assert!((index_maturity - 0.486_111_111_111_111_1).abs() < 1.0e-15);

        let value = quote.value().unwrap();
        assert!(
            (value - 0.085_125_076_735_947_71).abs() < 1.0e-12,
            "bias at vol 1.0 is {value}"
        );
    }

    /// The cache is dropped when the volatility changes through its ORDINARY
    /// observing handle: the second dylib value (0.0034402599837630118 at vol
    /// 0.2) is two orders of magnitude away from the first, so a quote that
    /// kept its cache could not pass.
    #[test]
    fn a_volatility_change_drops_the_cached_bias() {
        let (_settings, _index, volatility, quote) = fixture();
        assert!((quote.value().unwrap() - 0.085_125_076_735_947_71).abs() < 1.0e-12);

        volatility.set_value(0.2);

        let value = quote.value().unwrap();
        assert!(
            (value - 0.003_440_259_983_763_012).abs() < 1.0e-12,
            "bias at vol 0.2 is {value}"
        );
    }

    /// The evaluation date is the reference of both year fractions and is read
    /// lazily: moving it forward drops the cache and re-times the bias.
    ///
    /// No second dylib number is needed - the expectation is `convexity_bias`
    /// at the year fractions off the NEW evaluation date, so a port that froze
    /// the reference at construction (or one that never registered with the
    /// evaluation-date observable, keeping its cache) fails.
    #[test]
    fn an_evaluation_date_change_re_times_the_bias() {
        let (settings, index, _volatility, quote) = fixture();
        assert!((quote.value().unwrap() - 0.085_125_076_735_947_71).abs() < 1.0e-12);

        let moved = Date::new(25, Month::October, 2019);
        settings.set_evaluation_date(moved);

        let day_counter = index.day_counter();
        let expected = convexity_bias(
            95.419,
            day_counter.year_fraction(moved, quote.imm_date()),
            day_counter.year_fraction(moved, index.maturity_date(quote.imm_date()).unwrap()),
            1.0,
            0.03,
        )
        .unwrap();
        let value = quote.value().unwrap();
        assert!(
            (value - expected).abs() < 1.0e-15,
            "bias at the moved evaluation date is {value}, expected {expected}"
        );
        assert!(
            (value - 0.085_125_076_735_947_71).abs() > 1.0e-3,
            "the moved evaluation date must change the bias"
        );
    }

    /// `isValid` is false as soon as one handle is empty, and `value` then
    /// fails rather than reaching `convexity_bias`.
    #[test]
    fn an_empty_handle_makes_the_quote_invalid() {
        let settings = shared(Settings::<Date>::new());
        let today = Date::new(25, Month::September, 2019);
        settings.set_evaluation_date(today);
        let index = shared(Euribor::three_months(Handle::empty(), settings.clone()));
        let quote = FuturesConvAdjustmentQuote::new(
            &index,
            imm::next_date(today, true),
            Handle::new(shared(SimpleQuote::new(95.419)) as Shared<dyn Quote>),
            Handle::empty(),
            make_quote_handle(0.03).handle(),
            settings,
        )
        .expect("the index resolves the maturity of an IMM date");

        assert!(!quote.is_valid());
        assert!(quote.value().is_err());
    }

    /// The IMM-code constructor resolves the same futures date as the fixture's
    /// explicit one: `Z9` is December 2019 at an evaluation date in September
    /// 2019.
    #[test]
    fn the_imm_code_constructor_resolves_the_futures_date() {
        let (settings, index, _volatility, _quote) = fixture();
        let quote = FuturesConvAdjustmentQuote::from_imm_code(
            &index,
            "Z9",
            Handle::new(shared(SimpleQuote::new(95.419)) as Shared<dyn Quote>),
            Handle::new(shared(SimpleQuote::new(1.0)) as Shared<dyn Quote>),
            make_quote_handle(0.03).handle(),
            settings,
        )
        .expect("Z9 is an IMM code");

        assert_eq!(quote.imm_date(), Date::new(18, Month::December, 2019));
        assert!((quote.value().unwrap() - 0.085_125_076_735_947_71).abs() < 1.0e-12);
    }
}
