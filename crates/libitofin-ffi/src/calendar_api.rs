use crate::boundary::*;
use libitofin::time::calendar::Calendar;
use libitofin::time::calendars::*;

#[derive(Clone)]
pub(crate) struct NativeCalendar {
    pub inner: Calendar,
    pub horizon: Option<i32>,
    pub first_year: i32,
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
        46 => Some(uzbekistan::HOLIDAY_HORIZON),
        42 => Some(taiwan::HOLIDAY_HORIZON),
        41 => Some(southkorea::HOLIDAY_HORIZON),
        38 => Some(singapore::HOLIDAY_HORIZON),
        32 => Some(northmacedonia::HOLIDAY_HORIZON),
        31 => Some(newzealand::HOLIDAY_HORIZON),
        26 => Some(israel::HOLIDAY_HORIZON),
        24 => Some(india::HOLIDAY_HORIZON),
        22 => Some(hongkong::HOLIDAY_HORIZON),
        17 => Some(china::HOLIDAY_HORIZON),
        10 => Some(turkey::HOLIDAY_HORIZON),
        9 => Some(thailand::HOLIDAY_HORIZON),
        25 => Some(indonesia::HOLIDAY_HORIZON),
        36 => Some(saudiarabia::HOLIDAY_HORIZON),
        _ => None,
    };
    Ok(NativeCalendar {
        inner,
        horizon,
        first_year: if country == 35 && market == 1 {
            russia::MOEX_FIRST_YEAR
        } else {
            1901
        },
    })
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
        if date.year() < self.first_year {
            return Err(BindingError::invalid(format!(
                "calendar starts in {}",
                self.first_year
            )));
        }
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

#[unsafe(no_mangle)]
/// # Safety
/// Pointers and context must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_calendar_joint_new(
    ctx: *mut Context,
    handles: *const u64,
    len: usize,
    rule: i32,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            let rule = match rule {
                0 => JointCalendarRule::JoinHolidays,
                1 => JointCalendarRule::JoinBusinessDays,
                _ => return Err(BindingError::invalid("unknown joint-calendar rule")),
            };
            let handles = input_slice(handles, len)?;
            if handles.is_empty() {
                return Err(BindingError::invalid(
                    "a joint calendar needs at least one calendar",
                ));
            }
            let calendars = handles
                .iter()
                .map(|&id| c.get::<NativeCalendar>(id))
                .collect::<BindingResult<Vec<_>>>()?;
            let first_year = calendars.iter().map(|v| v.first_year).max().unwrap_or(1901);
            let horizon = calendars.iter().filter_map(|v| v.horizon).min();
            let inner = JointCalendar::new(calendars.into_iter().map(|v| v.inner).collect(), rule);
            output(
                out,
                c.insert(NativeCalendar {
                    inner,
                    horizon,
                    first_year,
                })?,
            )
        })
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Pointers and context must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_calendar_query(
    ctx: *mut Context,
    id: u64,
    serial: i32,
    query: i32,
    out: *mut u8,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let cal = c.get::<NativeCalendar>(id)?;
            let date = cal.checked(crate::time_api::date(serial)?)?;
            let value = match query {
                0 => cal.inner.is_business_day(date),
                1 => cal.inner.is_holiday(date),
                2 => cal.inner.is_weekend_on(date),
                _ => return Err(BindingError::invalid("unknown calendar query")),
            };
            output(out, u8::from(value))
        })
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Pointers and context must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_calendar_business_days_between(
    ctx: *mut Context,
    id: u64,
    from: i32,
    to: i32,
    include_first: u8,
    include_last: u8,
    out: *mut i32,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let cal = c.get::<NativeCalendar>(id)?;
            let from = cal.checked(crate::time_api::date(from)?)?;
            let to = cal.checked(crate::time_api::date(to)?)?;
            let first = crate::time_api::bool_flag(include_first)?;
            let last = crate::time_api::bool_flag(include_last)?;
            output(out, cal.inner.business_days_between(from, to, first, last))
        })
    }
}

