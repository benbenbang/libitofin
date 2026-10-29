use libitofin::currency::Currency;
use libitofin::handle::{Handle, RelinkableHandle};
use libitofin::indexes::iborindex::IborIndex;
use libitofin::instrument::Instrument;
use libitofin::instruments::{Bond, FixedRateBond, MakeVanillaSwap};
use libitofin::interestrate::Compounding;
use libitofin::math::interpolations::linear::Linear;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::bond::DiscountingBondEngine;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::risk::{
    DEFAULT_SHIFT, Rows, SensitivityAnalysis, aggregate_npv, bucket_analysis,
    bucket_analysis_matrix, bucket_analysis_vector, parallel_analysis,
};
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::TermStructure;
use libitofin::termstructures::bootstraphelper::RateHelper;
use libitofin::termstructures::bootstraptraits::ZeroYield;
use libitofin::termstructures::yields::{
    DepositRateHelper, FlatForward, PiecewiseYieldCurve, SwapRateHelper,
};
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::businessdayconvention::BusinessDayConvention::Unadjusted;
use libitofin::time::calendars::nullcalendar::NullCalendar;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use libitofin::time::frequency::Frequency;
use libitofin::time::period::Period;
use libitofin::time::schedule::MakeSchedule;
use libitofin::time::timeunit::TimeUnit::{Days, Months, Years};

const RATE: f64 = 0.03;

fn today() -> Date {
    Date::new(15, Month::June, 2026)
}

fn settings() -> Shared<Settings<Date>> {
    let settings = shared(Settings::new());
    settings.set_evaluation_date(today());
    settings
}

fn rel_near(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance * expected.abs(),
        "{actual} vs {expected} (relative error {})",
        ((actual - expected) / expected).abs()
    );
}

struct BondSetup {
    settings: Shared<Settings<Date>>,
    quote: Shared<SimpleQuote>,
    curve: Shared<FlatForward>,
    bond: SharedMut<Bond>,
}

impl BondSetup {
    fn new() -> BondSetup {
        let settings = settings();
        let quote = shared(SimpleQuote::new(RATE));
        let curve = shared(FlatForward::new(
            today(),
            Handle::new(Shared::clone(&quote) as Shared<dyn Quote>),
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
        ));
        let schedule = MakeSchedule::new()
            .from(today())
            .to(Date::new(15, Month::June, 2036))
            .with_frequency(Frequency::Annual)
            .with_calendar(NullCalendar::new())
            .with_convention(Unadjusted)
            .with_termination_date_convention(Unadjusted)
            .backwards()
            .build();
        let mut bond = FixedRateBond::new(
            0,
            100.0,
            schedule,
            vec![0.05],
            Actual365Fixed::new(),
            Unadjusted,
            100.0,
            Some(today()),
            None,
            None,
            NullCalendar::new(),
            Unadjusted,
            false,
            None,
            Shared::clone(&settings),
        )
        .unwrap();
        let engine = shared_mut(DiscountingBondEngine::new(
            Handle::new(Shared::clone(&curve) as Shared<dyn YieldTermStructure>),
            None,
            Shared::clone(&settings),
        ));
        bond.bond_mut()
            .base_mut()
            .set_pricing_engine(engine as SharedMut<dyn PricingEngine>);
        BondSetup {
            settings,
            quote,
            curve,
            bond: shared_mut(bond.into_bond()),
        }
    }

    fn instruments(&self) -> Vec<SharedMut<dyn Instrument>> {
        vec![SharedMut::clone(&self.bond) as SharedMut<dyn Instrument>]
    }

    fn analytic_derivatives(&self) -> (f64, f64) {
        let reference = self.curve.reference_date().unwrap();
        let include = self.settings.include_reference_date_events();
        let (mut first, mut second) = (0.0, 0.0);
        for cf in self.bond.borrow().cashflows() {
            if cf
                .has_occurred(&self.settings, Some(reference), Some(include))
                .unwrap()
            {
                continue;
            }
            let t = self.curve.time_from_reference(cf.date()).unwrap();
            let pv = cf.amount().unwrap() * self.curve.discount_date(cf.date(), false).unwrap();
            first -= t * pv;
            second += t * t * pv;
        }
        (first, second)
    }
}

