package itofin

/*
#include "itofin.h"
*/
import "C"

// MCConfig preserves omitted versus explicit zero settings. Engines use the
// native PseudoRandom policy; identical configuration with an explicit nonzero
// seed repeats exactly. An omitted or zero seed uses native seed generation.
type MCConfig struct {
	Steps, StepsPerYear, Samples        *uint
	AbsoluteTolerance                   *float64
	MaxSamples                          *uint
	Seed                                *uint32
	Antithetic                          *bool
	PolynomialOrder, CalibrationSamples *uint
}
type MCEuropeanEngine struct{ object }
type MCEuropeanHestonEngine struct{ object }
type MCAmericanEngine struct{ object }

func (cfg MCConfig) native() C.McConfig {
	var c C.McConfig
	if cfg.Steps != nil {
		c.present |= 1
		c.steps = C.size_t(*cfg.Steps)
	}
	if cfg.StepsPerYear != nil {
		c.present |= 2
		c.steps_per_year = C.size_t(*cfg.StepsPerYear)
	}
	if cfg.Samples != nil {
		c.present |= 4
		c.samples = C.size_t(*cfg.Samples)
	}
	if cfg.AbsoluteTolerance != nil {
		c.present |= 8
		c.absolute_tolerance = C.double(*cfg.AbsoluteTolerance)
	}
	if cfg.MaxSamples != nil {
		c.present |= 16
		c.max_samples = C.size_t(*cfg.MaxSamples)
	}
	if cfg.Seed != nil {
		c.present |= 32
		c.seed = C.uint32_t(*cfg.Seed)
	}
	if cfg.Antithetic != nil {
		c.present |= 64
		if *cfg.Antithetic {
			c.antithetic = 1
		}
	}
	if cfg.PolynomialOrder != nil {
		c.present |= 128
		c.polynomial_order = C.size_t(*cfg.PolynomialOrder)
	}
	if cfg.CalibrationSamples != nil {
		c.present |= 256
		c.calibration_samples = C.size_t(*cfg.CalibrationSamples)
	}
	return c
}
func (s *Session) newMCEngine(process object, kind int32, cfg MCConfig) (object, error) {
	if err := sameSession(s, process); err != nil {
		return object{}, err
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_mc_engine_new(s.ctx, C.uint64_t(process.id), C.int32_t(kind), cfg.native(), &id, &e), &e)
	})
	if err != nil {
		return object{}, err
	}
	return object{s, uint64(id)}, nil
}
func (s *Session) NewMCEuropeanEngine(p *BlackScholesProcess, cfg MCConfig) (*MCEuropeanEngine, error) {
	if p == nil {
		return nil, errNilArgument("process")
	}
	o, err := s.newMCEngine(p.object, 0, cfg)
	if err != nil {
		return nil, err
	}
	return &MCEuropeanEngine{o}, nil
}
func (s *Session) NewMCEuropeanHestonEngine(p *HestonProcess, cfg MCConfig) (*MCEuropeanHestonEngine, error) {
	if p == nil {
		return nil, errNilArgument("process")
	}
	o, err := s.newMCEngine(p.object, 1, cfg)
	if err != nil {
		return nil, err
	}
	return &MCEuropeanHestonEngine{o}, nil
}
func (s *Session) NewMCAmericanEngine(p *BlackScholesProcess, cfg MCConfig) (*MCAmericanEngine, error) {
	if p == nil {
		return nil, errNilArgument("process")
	}
	o, err := s.newMCEngine(p.object, 2, cfg)
	if err != nil {
		return nil, err
	}
	return &MCAmericanEngine{o}, nil
}
