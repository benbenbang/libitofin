package itofin

/*
#include "itofin.h"
*/
import "C"

// AnalyticGJREngine prices European plain-vanilla NPV. Greeks are unavailable.
type AnalyticGJREngine struct{ object }

// MCGJREngine prices European plain-vanilla NPV with a sampling error estimate.
type MCGJREngine struct{ object }

// GJRMCConfig requires exactly one positive step mode and one sample mode.
// Zero selects an unset field. Nonzero seeds reproduce paths; seed zero is randomized.
// Seeds must fit uint32. Antithetic pairs count as one sample observation.
// MaxSamples limits sampling; tolerance mode defaults to 50,000 when unset.
type GJRMCConfig struct {
	Steps, StepsPerYear, Samples uint
	AbsoluteTolerance            float64
	MaxSamples                   uint
	Seed                         uint64
	Antithetic                   bool
}

// NewAnalyticGJREngine retains the live model, including subsequent parameter changes.
func (s *Session) NewAnalyticGJREngine(model *GJRModel) (*AnalyticGJREngine, error) {
	if model == nil {
		return nil, errNilArgument("model")
	}
	if err := sameSession(s, model.object); err != nil {
		return nil, err
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_gjr_analytic_engine_new(s.ctx, C.uint64_t(model.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &AnalyticGJREngine{object{s, uint64(id)}}, nil
}

// NewMCGJREngine retains the process and preserves its selected discretization.
func (s *Session) NewMCGJREngine(process *GJRProcess, config GJRMCConfig) (*MCGJREngine, error) {
	if process == nil {
		return nil, errNilArgument("process")
	}
	if err := sameSession(s, process.object); err != nil {
		return nil, err
	}
	cfg := C.ItofinGjrMcConfig{steps: C.size_t(config.Steps), steps_per_year: C.size_t(config.StepsPerYear), samples: C.size_t(config.Samples), absolute_tolerance: C.double(config.AbsoluteTolerance), max_samples: C.size_t(config.MaxSamples), seed: C.uint64_t(config.Seed)}
	if config.Antithetic {
		cfg.antithetic = 1
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_gjr_mc_engine_new(s.ctx, C.uint64_t(process.id), &cfg, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &MCGJREngine{object{s, uint64(id)}}, nil
}

// SetAnalyticGJREngine attaches a retained analytic engine.
func (o *VanillaOption) SetAnalyticGJREngine(engine *AnalyticGJREngine) error {
	if engine == nil {
		return errNilArgument("engine")
	}
	return o.setEngine(engine.object, 2, 0)
}

// PriceAnalyticGJR attaches and prices in one session operation.
func (o *VanillaOption) PriceAnalyticGJR(engine *AnalyticGJREngine) (float64, error) {
	if engine == nil {
		return 0, errNilArgument("engine")
	}
	return o.price(engine.object, 2, 0)
}

// SetMCGJREngine attaches a retained Monte Carlo engine.
func (o *VanillaOption) SetMCGJREngine(engine *MCGJREngine) error {
	if engine == nil {
		return errNilArgument("engine")
	}
	return o.setEngine(engine.object, 2, 0)
}

// PriceMCGJR attaches and prices in one session operation.
func (o *VanillaOption) PriceMCGJR(engine *MCGJREngine) (float64, error) {
	if engine == nil {
		return 0, errNilArgument("engine")
	}
	return o.price(engine.object, 2, 0)
}
