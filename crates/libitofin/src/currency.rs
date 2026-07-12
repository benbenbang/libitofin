//! Currency specification.
//!
//! Port of `ql/currency.{hpp,cpp}`. A [`Currency`] carries its name, ISO 4217
//! three-letter and numeric codes, symbol, fraction symbol, and the number of
//! fractionary parts per unit. It is a value type, exercised through the indexes
//! and (later) money rather than by any dedicated numeric test.
//!
//! Only [`Currency::eur`] is provided here, as the [`IborIndex`] slice (Euribor)
//! needs it. The full `ql/currencies/*` catalogue is deferred to a later ticket.
//!
//! ## Divergences from QuantLib
//!
//! - **No empty placeholder.** QuantLib's default-constructed `Currency` holds a
//!   null `data_` and `QL_REQUIRE`s a non-null one on every inspector; its
//!   `operator==` treats two empty currencies as equal and `operator<<` prints
//!   `"null currency"` for one. This port omits the empty state: a [`Currency`]
//!   always holds a concrete specification, so its accessors never trip that
//!   null check, equality is purely by name (QuantLib's non-empty branch), and
//!   [`Display`](std::fmt::Display) always prints the ISO code. The "not yet
//!   set" placeholder used by higher layers is an `Option<Currency>` at those
//!   call sites, mirroring the [`DayCounter`](crate::time::daycounter) decision.
//! - **No `rounding` field.** QuantLib's `Currency` carries a `Rounding`
//!   convention (`ql/math/rounding.hpp`). Rounding is not ported and currency
//!   rounding is never exercised by index use, so the field is omitted here
//!   rather than carried as a stub; it will be added when money/rounding lands.
//! - **No `triangulationCurrency` or `minorUnitCodes`.** These serve exchange
//!   and money features the index slice does not need; they are deferred.

use std::fmt;

use crate::types::Integer;

/// Currency specification.
///
/// Two currencies are equal iff they share the same [`name`](Currency::name),
/// matching QuantLib's `operator==`.
#[derive(Clone, Debug)]
pub struct Currency {
    name: String,
    code: String,
    numeric_code: Integer,
    symbol: String,
    fraction_symbol: String,
    fractions_per_unit: Integer,
}

impl Currency {
    /// Builds a currency from its ISO 4217 specification.
    pub fn new(
        name: impl Into<String>,
        code: impl Into<String>,
        numeric_code: Integer,
        symbol: impl Into<String>,
        fraction_symbol: impl Into<String>,
        fractions_per_unit: Integer,
    ) -> Self {
        Currency {
            name: name.into(),
            code: code.into(),
            numeric_code,
            symbol: symbol.into(),
            fraction_symbol: fraction_symbol.into(),
            fractions_per_unit,
        }
    }

    /// The European Euro (ISO code `EUR`, numeric `978`, 100 cents per unit).
    ///
    /// Values match `EURCurrency` in QuantLib's `ql/currencies/europe.cpp`. Its
    /// `ClosestRounding(2)` convention is dropped along with the `rounding`
    /// field (see the module divergences).
    pub fn eur() -> Self {
        Currency::new("European Euro", "EUR", 978, "", "", 100)
    }

    /// Currency name, e.g. `"European Euro"`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// ISO 4217 three-letter code, e.g. `"EUR"`.
    pub fn code(&self) -> &str {
        &self.code
    }

    /// ISO 4217 numeric code, e.g. `978`.
    pub fn numeric_code(&self) -> Integer {
        self.numeric_code
    }

    /// Symbol, e.g. `"$"` (empty when the currency defines none).
    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    /// Fraction symbol, e.g. `"c"` (empty when the currency defines none).
    pub fn fraction_symbol(&self) -> &str {
        &self.fraction_symbol
    }

    /// Number of fractionary parts in a unit, e.g. `100`.
    pub fn fractions_per_unit(&self) -> Integer {
        self.fractions_per_unit
    }
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.code)
    }
}

impl PartialEq for Currency {
    fn eq(&self, other: &Currency) -> bool {
        self.name == other.name
    }
}

impl Eq for Currency {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eur_fields_match_quantlib() {
        let eur = Currency::eur();
        assert_eq!(eur.name(), "European Euro");
        assert_eq!(eur.code(), "EUR");
        assert_eq!(eur.numeric_code(), 978);
        assert_eq!(eur.symbol(), "");
        assert_eq!(eur.fraction_symbol(), "");
        assert_eq!(eur.fractions_per_unit(), 100);
    }

    #[test]
    fn accessors_round_trip_construction() {
        let c = Currency::new("British pound sterling", "GBP", 826, "\u{a3}", "p", 100);
        assert_eq!(c.name(), "British pound sterling");
        assert_eq!(c.code(), "GBP");
        assert_eq!(c.numeric_code(), 826);
        assert_eq!(c.symbol(), "\u{a3}");
        assert_eq!(c.fraction_symbol(), "p");
        assert_eq!(c.fractions_per_unit(), 100);
    }

    #[test]
    fn equality_is_by_name() {
        assert_eq!(Currency::eur(), Currency::eur());

        let gbp = Currency::new("British pound sterling", "GBP", 826, "\u{a3}", "p", 100);
        assert_ne!(Currency::eur(), gbp);

        let same_name_other_fields = Currency::new("European Euro", "XXX", 0, "z", "z", 1);
        assert_eq!(Currency::eur(), same_name_other_fields);
    }

    #[test]
    fn display_prints_iso_code() {
        assert_eq!(Currency::eur().to_string(), "EUR");
    }
}
