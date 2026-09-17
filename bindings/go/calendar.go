package itofin

/*
#include "itofin.h"
*/
import "C"

import (
	"fmt"
	"strings"
)

func (s *Session) countryCalendar(country int32, choices []string, market []string) (*Calendar, error) {
	if len(market) > 1 {
		return nil, fmt.Errorf("at most one market is accepted")
	}
	index := 0
	if len(market) == 1 {
		index = -1
		for i, name := range choices {
			if len(market[0]) == len(name) && strings.EqualFold(market[0], name) {
				index = i
				break
			}
		}
		if index < 0 {
			return nil, fmt.Errorf("unknown market %q, expected one of %s", market[0], strings.Join(choices, ", "))
		}
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_calendar_country_new(s.ctx, C.int32_t(country), C.int32_t(index), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &Calendar{object{s, uint64(id)}}, nil
}

func (s *Session) Botswana() (*Calendar, error)    { return s.countryCalendar(0, nil, nil) }
func (s *Session) Denmark() (*Calendar, error)     { return s.countryCalendar(1, nil, nil) }
func (s *Session) Finland() (*Calendar, error)     { return s.countryCalendar(2, nil, nil) }
func (s *Session) Hungary() (*Calendar, error)     { return s.countryCalendar(3, nil, nil) }
func (s *Session) Japan() (*Calendar, error)       { return s.countryCalendar(4, nil, nil) }
func (s *Session) Norway() (*Calendar, error)      { return s.countryCalendar(5, nil, nil) }
func (s *Session) SouthAfrica() (*Calendar, error) { return s.countryCalendar(6, nil, nil) }
func (s *Session) Sweden() (*Calendar, error)      { return s.countryCalendar(7, nil, nil) }
func (s *Session) Switzerland() (*Calendar, error) { return s.countryCalendar(8, nil, nil) }
func (s *Session) Thailand() (*Calendar, error)    { return s.countryCalendar(9, nil, nil) }
func (s *Session) Turkey() (*Calendar, error)      { return s.countryCalendar(10, nil, nil) }
func (s *Session) Argentina(market ...string) (*Calendar, error) {
	return s.countryCalendar(11, []string{"Merval"}, market)
}
func (s *Session) Australia(market ...string) (*Calendar, error) {
	return s.countryCalendar(12, []string{"Settlement", "ASX"}, market)
}
func (s *Session) Austria(market ...string) (*Calendar, error) {
	return s.countryCalendar(13, []string{"Settlement", "Exchange"}, market)
}
func (s *Session) Brazil(market ...string) (*Calendar, error) {
	return s.countryCalendar(14, []string{"Settlement", "Exchange"}, market)
}
func (s *Session) Canada(market ...string) (*Calendar, error) {
	return s.countryCalendar(15, []string{"Settlement", "TSX"}, market)
}
func (s *Session) Chile(market ...string) (*Calendar, error) {
	return s.countryCalendar(16, []string{"SSE"}, market)
}
func (s *Session) China(market ...string) (*Calendar, error) {
	return s.countryCalendar(17, []string{"SSE", "IB"}, market)
}
func (s *Session) Croatia(market ...string) (*Calendar, error) {
	return s.countryCalendar(18, []string{"ZSE"}, market)
}
func (s *Session) CzechRepublic(market ...string) (*Calendar, error) {
	return s.countryCalendar(19, []string{"PSE"}, market)
}
func (s *Session) France(market ...string) (*Calendar, error) {
	return s.countryCalendar(20, []string{"Settlement", "Exchange"}, market)
}
func (s *Session) Germany(market ...string) (*Calendar, error) {
	return s.countryCalendar(21, []string{"Settlement", "FrankfurtStockExchange", "Xetra", "Eurex", "Euwax"}, market)
}
func (s *Session) HongKong(market ...string) (*Calendar, error) {
	return s.countryCalendar(22, []string{"HKEx"}, market)
}
func (s *Session) Iceland(market ...string) (*Calendar, error) {
	return s.countryCalendar(23, []string{"ICEX"}, market)
}
func (s *Session) India(market ...string) (*Calendar, error) {
	return s.countryCalendar(24, []string{"NSE"}, market)
}
func (s *Session) Indonesia(market ...string) (*Calendar, error) {
	return s.countryCalendar(25, []string{"BEJ", "JSX", "IDX"}, market)
}
func (s *Session) Israel(market ...string) (*Calendar, error) {
	return s.countryCalendar(26, []string{"Settlement", "TASE", "SHIR", "Telbor"}, market)
}
func (s *Session) Italy(market ...string) (*Calendar, error) {
	return s.countryCalendar(27, []string{"Settlement", "Exchange"}, market)
}
func (s *Session) Malta(market ...string) (*Calendar, error) {
	return s.countryCalendar(28, []string{"MSE"}, market)
}
func (s *Session) Mexico(market ...string) (*Calendar, error) {
	return s.countryCalendar(29, []string{"BMV"}, market)
}
func (s *Session) Montenegro(market ...string) (*Calendar, error) {
	return s.countryCalendar(30, []string{"MNSE"}, market)
}
func (s *Session) NewZealand(market ...string) (*Calendar, error) {
	return s.countryCalendar(31, []string{"Wellington", "Auckland"}, market)
}
func (s *Session) NorthMacedonia(market ...string) (*Calendar, error) {
	return s.countryCalendar(32, []string{"MSE"}, market)
}
func (s *Session) Poland(market ...string) (*Calendar, error) {
	return s.countryCalendar(33, []string{"Settlement", "WSE"}, market)
}
func (s *Session) Romania(market ...string) (*Calendar, error) {
	return s.countryCalendar(34, []string{"Public", "BVB"}, market)
}
func (s *Session) Russia(market ...string) (*Calendar, error) {
	return s.countryCalendar(35, []string{"Settlement", "MOEX"}, market)
}
func (s *Session) SaudiArabia(market ...string) (*Calendar, error) {
	return s.countryCalendar(36, []string{"Tadawul"}, market)
}
func (s *Session) Serbia(market ...string) (*Calendar, error) {
	return s.countryCalendar(37, []string{"BSE"}, market)
}
func (s *Session) Singapore(market ...string) (*Calendar, error) {
	return s.countryCalendar(38, []string{"SGX"}, market)
}
func (s *Session) Slovakia(market ...string) (*Calendar, error) {
	return s.countryCalendar(39, []string{"BSSE"}, market)
}
func (s *Session) Slovenia(market ...string) (*Calendar, error) {
	return s.countryCalendar(40, []string{"LSE"}, market)
}
func (s *Session) SouthKorea(market ...string) (*Calendar, error) {
	return s.countryCalendar(41, []string{"Settlement", "KRX"}, market)
}
func (s *Session) Taiwan(market ...string) (*Calendar, error) {
	return s.countryCalendar(42, []string{"TSEC"}, market)
}
func (s *Session) Ukraine(market ...string) (*Calendar, error) {
	return s.countryCalendar(43, []string{"USE"}, market)
}
func (s *Session) UnitedKingdom(market ...string) (*Calendar, error) {
	return s.countryCalendar(44, []string{"Settlement", "Exchange", "Metals"}, market)
}
func (s *Session) UnitedStates(market ...string) (*Calendar, error) {
	return s.countryCalendar(45, []string{"Settlement", "NYSE", "GovernmentBond", "NERC", "LiborImpact", "FederalReserve", "SOFR"}, market)
}
func (s *Session) Uzbekistan(market ...string) (*Calendar, error) {
	return s.countryCalendar(46, []string{"UZSE"}, market)
}

func (s *Session) JointCalendar(calendars []*Calendar, rule ...string) (*Calendar, error) {
	if len(rule) > 1 {
		return nil, fmt.Errorf("at most one joint-calendar rule is accepted")
	}
	value := int32(0)
	if len(rule) == 1 {
		switch {
		case len(rule[0]) == len("JoinHolidays") && strings.EqualFold(rule[0], "JoinHolidays"):
		case len(rule[0]) == len("JoinBusinessDays") && strings.EqualFold(rule[0], "JoinBusinessDays"):
			value = 1
		default:
			return nil, fmt.Errorf("unknown joint-calendar rule, expected JoinHolidays, JoinBusinessDays")
		}
	}
	if len(calendars) == 0 {
		return nil, fmt.Errorf("a joint calendar needs at least one calendar")
	}
	ids := make([]C.uint64_t, len(calendars))
	for i, cal := range calendars {
		if cal == nil {
			return nil, fmt.Errorf("calendar required")
		}
		if err := sameSession(s, cal.object); err != nil {
			return nil, err
		}
		ids[i] = C.uint64_t(cal.id)
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_calendar_joint_new(s.ctx, &ids[0], C.size_t(len(ids)), C.int32_t(value), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &Calendar{object{s, uint64(id)}}, nil
}

func (c *Calendar) calendarQuery(date Date, query int32) (bool, error) {
	var out C.uint8_t
	err := c.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_calendar_query(c.session.ctx, C.uint64_t(c.id), C.int32_t(date.serial), C.int32_t(query), &out, &e), &e)
	})
	return out != 0, err
}
func (c *Calendar) IsBusinessDay(date Date) (bool, error) { return c.calendarQuery(date, 0) }
func (c *Calendar) IsHoliday(date Date) (bool, error)     { return c.calendarQuery(date, 1) }
func (c *Calendar) IsWeekend(date Date) (bool, error)     { return c.calendarQuery(date, 2) }

// BusinessDayCountOptions defaults to including the first date and excluding the last.
type BusinessDayCountOptions struct {
	IncludeFirst *bool
	IncludeLast  bool
}

func (c *Calendar) BusinessDaysBetween(from, to Date, options ...BusinessDayCountOptions) (int32, error) {
	if len(options) > 1 {
		return 0, fmt.Errorf("at most one business-day count option set is accepted")
	}
	first, last := true, false
	if len(options) == 1 {
		if options[0].IncludeFirst != nil {
			first = *options[0].IncludeFirst
		}
		last = options[0].IncludeLast
	}
	var out C.int32_t
	err := c.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_calendar_business_days_between(c.session.ctx, C.uint64_t(c.id), C.int32_t(from.serial), C.int32_t(to.serial), calendarBool(first), calendarBool(last), &out, &e), &e)
	})
	return int32(out), err
}

