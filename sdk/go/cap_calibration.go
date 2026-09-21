package itofin

/*
#include "itofin.h"
*/
import "C"

type CapHelper struct{ object }
type CapHelperConfig struct {
	Length              Period
	Volatility          *SimpleQuote
	Index               *IborIndex
	FixedLegFrequency   Frequency
	FixedLegDayCounter  *DayCounter
	IncludeFirstSwaplet bool
	Curve               *YieldTermStructure
	ErrorType           CalibrationErrorType
	VolatilityType      VolatilityType
	Shift               float64
}

func (s *Session) NewCapHelper(a CapHelperConfig) (*CapHelper, error) {
	if a.Volatility == nil || a.Index == nil || a.FixedLegDayCounter == nil || a.Curve == nil {
		return nil, errNilArgument("cap helper inputs")
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		if err := sameSession(s, a.Volatility.object, a.Index.object, a.FixedLegDayCounter.object, a.Curve.object); err != nil {
			return err
		}
		c := C.ItofinCapHelperConfig{length: C.int32_t(a.Length.Length), length_unit: C.int32_t(a.Length.Unit), volatility: C.uint64_t(a.Volatility.id), index: C.uint64_t(a.Index.id), fixed_frequency: C.int32_t(a.FixedLegFrequency), fixed_day_counter: C.uint64_t(a.FixedLegDayCounter.id), curve: C.uint64_t(a.Curve.id), error_type: C.int32_t(a.ErrorType), volatility_type: C.int32_t(a.VolatilityType), shift: C.double(a.Shift)}
		if a.IncludeFirstSwaplet {
			c.include_first_swaplet = 1
		}
		var e C.ItofinError
		return ffiError(C.itofin_cap_helper_new(s.ctx, c, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &CapHelper{object{s, uint64(id)}}, nil
}
func (h *CapHelper) value(field int32, vol float64) (float64, error) {
	if h == nil {
		return 0, errNilArgument("helper")
	}
	s := h.session
	var value C.double
	err := s.invoke(func() error {
		if err := sameSession(s, h.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_cap_helper_value(s.ctx, C.uint64_t(h.id), C.int32_t(field), C.double(vol), &value, &e), &e)
	})
	return float64(value), err
}
func (h *CapHelper) MarketValue() (float64, error)                  { return h.value(0, 0) }
func (h *CapHelper) BlackPrice(volatility float64) (float64, error) { return h.value(1, volatility) }
func (h *CapHelper) ModelValue() (float64, error)                   { return h.value(2, 0) }
func (h *CapHelper) CalibrationError() (float64, error)             { return h.value(3, 0) }
func (h *CapHelper) SetTreeEngine(engine *TreeCapFloorEngine) error {
	if h == nil || engine == nil {
		return errNilArgument("helper and engine")
	}
	s := h.session
	return s.invoke(func() error {
		if err := sameSession(s, h.object, engine.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_cap_helper_set_tree_engine(s.ctx, C.uint64_t(h.id), C.uint64_t(engine.id), &e), &e)
	})
}
func (h *CapHelper) MandatoryTimes() ([]float64, error) {
	if h == nil {
		return nil, errNilArgument("helper")
	}
	s := h.session
	var result []float64
	err := s.invoke(func() error {
		if err := sameSession(s, h.object); err != nil {
			return err
		}
		var n C.size_t
		var e C.ItofinError
		if err := ffiError(C.itofin_cap_helper_times(s.ctx, C.uint64_t(h.id), nil, 0, &n, &e), &e); err != nil {
			return err
		}
		if n == 0 {
			return nil
		}
		values := make([]C.double, int(n))
		if err := ffiError(C.itofin_cap_helper_times(s.ctx, C.uint64_t(h.id), &values[0], n, &n, &e), &e); err != nil {
			return err
		}
		result = make([]float64, int(n))
		for i, v := range values {
			result[i] = float64(v)
		}
		return nil
	})
	return result, err
}
func (m *HullWhite) CalibrateCaps(helpers []*CapHelper, method *LevenbergMarquardt, criteria *EndCriteria, fixReversion bool, steps uint) error {
	if m == nil || method == nil || criteria == nil {
		return errNilArgument("model, method and criteria")
	}
	s := m.session
	objects := []object{m.object, method.object, criteria.object}
	ids := make([]C.uint64_t, len(helpers))
	for i, h := range helpers {
		if h == nil {
			return errNilArgument("helper")
		}
		objects = append(objects, h.object)
		ids[i] = C.uint64_t(h.id)
	}
	return s.invoke(func() error {
		if err := sameSession(s, objects...); err != nil {
			return err
		}
		var ptr *C.uint64_t
		if len(ids) > 0 {
			ptr = &ids[0]
		}
		var fixed C.uint8_t
		if fixReversion {
			fixed = 1
		}
		var e C.ItofinError
		return ffiError(C.itofin_hullwhite_calibrate_caps(s.ctx, C.uint64_t(m.id), ptr, C.size_t(len(ids)), C.uint64_t(method.id), C.uint64_t(criteria.id), C.size_t(steps), fixed, &e), &e)
	})
}
