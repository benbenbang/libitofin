package itofin

/*
#include "itofin.h"
*/
import "C"
import "fmt"

type ConstantYoYOptionletVolatility struct{ object }
type YoYInflationCapFloorEngine struct{ object }
type YoYInflationOptionletCouponPricer struct{ object }
type ConstantYoYOptionletVolatilityConfig struct {
	Volatility           float64
	Quote                *SimpleQuote
	SettlementDays       uint32
	Calendar             *Calendar
	Convention           BusinessDayConvention
	DayCounter           *DayCounter
	ObservationLag       Period
	Frequency            Frequency
	IndexIsInterpolated  bool
	MinStrike, MaxStrike float64
	Settings             *Settings
}

func (s *Session) NewConstantYoYOptionletVolatility(a ConstantYoYOptionletVolatilityConfig) (*ConstantYoYOptionletVolatility, error) {
	if a.Calendar == nil || a.DayCounter == nil || a.Settings == nil {
		return nil, fmt.Errorf("calendar, day counter and settings required")
	}
	objects := []object{a.Calendar.object, a.DayCounter.object, a.Settings.object}
	var quote C.uint64_t
	var interp C.uint8_t
	if a.Quote != nil {
		objects = append(objects, a.Quote.object)
		quote = C.uint64_t(a.Quote.id)
	}
	if a.IndexIsInterpolated {
		interp = 1
	}
	if err := sameSession(s, objects...); err != nil {
		return nil, err
	}
	cfg := C.ItofinConstantYoYVolConfig{volatility: C.double(a.Volatility), quote: quote, settlement_days: C.uint32_t(a.SettlementDays), calendar: C.uint64_t(a.Calendar.id), convention: C.int32_t(a.Convention), day_counter: C.uint64_t(a.DayCounter.id), lag_length: C.int32_t(a.ObservationLag.Length), lag_unit: C.int32_t(a.ObservationLag.Unit), frequency: C.int32_t(a.Frequency), interpolated: interp, min_strike: C.double(a.MinStrike), max_strike: C.double(a.MaxStrike), settings: C.uint64_t(a.Settings.id)}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_constant_yoy_vol_new(s.ctx, cfg, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &ConstantYoYOptionletVolatility{object{s, uint64(id)}}, nil
}
func (v *ConstantYoYOptionletVolatility) metadata() (C.ItofinYoYVolMetadata, error) {
	var out C.ItofinYoYVolMetadata
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_constant_yoy_vol_metadata(v.session.ctx, C.uint64_t(v.id), &out, &e), &e)
	})
	return out, err
}
func (v *ConstantYoYOptionletVolatility) ObservationLag() (Period, error) {
	m, err := v.metadata()
	return Period{int32(m.lag_length), TimeUnit(m.lag_unit)}, err
}
func (v *ConstantYoYOptionletVolatility) Frequency() (Frequency, error) {
	m, err := v.metadata()
	return Frequency(m.frequency), err
}
func (v *ConstantYoYOptionletVolatility) IndexIsInterpolated() (bool, error) {
	m, err := v.metadata()
	return m.interpolated != 0, err
}
func (v *ConstantYoYOptionletVolatility) BaseDate() (Date, error) {
	var out C.int32_t
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_constant_yoy_vol_base_date(v.session.ctx, C.uint64_t(v.id), &out, &e), &e)
	})
	if err != nil {
		return Date{}, err
	}
	return DateFromSerial(int32(out))
}
func (v *ConstantYoYOptionletVolatility) value(d Date, strike float64, lag Period, variance bool) (float64, error) {
	var out C.double
	var flag C.uint8_t
	if variance {
		flag = 1
	}
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_constant_yoy_vol_value(v.session.ctx, C.uint64_t(v.id), C.int32_t(d.serial), C.double(strike), C.int32_t(lag.Length), C.int32_t(lag.Unit), flag, &out, &e), &e)
	})
	return float64(out), err
}
func (v *ConstantYoYOptionletVolatility) Volatility(d Date, strike float64, lag Period) (float64, error) {
	return v.value(d, strike, lag, false)
}
func (v *ConstantYoYOptionletVolatility) TotalVariance(d Date, strike float64, lag Period) (float64, error) {
	return v.value(d, strike, lag, true)
}
func (s *Session) newYoYEngine(index *YoYInflationIndex, vol *ConstantYoYOptionletVolatility, nominal *YieldTermStructure, distribution int32) (*YoYInflationCapFloorEngine, error) {
	if index == nil || vol == nil || nominal == nil {
		return nil, fmt.Errorf("index, volatility and nominal curve required")
	}
	if err := sameSession(s, index.object, vol.object, nominal.object); err != nil {
		return nil, err
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_yoy_capfloor_engine_new(s.ctx, C.uint64_t(index.id), C.uint64_t(vol.id), C.uint64_t(nominal.id), C.int32_t(distribution), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &YoYInflationCapFloorEngine{object{s, uint64(id)}}, nil
}
func (v *YoYInflationCapFloorEngine) Distribution() (string, error) {
	var out C.int32_t
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_yoy_capfloor_engine_distribution(v.session.ctx, C.uint64_t(v.id), &out, &e), &e)
	})
	if err != nil {
		return "", err
	}
	return []string{"black", "unit_displaced", "bachelier"}[int(out)], nil
}
func (s *Session) newYoYCouponPricer(vol *ConstantYoYOptionletVolatility, nominal *YieldTermStructure, distribution int32) (*YoYInflationOptionletCouponPricer, error) {
	if vol == nil {
		return nil, fmt.Errorf("volatility required")
	}
	objects := []object{vol.object}
	var curve C.uint64_t
	if nominal != nil {
		objects = append(objects, nominal.object)
		curve = C.uint64_t(nominal.id)
	}
	if err := sameSession(s, objects...); err != nil {
		return nil, err
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_yoy_coupon_pricer_new(s.ctx, C.uint64_t(vol.id), curve, C.int32_t(distribution), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &YoYInflationOptionletCouponPricer{object{s, uint64(id)}}, nil
}
func (s *Session) NewBlackYoYInflationCapFloorEngine(index *YoYInflationIndex, vol *ConstantYoYOptionletVolatility, nominal *YieldTermStructure) (*YoYInflationCapFloorEngine, error) {
	return s.newYoYEngine(index, vol, nominal, 0)
}
func (s *Session) NewBlackYoYInflationOptionletCouponPricer(vol *ConstantYoYOptionletVolatility, nominal *YieldTermStructure) (*YoYInflationOptionletCouponPricer, error) {
	return s.newYoYCouponPricer(vol, nominal, 0)
}
func (s *Session) NewUnitDisplacedYoYInflationCapFloorEngine(index *YoYInflationIndex, vol *ConstantYoYOptionletVolatility, nominal *YieldTermStructure) (*YoYInflationCapFloorEngine, error) {
	return s.newYoYEngine(index, vol, nominal, 1)
}
func (s *Session) NewUnitDisplacedYoYInflationOptionletCouponPricer(vol *ConstantYoYOptionletVolatility, nominal *YieldTermStructure) (*YoYInflationOptionletCouponPricer, error) {
	return s.newYoYCouponPricer(vol, nominal, 1)
}
func (s *Session) NewBachelierYoYInflationCapFloorEngine(index *YoYInflationIndex, vol *ConstantYoYOptionletVolatility, nominal *YieldTermStructure) (*YoYInflationCapFloorEngine, error) {
	return s.newYoYEngine(index, vol, nominal, 2)
}
func (s *Session) NewBachelierYoYInflationOptionletCouponPricer(vol *ConstantYoYOptionletVolatility, nominal *YieldTermStructure) (*YoYInflationOptionletCouponPricer, error) {
	return s.newYoYCouponPricer(vol, nominal, 2)
}