#[test]
fn centered_bucket_matches_the_analytic_bond_duration_and_convexity() {
    let setup = BondSetup::new();
    let (first, second) = setup.analytic_derivatives();
    let (d1, d2) = bucket_analysis(
        &setup.quote,
        &setup.instruments(),
        &[],
        DEFAULT_SHIFT,
        SensitivityAnalysis::default(),
        None,
    )
    .unwrap();
    rel_near(d1, first, 1e-6);
    rel_near(d2.unwrap(), second, 1e-6);
}

#[test]
fn one_side_bucket_has_no_second_order_and_weights_scale_the_result() {
    let setup = BondSetup::new();
    let (first, _) = setup.analytic_derivatives();
    let (d1, d2) = bucket_analysis(
        &setup.quote,
        &setup.instruments(),
        &[2.0],
        1e-6,
        SensitivityAnalysis::OneSide,
        None,
    )
    .unwrap();
    assert!(d2.is_none());
    rel_near(d1, 2.0 * first, 1e-4);
}

#[test]
fn a_successful_call_restores_the_quote_and_the_npv() {
    let setup = BondSetup::new();
    let instruments = setup.instruments();
    let reference = aggregate_npv(&instruments, &[1.0]).unwrap();
    for kind in [SensitivityAnalysis::OneSide, SensitivityAnalysis::Centered] {
        parallel_analysis(
            std::slice::from_ref(&setup.quote),
            &instruments,
            &[],
            DEFAULT_SHIFT,
            kind,
            None,
        )
        .unwrap();
        assert_eq!(setup.quote.value().unwrap(), RATE);
        let npv = aggregate_npv(&instruments, &[]).unwrap();
        assert!((npv - reference).abs() <= 1e-12, "{npv} vs {reference}");
    }
}

#[test]
fn edge_cases_are_errors_and_leave_the_quotes_untouched() {
    let setup = BondSetup::new();
    let instruments = setup.instruments();
    let quotes = [Shared::clone(&setup.quote)];
    let kind = SensitivityAnalysis::Centered;
    let message = |result: libitofin::errors::QlResult<(f64, Option<f64>)>| {
        result.unwrap_err().message().to_owned()
    };

    assert_eq!(
        message(parallel_analysis(
            &quotes,
            &instruments,
            &[],
            0.0,
            kind,
            None
        )),
        "zero shift not allowed"
    );
    assert_eq!(
        message(bucket_analysis(
            &setup.quote,
            &[],
            &[],
            DEFAULT_SHIFT,
            kind,
            None
        )),
        "empty instrument vector"
    );
    assert_eq!(
        message(parallel_analysis(
            &[],
            &instruments,
            &[],
            DEFAULT_SHIFT,
            kind,
            None
        )),
        "empty SimpleQuote vector"
    );
    assert_eq!(
        message(bucket_analysis(
            &setup.quote,
            &instruments,
            &[1.0, 2.0],
            DEFAULT_SHIFT,
            kind,
            None
        )),
        "dimension mismatch between instruments (1) and quantities (2)"
    );
    let invalid = [Shared::clone(&setup.quote), shared(SimpleQuote::new(None))];
    assert_eq!(
        message(parallel_analysis(
            &invalid,
            &instruments,
            &[],
            DEFAULT_SHIFT,
            kind,
            None
        )),
        "invalid SimpleQuote at position 1"
    );
    assert!(bucket_analysis_vector(&invalid, &instruments, &[], DEFAULT_SHIFT, kind).is_err());
    assert_eq!(setup.quote.value().unwrap(), RATE);
}

