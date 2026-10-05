use super::*;
use libitofin::handle::Handle;
use libitofin::interestrate::Compounding;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::shared::shared;
use libitofin::termstructures::volatility::{BlackConstantVol, BlackVolTermStructure};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::date::Month;
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use libitofin::time::frequency::Frequency;

pub struct Market {
    pub c: Context,
    pub swap: u64,
    pub engine: u64,
    pub process: u64,
    pub settings_id: u64,
    pub spot: Shared<SimpleQuote>,
    pub settings: Shared<Settings<Date>>,
    pub today: Date,
    pub maturity: Date,
}
impl Market {
    pub fn new() -> Self {
        let mut c = Context::new();
        let today = Date::new(5, Month::October, 2026);
        let maturity = today + 365;
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today);
        let settings_id = c.insert(settings.clone()).unwrap();
        let dc = Actual365Fixed::new();
        let flat = |rate| {
            Handle::new(shared(FlatForward::with_rate(
                today,
                rate,
                dc.clone(),
                Compounding::Continuous,
                Frequency::Annual,
            )) as Shared<dyn YieldTermStructure>)
        };
        let spot = shared(SimpleQuote::new(100.));
        let process = c
            .insert(shared(GeneralizedBlackScholesProcess::new(
                Handle::new(spot.clone() as Shared<dyn Quote>),
                flat(0.),
                flat(0.05),
                Handle::new(shared(BlackConstantVol::new(today, None, 0.2, dc))
                    as Shared<dyn BlackVolTermStructure>),
            )))
            .unwrap();
        let mut swap = 0;
        let mut engine = 0;
        let calls = [100., 110., 120.];
        let puts = [80., 90., 100.];
        assert_eq!(
            unsafe {
                itofin_variance_swap_new(
                    &mut c,
                    0,
                    0.04,
                    1000.,
                    today.serial_number(),
                    maturity.serial_number(),
                    settings_id,
                    &mut swap,
                    std::ptr::null_mut(),
                )
            },
            0
        );
        assert_eq!(
            unsafe {
                itofin_replicating_variance_swap_engine_new(
                    &mut c,
                    process,
                    5.,
                    calls.as_ptr(),
                    calls.len(),
                    puts.as_ptr(),
                    puts.len(),
                    &mut engine,
                    std::ptr::null_mut(),
                )
            },
            0
        );
        assert_eq!(
            unsafe { itofin_variance_swap_set_engine(&mut c, swap, engine, std::ptr::null_mut()) },
            0
        );
        Self {
            c,
            swap,
            engine,
            process,
            settings_id,
            spot,
            settings,
            today,
            maturity,
        }
    }
    pub fn value(&mut self, field: i32) -> f64 {
        let mut out = -91.;
        assert_eq!(
            unsafe {
                itofin_variance_swap_value(
                    &mut self.c,
                    self.swap,
                    field,
                    &mut out,
                    std::ptr::null_mut(),
                )
            },
            0
        );
        assert!(out.is_finite());
        out
    }
    pub fn integer(&mut self, field: i32) -> i32 {
        let mut out = -91;
        assert_eq!(
            unsafe {
                itofin_variance_swap_integer(
                    &mut self.c,
                    self.swap,
                    field,
                    &mut out,
                    std::ptr::null_mut(),
                )
            },
            0
        );
        out
    }
}