// HolidayList returns holidays inclusively; weekends are omitted by default.
func (c *Calendar) HolidayList(from, to Date, includeWeekends ...bool) ([]Date, error) {
	if len(includeWeekends) > 1 {
		return nil, fmt.Errorf("at most one weekend flag is accepted")
	}
	weekends := len(includeWeekends) == 1 && includeWeekends[0]
	var dates []Date
	err := c.session.invoke(func() error {
		var e C.ItofinError
		var size C.size_t
		call := func(out *C.int32_t, capacity C.size_t) error {
			return ffiError(C.itofin_calendar_holiday_list(c.session.ctx, C.uint64_t(c.id), C.int32_t(from.serial), C.int32_t(to.serial), calendarBool(weekends), out, capacity, &size, &e), &e)
		}
		if err := call(nil, 0); err != nil {
			return err
		}
		dates = make([]Date, int(size))
		if size == 0 {
			return nil
		}
		serials := make([]C.int32_t, int(size))
		if err := call(&serials[0], size); err != nil {
			return err
		}
		for i, value := range serials {
			dates[i] = Date{int32(value)}
		}
		return nil
	})
	return dates, err
}

func (c *Calendar) Equal(other *Calendar) (bool, error) {
	if other == nil {
		return false, nil
	}
	name, err := c.Name()
	if err != nil {
		return false, err
	}
	otherName, err := other.Name()
	return name == otherName, err
}

// Key returns the calendar name for semantic map keys, including across sessions.
func (c *Calendar) Key() (string, error) { return c.Name() }

func calendarBool(value bool) C.uint8_t {
	if value {
		return 1
	}
	return 0
}
