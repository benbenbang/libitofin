package itofin

/*
#include "itofin.h"
*/
import "C"
import "fmt"

type OptionletStripper1 struct{ object }
type OptionletStripperConfig struct {
	TermVolSurface     *CapFloorTermVolSurface
	IborIndex          *IborIndex
	VolatilityType     VolatilityType
	Accuracy           float64
	MaxIterations      uint32
	Displacement       float64
	Discount           *YieldTermStructure
	OptionletFrequency *Period
	OvernightIndex     *OvernightIndex
	SwitchStrike       *float64
	DontThrow          bool
}

func (s *Session) OptionletStripper1(x OptionletStripperConfig) (*OptionletStripper1, error) {
	if x.TermVolSurface == nil || (x.IborIndex == nil) == (x.OvernightIndex == nil) {
		return nil, fmt.Errorf("term volatility and exactly one ibor or overnight index are required")
	}
	deps := []object{x.TermVolSurface.object}
	var index object
	if x.IborIndex != nil {
		index = x.IborIndex.object
	} else {
		index = x.OvernightIndex.object
	}
	deps = append(deps, index)
	var discount uint64
	if x.Discount != nil {
		deps = append(deps, x.Discount.object)
		discount = x.Discount.id
	}
	if err := sameSession(s, deps...); err != nil {
		return nil, err
	}
	var length, unit, present C.int32_t
	if x.OptionletFrequency != nil {
		length = C.int32_t(x.OptionletFrequency.Length)
		unit = C.int32_t(x.OptionletFrequency.Unit)
		present = 1
	}
	if x.Accuracy == 0 {
		x.Accuracy = 1e-6
	}
	if x.MaxIterations == 0 {
		x.MaxIterations = 100
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		cfg := C.ItofinOptionletStripperConfig{surface: C.uint64_t(x.TermVolSurface.id), index: C.uint64_t(index.id), discount: C.uint64_t(discount), volatility_type: C.int32_t(x.VolatilityType), accuracy: C.double(x.Accuracy), max_iterations: C.uint32_t(x.MaxIterations), displacement: C.double(x.Displacement), frequency_length: length, frequency_unit: unit, has_frequency: C.uint8_t(present)}
		if x.SwitchStrike != nil {
			cfg.has_switch_strike = 1
			cfg.switch_strike = C.double(*x.SwitchStrike)
		}
		if x.DontThrow {
			cfg.dont_throw = 1
		}
		if x.OvernightIndex != nil {
			cfg.overnight = 1
		}
		return ffiError(C.itofin_optionlet_stripper_new_with_options(s.ctx, &cfg, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &OptionletStripper1{object{s, uint64(id)}}, nil
}
func (v *OptionletStripper1) SwitchStrike() (float64, error) {
	var out C.double
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_optionlet_stripper_switch_strike(v.session.ctx, C.uint64_t(v.id), &out, &e), &e)
	})
	return float64(out), err
}
func (v *OptionletStripper1) ATMOptionletRates() ([]float64, error) {
	var result []float64
	err := v.session.invoke(func() error {
		var n C.size_t
		var e C.ItofinError
		if err := ffiError(C.itofin_optionlet_stripper_rates(v.session.ctx, C.uint64_t(v.id), nil, 0, &n, &e), &e); err != nil {
			return err
		}
		if uint64(n) > uint64(^uint(0)>>1) {
			return fmt.Errorf("rates count exceeds Go slice range")
		}
		buf := make([]C.double, int(n))
		var ptr *C.double
		if len(buf) > 0 {
			ptr = &buf[0]
		}
		if err := ffiError(C.itofin_optionlet_stripper_rates(v.session.ctx, C.uint64_t(v.id), ptr, n, &n, &e), &e); err != nil {
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

type OptionletStripper interface{ strippedObject() object }

func (v *OptionletStripper1) strippedObject() object {
	if v == nil {
		return object{}
	}
	return v.object
}
func (v *OptionletStripper2) strippedObject() object {
	if v == nil {
		return object{}
	}
	return v.object
}

func (s *Session) StrippedOptionletAdapter(stripper OptionletStripper, settings *Settings) (*OptionletVolatilityStructure, error) {
	if stripper == nil || settings == nil {
		return nil, fmt.Errorf("stripper and settings are required")
	}
	if err := sameSession(s, stripper.strippedObject(), settings.object); err != nil {
		return nil, err
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_stripped_optionlet_adapter_new(s.ctx, C.uint64_t(stripper.strippedObject().id), C.uint64_t(settings.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &OptionletVolatilityStructure{object{s, uint64(id)}}, nil
}