#[unsafe(no_mangle)]
/// # Safety
/// Pointers and context must satisfy the crate-level C caller contract.
pub unsafe extern "C" fn itofin_calendar_holiday_list(
    ctx: *mut Context,
    id: u64,
    from: i32,
    to: i32,
    include_weekends: u8,
    out: *mut i32,
    capacity: usize,
    required: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(required)?;
            let cal = c.get::<NativeCalendar>(id)?;
            let from = cal.checked(crate::time_api::date(from)?)?;
            let to = cal.checked(crate::time_api::date(to)?)?;
            if to < from {
                return Err(BindingError::invalid(
                    "holiday-list start must not follow end",
                ));
            }
            let weekends = crate::time_api::bool_flag(include_weekends)?;
            let dates = cal.inner.holiday_list(from, to, weekends);
            output(required, dates.len())?;
            if capacity == 0 {
                return Ok(());
            }
            if capacity < dates.len() {
                return Err(BindingError::invalid("holiday buffer too small"));
            }
            check_ptr(out)?;
            for (i, date) in dates.iter().enumerate() {
                output(out.add(i), date.serial_number())?;
            }
            Ok(())
        })
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
    #[test]
    fn constrained_calendars_and_joints_reject_unsupported_dates() {
        let mut c = Context::new();
        for country in 0..47 {
            let calendar = country_calendar(country, 0).unwrap();
            let Some(horizon) = calendar.horizon else {
                continue;
            };
            let id = c.insert(calendar).unwrap();
            let outside = Date::new(1, Month::January, horizon + 1).serial_number();
            let inside = Date::new(1, Month::June, horizon).serial_number();
            let mut out = 99;
            unsafe {
                for rule in 0..2 {
                    let mut joint = 0;
                    assert_eq!(
                        itofin_calendar_joint_new(
                            &mut c,
                            &id,
                            1,
                            rule,
                            &mut joint,
                            std::ptr::null_mut()
                        ),
                        0
                    );
                    for handle in [id, joint] {
                        for query in 0..3 {
                            assert_eq!(
                                itofin_calendar_query(
                                    &mut c,
                                    handle,
                                    outside,
                                    query,
                                    &mut out,
                                    std::ptr::null_mut()
                                ),
                                INVALID_ARGUMENT
                            );
                        }
                        assert_eq!(
                            itofin_calendar_query(
                                &mut c,
                                handle,
                                inside,
                                0,
                                &mut out,
                                std::ptr::null_mut()
                            ),
                            0
                        );
                    }
                }
            }
        }
        let moex = country_calendar(35, 1).unwrap();
        assert!(moex.checked(Date::new(31, Month::December, 2011)).is_err());
        assert!(moex.is_holiday(Date::new(1, Month::January, 2012)).is_ok());
        let id = c.insert(moex).unwrap();
        let before = Date::new(31, Month::December, 2011).serial_number();
        let inside = Date::new(1, Month::January, 2012).serial_number();
        unsafe {
            for rule in 0..2 {
                let mut joint = 0;
                assert_eq!(
                    itofin_calendar_joint_new(
                        &mut c,
                        &id,
                        1,
                        rule,
                        &mut joint,
                        std::ptr::null_mut()
                    ),
                    0
                );
                let mut out = 99;
                for handle in [id, joint] {
                    assert_eq!(
                        itofin_calendar_query(
                            &mut c,
                            handle,
                            before,
                            0,
                            &mut out,
                            std::ptr::null_mut()
                        ),
                        INVALID_ARGUMENT
                    );
                    assert_eq!(
                        itofin_calendar_query(
                            &mut c,
                            handle,
                            inside,
                            0,
                            &mut out,
                            std::ptr::null_mut()
                        ),
                        0
                    );
                }
            }
        }
    }

    #[test]
    fn queries_validate_buffers_flags_and_contexts() {
        let mut c = Context::new();
        let id = c
            .insert(NativeCalendar {
                inner: Target::new(),
                horizon: None,
                first_year: 1901,
            })
            .unwrap();
        let start = Date::new(1, Month::December, 2025).serial_number();
        let end = Date::new(31, Month::December, 2025).serial_number();
        let mut len = 0;
        let mut buffer = [-1; 2];
        let mut flag = 99;
        unsafe {
            assert_eq!(
                itofin_calendar_holiday_list(
                    &mut c,
                    id,
                    start,
                    end,
                    0,
                    std::ptr::null_mut(),
                    0,
                    &mut len,
                    std::ptr::null_mut()
                ),
                0
            );
            assert_eq!(len, 2);
            assert_eq!(
                itofin_calendar_holiday_list(
                    &mut c,
                    id,
                    start,
                    end,
                    0,
                    buffer.as_mut_ptr(),
                    1,
                    &mut len,
                    std::ptr::null_mut()
                ),
                INVALID_ARGUMENT
            );
            assert_eq!(buffer, [-1; 2]);
            assert_eq!(
                itofin_calendar_holiday_list(
                    &mut c,
                    id,
                    start,
                    end,
                    0,
                    buffer.as_mut_ptr(),
                    2,
                    &mut len,
                    std::ptr::null_mut()
                ),
                0
            );
            assert_eq!(
                buffer,
                [
                    Date::new(25, Month::December, 2025).serial_number(),
                    Date::new(26, Month::December, 2025).serial_number()
                ]
            );
            assert_eq!(
                itofin_calendar_query(&mut c, id, start, 9, &mut flag, std::ptr::null_mut()),
                INVALID_ARGUMENT
            );
            assert_eq!(flag, 99);
            assert_eq!(
                itofin_calendar_holiday_list(
                    &mut c,
                    id,
                    start,
                    end,
                    2,
                    buffer.as_mut_ptr(),
                    2,
                    &mut len,
                    std::ptr::null_mut()
                ),
                INVALID_ARGUMENT
            );
            let mut foreign = Context::new();
            assert_eq!(
                itofin_calendar_query(&mut foreign, id, start, 0, &mut flag, std::ptr::null_mut()),
                INVALID_HANDLE
            );
            let mut joint = 0;
            assert_eq!(
                itofin_calendar_joint_new(
                    &mut foreign,
                    &id,
                    1,
                    0,
                    &mut joint,
                    std::ptr::null_mut()
                ),
                INVALID_HANDLE
            );
            assert_eq!(
                itofin_calendar_joint_new(&mut c, &id, 1, 3, &mut joint, std::ptr::null_mut()),
                INVALID_ARGUMENT
            );
            assert_eq!(
                itofin_calendar_joint_new(
                    &mut c,
                    std::ptr::null(),
                    0,
                    0,
                    &mut joint,
                    std::ptr::null_mut()
                ),
                INVALID_ARGUMENT
            );
            assert_eq!(
                itofin_calendar_query(&mut c, id, start, 0, &mut flag, std::ptr::null_mut()),
                0
            );
        }
    }
}
