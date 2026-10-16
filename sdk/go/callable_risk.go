package itofin

/*
#include "itofin.h"
*/
import "C"

type CallableBondRiskOptions struct{ Bump *float64 }

func callableRisk(b *Bond, spread float64, engine *TreeCallableFixedRateBondEngine, dc *DayCounter, compounding Compounding, frequency Frequency, options *CallableBondRiskOptions, convexity bool) (float64, error) {
	if b == nil || b.session == nil || engine == nil || dc == nil {
		return 0, errNilArgument("bond/session, tree engine and day counter")
	}
	a := C.ItofinCallableBondRiskConfig{tree_engine: C.uint64_t(engine.id), day_counter: C.uint64_t(dc.id), compounding: C.int32_t(compounding), frequency: C.int32_t(frequency)}
	if options != nil && options.Bump != nil {
		a.flags = 1
		a.bump = C.double(*options.Bump)
	}
	s := b.session
	var out C.double
	err := s.invoke(func() error {
		if err := sameSession(s, b.object, engine.object, dc.object); err != nil {
			return err
		}
		var e C.ItofinError
		if convexity {
			return ffiError(C.itofin_callable_bond_effective_convexity(s.ctx, C.uint64_t(b.id), C.double(spread), a, &out, &e), &e)
		}
		return ffiError(C.itofin_callable_bond_effective_duration(s.ctx, C.uint64_t(b.id), C.double(spread), a, &out, &e), &e)
	})
	return float64(out), err
}
func (b *CallableFixedRateBond) EffectiveDuration(spread float64, engine *TreeCallableFixedRateBondEngine, dayCounter *DayCounter, compounding Compounding, frequency Frequency, options *CallableBondRiskOptions) (float64, error) {
	if b == nil {
		return 0, errNilArgument("bond")
	}
	return callableRisk(&b.Bond, spread, engine, dayCounter, compounding, frequency, options, false)
}
func (b *CallableFixedRateBond) EffectiveConvexity(spread float64, engine *TreeCallableFixedRateBondEngine, dayCounter *DayCounter, compounding Compounding, frequency Frequency, options *CallableBondRiskOptions) (float64, error) {
	if b == nil {
		return 0, errNilArgument("bond")
	}
	return callableRisk(&b.Bond, spread, engine, dayCounter, compounding, frequency, options, true)
}
func (b *CallableZeroCouponBond) EffectiveDuration(spread float64, engine *TreeCallableFixedRateBondEngine, dayCounter *DayCounter, compounding Compounding, frequency Frequency, options *CallableBondRiskOptions) (float64, error) {
	if b == nil {
		return 0, errNilArgument("bond")
	}
	return callableRisk(&b.Bond, spread, engine, dayCounter, compounding, frequency, options, false)
}
func (b *CallableZeroCouponBond) EffectiveConvexity(spread float64, engine *TreeCallableFixedRateBondEngine, dayCounter *DayCounter, compounding Compounding, frequency Frequency, options *CallableBondRiskOptions) (float64, error) {
	if b == nil {
		return 0, errNilArgument("bond")
	}
	return callableRisk(&b.Bond, spread, engine, dayCounter, compounding, frequency, options, true)
}
