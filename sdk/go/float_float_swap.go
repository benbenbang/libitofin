package itofin

/*
#include "itofin.h"
*/
import "C"

// FloatFloatSwap pays the first Ibor leg and receives the second in one currency.
type FloatFloatSwap struct{ object }

// FloatFloatSwapConfig defines plain equal-notional legs with no principal exchanges.
// A nil PaymentConvention preserves each schedule's own convention.
type FloatFloatSwapConfig struct {
	Nominal, PaySpread, ReceiveSpread float64
	PaySchedule, ReceiveSchedule      *Schedule
	PayIndex, ReceiveIndex            *IborIndex
	PayDayCounter, ReceiveDayCounter  *DayCounter
	Settings                          *Settings
	PaymentConvention                 *BusinessDayConvention
}

// NewFloatFloatSwap retains the original indexes, Settings and native swap.
func (s *Session) NewFloatFloatSwap(a FloatFloatSwapConfig) (*FloatFloatSwap, error) {
	if a.PaySchedule == nil || a.ReceiveSchedule == nil || a.PayIndex == nil || a.ReceiveIndex == nil || a.PayDayCounter == nil || a.ReceiveDayCounter == nil || a.Settings == nil {
		return nil, errNilArgument("float-float swap argument")
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		if err := sameSession(s, a.PaySchedule.object, a.ReceiveSchedule.object, a.PayIndex.object, a.ReceiveIndex.object, a.PayDayCounter.object, a.ReceiveDayCounter.object, a.Settings.object); err != nil {
			return err
		}
		c := C.ItofinFloatFloatSwapConfig{nominal: C.double(a.Nominal), pay_schedule: C.uint64_t(a.PaySchedule.id), pay_index: C.uint64_t(a.PayIndex.id), pay_spread: C.double(a.PaySpread), pay_day_counter: C.uint64_t(a.PayDayCounter.id), receive_schedule: C.uint64_t(a.ReceiveSchedule.id), receive_index: C.uint64_t(a.ReceiveIndex.id), receive_spread: C.double(a.ReceiveSpread), receive_day_counter: C.uint64_t(a.ReceiveDayCounter.id), settings: C.uint64_t(a.Settings.id)}
		if a.PaymentConvention != nil {
			c.has_payment_convention = 1
			c.payment_convention = C.int32_t(*a.PaymentConvention)
		}
		var e C.ItofinError
		return ffiError(C.itofin_float_float_swap_new(s.ctx, &c, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &FloatFloatSwap{object{s, uint64(id)}}, nil
}

// SetEngine attaches a live discount curve using native date and flow defaults.
// Settings must be the same allocation used to construct the swap and indexes.
func (v *FloatFloatSwap) SetEngine(curve *YieldTermStructure, settings *Settings) error {
	if v == nil || curve == nil || settings == nil {
		return errNilArgument("float-float swap engine argument")
	}
	s := v.session
	return s.invoke(func() error {
		if err := sameSession(s, v.object, curve.object, settings.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_float_float_swap_set_engine(s.ctx, C.uint64_t(v.id), C.uint64_t(curve.id), C.uint64_t(settings.id), &e), &e)
	})
}
func (v *FloatFloatSwap) value(query int32) (float64, error) {
	if v == nil {
		return 0, errNilArgument("float-float swap")
	}
	s := v.session
	var out C.double
	err := s.invoke(func() error {
		if err := sameSession(s, v.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_float_float_swap_value(s.ctx, C.uint64_t(v.id), C.int32_t(query), &out, &e), &e)
	})
	return float64(out), err
}
func (v *FloatFloatSwap) action(action int32) error {
	if v == nil {
		return errNilArgument("float-float swap")
	}
	s := v.session
	return s.invoke(func() error {
		if err := sameSession(s, v.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_float_float_swap_action(s.ctx, C.uint64_t(v.id), C.int32_t(action), &e), &e)
	})
}

// NPV returns signed paid PV plus signed received PV.
func (v *FloatFloatSwap) NPV() (float64, error) { return v.value(0) }

// PayLegNPV returns the signed paid-leg present value.
func (v *FloatFloatSwap) PayLegNPV() (float64, error) { return v.value(1) }

// ReceiveLegNPV returns the signed received-leg present value.
func (v *FloatFloatSwap) ReceiveLegNPV() (float64, error) { return v.value(2) }

// PayLegBPS returns signed paid-leg sensitivity for a one-basis-point spread shift.
func (v *FloatFloatSwap) PayLegBPS() (float64, error) { return v.value(3) }

// ReceiveLegBPS returns signed received-leg sensitivity for a one-basis-point spread shift.
func (v *FloatFloatSwap) ReceiveLegBPS() (float64, error) { return v.value(4) }

// FairPaySpread returns the absolute paid spread zeroing NPV with the received spread fixed.
func (v *FloatFloatSwap) FairPaySpread() (float64, error) { return v.value(5) }

// FairReceiveSpread returns the absolute received spread zeroing NPV with the paid spread fixed.
func (v *FloatFloatSwap) FairReceiveSpread() (float64, error) { return v.value(6) }

// IsCalculated reads the native cache flag without calculating.
func (v *FloatFloatSwap) IsCalculated() (bool, error) { x, e := v.value(7); return x != 0, e }

// IsExpired tests native expiry without calculating.
func (v *FloatFloatSwap) IsExpired() (bool, error) { x, e := v.value(8); return x != 0, e }

// Calculate populates native cached results if needed.
func (v *FloatFloatSwap) Calculate() error { return v.action(0) }

// Recalculate forces native repricing, including while frozen.
func (v *FloatFloatSwap) Recalculate() error { return v.action(1) }

// Freeze pins native cached results.
func (v *FloatFloatSwap) Freeze() error { return v.action(2) }

// Unfreeze resumes native observer-driven repricing.
func (v *FloatFloatSwap) Unfreeze() error { return v.action(3) }
