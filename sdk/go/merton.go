package itofin

/*
#include "itofin.h"
*/
import "C"
import "fmt"

// Merton76Process retains observable market inputs and jump quotes.
type Merton76Process struct{ object }

// JumpDiffusionEngine prices European plain-vanilla options by a Poisson series.
type JumpDiffusionEngine struct{ object }

// NewMerton76Process retains all inputs after their external handles close.
func (s *Session) NewMerton76Process(spot *SimpleQuote, riskFree, dividend *YieldTermStructure, volatility *BlackVolTermStructure, jumpIntensity, logMeanJump, logJumpVolatility *SimpleQuote) (*Merton76Process, error) {
	if spot == nil || riskFree == nil || dividend == nil || volatility == nil || jumpIntensity == nil || logMeanJump == nil || logJumpVolatility == nil {
		return nil, errNilArgument("market or jump quote")
	}
	if err := sameSession(s, spot.object, riskFree.object, dividend.object, volatility.object, jumpIntensity.object, logMeanJump.object, logJumpVolatility.object); err != nil {
		return nil, err
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_merton76_new(s.ctx, C.uint64_t(spot.id), C.uint64_t(riskFree.id), C.uint64_t(dividend.id), C.uint64_t(volatility.id), C.uint64_t(jumpIntensity.id), C.uint64_t(logMeanJump.id), C.uint64_t(logJumpVolatility.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &Merton76Process{object{s, uint64(id)}}, nil
}

func (p *Merton76Process) value(field int32) (float64, error) {
	if p == nil {
		return 0, errNilArgument("process")
	}
	if err := sameSession(p.session, p.object); err != nil {
		return 0, err
	}
	var value C.double
	err := p.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_merton76_value(p.session.ctx, C.uint64_t(p.id), C.int32_t(field), &value, &e), &e)
	})
	return float64(value), err
}

// Spot returns the retained market's current spot.
func (p *Merton76Process) Spot() (float64, error) { return p.value(0) }

// JumpIntensity returns the current annual Poisson intensity.
func (p *Merton76Process) JumpIntensity() (float64, error) { return p.value(1) }

// LogMeanJump returns the current mean of the normally distributed log jump.
func (p *Merton76Process) LogMeanJump() (float64, error) { return p.value(2) }

// LogJumpVolatility returns the current standard deviation of the log jump.
func (p *Merton76Process) LogJumpVolatility() (float64, error) { return p.value(3) }

// Time converts a date using the retained market's risk-free day counter.
func (p *Merton76Process) Time(date Date) (float64, error) {
	if p == nil {
		return 0, errNilArgument("process")
	}
	if err := sameSession(p.session, p.object); err != nil {
		return 0, err
	}
	var value C.double
	err := p.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_merton76_time(p.session.ctx, C.uint64_t(p.id), C.int32_t(date.Serial()), &value, &e), &e)
	})
	return float64(value), err
}

// NewJumpDiffusionEngine retains its process. Conventional settings are 1e-4
// relative accuracy and 100 iterations; maxIterations must be in [1, 100000].
func (s *Session) NewJumpDiffusionEngine(process *Merton76Process, relativeAccuracy float64, maxIterations int) (*JumpDiffusionEngine, error) {
	if process == nil {
		return nil, errNilArgument("process")
	}
	if maxIterations < 1 || maxIterations > 100000 {
		return nil, fmt.Errorf("itofin: max iterations must be in [1, 100000]")
	}
	if err := sameSession(s, process.object); err != nil {
		return nil, err
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_jump_diffusion_engine_new(s.ctx, C.uint64_t(process.id), C.double(relativeAccuracy), C.size_t(maxIterations), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &JumpDiffusionEngine{object{s, uint64(id)}}, nil
}

// SetJumpDiffusionEngine attaches a retained Merton pricing engine.
func (o *VanillaOption) SetJumpDiffusionEngine(engine *JumpDiffusionEngine) error {
	if engine == nil {
		return errNilArgument("engine")
	}
	return o.setEngine(engine.object, 2, 0)
}

// PriceJumpDiffusion attaches and prices within one session operation.
func (o *VanillaOption) PriceJumpDiffusion(engine *JumpDiffusionEngine) (float64, error) {
	if engine == nil {
		return 0, errNilArgument("engine")
	}
	return o.price(engine.object, 2, 0)
}
