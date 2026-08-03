//! Region, i.e. geographical area, specification.
//!
//! Port of `ql/indexes/region.{hpp,cpp}`. A [`Region`] carries a name and a
//! code, and exists for inflation applicability: an inflation index name is
//! `region.name() + " " + family_name`, which is in turn the D11 fixing-store
//! key (`"UK RPI"`, `"EU HICP"`). Exercised through the inflation indexes
//! rather than by any dedicated numeric test.
//!
//! ## Divergences from QuantLib
//!
//! - **No `shared_ptr<Data>` indirection.** QuantLib's `Region` holds an
//!   `ext::shared_ptr<Data>` (`region.hpp:44-53`) that each concrete
//!   constructor points at a function-local `static`, so every `UKRegion`
//!   shares one allocation. That is a C++ allocation optimisation with no
//!   Rust counterpart worth carrying: this port stores the name and code
//!   inline. Behaviour is unchanged, since QuantLib compares regions by name
//!   (`region.hpp:121-127`), not by data pointer.
//! - **No abstract base.** QuantLib models the concretes as subclasses whose
//!   only content is the constructor. Rust gets associated constructor
//!   functions on the one concrete [`Region`] type instead; nothing in the
//!   library upcasts a region, so the subclass hierarchy buys nothing.
//! - **Only the regions batch 1 needs.** `AustraliaRegion`, `FranceRegion`,
//!   `USRegion` and `ZARegion` (`region.cpp:31-59`) are deliberately not
//!   ported yet; they arrive with the inflation indexes that use them.
//!   [`Region::new`] ports `CustomRegion` and can express any of them
//!   meanwhile.

/// Geographical or economic region, used for inflation applicability.
///
/// Two regions are equal iff they share the same [`name`](Region::name),
/// matching QuantLib's `operator==`.
#[derive(Clone, Debug)]
pub struct Region {
    name: String,
    code: String,
}

impl Region {
    /// Builds a region from its name and code.
    ///
    /// Ports `CustomRegion` (`region.hpp:67-71`), which exists so that a
    /// one-off region needs no new class.
    pub fn new(name: impl Into<String>, code: impl Into<String>) -> Self {
        Region {
            name: name.into(),
            code: code.into(),
        }
    }

    /// The European Union as a region (name and code both `"EU"`).
    ///
    /// Values match `EURegion` in `ql/indexes/region.cpp:36-39`.
    pub fn eu() -> Self {
        Region::new("EU", "EU")
    }

    /// The United Kingdom as a region (name and code both `"UK"`).
    ///
    /// Values match `UKRegion` in `ql/indexes/region.cpp:46-49`.
    pub fn uk() -> Self {
        Region::new("UK", "UK")
    }

    /// Region name, e.g. `"UK"`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Region code, e.g. `"UK"`.
    pub fn code(&self) -> &str {
        &self.code
    }
}

impl PartialEq for Region {
    fn eq(&self, other: &Region) -> bool {
        self.name == other.name
    }
}

impl Eq for Region {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uk_fields_match_quantlib() {
        let uk = Region::uk();
        assert_eq!(uk.name(), "UK");
        assert_eq!(uk.code(), "UK");
    }

    #[test]
    fn eu_fields_match_quantlib() {
        let eu = Region::eu();
        assert_eq!(eu.name(), "EU");
        assert_eq!(eu.code(), "EU");
    }

    #[test]
    fn accessors_round_trip_construction() {
        let france = Region::new("France", "FR");
        assert_eq!(france.name(), "France");
        assert_eq!(france.code(), "FR");
    }

    #[test]
    fn equality_is_by_name() {
        assert_eq!(Region::uk(), Region::uk());
        assert_ne!(Region::uk(), Region::eu());

        let same_name_other_code = Region::new("UK", "GB");
        assert_eq!(Region::uk(), same_name_other_code);
    }
}
