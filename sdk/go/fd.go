package itofin

/*
#include "itofin.h"
*/
import "C"

import "fmt"

// FdScheme selects a supported finite-difference rollback scheme.
type FdScheme int32

const (
	FdDouglas FdScheme = iota
	FdImplicitEuler
)

// FdConfig preserves omitted grid settings. Defaults are a 100 by 100 grid,
// zero damping steps, and the Douglas scheme.
type FdConfig struct {
	TGrid, XGrid, DampingSteps *uint
	Scheme                     *FdScheme
}

type FdBlackScholesVanillaEngine struct{ object }

func (cfg FdConfig) native() C.ItofinFdConfig {
	var c C.ItofinFdConfig
	if cfg.TGrid != nil {
		c.present |= 1
		c.t_grid = C.uint64_t(*cfg.TGrid)
	}
	if cfg.XGrid != nil {
		c.present |= 2
		c.x_grid = C.uint64_t(*cfg.XGrid)
	}
	if cfg.DampingSteps != nil {
		c.present |= 4
		c.damping_steps = C.uint64_t(*cfg.DampingSteps)
	}
	if cfg.Scheme != nil {
		c.present |= 8
		c.scheme = C.int32_t(*cfg.Scheme)
	}
	return c
}

// NewFdBlackScholesVanillaEngine retains the process and supports European,
// American and Bermudan vanilla exercise. Only Douglas and implicit Euler
// schemes are available in the core.
func (s *Session) NewFdBlackScholesVanillaEngine(p *BlackScholesProcess, cfg FdConfig) (*FdBlackScholesVanillaEngine, error) {
	if p == nil {
		return nil, errNilArgument("process")
	}
	if err := sameSession(s, p.object); err != nil {
		return nil, err
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_fd_black_scholes_engine_new(s.ctx, C.uint64_t(p.id), cfg.native(), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &FdBlackScholesVanillaEngine{object{s, uint64(id)}}, nil
}

// SetFdEngine attaches the engine. Delta, gamma and theta are available
// through the existing VanillaOption methods after calculation.
func (o *VanillaOption) SetFdEngine(e *FdBlackScholesVanillaEngine) error {
	if e == nil {
		return errNilArgument("engine")
	}
	return o.setEngine(e.object, 2, 0)
}

// PriceFd attaches and prices through one session operation.
func (o *VanillaOption) PriceFd(e *FdBlackScholesVanillaEngine) (float64, error) {
	if e == nil {
		return 0, errNilArgument("engine")
	}
	return o.price(e.object, 2, 0)
}

func cashDividendArgs(dates []Date, amounts []float64) (*C.int32_t, *C.double, C.size_t, error) {
	if len(dates) != len(amounts) {
		return nil, nil, 0, fmt.Errorf("dividend dates and amounts must have equal lengths")
	}
	if len(dates) == 0 {
		return nil, nil, 0, nil
	}
	serials := make([]C.int32_t, len(dates))
	values := make([]C.double, len(amounts))
	for i := range serials {
		serials[i] = C.int32_t(dates[i].serial)
		values[i] = C.double(amounts[i])
	}
	return &serials[0], &values[0], C.size_t(len(serials)), nil
}

// NewFdBlackScholesVanillaEngineWithCashDividends pays one fixed cash
// dividend amounts[i] on dates[i] under the Spot model; dividends outside the
// option life are dropped. Empty slices give the plain engine. The Escrowed
// cash-dividend model is not offered (deferred to #828). A dividend on the
// evaluation date leaves theta unavailable.
func (s *Session) NewFdBlackScholesVanillaEngineWithCashDividends(p *BlackScholesProcess, cfg FdConfig, dates []Date, amounts []float64) (*FdBlackScholesVanillaEngine, error) {
	if p == nil {
		return nil, errNilArgument("process")
	}
	if err := sameSession(s, p.object); err != nil {
		return nil, err
	}
	d, a, n, err := cashDividendArgs(dates, amounts)
	if err != nil {
		return nil, err
	}
	var id C.uint64_t
	err = s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_fd_black_scholes_engine_with_cash_dividends(s.ctx, C.uint64_t(p.id), cfg.native(), d, a, n, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &FdBlackScholesVanillaEngine{object{s, uint64(id)}}, nil
}

// AnalyticDividendEuropeanEngine prices European vanilla options on a spot
// reduced by the present value of discrete cash dividends.
type AnalyticDividendEuropeanEngine struct{ object }

// NewAnalyticDividendEuropeanEngine retains the process and pays one fixed
// cash dividend amounts[i] on dates[i]. Pricing a non-European exercise fails.
func (s *Session) NewAnalyticDividendEuropeanEngine(p *BlackScholesProcess, dates []Date, amounts []float64) (*AnalyticDividendEuropeanEngine, error) {
	if p == nil {
		return nil, errNilArgument("process")
	}
	if err := sameSession(s, p.object); err != nil {
		return nil, err
	}
	d, a, n, err := cashDividendArgs(dates, amounts)
	if err != nil {
		return nil, err
	}
	var id C.uint64_t
	err = s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_analytic_dividend_european_engine_new(s.ctx, C.uint64_t(p.id), d, a, n, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &AnalyticDividendEuropeanEngine{object{s, uint64(id)}}, nil
}

// SetAnalyticDividendEngine attaches the analytic dividend engine.
func (o *VanillaOption) SetAnalyticDividendEngine(e *AnalyticDividendEuropeanEngine) error {
	if e == nil {
		return errNilArgument("engine")
	}
	return o.setEngine(e.object, 2, 0)
}

// PriceAnalyticDividend attaches the analytic dividend engine and prices.
func (o *VanillaOption) PriceAnalyticDividend(e *AnalyticDividendEuropeanEngine) (float64, error) {
	if e == nil {
		return 0, errNilArgument("engine")
	}
	return o.price(e.object, 2, 0)
}
