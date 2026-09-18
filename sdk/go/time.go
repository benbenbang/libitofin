package itofin

/*
#include "itofin.h"
*/
import "C"
import (
	"fmt"
	"unsafe"
)

// Date is an immutable, comparable calendar day. Its zero value is invalid.
// Dates can be used directly as map keys.
type Date struct{ serial int32 }

func (d Date) Serial() int32 { return d.serial }
func DateFromSerial(serial int32) (Date, error) {
	var out C.ItofinDateParts
	var e C.ItofinError
	err := ffiError(C.itofin_date_parts(C.int32_t(serial), &out, &e), &e)
	if err != nil {
		return Date{}, err
	}
	return Date{serial}, nil
}
func NewDate(day, month, year int) (Date, error) {
	if day < 1 || day > 31 || month < 1 || month > 12 || year < 1901 || year > 2199 {
		return Date{}, fmt.Errorf("invalid date components")
	}
	var out C.int32_t
	var e C.ItofinError
	err := ffiError(C.itofin_date_new(C.int32_t(day), C.int32_t(month), C.int32_t(year), &out, &e), &e)
	if err != nil {
		return Date{}, err
	}
	return Date{int32(out)}, nil
}
func (d Date) parts() C.ItofinDateParts {
	var out C.ItofinDateParts
	var e C.ItofinError
	C.itofin_date_parts(C.int32_t(d.serial), &out, &e)
	return out
}
func (d Date) Day() int   { return int(d.parts().day) }
func (d Date) Month() int { return int(d.parts().month) }
func (d Date) Year() int  { return int(d.parts().year) }
func (d Date) String() string {
	p := d.parts()
	return fmt.Sprintf("Date(%d, %d, %d)", p.day, p.month, p.year)
}
func (d Date) AddDays(days int64) (Date, error) {
	var out C.int32_t
	var e C.ItofinError
	err := ffiError(C.itofin_date_shift(C.int32_t(d.serial), C.int64_t(days), &out, &e), &e)
	if err != nil {
		return Date{}, err
	}
	return Date{int32(out)}, nil
}
func (d Date) SubtractDays(days int64) (Date, error) {
	if days == -1<<63 {
		return Date{}, fmt.Errorf("date shift overflow")
	}
	return d.AddDays(-days)
}
func (d Date) DaysSince(other Date) int { return int(d.serial - other.serial) }
func IsIMMDate(d Date, mainCycle bool) (bool, error) {
	var out C.uint8_t
	var e C.ItofinError
	var main C.uint8_t
	if mainCycle {
		main = 1
	}
	err := ffiError(C.itofin_is_imm_date(C.int32_t(d.serial), main, &out, &e), &e)
	return out != 0, err
}
func NextIMMDate(d Date, mainCycle bool) (Date, error) {
	var out C.int32_t
	var e C.ItofinError
	var main C.uint8_t
	if mainCycle {
		main = 1
	}
	err := ffiError(C.itofin_next_imm_date(C.int32_t(d.serial), main, &out, &e), &e)
	if err != nil {
		return Date{}, err
	}
	return Date{int32(out)}, nil
}

type TimeUnit int32

const (
	Days TimeUnit = iota
	Weeks
	Months
	Years
)

func (u TimeUnit) String() string {
	switch u {
	case Days:
		return "Days"
	case Weeks:
		return "Weeks"
	case Months:
		return "Months"
	case Years:
		return "Years"
	}
	return "InvalidTimeUnit"
}

type Period struct {
	Length int32
	Unit   TimeUnit
}

func NewPeriod(n int32, unit TimeUnit) (Period, error) {
	if unit < Days || unit > Years {
		return Period{}, fmt.Errorf("unknown time unit")
	}
	return Period{n, unit}, nil
}

