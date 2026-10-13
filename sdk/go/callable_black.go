package itofin

/*
#include "itofin.h"
*/
import "C"

type BlackCallableFixedRateBondEngine struct{ object }

type CallableImpliedVolatilityOptions struct {
	PriceType      BondPriceType
	Accuracy       *float64
	MaxEvaluations *uint
	MinVol         *float64
	MaxVol         *float64
}

func (s *Session) NewBlackCallableFixedRateBondEngine(yieldVolatility *SimpleQuote, discountCurve *YieldTermStructure, settings *Settings) (*BlackCallableFixedRateBondEngine, error) {
	if yieldVolatility == nil || discountCurve == nil || settings == nil {
		return nil, errNilArgument("yield volatility, discount curve and settings")
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		if err := sameSession(s, yieldVolatility.object, discountCurve.object, settings.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_black_callable_bond_engine_new(s.ctx, C.uint64_t(yieldVolatility.id), C.uint64_t(discountCurve.id), C.uint64_t(settings.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &BlackCallableFixedRateBondEngine{object{s, uint64(id)}}, nil
}
func setCallableBlackEngine(b *Bond, engine *BlackCallableFixedRateBondEngine) error {
	if b == nil || b.session == nil || engine == nil {
		return errNilArgument("bond/session and Black engine")
	}
	s := b.session
	return s.invoke(func() error {
		if err := sameSession(s, b.object, engine.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_callable_bond_set_black_engine(s.ctx, C.uint64_t(b.id), C.uint64_t(engine.id), &e), &e)
	})
}
func (b *CallableFixedRateBond) SetBlackEngine(engine *BlackCallableFixedRateBondEngine) error {
	if b == nil {
		return errNilArgument("bond")
	}
	return setCallableBlackEngine(&b.Bond, engine)
}
func (b *CallableZeroCouponBond) SetBlackEngine(engine *BlackCallableFixedRateBondEngine) error {
	if b == nil {
		return errNilArgument("bond")
	}
	return setCallableBlackEngine(&b.Bond, engine)
}
func callableImpliedVolatility(b *Bond, targetPrice float64, discountCurve *YieldTermStructure, options *CallableImpliedVolatilityOptions) (float64, error) {
	if b == nil || b.session == nil || discountCurve == nil {
		return 0, errNilArgument("bond/session and discount curve")
	}
	var a C.ItofinCallableImpliedVolatilityConfig
	kind := Clean
	if options != nil {
		kind = options.PriceType
		if options.Accuracy != nil {
			a.flags |= 1
			a.accuracy = C.double(*options.Accuracy)
		}
		if options.MaxEvaluations != nil {
			a.flags |= 2
			a.max_evaluations = C.uint64_t(*options.MaxEvaluations)
		}
		if options.MinVol != nil {
			a.flags |= 4
			a.min_vol = C.double(*options.MinVol)
		}
		if options.MaxVol != nil {
			a.flags |= 8
			a.max_vol = C.double(*options.MaxVol)
		}
	}
	var out C.double
	s := b.session
	err := s.invoke(func() error {
		if err := sameSession(s, b.object, discountCurve.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_callable_bond_implied_volatility(s.ctx, C.uint64_t(b.id), C.double(targetPrice), C.int32_t(kind), C.uint64_t(discountCurve.id), a, &out, &e), &e)
	})
	return float64(out), err
}
func (b *CallableFixedRateBond) ImpliedVolatility(targetPrice float64, discountCurve *YieldTermStructure, options *CallableImpliedVolatilityOptions) (float64, error) {
	if b == nil {
		return 0, errNilArgument("bond")
	}
	return callableImpliedVolatility(&b.Bond, targetPrice, discountCurve, options)
}
func (b *CallableZeroCouponBond) ImpliedVolatility(targetPrice float64, discountCurve *YieldTermStructure, options *CallableImpliedVolatilityOptions) (float64, error) {
	if b == nil {
		return 0, errNilArgument("bond")
	}
	return callableImpliedVolatility(&b.Bond, targetPrice, discountCurve, options)
}
