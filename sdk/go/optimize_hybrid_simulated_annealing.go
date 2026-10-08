package itofin

/*
#include "itofin.h"
*/
import "C"

import "fmt"

// HybridSimulatedAnnealing selects seeded single-chain annealing with coordinate polish.
// Bounds are mandatory and finite. Nil controls select temperature 1, cooling .95,
// normalized proposal step .25, local interval 10, local steps 4 and reannealing 100.
// Nil tolerances select 1e-6/1e-8; zero budgets select the shared global defaults.
// Convergence describes local polish, not proof of a global optimum.
type HybridSimulatedAnnealing struct {
	Bounds                                                  [][2]float64
	Seed                                                    uint64
	MaxIter, MaxFev                                         int
	XAtol, FAtol, InitialTemperature, CoolingRate, StepSize *float64
	LocalSearchInterval, LocalSearchSteps, ReannealInterval *int
}

func (HybridSimulatedAnnealing) optimizeMethod() {}

func validateHybridSimulatedAnnealing(method HybridSimulatedAnnealing, x0 []float64) error {
	shared := DifferentialEvolution{Bounds: method.Bounds, Seed: method.Seed,
		MaxIter: method.MaxIter, MaxFev: method.MaxFev, XAtol: method.XAtol, FAtol: method.FAtol}
	if err := validateDifferentialEvolution(shared, x0); err != nil {
		return err
	}
	if method.InitialTemperature != nil && (!validNonnegative(*method.InitialTemperature) || *method.InitialTemperature == 0) {
		return fmt.Errorf("%w: initial temperature must be finite and positive", ErrInvalidArgument)
	}
	if method.CoolingRate != nil && (!validNonnegative(*method.CoolingRate) || *method.CoolingRate == 0 || *method.CoolingRate >= 1) {
		return fmt.Errorf("%w: cooling rate must be in (0, 1)", ErrInvalidArgument)
	}
	if method.StepSize != nil && (!validNonnegative(*method.StepSize) || *method.StepSize == 0 || *method.StepSize > 1) {
		return fmt.Errorf("%w: step size must be in (0, 1]", ErrInvalidArgument)
	}
	for _, control := range []struct {
		value   *int
		maximum int
		name    string
	}{
		{method.LocalSearchInterval, 1_000_000, "local search interval"},
		{method.LocalSearchSteps, 256, "local search steps"},
		{method.ReannealInterval, 1_000_000, "reanneal interval"},
	} {
		if control.value != nil && (*control.value < 1 || *control.value > control.maximum) {
			return fmt.Errorf("%w: %s must be in 1..%d", ErrInvalidArgument, control.name, control.maximum)
		}
	}
	return nil
}

func hybridSimulatedAnnealingOptions(method HybridSimulatedAnnealing) C.ItofinHybridSimulatedAnnealingOptions {
	options := C.ItofinHybridSimulatedAnnealingOptions{seed: C.uint64_t(method.Seed), maxiter: C.size_t(method.MaxIter), maxfev: C.size_t(method.MaxFev)}
	if method.XAtol != nil {
		options.xatol, options.has_xatol = C.double(*method.XAtol), true
	}
	if method.FAtol != nil {
		options.fatol, options.has_fatol = C.double(*method.FAtol), true
	}
	if method.InitialTemperature != nil {
		options.initial_temperature, options.has_initial_temperature = C.double(*method.InitialTemperature), true
	}
	if method.CoolingRate != nil {
		options.cooling_rate, options.has_cooling_rate = C.double(*method.CoolingRate), true
	}
	if method.StepSize != nil {
		options.step_size, options.has_step_size = C.double(*method.StepSize), true
	}
	if method.LocalSearchInterval != nil {
		options.local_search_interval, options.has_local_search_interval = C.size_t(*method.LocalSearchInterval), true
	}
	if method.LocalSearchSteps != nil {
		options.local_search_steps, options.has_local_search_steps = C.size_t(*method.LocalSearchSteps), true
	}
	if method.ReannealInterval != nil {
		options.reanneal_interval, options.has_reanneal_interval = C.size_t(*method.ReannealInterval), true
	}
	return options
}

func runHybridSimulatedAnnealing(objective *C.ItofinObjective, x0 []float64, method HybridSimulatedAnnealing, out *C.ItofinOptimizeResult, e *C.ItofinError) C.int32_t {
	lower, upper := make([]float64, len(x0)), make([]float64, len(x0))
	for i, pair := range method.Bounds {
		lower[i], upper[i] = pair[0], pair[1]
	}
	options := hybridSimulatedAnnealingOptions(method)
	return C.itofin_optimize_hybrid_simulated_annealing(objective, ratesPtr(x0), C.size_t(len(x0)),
		ratesPtr(lower), C.size_t(len(lower)), ratesPtr(upper), C.size_t(len(upper)), &options, out, e)
}
