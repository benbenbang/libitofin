//! Quote for the delta of an option and its volatility.
//!
//! Port of `ql/quotes/deltavolquote.hpp+cpp`. The C++ nested enums
//! `DeltaVolQuote::DeltaType`/`AtmType` become module-level enums; the `Atm`
//! prefix on the C++ `AtmType` values is a name-leakage artifact and is
//! dropped (`DeltaVolQuote::AtmSpot` reads `AtmType::Spot` here).

use crate::errors::QlResult;
use crate::handle::{AsObservable, Handle};
use crate::patterns::observable::{Observable, Observer, ResetThenNotify};
use crate::shared::{Shared, SharedMut, shared};
use crate::types::{Real, Time};

use super::Quote;

/// Delta convention of a [`DeltaVolQuote`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DeltaType {
    /// Spot delta, e.g. the usual Black-Scholes delta.
    Spot,
    /// Forward delta.
    Fwd,
    /// Premium-adjusted spot delta.
    PaSpot,
    /// Premium-adjusted forward delta.
    PaFwd,
}

/// At-the-money convention of a [`DeltaVolQuote`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AtmType {
    /// Default, if not an ATM quote.
    Null,
    /// K = S_0.
    Spot,
    /// K = F.
    Fwd,
    /// Call delta = put delta.
    DeltaNeutral,
    /// K such that vega is maximal.
    VegaMax,
    /// K such that gamma is maximal.
    GammaMax,
    /// K such that call delta = 0.50 (only for forward delta).
    PutCall50,
}

/// Quote for the delta of an option and its volatility.
///
/// Mirrors QuantLib's `DeltaVolQuote`: a volatility quote tagged with the
/// delta and maturity it belongs to, forwarding every notification of the
/// underlying volatility handle.
pub struct DeltaVolQuote {
    delta: Option<Real>,
    vol: Handle<dyn Quote>,
    delta_type: DeltaType,
    maturity: Time,
    atm_type: AtmType,
    observable: Shared<Observable>,
    _listener: SharedMut<ResetThenNotify>,
}

impl DeltaVolQuote {
    /// Standard constructor: delta versus volatility.
    pub fn new(delta: Real, vol: Handle<dyn Quote>, maturity: Time, delta_type: DeltaType) -> Self {
        Self::build(Some(delta), vol, delta_type, maturity, AtmType::Null)
    }

    /// Additional constructor for a special ATM quote.
    ///
    /// The C++ constructor leaves `delta_` uninitialized here (reading it is
    /// undefined behavior); the port stores `None` instead.
    pub fn new_atm(
        vol: Handle<dyn Quote>,
        delta_type: DeltaType,
        maturity: Time,
        atm_type: AtmType,
    ) -> Self {
        Self::build(None, vol, delta_type, maturity, atm_type)
    }

    fn build(
        delta: Option<Real>,
        vol: Handle<dyn Quote>,
        delta_type: DeltaType,
        maturity: Time,
        atm_type: AtmType,
    ) -> Self {
        let observable = shared(Observable::new());
        let listener = ResetThenNotify::forwarding(Shared::clone(&observable));
        vol.register_observer(&(listener.clone() as SharedMut<dyn Observer>));
        DeltaVolQuote {
            delta,
            vol,
            delta_type,
            maturity,
            atm_type,
            observable,
            _listener: listener,
        }
    }

    /// The delta this volatility belongs to; `None` for the ATM constructor,
    /// where C++ leaves it uninitialized.
    pub fn delta(&self) -> Option<Real> {
        self.delta
    }

    /// The maturity this volatility belongs to.
    pub fn maturity(&self) -> Time {
        self.maturity
    }

    /// The ATM convention of the quote.
    pub fn atm_type(&self) -> AtmType {
        self.atm_type
    }

    /// The delta convention of the quote.
    pub fn delta_type(&self) -> DeltaType {
        self.delta_type
    }
}

impl AsObservable for DeltaVolQuote {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}

impl Quote for DeltaVolQuote {
    fn value(&self) -> QlResult<Real> {
        self.vol.current_link()?.value()
    }

    fn is_valid(&self) -> bool {
        self.vol.current_link().is_ok_and(|q| q.is_valid())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quotes::{SimpleQuote, make_quote_handle};
    use crate::shared::shared;
    use crate::test_support::{Flag, as_observer};

    #[test]
    fn value_and_inspectors_mirror_the_construction() {
        let vol = shared(SimpleQuote::new(0.2));
        let quote = DeltaVolQuote::new(0.25, Handle::new(vol), 1.5, DeltaType::Spot);

        assert!(quote.is_valid());
        assert_eq!(quote.value().unwrap(), 0.2);
        assert_eq!(quote.delta(), Some(0.25));
        assert_eq!(quote.maturity(), 1.5);
        assert_eq!(quote.delta_type(), DeltaType::Spot);
        assert_eq!(quote.atm_type(), AtmType::Null);
    }

    #[test]
    fn atm_constructor_has_no_delta() {
        let vol = shared(SimpleQuote::new(0.1));
        let quote = DeltaVolQuote::new_atm(Handle::new(vol), DeltaType::Fwd, 0.5, AtmType::Spot);

        assert_eq!(quote.delta(), None);
        assert_eq!(quote.atm_type(), AtmType::Spot);
        assert_eq!(quote.delta_type(), DeltaType::Fwd);
    }

    #[test]
    fn vol_change_and_relink_notify_observers() {
        let rh = make_quote_handle(0.2);
        let quote = DeltaVolQuote::new(0.25, rh.handle(), 1.0, DeltaType::PaSpot);

        let flag = Flag::new();
        quote.observable().register_observer(&as_observer(&flag));

        let vol = shared(SimpleQuote::new(0.3));
        rh.link_to(vol.clone());
        assert!(Flag::is_up(&flag), "relink must reach quote observers");
        assert_eq!(quote.value().unwrap(), 0.3);

        Flag::lower(&flag);
        vol.set_value(0.4);
        assert!(Flag::is_up(&flag), "vol change must reach quote observers");
        assert_eq!(quote.value().unwrap(), 0.4);
    }

    #[test]
    fn empty_or_invalid_vol_is_invalid() {
        let quote = DeltaVolQuote::new(0.25, Handle::empty(), 1.0, DeltaType::Spot);
        assert!(!quote.is_valid());
        assert!(quote.value().is_err());
    }
}
