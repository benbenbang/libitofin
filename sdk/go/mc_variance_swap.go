package itofin

/*
#include "itofin.h"
*/
import "C"

// MCVarianceSwapEngine integrates log-price diffusion squared along seeded paths.
type MCVarianceSwapEngine struct{ object }

// MCVarianceSwapConfig requires exactly one positive grid and one sampling mode.
// Zero selects an unset option. Tolerance is in annualized variance units.
// Tolerance mode starts at 1023 samples and defaults to MaxSamples 50,000.
// Nonzero uint32 seeds reproduce paths; seed zero is randomized. Samples need >=2.
// Caps are 100,000 steps, 1,000,000 samples and 100,000,000 estimated path work.
type MCVarianceSwapConfig struct {
	Steps, StepsPerYear, Samples uint
	AbsoluteTolerance            float64
	MaxSamples                   uint
	Seed                         uint64
}

// NewMCVarianceSwapEngine retains the live process, not a frozen market snapshot.
func (s *Session) NewMCVarianceSwapEngine(process *BlackScholesProcess, config MCVarianceSwapConfig) (*MCVarianceSwapEngine, error) {
	if process == nil {
		return nil, errNilArgument("process")
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		if err := sameSession(s, process.object); err != nil {
			return err
		}
		cfg := C.ItofinVarianceSwapMcConfig{steps: C.size_t(config.Steps), steps_per_year: C.size_t(config.StepsPerYear), samples: C.size_t(config.Samples), absolute_tolerance: C.double(config.AbsoluteTolerance), max_samples: C.size_t(config.MaxSamples), seed: C.uint64_t(config.Seed)}
		var e C.ItofinError
		return ffiError(C.itofin_mc_variance_swap_engine_new(s.ctx, C.uint64_t(process.id), &cfg, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &MCVarianceSwapEngine{object{s, uint64(id)}}, nil
}

// SetMCEngine attaches a retained typed MC engine without changing SetEngine.
func (v *VarianceSwap) SetMCEngine(engine *MCVarianceSwapEngine) error {
	if v == nil || engine == nil {
		return errNilArgument("variance swap or engine")
	}
	s := v.session
	return s.invoke(func() error {
		if err := sameSession(s, v.object, engine.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_variance_swap_set_engine(s.ctx, C.uint64_t(v.id), C.uint64_t(engine.id), &e), &e)
	})
}

// ErrorEstimate returns native signed monetary sampling error, negative for shorts.
func (v *VarianceSwap) ErrorEstimate() (float64, error) { return v.value(4) }

// VarianceError returns nonnegative annualized variance sampling standard error.
// Replicating engines and expired MC results do not provide this statistic.
func (v *VarianceSwap) VarianceError() (float64, error) { return v.value(5) }

// Samples returns actual MC observations, unavailable for replication or expiry.
func (v *VarianceSwap) Samples() (uint, error) {
	if v == nil {
		return 0, errNilArgument("variance swap")
	}
	var out C.size_t
	s := v.session
	err := s.invoke(func() error {
		if err := sameSession(s, v.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_variance_swap_samples(s.ctx, C.uint64_t(v.id), &out, &e), &e)
	})
	return uint(out), err
}
