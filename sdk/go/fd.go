package itofin

/*
#include "itofin.h"
*/
import "C"

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
