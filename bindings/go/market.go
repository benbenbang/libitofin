package itofin

/*
#include "itofin.h"
*/
import "C"

// SimpleQuote is a live observable quote; updates invalidate dependent valuations.
type SimpleQuote struct{ object }
type BlackScholesProcess struct{ object }

type BlackScholesConfig struct {
	Spot, RiskFreeRate, DividendYield, Volatility float64
	ReferenceDate                                 Date
	DayCounter                                    *DayCounter
}

func (s *Session) NewSimpleQuote(value float64) (*SimpleQuote, error) {
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_quote_new(s.ctx, C.double(value), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &SimpleQuote{object{s, uint64(id)}}, nil
}
func (q *SimpleQuote) Value() (float64, error) {
	var v C.double
	err := q.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_quote_value(q.session.ctx, C.uint64_t(q.id), &v, &e), &e)
	})
	return float64(v), err
}
func (q *SimpleQuote) SetValue(v float64) error {
	return q.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_quote_set(q.session.ctx, C.uint64_t(q.id), C.double(v), &e), &e)
	})
}
func (s *Session) NewBlackScholesProcess(cfg BlackScholesConfig) (*BlackScholesProcess, error) {
	if cfg.DayCounter == nil {
		return nil, errNilArgument("day counter")
	}
	if err := sameSession(s, cfg.DayCounter.object); err != nil {
		return nil, err
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_black_scholes_new(s.ctx, C.double(cfg.Spot), C.double(cfg.RiskFreeRate), C.double(cfg.DividendYield), C.double(cfg.Volatility), C.int32_t(cfg.ReferenceDate.Serial()), C.uint64_t(cfg.DayCounter.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &BlackScholesProcess{object{s, uint64(id)}}, nil
}
func (p *BlackScholesProcess) rate(dividend bool) (float64, error) {
	var v C.double
	err := p.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_black_scholes_rate(p.session.ctx, C.uint64_t(p.id), C.bool(dividend), &v, &e), &e)
	})
	return float64(v), err
}
func (p *BlackScholesProcess) RiskFreeRate() (float64, error)  { return p.rate(false) }
func (p *BlackScholesProcess) DividendYield() (float64, error) { return p.rate(true) }

func (s *Session) NewBlackScholesProcessFromCurves(spot float64, riskFree, dividend *YieldTermStructure, vol *BlackVolTermStructure) (*BlackScholesProcess, error) {
	if riskFree == nil || dividend == nil || vol == nil {
		return nil, errNilArgument("market curve")
	}
	if err := sameSession(s, riskFree.object, dividend.object, vol.object); err != nil {
		return nil, err
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_black_scholes_from_curves(s.ctx, C.double(spot), C.uint64_t(riskFree.id), C.uint64_t(dividend.id), C.uint64_t(vol.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &BlackScholesProcess{object{s, uint64(id)}}, nil
}