// Key is the canonical comparable form; use it as a map key for semantic equality.
func (p Period) Key() Period {
	if p.Length == 0 {
		return Period{0, Days}
	}
	if p.Unit == Days && p.Length%7 == 0 {
		return Period{p.Length / 7, Weeks}
	}
	if p.Unit == Months && p.Length%12 == 0 {
		return Period{p.Length / 12, Years}
	}
	return p
}
func (p Period) Equal(other Period) bool { return p.Key() == other.Key() }
func (p Period) String() string          { return fmt.Sprintf("Period(%d, %s)", p.Length, p.Unit) }

type Frequency int32

const (
	Annual Frequency = iota
	Semiannual
	Quarterly
	Monthly
)

type BusinessDayConvention int32

const (
	ModifiedFollowing BusinessDayConvention = iota
	Following
	Unadjusted
	Preceding
	ModifiedPreceding
	HalfMonthModifiedFollowing
	Nearest
)

type DateGeneration int32

const (
	Backward DateGeneration = iota
	Forward
	Zero
	ThirdWednesday
	ThirdWednesdayInclusive
	Twentieth
	TwentiethIMM
	OldCDS
	CDS
	CDS2015
)

type DayCounter struct{ object }
type Calendar struct{ object }
type Schedule struct{ object }

func (s *Session) newDayCounter(kind int32) (*DayCounter, error) {
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_day_counter_new(s.ctx, C.int32_t(kind), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &DayCounter{object{s, uint64(id)}}, nil
}
func (s *Session) Actual360() (*DayCounter, error)          { return s.newDayCounter(0) }
func (s *Session) Actual365Fixed() (*DayCounter, error)     { return s.newDayCounter(1) }
func (s *Session) ActualActualISDA() (*DayCounter, error)   { return s.newDayCounter(2) }
func (s *Session) Thirty360BondBasis() (*DayCounter, error) { return s.newDayCounter(3) }
func (d *DayCounter) YearFraction(start, end Date) (float64, error) {
	var out C.double
	err := d.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_day_counter_year_fraction(d.session.ctx, C.uint64_t(d.id), C.int32_t(start.serial), C.int32_t(end.serial), &out, &e), &e)
	})
	return float64(out), err
}
func (o object) timeName(kind int32) (string, error) {
	var name string
	err := o.session.invoke(func() error {
		var e C.ItofinError
		var size C.size_t
		if err := ffiError(C.itofin_time_name(o.session.ctx, C.uint64_t(o.id), C.int32_t(kind), nil, 0, &size, &e), &e); err != nil {
			return err
		}
		if size == 0 {
			return nil
		}
		buffer := make([]byte, int(size))
		if err := ffiError(C.itofin_time_name(o.session.ctx, C.uint64_t(o.id), C.int32_t(kind), (*C.uint8_t)(unsafe.Pointer(&buffer[0])), size, &size, &e), &e); err != nil {
			return err
		}
		name = string(buffer)
		return nil
	})
	return name, err
}
func (d *DayCounter) Name() (string, error) { return d.object.timeName(0) }
func (d *DayCounter) Equal(other *DayCounter) (bool, error) {
	if other == nil {
		return false, nil
	}
	a, err := d.Name()
	if err != nil {
		return false, err
	}
	b, err := other.Name()
	return a == b, err
}
func (s *Session) newCalendar(kind int32) (*Calendar, error) {
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_calendar_new(s.ctx, C.int32_t(kind), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &Calendar{object{s, uint64(id)}}, nil
}
func (s *Session) Target() (*Calendar, error)       { return s.newCalendar(0) }
func (s *Session) NullCalendar() (*Calendar, error) { return s.newCalendar(1) }
func (s *Session) WeekendsOnly() (*Calendar, error) { return s.newCalendar(2) }
func (c *Calendar) Name() (string, error)           { return c.object.timeName(1) }
func (c *Calendar) Adjust(d Date, rule BusinessDayConvention) (Date, error) {
	var out C.int32_t
	err := c.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_calendar_adjust(c.session.ctx, C.uint64_t(c.id), C.int32_t(d.serial), C.int32_t(rule), &out, &e), &e)
	})
	if err != nil {
		return Date{}, err
	}
	return Date{int32(out)}, nil
}
func (c *Calendar) Advance(d Date, n int32, unit TimeUnit, rule BusinessDayConvention, endOfMonth bool) (Date, error) {
	var out C.int32_t
	var eom C.uint8_t
	if endOfMonth {
		eom = 1
	}
	err := c.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_calendar_advance(c.session.ctx, C.uint64_t(c.id), C.int32_t(d.serial), C.int32_t(n), C.int32_t(unit), C.int32_t(rule), eom, &out, &e), &e)
	})
	if err != nil {
		return Date{}, err
	}
	return Date{int32(out)}, nil
}

