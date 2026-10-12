package itofin

/*
#include "itofin.h"
*/
import "C"
import "fmt"

type Compounding int32

const (
	Simple               Compounding = 0
	Compounded           Compounding = 1
	Continuous           Compounding = 2
	SimpleThenCompounded Compounding = 3
	CompoundedThenSimple Compounding = 4
)

type AnalyticsSolverConfig struct {
	Accuracy       *float64
	MaxEvaluations *uint
	Guess          *float64
}
type BondYieldConfig struct {
	DayCounter     *DayCounter
	Compounding    Compounding
	Frequency      Frequency
	SettlementDate *Date
}
type BondYieldSolveConfig struct {
	BondYieldConfig
	PriceType BondPriceType
	AnalyticsSolverConfig
}
type BondZSpreadConfig struct {
	Discount       *YieldTermStructure
	Compounding    Compounding
	Frequency      Frequency
	SettlementDate *Date
}
type BondZSpreadSolveConfig struct {
	BondZSpreadConfig
	PriceType BondPriceType
	AnalyticsSolverConfig
}
type CashFlowsZSpreadConfig struct {
	CashFlowsNPVConfig
	Compounding Compounding
	Frequency   Frequency
}
type CashFlowsZSpreadSolveConfig struct {
	CashFlowsZSpreadConfig
	AnalyticsSolverConfig
}

