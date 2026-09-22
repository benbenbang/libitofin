package itofin

/*
#include "itofin.h"
*/
import "C"
import (
	"fmt"
	"runtime"
)

type CapFloorTermVolCurve struct{ object }
type CapFloorTermVolCurveConfig struct {
	ReferenceDate  Date
	SettlementDays uint32
	Calendar       *Calendar
	Convention     BusinessDayConvention
	DayCounter     *DayCounter
	Settings       *Settings
	OptionTenors   []Period
	Quotes         []*SimpleQuote
}

func (s *Session) CapFloorTermVolCurve(x CapFloorTermVolCurveConfig) (*CapFloorTermVolCurve, error) {
	if x.Calendar == nil || x.DayCounter == nil || len(x.OptionTenors) == 0 || len(x.OptionTenors) != len(x.Quotes) {
		return nil, fmt.Errorf("calendar, day counter and matching nonempty tenors/quotes are required")
	}
	deps := []object{x.Calendar.object, x.DayCounter.object}
	cfg := C.ItofinCapFloorTermVolCurveConfig{reference_date: C.int32_t(x.ReferenceDate.serial), settlement_days: C.uint32_t(x.SettlementDays), calendar: C.uint64_t(x.Calendar.id), convention: C.int32_t(x.Convention), day_counter: C.uint64_t(x.DayCounter.id), count: C.size_t(len(x.OptionTenors))}
	if x.Settings != nil {
		deps = append(deps, x.Settings.object)
		cfg.settings = C.uint64_t(x.Settings.id)
	}
	lengths, units := make([]C.int32_t, len(x.OptionTenors)), make([]C.int32_t, len(x.OptionTenors))
	quotes := make([]C.uint64_t, len(x.Quotes))
	for i, p := range x.OptionTenors {
		if x.Quotes[i] == nil {
			return nil, fmt.Errorf("volatility quote is required")
		}
		deps = append(deps, x.Quotes[i].object)
		lengths[i] = C.int32_t(p.Length)
		units[i] = C.int32_t(p.Unit)
		quotes[i] = C.uint64_t(x.Quotes[i].id)
	}
	if err := sameSession(s, deps...); err != nil {
		return nil, err
	}
	cfg.tenor_lengths = &lengths[0]
	cfg.tenor_units = &units[0]
	cfg.quotes = &quotes[0]
	var id C.uint64_t
	err := s.invoke(func() error {
		var pin runtime.Pinner
		defer pin.Unpin()
		pin.Pin(&lengths[0])
		pin.Pin(&units[0])
		pin.Pin(&quotes[0])
		var e C.ItofinError
		return ffiError(C.itofin_capfloor_term_vol_curve_new(s.ctx, &cfg, &id, &e), &e)
	})
	runtime.KeepAlive(lengths)
	runtime.KeepAlive(units)
	runtime.KeepAlive(quotes)
	if err != nil {
		return nil, err
	}
	return &CapFloorTermVolCurve{object{s, uint64(id)}}, nil
}
func (v *CapFloorTermVolCurve) Volatility(tenor Period, extrapolate bool) (float64, error) {
	var out C.double
	var extra C.uint8_t
	if extrapolate {
		extra = 1
	}
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_capfloor_term_vol_curve_value(v.session.ctx, C.uint64_t(v.id), C.int32_t(tenor.Length), C.int32_t(tenor.Unit), extra, &out, &e), &e)
	})
	return float64(out), err
}
func (v *CapFloorTermVolCurve) OptionTimes() ([]float64, error) { return optionletValues(v.object, 0) }

type OptionletStripper2 struct{ object }

func (s *Session) OptionletStripper2(stripper *OptionletStripper1, curve *CapFloorTermVolCurve) (*OptionletStripper2, error) {
	if stripper == nil || curve == nil {
		return nil, fmt.Errorf("stripper and ATM curve are required")
	}
	if err := sameSession(s, stripper.object, curve.object); err != nil {
		return nil, err
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_optionlet_stripper2_new(s.ctx, C.uint64_t(stripper.id), C.uint64_t(curve.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &OptionletStripper2{object{s, uint64(id)}}, nil
}
func (v *OptionletStripper2) SpreadsVol() ([]float64, error) { return optionletValues(v.object, 1) }
func (v *OptionletStripper2) ATMCapFloorStrikes() ([]float64, error) {
	return optionletValues(v.object, 2)
}
func (v *OptionletStripper2) ATMCapFloorPrices() ([]float64, error) {
	return optionletValues(v.object, 3)
}
func optionletValues(v object, kind int32) ([]float64, error) {
	var result []float64
	err := v.session.invoke(func() error {
		var e C.ItofinError
		var n C.size_t
		if err := ffiError(C.itofin_optionlet_completion_values(v.session.ctx, C.uint64_t(v.id), C.int32_t(kind), nil, 0, &n, &e), &e); err != nil {
			return err
		}
		if uint64(n) > uint64(^uint(0)>>1) {
			return fmt.Errorf("optionlet values exceed Go slice range")
		}
		buf := make([]C.double, int(n))
		var ptr *C.double
		if len(buf) > 0 {
			ptr = &buf[0]
		}
		if err := ffiError(C.itofin_optionlet_completion_values(v.session.ctx, C.uint64_t(v.id), C.int32_t(kind), ptr, n, &n, &e), &e); err != nil {
			return err
		}
		result = make([]float64, len(buf))
		for i, x := range buf {
			result[i] = float64(x)
		}
		return nil
	})
	return result, err
}

type OptionletSmileSection struct{ object }

func (v *OptionletVolatilityStructure) smile(kind int32, time float64, date Date, tenor Period, extrapolate bool) (*OptionletSmileSection, error) {
	var id C.uint64_t
	var extra C.uint8_t
	if extrapolate {
		extra = 1
	}
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_optionlet_smile_new(v.session.ctx, C.uint64_t(v.id), C.int32_t(kind), C.double(time), C.int32_t(date.serial), C.int32_t(tenor.Length), C.int32_t(tenor.Unit), extra, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &OptionletSmileSection{object{v.session, uint64(id)}}, nil
}
func (v *OptionletVolatilityStructure) SmileSection(time float64, extrapolate bool) (*OptionletSmileSection, error) {
	return v.smile(0, time, Date{}, Period{}, extrapolate)
}
func (v *OptionletVolatilityStructure) SmileSectionDate(date Date, extrapolate bool) (*OptionletSmileSection, error) {
	return v.smile(1, 0, date, Period{}, extrapolate)
}
func (v *OptionletVolatilityStructure) SmileSectionTenor(tenor Period, extrapolate bool) (*OptionletSmileSection, error) {
	return v.smile(2, 0, Date{}, tenor, extrapolate)
}
func (v *OptionletSmileSection) query(kind int32, strike float64) (float64, error) {
	var out C.double
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_optionlet_smile_value(v.session.ctx, C.uint64_t(v.id), C.int32_t(kind), C.double(strike), &out, &e), &e)
	})
	return float64(out), err
}
func (v *OptionletSmileSection) Volatility(strike float64) (float64, error) {
	return v.query(0, strike)
}
func (v *OptionletSmileSection) Variance(strike float64) (float64, error) { return v.query(1, strike) }
func (v *OptionletSmileSection) ExerciseTime() (float64, error)           { return v.query(2, 0) }
