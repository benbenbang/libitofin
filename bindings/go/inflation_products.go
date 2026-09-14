package itofin

/*
#include "itofin.h"
*/
import "C"
import "fmt"

type DiscountingSwapEngine struct{ object }
type ZeroCouponInflationSwap struct{ object }
type YearOnYearInflationSwap struct{ object }

func (s *Session) NewDiscountingSwapEngine(discount *YieldTermStructure, settings *Settings) (*DiscountingSwapEngine, error) {
	if discount == nil || settings == nil {
		return nil, fmt.Errorf("discount and settings required")
	}
	if err := sameSession(s, discount.object, settings.object); err != nil {
		return nil, err
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_discounting_swap_engine_new(s.ctx, C.uint64_t(discount.id), C.uint64_t(settings.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &DiscountingSwapEngine{object{s, uint64(id)}}, nil
}

type ZeroCouponInflationSwapConfig struct {
	Type                SwapType
	Nominal             float64
	Start, Maturity     Date
	FixedCalendar       *Calendar
	FixedConvention     BusinessDayConvention
	DayCounter          *DayCounter
	FixedRate           float64
	Index               *ZeroInflationIndex
	ObservationLag      Period
	Interpolation       CpiInterpolationType
	InflationCalendar   *Calendar
	InflationConvention *BusinessDayConvention
	Settings            *Settings
}

func (s *Session) NewZeroCouponInflationSwap(a ZeroCouponInflationSwapConfig) (*ZeroCouponInflationSwap, error) {
	if a.FixedCalendar == nil || a.DayCounter == nil || a.Index == nil || a.Settings == nil {
		return nil, fmt.Errorf("calendar, day counter, index and settings required")
	}
	objects := []object{a.FixedCalendar.object, a.DayCounter.object, a.Index.object, a.Settings.object}
	var inflationCal C.uint64_t
	if a.InflationCalendar != nil {
		objects = append(objects, a.InflationCalendar.object)
		inflationCal = C.uint64_t(a.InflationCalendar.id)
	}
	if err := sameSession(s, objects...); err != nil {
		return nil, err
	}
	inflationConv := C.int32_t(-1)
	if a.InflationConvention != nil {
		inflationConv = C.int32_t(*a.InflationConvention)
	}
	cfg := C.ItofinZeroInflationSwapConfig{swap_type: C.int32_t(a.Type), nominal: C.double(a.Nominal), start: C.int32_t(a.Start.serial), maturity: C.int32_t(a.Maturity.serial), fixed_calendar: C.uint64_t(a.FixedCalendar.id), fixed_convention: C.int32_t(a.FixedConvention), day_counter: C.uint64_t(a.DayCounter.id), fixed_rate: C.double(a.FixedRate), index: C.uint64_t(a.Index.id), lag_length: C.int32_t(a.ObservationLag.Length), lag_unit: C.int32_t(a.ObservationLag.Unit), interpolation: C.int32_t(a.Interpolation), inflation_calendar: inflationCal, inflation_convention: inflationConv, settings: C.uint64_t(a.Settings.id)}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_zero_inflation_swap_new(s.ctx, cfg, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &ZeroCouponInflationSwap{object{s, uint64(id)}}, nil
}

type YearOnYearInflationSwapConfig struct {
	Type              SwapType
	Nominal           float64
	FixedSchedule     *Schedule
	FixedRate         float64
	FixedDayCounter   *DayCounter
	YoYSchedule       *Schedule
	Index             *YoYInflationIndex
	ObservationLag    Period
	Interpolation     CpiInterpolationType
	Spread            float64
	YoYDayCounter     *DayCounter
	PaymentCalendar   *Calendar
	PaymentConvention BusinessDayConvention
	Settings          *Settings
}

func (s *Session) NewYearOnYearInflationSwap(a YearOnYearInflationSwapConfig) (*YearOnYearInflationSwap, error) {
	if a.FixedSchedule == nil || a.FixedDayCounter == nil || a.YoYSchedule == nil || a.Index == nil || a.YoYDayCounter == nil || a.PaymentCalendar == nil || a.Settings == nil {
		return nil, fmt.Errorf("schedules, day counters, index, calendar and settings required")
	}
	if err := sameSession(s, a.FixedSchedule.object, a.FixedDayCounter.object, a.YoYSchedule.object, a.Index.object, a.YoYDayCounter.object, a.PaymentCalendar.object, a.Settings.object); err != nil {
		return nil, err
	}
	cfg := C.ItofinYoYInflationSwapConfig{swap_type: C.int32_t(a.Type), nominal: C.double(a.Nominal), fixed_schedule: C.uint64_t(a.FixedSchedule.id), fixed_rate: C.double(a.FixedRate), fixed_day_counter: C.uint64_t(a.FixedDayCounter.id), yoy_schedule: C.uint64_t(a.YoYSchedule.id), index: C.uint64_t(a.Index.id), lag_length: C.int32_t(a.ObservationLag.Length), lag_unit: C.int32_t(a.ObservationLag.Unit), interpolation: C.int32_t(a.Interpolation), spread: C.double(a.Spread), yoy_day_counter: C.uint64_t(a.YoYDayCounter.id), payment_calendar: C.uint64_t(a.PaymentCalendar.id), payment_convention: C.int32_t(a.PaymentConvention), settings: C.uint64_t(a.Settings.id)}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_yoy_inflation_swap_new(s.ctx, cfg, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &YearOnYearInflationSwap{object{s, uint64(id)}}, nil
}
func (v *ZeroCouponInflationSwap) SetEngine(engine *DiscountingSwapEngine) error {
	if engine == nil {
		return fmt.Errorf("engine required")
	}
	if err := sameSession(v.session, engine.object); err != nil {
		return err
	}
	return v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_zero_inflation_swap_set_engine(v.session.ctx, C.uint64_t(v.id), C.uint64_t(engine.id), &e), &e)
	})
}
func (v *ZeroCouponInflationSwap) Calculate() error {
	return v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_zero_inflation_swap_calculate(v.session.ctx, C.uint64_t(v.id), &e), &e)
	})
}
func (v *ZeroCouponInflationSwap) IsCalculated() (bool, error) {
	var out C.uint8_t
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_zero_inflation_swap_is_calculated(v.session.ctx, C.uint64_t(v.id), &out, &e), &e)
	})
	return out != 0, err
}
func (v *ZeroCouponInflationSwap) Price(engine *DiscountingSwapEngine) (float64, error) {
	if engine == nil {
		return 0, fmt.Errorf("engine required")
	}
	if err := sameSession(v.session, engine.object); err != nil {
		return 0, err
	}
	var out C.double
	err := v.session.invoke(func() error {
		var e C.ItofinError
		if err := ffiError(C.itofin_zero_inflation_swap_set_engine(v.session.ctx, C.uint64_t(v.id), C.uint64_t(engine.id), &e), &e); err != nil {
			return err
		}
		return ffiError(C.itofin_zero_inflation_swap_npv(v.session.ctx, C.uint64_t(v.id), &out, &e), &e)
	})
	return float64(out), err
}
func (v *ZeroCouponInflationSwap) Results() (*Results, error) {
	var result *Results
	err := v.session.invoke(func() error {
		var id C.uint64_t
		var e C.ItofinError
		if err := ffiError(C.itofin_zero_inflation_swap_results(v.session.ctx, C.uint64_t(v.id), &id, &e), &e); err != nil {
			return err
		}
		var err error
		result, err = v.session.readResults(uint64(id))
		return err
	})
	return result, err
}
func (v *ZeroCouponInflationSwap) NPV() (float64, error) {
	var out C.double
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_zero_inflation_swap_npv(v.session.ctx, C.uint64_t(v.id), &out, &e), &e)
	})
	return float64(out), err
}
func (v *ZeroCouponInflationSwap) FairRate() (float64, error) {
	var out C.double
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_zero_inflation_swap_fair_rate(v.session.ctx, C.uint64_t(v.id), &out, &e), &e)
	})
	return float64(out), err
}
func (v *ZeroCouponInflationSwap) FixedLegNPV() (float64, error) {
	var out C.double
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_zero_inflation_swap_fixed_leg_npv(v.session.ctx, C.uint64_t(v.id), &out, &e), &e)
	})
	return float64(out), err
}
func (v *ZeroCouponInflationSwap) InflationLegNPV() (float64, error) {
	var out C.double
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_zero_inflation_swap_inflation_leg_npv(v.session.ctx, C.uint64_t(v.id), &out, &e), &e)
	})
	return float64(out), err
}
func (v *ZeroCouponInflationSwap) FixedLegBPS() (float64, error) {
	var out C.double
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_zero_inflation_swap_fixed_leg_bps(v.session.ctx, C.uint64_t(v.id), &out, &e), &e)
	})
	return float64(out), err
}
func (v *ZeroCouponInflationSwap) MaturityDate() (Date, error) {
	var out C.int32_t
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_zero_inflation_swap_maturity_date(v.session.ctx, C.uint64_t(v.id), &out, &e), &e)
	})
	if err != nil {
		return Date{}, err
	}
	return DateFromSerial(int32(out))
}
func (v *ZeroCouponInflationSwap) ObsDate() (Date, error) {
	var out C.int32_t
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_zero_inflation_swap_obs_date(v.session.ctx, C.uint64_t(v.id), &out, &e), &e)
	})
	if err != nil {
		return Date{}, err
	}
	return DateFromSerial(int32(out))
}
func (v *ZeroCouponInflationSwap) InflationFixingDate() (Date, error) {
	var out C.int32_t
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_zero_inflation_swap_inflation_fixing_date(v.session.ctx, C.uint64_t(v.id), &out, &e), &e)
	})
	if err != nil {
		return Date{}, err
	}
	return DateFromSerial(int32(out))
}
func (v *YearOnYearInflationSwap) SetEngine(engine *DiscountingSwapEngine) error {
	if engine == nil {
		return fmt.Errorf("engine required")
	}
	if err := sameSession(v.session, engine.object); err != nil {
		return err
	}
	return v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_yoy_inflation_swap_set_engine(v.session.ctx, C.uint64_t(v.id), C.uint64_t(engine.id), &e), &e)
	})
}
func (v *YearOnYearInflationSwap) Calculate() error {
	return v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_yoy_inflation_swap_calculate(v.session.ctx, C.uint64_t(v.id), &e), &e)
	})
}
func (v *YearOnYearInflationSwap) IsCalculated() (bool, error) {
	var out C.uint8_t
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_yoy_inflation_swap_is_calculated(v.session.ctx, C.uint64_t(v.id), &out, &e), &e)
	})
	return out != 0, err
}
func (v *YearOnYearInflationSwap) Price(engine *DiscountingSwapEngine) (float64, error) {
	if engine == nil {
		return 0, fmt.Errorf("engine required")
	}
	if err := sameSession(v.session, engine.object); err != nil {
		return 0, err
	}
	var out C.double
	err := v.session.invoke(func() error {
		var e C.ItofinError
		if err := ffiError(C.itofin_yoy_inflation_swap_set_engine(v.session.ctx, C.uint64_t(v.id), C.uint64_t(engine.id), &e), &e); err != nil {
			return err
		}
		return ffiError(C.itofin_yoy_inflation_swap_npv(v.session.ctx, C.uint64_t(v.id), &out, &e), &e)
	})
	return float64(out), err
}
func (v *YearOnYearInflationSwap) Results() (*Results, error) {
	var result *Results
	err := v.session.invoke(func() error {
		var id C.uint64_t
		var e C.ItofinError
		if err := ffiError(C.itofin_yoy_inflation_swap_results(v.session.ctx, C.uint64_t(v.id), &id, &e), &e); err != nil {
			return err
		}
		var err error
		result, err = v.session.readResults(uint64(id))
		return err
	})
	return result, err
}
func (v *YearOnYearInflationSwap) NPV() (float64, error) {
	var out C.double
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_yoy_inflation_swap_npv(v.session.ctx, C.uint64_t(v.id), &out, &e), &e)
	})
	return float64(out), err
}
func (v *YearOnYearInflationSwap) FairRate() (float64, error) {
	var out C.double
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_yoy_inflation_swap_fair_rate(v.session.ctx, C.uint64_t(v.id), &out, &e), &e)
	})
	return float64(out), err
}
func (v *YearOnYearInflationSwap) FairSpread() (float64, error) {
	var out C.double
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_yoy_inflation_swap_fair_spread(v.session.ctx, C.uint64_t(v.id), &out, &e), &e)
	})
	return float64(out), err
}
func (v *YearOnYearInflationSwap) FixedLegNPV() (float64, error) {
	var out C.double
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_yoy_inflation_swap_fixed_leg_npv(v.session.ctx, C.uint64_t(v.id), &out, &e), &e)
	})
	return float64(out), err
}
func (v *YearOnYearInflationSwap) YoYLegNPV() (float64, error) {
	var out C.double
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_yoy_inflation_swap_yoy_leg_npv(v.session.ctx, C.uint64_t(v.id), &out, &e), &e)
	})
	return float64(out), err
}
func (v *YearOnYearInflationSwap) FixedRate() (float64, error) {
	var out C.double
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_yoy_inflation_swap_fixed_rate(v.session.ctx, C.uint64_t(v.id), &out, &e), &e)
	})
	return float64(out), err
}
func (v *YearOnYearInflationSwap) Spread() (float64, error) {
	var out C.double
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_yoy_inflation_swap_spread(v.session.ctx, C.uint64_t(v.id), &out, &e), &e)
	})
	return float64(out), err
}