func analyticsSolver(a AnalyticsSolverConfig) C.ItofinAnalyticsSolverConfig {
	c := C.ItofinAnalyticsSolverConfig{}
	if a.Accuracy != nil {
		c.flags |= 1
		c.accuracy = C.double(*a.Accuracy)
	}
	if a.MaxEvaluations != nil {
		c.flags |= 2
		c.max_evaluations = C.uint64_t(*a.MaxEvaluations)
	}
	if a.Guess != nil {
		c.flags |= 4
		c.guess = C.double(*a.Guess)
	}
	return c
}
func yieldAnalyticsConfig(a BondYieldConfig) (C.ItofinBondYieldAnalyticsConfig, []object, error) {
	if a.DayCounter == nil {
		return C.ItofinBondYieldAnalyticsConfig{}, nil, fmt.Errorf("day counter required")
	}
	serial, err := bondDate(a.SettlementDate)
	return C.ItofinBondYieldAnalyticsConfig{day_counter: C.uint64_t(a.DayCounter.id), compounding: C.int32_t(a.Compounding), frequency: C.int32_t(a.Frequency), settlement: serial}, []object{a.DayCounter.object}, err
}
func spreadAnalyticsConfig(a BondZSpreadConfig) (C.ItofinBondZSpreadAnalyticsConfig, []object, error) {
	if a.Discount == nil {
		return C.ItofinBondZSpreadAnalyticsConfig{}, nil, fmt.Errorf("discount curve required")
	}
	serial, err := bondDate(a.SettlementDate)
	return C.ItofinBondZSpreadAnalyticsConfig{discount: C.uint64_t(a.Discount.id), compounding: C.int32_t(a.Compounding), frequency: C.int32_t(a.Frequency), settlement: serial}, []object{a.Discount.object}, err
}
func (b *Bond) analytics(objects []object, call func(*C.ItofinContext, C.uint64_t, *C.double, *C.ItofinError) C.int32_t) (float64, error) {
	if b == nil {
		return 0, fmt.Errorf("bond required")
	}
	var out C.double
	s := b.session
	err := s.invoke(func() error {
		if err := sameSession(s, append([]object{b.object}, objects...)...); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(call(s.ctx, C.uint64_t(b.id), &out, &e), &e)
	})
	return float64(out), err
}
func (b *Bond) CleanPriceFromCurve(discount *YieldTermStructure, settlement *Date) (float64, error) {
	if discount == nil {
		return 0, fmt.Errorf("discount curve required")
	}
	serial, err := bondDate(settlement)
	if err != nil {
		return 0, err
	}
	return b.analytics([]object{discount.object}, func(ctx *C.ItofinContext, id C.uint64_t, out *C.double, e *C.ItofinError) C.int32_t {
		return C.itofin_bond_clean_price_from_curve(ctx, id, C.uint64_t(discount.id), serial, out, e)
	})
}
func (b *Bond) CleanPriceFromYield(rate float64, a BondYieldConfig) (float64, error) {
	c, objects, err := yieldAnalyticsConfig(a)
	if err != nil {
		return 0, err
	}
	return b.analytics(objects, func(ctx *C.ItofinContext, id C.uint64_t, out *C.double, e *C.ItofinError) C.int32_t {
		return C.itofin_bond_clean_price_from_yield(ctx, id, C.double(rate), c, out, e)
	})
}
func (b *Bond) CleanPriceAtZSpread(spread float64, a BondZSpreadConfig) (float64, error) {
	c, objects, err := spreadAnalyticsConfig(a)
	if err != nil {
		return 0, err
	}
	return b.analytics(objects, func(ctx *C.ItofinContext, id C.uint64_t, out *C.double, e *C.ItofinError) C.int32_t {
		return C.itofin_bond_clean_price_at_z_spread(ctx, id, C.double(spread), c, out, e)
	})
}
func (b *Bond) DirtyPriceFromCurve(discount *YieldTermStructure, settlement *Date) (float64, error) {
	if discount == nil {
		return 0, fmt.Errorf("discount curve required")
	}
	serial, err := bondDate(settlement)
	if err != nil {
		return 0, err
	}
	return b.analytics([]object{discount.object}, func(ctx *C.ItofinContext, id C.uint64_t, out *C.double, e *C.ItofinError) C.int32_t {
		return C.itofin_bond_dirty_price_from_curve(ctx, id, C.uint64_t(discount.id), serial, out, e)
	})
}
func (b *Bond) DirtyPriceFromYield(rate float64, a BondYieldConfig) (float64, error) {
	c, objects, err := yieldAnalyticsConfig(a)
	if err != nil {
		return 0, err
	}
	return b.analytics(objects, func(ctx *C.ItofinContext, id C.uint64_t, out *C.double, e *C.ItofinError) C.int32_t {
		return C.itofin_bond_dirty_price_from_yield(ctx, id, C.double(rate), c, out, e)
	})
}
func (b *Bond) DirtyPriceAtZSpread(spread float64, a BondZSpreadConfig) (float64, error) {
	c, objects, err := spreadAnalyticsConfig(a)
	if err != nil {
		return 0, err
	}
	return b.analytics(objects, func(ctx *C.ItofinContext, id C.uint64_t, out *C.double, e *C.ItofinError) C.int32_t {
		return C.itofin_bond_dirty_price_at_z_spread(ctx, id, C.double(spread), c, out, e)
	})
}
func (b *Bond) YieldRate(price float64, a BondYieldSolveConfig) (float64, error) {
	c, objects, err := yieldAnalyticsConfig(a.BondYieldConfig)
	if err != nil {
		return 0, err
	}
	solve := analyticsSolver(a.AnalyticsSolverConfig)
	return b.analytics(objects, func(ctx *C.ItofinContext, id C.uint64_t, out *C.double, e *C.ItofinError) C.int32_t {
		return C.itofin_bond_yield_rate(ctx, id, C.double(price), C.int32_t(a.PriceType), c, solve, out, e)
	})
}
func (b *Bond) ZSpread(price float64, a BondZSpreadSolveConfig) (float64, error) {
	c, objects, err := spreadAnalyticsConfig(a.BondZSpreadConfig)
	if err != nil {
		return 0, err
	}
	solve := analyticsSolver(a.AnalyticsSolverConfig)
	return b.analytics(objects, func(ctx *C.ItofinContext, id C.uint64_t, out *C.double, e *C.ItofinError) C.int32_t {
		return C.itofin_bond_z_spread(ctx, id, C.double(price), C.int32_t(a.PriceType), c, solve, out, e)
	})
}
func cashflowsSpreadConfig(a CashFlowsZSpreadConfig) (C.ItofinCashFlowsZSpreadConfig, []object, error) {
	if a.Discount == nil || a.Settings == nil {
		return C.ItofinCashFlowsZSpreadConfig{}, nil, fmt.Errorf("discount curve and settings required")
	}
	settlement, err := bondDate(a.SettlementDate)
	if err != nil {
		return C.ItofinCashFlowsZSpreadConfig{}, nil, err
	}
	npvDate, err := bondDate(a.NPVDate)
	if err != nil {
		return C.ItofinCashFlowsZSpreadConfig{}, nil, err
	}
	include := C.int32_t(-1)
	if a.IncludeSettlementDateFlows != nil {
		include = 0
		if *a.IncludeSettlementDateFlows {
			include = 1
		}
	}
	return C.ItofinCashFlowsZSpreadConfig{discount: C.uint64_t(a.Discount.id), settings: C.uint64_t(a.Settings.id), include_settlement: include, settlement: settlement, npv_date: npvDate, compounding: C.int32_t(a.Compounding), frequency: C.int32_t(a.Frequency)}, []object{a.Discount.object, a.Settings.object}, nil
}
func (l *Leg) analytics(objects []object, call func(*C.ItofinContext, C.uint64_t, *C.double, *C.ItofinError) C.int32_t) (float64, error) {
	if l == nil {
		return 0, fmt.Errorf("leg required")
	}
	var out C.double
	s := l.session
	err := s.invoke(func() error {
		if err := sameSession(s, append([]object{l.object}, objects...)...); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(call(s.ctx, C.uint64_t(l.id), &out, &e), &e)
	})
	return float64(out), err
}
func (l *Leg) NPVAtZSpread(spread float64, a CashFlowsZSpreadConfig) (float64, error) {
	c, objects, err := cashflowsSpreadConfig(a)
	if err != nil {
		return 0, err
	}
	return l.analytics(objects, func(ctx *C.ItofinContext, id C.uint64_t, out *C.double, e *C.ItofinError) C.int32_t {
		return C.itofin_cashflows_npv_at_z_spread(ctx, id, C.double(spread), c, out, e)
	})
}
func (l *Leg) ZSpread(npv float64, a CashFlowsZSpreadSolveConfig) (float64, error) {
	c, objects, err := cashflowsSpreadConfig(a.CashFlowsZSpreadConfig)
	if err != nil {
		return 0, err
	}
	solve := analyticsSolver(a.AnalyticsSolverConfig)
	return l.analytics(objects, func(ctx *C.ItofinContext, id C.uint64_t, out *C.double, e *C.ItofinError) C.int32_t {
		return C.itofin_cashflows_z_spread(ctx, id, C.double(npv), c, solve, out, e)
	})
}
