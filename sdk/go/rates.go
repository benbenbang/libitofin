package itofin

/*
#include "itofin.h"
*/
import "C"
import "fmt"

type SwapType int32

const (
	SwapPayer SwapType = iota
	SwapReceiver
)

type VanillaSwap struct{ object }
type OvernightIndexedSwap struct{ object }
type VanillaSwapConfig struct {
	Type                                SwapType
	Nominal, FixedRate, Spread          float64
	FixedSchedule, FloatingSchedule     *Schedule
	FixedDayCounter, FloatingDayCounter *DayCounter
	Index                               *IborIndex
	Settings                            *Settings
}

func (s *Session) NewVanillaSwap(a VanillaSwapConfig) (*VanillaSwap, error) {
	if a.FixedSchedule == nil || a.FloatingSchedule == nil || a.FixedDayCounter == nil || a.FloatingDayCounter == nil || a.Index == nil || a.Settings == nil {
		return nil, fmt.Errorf("swap arguments must not be nil")
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		if err := sameSession(s, a.FixedSchedule.object, a.FloatingSchedule.object, a.FixedDayCounter.object, a.FloatingDayCounter.object, a.Index.object, a.Settings.object); err != nil {
			return err
		}
		c := C.ItofinVanillaSwapConfig{swap_type: C.int32_t(a.Type), nominal: C.double(a.Nominal), fixed_rate: C.double(a.FixedRate), spread: C.double(a.Spread), fixed_schedule: C.uint64_t(a.FixedSchedule.id), floating_schedule: C.uint64_t(a.FloatingSchedule.id), fixed_day_counter: C.uint64_t(a.FixedDayCounter.id), floating_day_counter: C.uint64_t(a.FloatingDayCounter.id), index: C.uint64_t(a.Index.id), settings: C.uint64_t(a.Settings.id)}
		var e C.ItofinError
		return ffiError(C.itofin_vanilla_swap_new(s.ctx, c, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &VanillaSwap{object{s, uint64(id)}}, nil
}

// MakeVanillaSwapConfig preserves omitted overrides, including a nil par rate.
type MakeVanillaSwapConfig struct {
	Tenor, ForwardStart Period
	Index               *IborIndex
	Settings            *Settings
	FixedRate, Nominal  *float64
	EffectiveDate       *Date
	FixedLegTenor       *Period
	FixedLegDayCounter  *DayCounter
}

func (s *Session) MakeVanillaSwap(a MakeVanillaSwapConfig) (*VanillaSwap, error) {
	if a.Index == nil || a.Settings == nil {
		return nil, fmt.Errorf("index and settings required")
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		objects := []object{a.Index.object, a.Settings.object}
		if a.FixedLegDayCounter != nil {
			objects = append(objects, a.FixedLegDayCounter.object)
		}
		if err := sameSession(s, objects...); err != nil {
			return err
		}
		c := C.ItofinMakeSwapConfig{tenor_length: C.int32_t(a.Tenor.Length), tenor_unit: C.int32_t(a.Tenor.Unit), forward_length: C.int32_t(a.ForwardStart.Length), forward_unit: C.int32_t(a.ForwardStart.Unit), index: C.uint64_t(a.Index.id), settings: C.uint64_t(a.Settings.id)}
		if a.FixedRate != nil {
			c.flags |= 1
			c.fixed_rate = C.double(*a.FixedRate)
		}
		if a.EffectiveDate != nil {
			c.flags |= 2
			c.effective_date = C.int32_t(a.EffectiveDate.Serial())
		}
		if a.Nominal != nil {
			c.flags |= 4
			c.nominal = C.double(*a.Nominal)
		}
		if a.FixedLegTenor != nil {
			c.flags |= 8
			c.fixed_length = C.int32_t(a.FixedLegTenor.Length)
			c.fixed_unit = C.int32_t(a.FixedLegTenor.Unit)
		}
		if a.FixedLegDayCounter != nil {
			c.flags |= 16
			c.fixed_day_counter = C.uint64_t(a.FixedLegDayCounter.id)
		}
		var e C.ItofinError
		return ffiError(C.itofin_make_vanilla_swap(s.ctx, c, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &VanillaSwap{object{s, uint64(id)}}, nil
}

type MakeOisConfig struct {
	Tenor, ForwardStart Period
	Index               *OvernightIndex
	Settings            *Settings
	FixedLegDayCounter  *DayCounter
	FixedRate, Nominal  *float64
	EffectiveDate       *Date
	PaymentLag          *int32
	Discount            *YieldTermStructure
	Averaging           *RateAveraging
}

func (s *Session) MakeOis(a MakeOisConfig) (*OvernightIndexedSwap, error) {
	if a.Index == nil || a.Settings == nil {
		return nil, fmt.Errorf("index and settings required")
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		objects := []object{a.Index.object, a.Settings.object}
		if a.FixedLegDayCounter != nil {
			objects = append(objects, a.FixedLegDayCounter.object)
		}
		if a.Discount != nil {
			objects = append(objects, a.Discount.object)
		}
		if err := sameSession(s, objects...); err != nil {
			return err
		}
		c := C.ItofinMakeSwapConfig{tenor_length: C.int32_t(a.Tenor.Length), tenor_unit: C.int32_t(a.Tenor.Unit), forward_length: C.int32_t(a.ForwardStart.Length), forward_unit: C.int32_t(a.ForwardStart.Unit), index: C.uint64_t(a.Index.id), settings: C.uint64_t(a.Settings.id)}
		if a.FixedRate != nil {
			c.flags |= 1
			c.fixed_rate = C.double(*a.FixedRate)
		}
		if a.EffectiveDate != nil {
			c.flags |= 2
			c.effective_date = C.int32_t(a.EffectiveDate.Serial())
		}
		if a.Nominal != nil {
			c.flags |= 4
			c.nominal = C.double(*a.Nominal)
		}
		if a.FixedLegDayCounter != nil {
			c.flags |= 16
			c.fixed_day_counter = C.uint64_t(a.FixedLegDayCounter.id)
		}
		if a.PaymentLag != nil {
			c.flags |= 32
			c.payment_lag = C.int32_t(*a.PaymentLag)
		}
		if a.Discount != nil {
			c.flags |= 64
			c.discount = C.uint64_t(a.Discount.id)
		}
		if a.Averaging != nil {
			c.flags |= 128
			c.averaging = C.int32_t(*a.Averaging)
		}
		var e C.ItofinError
		return ffiError(C.itofin_make_ois(s.ctx, c, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &OvernightIndexedSwap{object{s, uint64(id)}}, nil
}
func (v *VanillaSwap) SetEngine(discount *YieldTermStructure, settings *Settings) error {
	if v == nil || discount == nil || settings == nil {
		return fmt.Errorf("swap, curve and settings required")
	}
	s := v.session
	return s.invoke(func() error {
		if err := sameSession(s, v.object, discount.object, settings.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_vanilla_swap_set_engine(s.ctx, C.uint64_t(v.id), C.uint64_t(discount.id), C.uint64_t(settings.id), &e), &e)
	})
}
func (v *VanillaSwap) Price(discount *YieldTermStructure, settings *Settings) (float64, error) {
	if v == nil || discount == nil || settings == nil {
		return 0, fmt.Errorf("swap, discount and settings required")
	}
	s := v.session
	var value C.double
	err := s.invoke(func() error {
		if err := sameSession(s, v.object, discount.object, settings.object); err != nil {
			return err
		}
		var e C.ItofinError
		if err := ffiError(C.itofin_vanilla_swap_set_engine(s.ctx, C.uint64_t(v.id), C.uint64_t(discount.id), C.uint64_t(settings.id), &e), &e); err != nil {
			return err
		}
		return ffiError(C.itofin_swap_value(s.ctx, C.uint64_t(v.id), 0, 0, &value, &e), &e)
	})
	return float64(value), err
}
func swapValue(o object, kind, field int32) (float64, error) {
	var out C.double
	s := o.session
	err := s.invoke(func() error {
		if err := sameSession(s, o); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_swap_value(s.ctx, C.uint64_t(o.id), C.int32_t(kind), C.int32_t(field), &out, &e), &e)
	})
	return float64(out), err
}
func swapResults(o object, kind int32) (*Results, error) {
	var result *Results
	s := o.session
	err := s.invoke(func() error {
		if err := sameSession(s, o); err != nil {
			return err
		}
		var e C.ItofinError
		var id C.uint64_t
		if err := ffiError(C.itofin_swap_results(s.ctx, C.uint64_t(o.id), C.int32_t(kind), &id, &e), &e); err != nil {
			return err
		}
		var err error
		result, err = s.readResults(uint64(id))
		return err
	})
	return result, err
}
func (v *VanillaSwap) NPV() (float64, error)       { return swapValue(v.object, 0, 0) }
func (v *VanillaSwap) FairRate() (float64, error)  { return swapValue(v.object, 0, 1) }
func (v *VanillaSwap) Nominal() (float64, error)   { return swapValue(v.object, 0, 2) }
func (v *VanillaSwap) FixedRate() (float64, error) { return swapValue(v.object, 0, 3) }
func (v *VanillaSwap) IsCalculated() (bool, error) {
	x, e := swapValue(v.object, 0, 4)
	return x != 0, e
}
func (v *VanillaSwap) Calculate() error                     { _, e := swapValue(v.object, 0, 5); return e }
func (v *VanillaSwap) Results() (*Results, error)           { return swapResults(v.object, 0) }
func (v *OvernightIndexedSwap) NPV() (float64, error)       { return swapValue(v.object, 1, 0) }
func (v *OvernightIndexedSwap) FairRate() (float64, error)  { return swapValue(v.object, 1, 1) }
func (v *OvernightIndexedSwap) Nominal() (float64, error)   { return swapValue(v.object, 1, 2) }
func (v *OvernightIndexedSwap) FixedRate() (float64, error) { return swapValue(v.object, 1, 3) }
func (v *OvernightIndexedSwap) IsCalculated() (bool, error) {
	x, e := swapValue(v.object, 1, 4)
	return x != 0, e
}
func (v *OvernightIndexedSwap) Calculate() error           { _, e := swapValue(v.object, 1, 5); return e }
func (v *OvernightIndexedSwap) Results() (*Results, error) { return swapResults(v.object, 1) }
func (v *OvernightIndexedSwap) Price() (float64, error)    { return v.NPV() }
