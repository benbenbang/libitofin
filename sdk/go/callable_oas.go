package itofin

/*
#include "itofin.h"
*/
import "C"

type CallableOASConfig struct {
	DayCounter     *DayCounter
	Compounding    Compounding
	Frequency      Frequency
	SettlementDate *Date
}
type CallableOASSolveConfig struct {
	CallableOASConfig
	AnalyticsSolverConfig
}

func callableOAS(b *Bond, value float64, engine *TreeCallableFixedRateBondEngine, config CallableOASConfig, solve *AnalyticsSolverConfig) (float64, error) {
	if b == nil || b.session == nil || engine == nil || config.DayCounter == nil {
		return 0, errNilArgument("bond/session, tree engine and day counter")
	}
	var a C.ItofinCallableOASConfig
	a.day_counter = C.uint64_t(config.DayCounter.id)
	a.compounding = C.int32_t(config.Compounding)
	a.frequency = C.int32_t(config.Frequency)
	if config.SettlementDate != nil {
		a.settlement_flags = 1
		a.settlement = C.int32_t(config.SettlementDate.Serial())
	}
	var out C.double
	s := b.session
	err := s.invoke(func() error {
		if err := sameSession(s, b.object, engine.object, config.DayCounter.object); err != nil {
			return err
		}
		var e C.ItofinError
		if solve != nil {
			return ffiError(C.itofin_callable_bond_oas(s.ctx, C.uint64_t(b.id), C.double(value), C.uint64_t(engine.id), a, analyticsSolver(*solve), &out, &e), &e)
		}
		return ffiError(C.itofin_callable_bond_clean_price_oas(s.ctx, C.uint64_t(b.id), C.double(value), C.uint64_t(engine.id), a, &out, &e), &e)
	})
	return float64(out), err
}
func (b *CallableFixedRateBond) CleanPriceOAS(spread float64, engine *TreeCallableFixedRateBondEngine, config CallableOASConfig) (float64, error) {
	if b == nil {
		return 0, errNilArgument("bond")
	}
	return callableOAS(&b.Bond, spread, engine, config, nil)
}
func (b *CallableZeroCouponBond) CleanPriceOAS(spread float64, engine *TreeCallableFixedRateBondEngine, config CallableOASConfig) (float64, error) {
	if b == nil {
		return 0, errNilArgument("bond")
	}
	return callableOAS(&b.Bond, spread, engine, config, nil)
}
func (b *CallableFixedRateBond) OAS(cleanPrice float64, engine *TreeCallableFixedRateBondEngine, config CallableOASSolveConfig) (float64, error) {
	if b == nil {
		return 0, errNilArgument("bond")
	}
	return callableOAS(&b.Bond, cleanPrice, engine, config.CallableOASConfig, &config.AnalyticsSolverConfig)
}
func (b *CallableZeroCouponBond) OAS(cleanPrice float64, engine *TreeCallableFixedRateBondEngine, config CallableOASSolveConfig) (float64, error) {
	if b == nil {
		return 0, errNilArgument("bond")
	}
	return callableOAS(&b.Bond, cleanPrice, engine, config.CallableOASConfig, &config.AnalyticsSolverConfig)
}
