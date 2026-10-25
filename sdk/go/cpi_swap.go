package itofin

/*
#include "itofin.h"
*/
import "C"

// CPISwap exchanges CPI-scaled fixed coupons against plain Ibor coupons.
// Payer pays Ibor and receives CPI. Receiver reverses both signs.
type CPISwap struct{ object }

// CPISwapConfig defines constant-notional coupon legs and terminal exchanges.
// InflationNominal nil uses Nominal. SubtractInflationNominal subtracts the CPI
// nominal from both terminal exchanges. There is no initial exchange.
// Both schedules require at least two dates. Ibor fixing days use the index default.
type CPISwapConfig struct {
	Type                     SwapType
	Nominal                  float64
	InflationNominal         *float64
	SubtractInflationNominal bool
	Spread                   float64
	FloatSchedule            *Schedule
	FloatIndex               *IborIndex
	FloatDayCounter          *DayCounter
	FloatConvention          BusinessDayConvention
	FixedRate, BaseCPI       float64
	FixedSchedule            *Schedule
	FixedIndex               *ZeroInflationIndex
	FixedDayCounter          *DayCounter
	FixedConvention          BusinessDayConvention
	ObservationLag           Period
	Interpolation            CpiInterpolationType
	Settings                 *Settings
}

// NewCPISwap retains live indexes, curves and Settings through the native instrument.
func (s *Session) NewCPISwap(a CPISwapConfig) (*CPISwap, error) {
	if a.FloatSchedule == nil || a.FloatIndex == nil || a.FloatDayCounter == nil || a.FixedSchedule == nil || a.FixedIndex == nil || a.FixedDayCounter == nil || a.Settings == nil {
		return nil, errNilArgument("CPI swap argument")
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		if err := sameSession(s, a.FloatSchedule.object, a.FloatIndex.object, a.FloatDayCounter.object, a.FixedSchedule.object, a.FixedIndex.object, a.FixedDayCounter.object, a.Settings.object); err != nil {
			return err
		}
		nominal := a.Nominal
		if a.InflationNominal != nil {
			nominal = *a.InflationNominal
		}
		c := C.ItofinCpiSwapConfig{swap_type: C.int32_t(a.Type), nominal: C.double(a.Nominal), inflation_nominal: C.double(nominal), spread: C.double(a.Spread), float_schedule: C.uint64_t(a.FloatSchedule.id), float_index: C.uint64_t(a.FloatIndex.id), float_day_counter: C.uint64_t(a.FloatDayCounter.id), float_convention: C.int32_t(a.FloatConvention), fixed_rate: C.double(a.FixedRate), base_cpi: C.double(a.BaseCPI), fixed_schedule: C.uint64_t(a.FixedSchedule.id), fixed_index: C.uint64_t(a.FixedIndex.id), fixed_day_counter: C.uint64_t(a.FixedDayCounter.id), fixed_convention: C.int32_t(a.FixedConvention), lag_length: C.int32_t(a.ObservationLag.Length), lag_unit: C.int32_t(a.ObservationLag.Unit), interpolation: C.int32_t(a.Interpolation), settings: C.uint64_t(a.Settings.id)}
		if a.SubtractInflationNominal {
			c.subtract_inflation_nominal = 1
		}
		var e C.ItofinError
		return ffiError(C.itofin_cpi_swap_new(s.ctx, &c, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &CPISwap{object{s, uint64(id)}}, nil
}

// SetEngine attaches a retained live nominal discount curve with native date defaults.
// Settings must be the same allocation used to construct the swap and both indexes.
func (v *CPISwap) SetEngine(curve *YieldTermStructure, settings *Settings) error {
	if v == nil || curve == nil || settings == nil {
		return errNilArgument("CPI swap engine argument")
	}
	s := v.session
	return s.invoke(func() error {
		if err := sameSession(s, v.object, curve.object, settings.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_cpi_swap_set_engine(s.ctx, C.uint64_t(v.id), C.uint64_t(curve.id), C.uint64_t(settings.id), &e), &e)
	})
}
func (v *CPISwap) value(query int32) (float64, error) {
	if v == nil {
		return 0, errNilArgument("CPI swap")
	}
	s := v.session
	var out C.double
	err := s.invoke(func() error {
		if err := sameSession(s, v.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_cpi_swap_value(s.ctx, C.uint64_t(v.id), C.int32_t(query), &out, &e), &e)
	})
	return float64(out), err
}
func (v *CPISwap) action(action int32) error {
	if v == nil {
		return errNilArgument("CPI swap")
	}
	s := v.session
	return s.invoke(func() error {
		if err := sameSession(s, v.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_cpi_swap_action(s.ctx, C.uint64_t(v.id), C.int32_t(action), &e), &e)
	})
}

// NPV returns signed CPI plus Ibor leg present values, including terminal exchanges.
func (v *CPISwap) NPV() (float64, error) { return v.value(0) }

// FixedLegNPV returns signed CPI-leg present value including indexed principal.
func (v *CPISwap) FixedLegNPV() (float64, error) { return v.value(1) }

// FloatLegNPV returns signed Ibor-leg present value including any nominal exchange.
func (v *CPISwap) FloatLegNPV() (float64, error) { return v.value(2) }

// FloatLegBPS returns signed sensitivity to a one-basis-point Ibor spread increase.
func (v *CPISwap) FloatLegBPS() (float64, error) { return v.value(3) }

// FairSpread returns the absolute Ibor spread that zeroes NPV with the CPI leg fixed.
func (v *CPISwap) FairSpread() (float64, error) { return v.value(4) }

// IsCalculated reads the native cache flag without calculating.
func (v *CPISwap) IsCalculated() (bool, error) { x, e := v.value(5); return x != 0, e }

// IsExpired tests native expiry without calculating.
func (v *CPISwap) IsExpired() (bool, error) { x, e := v.value(6); return x != 0, e }

// Calculate populates cached results if needed.
func (v *CPISwap) Calculate() error { return v.action(0) }

// Recalculate forces native repricing, including while frozen.
func (v *CPISwap) Recalculate() error { return v.action(1) }

// Freeze pins native cached results.
func (v *CPISwap) Freeze() error { return v.action(2) }

// Unfreeze resumes native observer-driven repricing.
func (v *CPISwap) Unfreeze() error { return v.action(3) }
