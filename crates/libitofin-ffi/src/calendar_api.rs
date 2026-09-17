use crate::boundary::*;
use libitofin::time::calendar::Calendar;
use libitofin::time::calendars::*;

#[derive(Clone)]
pub(crate) struct NativeCalendar {
    pub inner: Calendar,
    pub horizon: Option<i32>,
}

fn country_calendar(country: i32, market: i32) -> BindingResult<NativeCalendar> {
    macro_rules! market {
        ($module:ident, $type:ident, $($variant:ident),+) => {{
            let variants = [$($module::Market::$variant),+];
            let value = usize::try_from(market).ok().and_then(|i| variants.get(i))
                .ok_or_else(|| BindingError::invalid("unknown calendar market"))?;
            $type::new(*value)
        }};
    }
    let inner = match country {
        0 if market == 0 => Botswana::new(),
        1 if market == 0 => Denmark::new(),
        2 if market == 0 => Finland::new(),
        3 if market == 0 => Hungary::new(),
        4 if market == 0 => Japan::new(),
        5 if market == 0 => Norway::new(),
        6 if market == 0 => SouthAfrica::new(),
        7 if market == 0 => Sweden::new(),
        8 if market == 0 => Switzerland::new(),
        9 if market == 0 => Thailand::new(),
        10 if market == 0 => Turkey::new(),
        11 => market!(argentina, Argentina, Merval),
        12 => market!(australia, Australia, Settlement, Asx),
        13 => market!(austria, Austria, Settlement, Exchange),
        14 => market!(brazil, Brazil, Settlement, Exchange),
        15 => market!(canada, Canada, Settlement, Tsx),
        16 => market!(chile, Chile, Sse),
        17 => market!(china, China, Sse, Ib),
        18 => market!(croatia, Croatia, Zse),
        19 => market!(czechrepublic, CzechRepublic, Pse),
        20 => market!(france, France, Settlement, Exchange),
        21 => market!(
            germany,
            Germany,
            Settlement,
            FrankfurtStockExchange,
            Xetra,
            Eurex,
            Euwax
        ),
        22 => market!(hongkong, HongKong, HKEx),
        23 => market!(iceland, Iceland, Icex),
        24 => market!(india, India, Nse),
        25 => market!(indonesia, Indonesia, Bej, Jsx, Idx),
        26 => market!(israel, Israel, Settlement, Tase, Shir, Telbor),
        27 => market!(italy, Italy, Settlement, Exchange),
        28 => market!(malta, Malta, Mse),
        29 => market!(mexico, Mexico, Bmv),
        30 => market!(montenegro, Montenegro, Mnse),
        31 => market!(newzealand, NewZealand, Wellington, Auckland),
        32 => market!(northmacedonia, NorthMacedonia, Mse),
        33 => market!(poland, Poland, Settlement, Wse),
        34 => market!(romania, Romania, Public, Bvb),
        35 => market!(russia, Russia, Settlement, Moex),
        36 => market!(saudiarabia, SaudiArabia, Tadawul),
        37 => market!(serbia, Serbia, Bse),
        38 => market!(singapore, Singapore, Sgx),
        39 => market!(slovakia, Slovakia, Bsse),
        40 => market!(slovenia, Slovenia, Lse),
        41 => market!(southkorea, SouthKorea, Settlement, Krx),
        42 => market!(taiwan, Taiwan, Tsec),
        43 => market!(ukraine, Ukraine, Use),
        44 => market!(unitedkingdom, UnitedKingdom, Settlement, Exchange, Metals),
        45 => market!(
            unitedstates,
            UnitedStates,
            Settlement,
            Nyse,
            GovernmentBond,
            Nerc,
            LiborImpact,
            FederalReserve,
            Sofr
        ),
        46 => market!(uzbekistan, Uzbekistan, Uzse),
        _ => return Err(BindingError::invalid("unknown calendar or market")),
    };
    let horizon = match country {
        25 => Some(indonesia::HOLIDAY_HORIZON),
        36 => Some(saudiarabia::HOLIDAY_HORIZON),
        _ => None,
    };
    Ok(NativeCalendar { inner, horizon })
}

#[unsafe(no_mangle)]
/// # Safety
/// Pointers and context must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_calendar_country_new(
    ctx: *mut Context,
    country: i32,
    market: i32,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            output(out, c.insert(country_calendar(country, market)?)?)
        })
    }
}

impl NativeCalendar {
    pub(crate) fn checked(
        &self,
        date: libitofin::time::date::Date,
    ) -> BindingResult<libitofin::time::date::Date> {
        if let Some(horizon) = self.horizon
            && date.year() > horizon
        {
            return Err(BindingError::invalid(format!(
                "holidays are tabulated only through {horizon}"
            )));
        }
        Ok(date)
    }
    pub(crate) fn is_holiday(&self, date: libitofin::time::date::Date) -> BindingResult<bool> {
        Ok(self.inner.is_holiday(self.checked(date)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libitofin::time::date::{Date, Month};

    #[test]
    fn country_markets_and_horizons() {
        for country in 0..47 {
            assert!(
                !country_calendar(country, 0)
                    .unwrap()
                    .inner
                    .name()
                    .is_empty()
            );
            assert!(country_calendar(country, -1).is_err());
            assert!(country_calendar(country, 100).is_err());
        }
        assert!(country_calendar(47, 0).is_err());
        let us = country_calendar(45, 0).unwrap();
        for (day, month) in [
            (1, Month::January),
            (19, Month::January),
            (16, Month::February),
            (31, Month::May),
            (5, Month::July),
            (6, Month::September),
            (11, Month::October),
            (11, Month::November),
            (25, Month::November),
            (24, Month::December),
            (31, Month::December),
        ] {
            assert!(us.is_holiday(Date::new(day, month, 2004)).unwrap());
        }
        for (country, year) in [(25, 2014), (36, 2022)] {
            let cal = country_calendar(country, 0).unwrap();
            assert!(cal.checked(Date::new(1, Month::June, year)).is_ok());
            assert!(cal.checked(Date::new(1, Month::January, year + 1)).is_err());
        }
    }
}
