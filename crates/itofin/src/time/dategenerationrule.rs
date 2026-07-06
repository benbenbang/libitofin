//! Date-generation rules for schedules.
//!
//! Port of `ql/time/dategenerationrule.hpp`. These conventions specify the
//! rule used to generate dates in a `Schedule`. QuantLib nests the enum as
//! `DateGeneration::Rule` purely for namespacing; here the enum itself is
//! [`DateGeneration`], so call sites like `DateGeneration::Backward` read the
//! same as the C++.

use std::fmt;

/// The rule used to generate the dates of a `Schedule`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DateGeneration {
    /// Backward from termination date to effective date.
    Backward,
    /// Forward from effective date to termination date.
    Forward,
    /// No intermediate dates between effective date and termination date.
    Zero,
    /// All dates but effective date and termination date are taken to be on
    /// the third Wednesday of their month (with forward calculation).
    ThirdWednesday,
    /// All dates including effective date and termination date are taken to
    /// be on the third Wednesday of their month (with forward calculation).
    ThirdWednesdayInclusive,
    /// All dates but the effective date are taken to be the twentieth of
    /// their month (used for CDS schedules in emerging markets). The
    /// termination date is also modified.
    Twentieth,
    /// All dates but the effective date are taken to be the twentieth of an
    /// IMM month (used for CDS schedules). The termination date is also
    /// modified.
    TwentiethIMM,
    /// Same as [`TwentiethIMM`](Self::TwentiethIMM) with unrestricted date
    /// ends and long/short stub coupon period (old CDS convention).
    OldCDS,
    /// Credit derivatives standard rule since 'Big Bang' changes in 2009.
    CDS,
    /// Credit derivatives standard rule since December 20th, 2015.
    CDS2015,
}

impl fmt::Display for DateGeneration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            DateGeneration::Backward => "Backward",
            DateGeneration::Forward => "Forward",
            DateGeneration::Zero => "Zero",
            DateGeneration::ThirdWednesday => "ThirdWednesday",
            DateGeneration::ThirdWednesdayInclusive => "ThirdWednesdayInclusive",
            DateGeneration::Twentieth => "Twentieth",
            DateGeneration::TwentiethIMM => "TwentiethIMM",
            DateGeneration::OldCDS => "OldCDS",
            DateGeneration::CDS => "CDS",
            DateGeneration::CDS2015 => "CDS2015",
        };
        f.write_str(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_matches_quantlib_labels() {
        let cases = [
            (DateGeneration::Backward, "Backward"),
            (DateGeneration::Forward, "Forward"),
            (DateGeneration::Zero, "Zero"),
            (DateGeneration::ThirdWednesday, "ThirdWednesday"),
            (
                DateGeneration::ThirdWednesdayInclusive,
                "ThirdWednesdayInclusive",
            ),
            (DateGeneration::Twentieth, "Twentieth"),
            (DateGeneration::TwentiethIMM, "TwentiethIMM"),
            (DateGeneration::OldCDS, "OldCDS"),
            (DateGeneration::CDS, "CDS"),
            (DateGeneration::CDS2015, "CDS2015"),
        ];
        for (rule, label) in cases {
            assert_eq!(rule.to_string(), label);
        }
    }
}
