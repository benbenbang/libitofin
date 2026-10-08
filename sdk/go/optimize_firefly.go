package itofin

/*
#include "itofin.h"
*/
import "C"

import (
	"fmt"
	"unsafe"
)

// Firefly selects seeded bounded pairwise attraction with random perturbations.
// Population, seed, bounds and budgets follow DifferentialEvolution.
// Nil coefficients select alpha .25, beta0 1, gamma 1 and decay .97.
// Explicit zero alpha/gamma are supported. Convergence is not a global guarantee.
type Firefly struct {
	Bounds                                        [][2]float64
	Seed                                          uint64
	PopulationSize                                int
	InitialPopulation                             [][]float64
	MaxIter, MaxFev                               int
	XAtol, FAtol, Alpha, Beta0, Gamma, AlphaDecay *float64
}

func (Firefly) optimizeMethod() {}

func (method Firefly) globalOptions() DifferentialEvolution {
	return DifferentialEvolution{Bounds: method.Bounds, Seed: method.Seed,
		PopulationSize: method.PopulationSize, InitialPopulation: method.InitialPopulation,
		MaxIter: method.MaxIter, MaxFev: method.MaxFev, XAtol: method.XAtol, FAtol: method.FAtol}
}

func validateFirefly(method Firefly, x0 []float64) error {
	if err := validateDifferentialEvolution(method.globalOptions(), x0); err != nil {
		return err
	}
	for _, coefficient := range []struct {
		value     *float64
		maximum   float64
		allowZero bool
		name      string
	}{
		{method.Alpha, 1, true, "alpha"}, {method.Beta0, 1, false, "beta0"},
		{method.Gamma, 1_000_000, true, "gamma"}, {method.AlphaDecay, 1, false, "alpha decay"},
	} {
		if coefficient.value != nil && (!validNonnegative(*coefficient.value) || *coefficient.value > coefficient.maximum || (!coefficient.allowZero && *coefficient.value == 0)) {
			return fmt.Errorf("%w: invalid firefly %s", ErrInvalidArgument, coefficient.name)
		}
	}
	return nil
}

func fireflyOptions(method Firefly) C.ItofinFireflyOptions {
	options := C.ItofinFireflyOptions{global: differentialEvolutionOptions(method.globalOptions()).global}
	if method.Alpha != nil {
		options.alpha, options.has_alpha = C.double(*method.Alpha), true
	}
	if method.Beta0 != nil {
		options.beta0, options.has_beta0 = C.double(*method.Beta0), true
	}
	if method.Gamma != nil {
		options.gamma, options.has_gamma = C.double(*method.Gamma), true
	}
	if method.AlphaDecay != nil {
		options.alpha_decay, options.has_alpha_decay = C.double(*method.AlphaDecay), true
	}
	return options
}

func runFirefly(objective *C.ItofinObjective, x0 []float64, method Firefly, out *C.ItofinOptimizeResult, e *C.ItofinError) C.int32_t {
	lower, upper := make([]float64, len(x0)), make([]float64, len(x0))
	for i, pair := range method.Bounds {
		lower[i], upper[i] = pair[0], pair[1]
	}
	population := make([]float64, 0, len(method.InitialPopulation)*len(x0))
	for _, row := range method.InitialPopulation {
		population = append(population, row...)
	}
	options := fireflyOptions(method)
	return C.itofin_optimize_firefly(objective,
		(*C.double)(unsafe.Pointer(unsafe.SliceData(x0))), C.size_t(len(x0)),
		ratesPtr(lower), C.size_t(len(lower)), ratesPtr(upper), C.size_t(len(upper)),
		ratesPtr(population), C.size_t(len(method.InitialPopulation)), C.size_t(len(population)), &options, out, e)
}