struct CurveSetup {
    quotes: Vec<Shared<SimpleQuote>>,
    swap: SharedMut<dyn Instrument>,
}

impl CurveSetup {
    fn new() -> CurveSetup {
        let settings = settings();
        let index = |forwarding: Handle<dyn YieldTermStructure>| {
            IborIndex::new(
                "sensitivity".to_owned(),
                Period::new(6, Months),
                0,
                Currency::eur(),
                NullCalendar::new(),
                Unadjusted,
                false,
                Actual365Fixed::new(),
                forwarding,
                Shared::clone(&settings),
            )
        };
        let helper_index = index(Handle::empty());
        let quotes: Vec<Shared<SimpleQuote>> = [0.030, 0.032, 0.035, 0.038]
            .into_iter()
            .map(|rate| shared(SimpleQuote::new(rate)))
            .collect();
        let handle =
            |quote: &Shared<SimpleQuote>| Handle::new(Shared::clone(quote) as Shared<dyn Quote>);
        let mut helpers: Vec<Shared<dyn RateHelper>> =
            vec![DepositRateHelper::new(handle(&quotes[0]), &helper_index)];
        for (quote, years) in quotes[1..].iter().zip([2, 5, 10]) {
            helpers.push(SwapRateHelper::new(
                handle(quote),
                Period::new(years, Years),
                NullCalendar::new(),
                Frequency::Annual,
                Unadjusted,
                Actual365Fixed::new(),
                &helper_index,
            ));
        }
        let curve = PiecewiseYieldCurve::<ZeroYield, Linear>::new(
            today(),
            helpers,
            Actual365Fixed::new(),
            Linear,
        )
        .unwrap();
        let forwarding = RelinkableHandle::new(curve as Shared<dyn YieldTermStructure>);
        let swap = MakeVanillaSwap::new(
            Period::new(7, Years),
            shared(index(forwarding.handle())),
            Some(0.04),
            Period::new(0, Days),
            settings,
        )
        .with_fixed_leg_tenor(Period::new(1, Years))
        .with_fixed_leg_day_count(Actual365Fixed::new())
        .build()
        .unwrap();
        CurveSetup {
            quotes,
            swap: shared_mut(swap) as SharedMut<dyn Instrument>,
        }
    }

    fn values(&self) -> Vec<f64> {
        self.quotes.iter().map(|q| q.value().unwrap()).collect()
    }
}

#[test]
fn parallel_first_order_equals_the_sum_of_the_bucket_first_orders() {
    let setup = CurveSetup::new();
    let instruments = [SharedMut::clone(&setup.swap)];
    let kind = SensitivityAnalysis::Centered;
    let (parallel, _) =
        parallel_analysis(&setup.quotes, &instruments, &[], DEFAULT_SHIFT, kind, None).unwrap();
    let (buckets, second) =
        bucket_analysis_vector(&setup.quotes, &instruments, &[], DEFAULT_SHIFT, kind).unwrap();
    assert_eq!(buckets.len(), setup.quotes.len());
    assert!(second.iter().all(Option::is_some));
    assert!(parallel.abs() > 1e-3);
    rel_near(buckets.iter().sum(), parallel, 1e-6);
}

#[test]
fn a_failing_reprice_is_an_error_and_restores_every_quote() {
    let setup = CurveSetup::new();
    let instruments = [SharedMut::clone(&setup.swap)];
    let originals = setup.values();
    let reference = aggregate_npv(&instruments, &[]).unwrap();

    let result = parallel_analysis(
        &setup.quotes,
        &instruments,
        &[],
        10.0,
        SensitivityAnalysis::Centered,
        None,
    );

    let message = result.unwrap_err().message().to_owned();
    assert!(message.contains("bootstrap failed"), "{message}");
    assert_eq!(setup.values(), originals);
    let npv = aggregate_npv(&instruments, &[]).unwrap();
    assert!((npv - reference).abs() <= 1e-12, "{npv} vs {reference}");
}

