//! Shared Islamic holiday lookups (Eid al-Fitr, Eid al-Adha).
//!
//! Port of `ql/time/calendars/islamicholidays.{hpp,cpp}`. Eid dates depend on
//! moon sighting and are tabulated rather than computed. QuantLib exposes two
//! approaches; only the moon-sighting method (used by South/Central Asia and
//! the Middle East, and by the calendars in this port) is tabulated in the
//! upstream source, so that is what is ported here.

/// The moon-sighting method for Eid dates, used by South Asia, Central Asia and
/// much of the Middle East and North Africa.
///
/// The tabulated Eid al-Fitr and Eid al-Adha dates run only through 2040
/// (matching QuantLib). These helpers do not panic beyond that horizon; each
/// calendar that consumes them (North Macedonia, Uzbekistan) owns the horizon
/// panic at its own `is_business_day` level.
pub mod moon_sighting {
    use crate::time::date::{Date, Month};

    /// Whether `d` is Eid al-Fitr under the moon-sighting method.
    pub fn is_eid_al_fitr(d: Date) -> bool {
        const DATES: &[(i32, Month, i32)] = &[
            (20, Month::March, 2026),
            (10, Month::March, 2027),
            (27, Month::February, 2028),
            (15, Month::February, 2029),
            (5, Month::February, 2030),
            (25, Month::January, 2031),
            (14, Month::January, 2032),
            (2, Month::January, 2033),
            (23, Month::December, 2033),
            (12, Month::December, 2034),
            (1, Month::December, 2035),
            (19, Month::November, 2036),
            (8, Month::November, 2037),
            (29, Month::October, 2038),
            (19, Month::October, 2039),
            (7, Month::October, 2040),
        ];
        DATES.iter().any(|&(day, m, y)| d == Date::new(day, m, y))
    }

    /// Whether `d` is Eid al-Adha under the moon-sighting method.
    pub fn is_eid_al_adha(d: Date) -> bool {
        const DATES: &[(i32, Month, i32)] = &[
            (27, Month::May, 2026),
            (17, Month::May, 2027),
            (5, Month::May, 2028),
            (24, Month::April, 2029),
            (13, Month::April, 2030),
            (3, Month::April, 2031),
            (22, Month::March, 2032),
            (11, Month::March, 2033),
            (28, Month::February, 2034),
            (18, Month::February, 2035),
            (7, Month::February, 2036),
            (27, Month::January, 2037),
            (16, Month::January, 2038),
            (5, Month::January, 2039),
            (26, Month::December, 2039),
            (15, Month::December, 2040),
        ];
        DATES.iter().any(|&(day, m, y)| d == Date::new(day, m, y))
    }
}

#[cfg(test)]
mod tests {
    use super::moon_sighting::{is_eid_al_adha, is_eid_al_fitr};
    use crate::time::date::{Date, Month};

    #[test]
    fn known_eid_dates() {
        assert!(is_eid_al_fitr(Date::new(20, Month::March, 2026)));
        assert!(!is_eid_al_fitr(Date::new(21, Month::March, 2026)));
        assert!(is_eid_al_adha(Date::new(27, Month::May, 2026)));
        assert!(!is_eid_al_adha(Date::new(20, Month::March, 2026)));
    }
}
