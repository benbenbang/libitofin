package itofin

/*
#include "itofin.h"
*/
import "C"
import "fmt"

// ConstantRateVolConfig supports fixed/moving dates and scalar/live-quote market data.
// A nonnil Settings selects a moving date; Quote selects live volatility.
type ConstantRateVolConfig struct {
	ReferenceDate  Date
	SettlementDays uint32
	Calendar       *Calendar
	Convention     BusinessDayConvention
	Volatility     float64
	DayCounter     *DayCounter
	VolatilityType VolatilityType
	Shift          float64
	Quote          *SimpleQuote
	Settings       *Settings
}

func (s *Session) rateVolConfig(x ConstantRateVolConfig) (C.ItofinConstantRateVolConfig, error) {
	var cfg C.ItofinConstantRateVolConfig
	if x.Calendar == nil || x.DayCounter == nil {
		return cfg, fmt.Errorf("calendar and day counter are required")
	}
	deps := []object{x.Calendar.object, x.DayCounter.object}
	cfg.reference_date = C.int32_t(x.ReferenceDate.serial)
	cfg.settlement_days = C.uint32_t(x.SettlementDays)
	cfg.calendar = C.uint64_t(x.Calendar.id)
	cfg.convention = C.int32_t(x.Convention)
	cfg.volatility = C.double(x.Volatility)
	cfg.day_counter = C.uint64_t(x.DayCounter.id)
	cfg.volatility_type = C.int32_t(x.VolatilityType)
	cfg.shift = C.double(x.Shift)
	if x.Quote != nil {
		deps = append(deps, x.Quote.object)
		cfg.quote = C.uint64_t(x.Quote.id)
	}
	if x.Settings != nil {
		deps = append(deps, x.Settings.object)
		cfg.settings = C.uint64_t(x.Settings.id)
	}
	return cfg, sameSession(s, deps...)
}

type SwaptionVolatilityStructure struct{ object }

func (s *Session) ConstantSwaptionVolatility(x ConstantRateVolConfig) (*SwaptionVolatilityStructure, error) {
	cfg, err := s.rateVolConfig(x)
	if err != nil {
		return nil, err
	}
	var id C.uint64_t
	err = s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_constant_swaption_vol_new(s.ctx, &cfg, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &SwaptionVolatilityStructure{object{s, uint64(id)}}, nil
}
func (v *SwaptionVolatilityStructure) control(action int32) (int32, error) {
	var out C.int32_t
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_rate_vol_control(v.session.ctx, C.uint64_t(v.id), 0, C.int32_t(action), &out, &e), &e)
	})
	return int32(out), err
}
func (v *SwaptionVolatilityStructure) ReferenceDate() (Date, error) {
	d, err := v.control(0)
	return Date{serial: d}, err
}

type OptionletVolatilityStructure struct{ object }

func (s *Session) ConstantOptionletVolatility(x ConstantRateVolConfig) (*OptionletVolatilityStructure, error) {
	cfg, err := s.rateVolConfig(x)
	if err != nil {
		return nil, err
	}
	var id C.uint64_t
	err = s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_constant_optionlet_vol_new(s.ctx, &cfg, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &OptionletVolatilityStructure{object{s, uint64(id)}}, nil
}
func (v *OptionletVolatilityStructure) control(action int32) (int32, error) {
	var out C.int32_t
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_rate_vol_control(v.session.ctx, C.uint64_t(v.id), 1, C.int32_t(action), &out, &e), &e)
	})
	return int32(out), err
}
func (v *OptionletVolatilityStructure) ReferenceDate() (Date, error) {
	d, err := v.control(0)
	return Date{serial: d}, err
}
func (v *SwaptionVolatilityStructure) query(kind int32, option, swap Period, date Date, length, strike float64, extrapolate bool) (float64, error) {
	var out C.double
	var ext C.int32_t
	if extrapolate {
		ext = 1
	}
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_swaption_vol_query(v.session.ctx, C.uint64_t(v.id), C.int32_t(kind), C.int32_t(option.Length), C.int32_t(option.Unit), C.int32_t(swap.Length), C.int32_t(swap.Unit), C.int32_t(date.serial), C.double(length), C.double(strike), ext, &out, &e), &e)
	})
	return float64(out), err
}
func (v *SwaptionVolatilityStructure) Volatility(option, swap Period, strike float64, extrapolate bool) (float64, error) {
	return v.query(0, option, swap, Date{}, 0, strike, extrapolate)
}

// VolatilityDate queries a fixed exercise date and swap length in years.
func (v *SwaptionVolatilityStructure) VolatilityDate(date Date, swapLength, strike float64, extrapolate bool) (float64, error) {
	return v.query(3, Period{}, Period{}, date, swapLength, strike, extrapolate)
}
func (v *SwaptionVolatilityStructure) BlackVariance(option, swap Period, strike float64, extrapolate bool) (float64, error) {
	return v.query(1, option, swap, Date{}, 0, strike, extrapolate)
}
func (v *SwaptionVolatilityStructure) Shift(date Date, swapLength float64, extrapolate bool) (float64, error) {
	return v.query(2, Period{}, Period{}, date, swapLength, 0, extrapolate)
}
func (v *OptionletVolatilityStructure) query(kind int32, option Period, date Date, strike float64, extrapolate bool) (float64, error) {
	var out C.double
	var ext C.int32_t
	if extrapolate {
		ext = 1
	}
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_optionlet_vol_query(v.session.ctx, C.uint64_t(v.id), C.int32_t(kind), C.int32_t(option.Length), C.int32_t(option.Unit), C.int32_t(date.serial), C.double(strike), ext, &out, &e), &e)
	})
	return float64(out), err
}
func (v *OptionletVolatilityStructure) Volatility(option Period, strike float64, extrapolate bool) (float64, error) {
	return v.query(0, option, Date{}, strike, extrapolate)
}
func (v *OptionletVolatilityStructure) BlackVariance(option Period, strike float64, extrapolate bool) (float64, error) {
	return v.query(1, option, Date{}, strike, extrapolate)
}
func (v *OptionletVolatilityStructure) VolatilityDate(date Date, strike float64, extrapolate bool) (float64, error) {
	return v.query(2, Period{}, date, strike, extrapolate)
}
func (v *OptionletVolatilityStructure) Displacement() (float64, error) {
	return v.query(3, Period{}, Date{}, 0, false)
}
func (v *OptionletVolatilityStructure) AllowsExtrapolation() (bool, error) {
	x, err := v.control(1)
	return x != 0, err
}
func (v *OptionletVolatilityStructure) EnableExtrapolation() error {
	_, err := v.control(2)
	return err
}
func (v *OptionletVolatilityStructure) DisableExtrapolation() error {
	_, err := v.control(3)
	return err
}