type ScheduleConfig struct {
	Start, End Date
	Frequency  Frequency
	Calendar   *Calendar
	Convention BusinessDayConvention
	// Nil selects Forward; a pointer permits an explicit Backward rule.
	Rule                  *DateGeneration
	TerminationConvention *BusinessDayConvention
}

func (s *Session) NewSchedule(cfg ScheduleConfig) (*Schedule, error) {
	if cfg.Calendar == nil {
		return nil, fmt.Errorf("calendar required")
	}
	if err := sameSession(s, cfg.Calendar.object); err != nil {
		return nil, err
	}
	rule := Forward
	if cfg.Rule != nil {
		rule = *cfg.Rule
	}
	term := C.int32_t(-1)
	if cfg.TerminationConvention != nil {
		term = C.int32_t(*cfg.TerminationConvention)
	}
	native := C.ItofinScheduleConfig{start: C.int32_t(cfg.Start.serial), end: C.int32_t(cfg.End.serial), frequency: C.int32_t(cfg.Frequency), calendar: C.uint64_t(cfg.Calendar.id), convention: C.int32_t(cfg.Convention), rule: C.int32_t(rule), termination_convention: term}
	var id C.uint64_t
	err := s.invoke(func() error { var e C.ItofinError; return ffiError(C.itofin_schedule_new(s.ctx, native, &id, &e), &e) })
	if err != nil {
		return nil, err
	}
	return &Schedule{object{s, uint64(id)}}, nil
}
func (s *Schedule) Dates() ([]Date, error) {
	var dates []Date
	err := s.session.invoke(func() error {
		var e C.ItofinError
		var size C.size_t
		if err := ffiError(C.itofin_schedule_dates(s.session.ctx, C.uint64_t(s.id), nil, 0, &size, &e), &e); err != nil {
			return err
		}
		dates = make([]Date, int(size))
		if size == 0 {
			return nil
		}
		serials := make([]C.int32_t, int(size))
		if err := ffiError(C.itofin_schedule_dates(s.session.ctx, C.uint64_t(s.id), &serials[0], size, &size, &e), &e); err != nil {
			return err
		}
		for i, v := range serials {
			dates[i] = Date{int32(v)}
		}
		return nil
	})
	return dates, err
}
func (s *Schedule) Size() (int, error) { dates, err := s.Dates(); return len(dates), err }
func (s *Schedule) Date(index int) (Date, error) {
	dates, err := s.Dates()
	if err != nil {
		return Date{}, err
	}
	if index < 0 || index >= len(dates) {
		return Date{}, fmt.Errorf("schedule index out of range")
	}
	return dates[index], nil
}

// Key returns the convention name for semantic map keys, including independently
// constructed day counters and counters belonging to separate sessions.
func (d *DayCounter) Key() (string, error) { return d.Name() }
func (d *DayCounter) Repr() (string, error) {
	name, err := d.Name()
	if err != nil {
		return "", err
	}
	return "DayCounter(" + name + ")", nil
}
func (c *Calendar) Repr() (string, error) {
	name, err := c.Name()
	if err != nil {
		return "", err
	}
	return "Calendar(" + name + ")", nil
}