impl CurveSetup {
    fn ragged(&self) -> Vec<Vec<Shared<SimpleQuote>>> {
        let q = |i: usize| Shared::clone(&self.quotes[i]);
        vec![vec![q(0), q(1)], vec![], vec![q(2), q(3)]]
    }
}

#[test]
fn the_matrix_bucket_equals_the_flattened_vector_bucket_reshaped() {
    for kind in [SensitivityAnalysis::OneSide, SensitivityAnalysis::Centered] {
        let flat = CurveSetup::new();
        let (first, second) = bucket_analysis_vector(
            &flat.quotes,
            &[SharedMut::clone(&flat.swap)],
            &[],
            DEFAULT_SHIFT,
            kind,
        )
        .unwrap();
        let ragged = CurveSetup::new();
        let (matrix_first, matrix_second) = bucket_analysis_matrix(
            &ragged.ragged(),
            &[SharedMut::clone(&ragged.swap)],
            &[],
            DEFAULT_SHIFT,
            kind,
        )
        .unwrap();
        assert_eq!(
            matrix_first,
            vec![first[..2].to_vec(), vec![], first[2..].to_vec()]
        );
        assert_eq!(
            matrix_second,
            vec![second[..2].to_vec(), vec![], second[2..].to_vec()]
        );
    }
    let setup = CurveSetup::new();
    let instruments = [SharedMut::clone(&setup.swap)];
    let (first, second) = bucket_analysis_matrix(
        &[vec![], vec![]],
        &instruments,
        &[],
        DEFAULT_SHIFT,
        SensitivityAnalysis::Centered,
    )
    .unwrap();
    assert_eq!((first.len(), second.len()), (2, 2));
    assert!(first.iter().all(Vec::is_empty) && second.iter().all(Vec::is_empty));
}

#[test]
fn matrix_edge_cases_are_errors() {
    let setup = CurveSetup::new();
    let instruments = [SharedMut::clone(&setup.swap)];
    let kind = SensitivityAnalysis::Centered;
    let message = |result: libitofin::errors::QlResult<(Rows<f64>, Rows<Option<f64>>)>| {
        result.unwrap_err().message().to_owned()
    };
    assert_eq!(
        message(bucket_analysis_matrix(
            &[],
            &instruments,
            &[],
            DEFAULT_SHIFT,
            kind
        )),
        "empty SimpleQuote range"
    );
    assert_eq!(
        message(bucket_analysis_matrix(
            &setup.ragged(),
            &[],
            &[],
            DEFAULT_SHIFT,
            kind
        )),
        "empty instrument vector"
    );
    assert_eq!(
        message(bucket_analysis_matrix(
            &setup.ragged(),
            &instruments,
            &[],
            0.0,
            kind
        )),
        "zero shift not allowed"
    );
    let invalid = vec![
        vec![Shared::clone(&setup.quotes[0])],
        vec![shared(SimpleQuote::new(None))],
    ];
    assert_eq!(
        message(bucket_analysis_matrix(
            &invalid,
            &instruments,
            &[],
            DEFAULT_SHIFT,
            kind
        )),
        "invalid SimpleQuote at position 1"
    );
}

#[test]
fn a_failing_matrix_reprice_is_an_error_and_restores_every_quote() {
    let setup = CurveSetup::new();
    let instruments = [SharedMut::clone(&setup.swap)];
    let originals = setup.values();
    let reference = aggregate_npv(&instruments, &[]).unwrap();

    let result = bucket_analysis_matrix(
        &setup.ragged(),
        &instruments,
        &[],
        10.0,
        SensitivityAnalysis::Centered,
    );

    let message = result.unwrap_err().message().to_owned();
    assert!(message.contains("bootstrap failed"), "{message}");
    assert_eq!(setup.values(), originals);
    let npv = aggregate_npv(&instruments, &[]).unwrap();
    assert!((npv - reference).abs() <= 1e-12, "{npv} vs {reference}");
}
