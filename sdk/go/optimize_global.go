package itofin

/*
#include "itofin.h"
*/
import "C"

import (
	"fmt"
	"math"
	"unsafe"
)

// DifferentialEvolution selects deterministic, bounded DE/rand/1/bin search.
// Bounds are mandatory and finite. PopulationSize is the actual row count;
// zero selects max(8, 15*free dimensions), or the supplied initial row count.
// InitialPopulation is preserved exactly. Nil scalar options select defaults:
// x/f tolerances 1e-6/1e-8, mutation .8, recombination .9. Seed zero is fixed.
type DifferentialEvolution struct {
	Bounds                                [][2]float64
	Seed                                  uint64
	PopulationSize                        int
	InitialPopulation                     [][]float64
	MaxIter, MaxFev                       int
	XAtol, FAtol, Mutation, Recombination *float64
}

func (DifferentialEvolution) optimizeMethod() {}

func validateDifferentialEvolution(method DifferentialEvolution, x0 []float64) error {
	invalid := func(message string) error { return fmt.Errorf("%w: %s", ErrInvalidArgument, message) }
	if len(x0) == 0 || len(x0) > 256 || len(method.Bounds) != len(x0) {
		return invalid("global dimensions must be in 1..256 and bounds must match x0")
	}
	if method.MaxIter < 0 || method.MaxIter > 1_000_000 || method.MaxFev < 0 || method.MaxFev > 10_000_000 {
		return invalid("global budget exceeds resource limits")
	}
	if method.PopulationSize != 0 && (method.PopulationSize < 4 || method.PopulationSize > 4096) {
		return invalid("population size must be in 4..4096")
	}
	for _, tolerance := range []*float64{method.XAtol, method.FAtol} {
		if tolerance != nil && !validNonnegative(*tolerance) {
			return invalid("global tolerances must be finite and nonnegative")
		}
	}
	if method.Mutation != nil && (!validNonnegative(*method.Mutation) || *method.Mutation == 0 || *method.Mutation > 2) {
		return invalid("mutation must be in (0, 2]")
	}
	if method.Recombination != nil && (!validNonnegative(*method.Recombination) || *method.Recombination > 1) {
		return invalid("recombination must be in [0, 1]")
	}
	for i, pair := range method.Bounds {
		if math.IsNaN(pair[0]) || math.IsNaN(pair[1]) || math.IsInf(pair[0], 0) || math.IsInf(pair[1], 0) ||
			pair[0] > pair[1] || math.IsInf(pair[1]-pair[0], 0) ||
			math.IsNaN(x0[i]) || math.IsInf(x0[i], 0) || x0[i] < pair[0] || x0[i] > pair[1] {
			return invalid("global bounds and x0 must be finite, ordered and in range")
		}
	}
	rows := len(method.InitialPopulation)
	if method.InitialPopulation != nil {
		if rows < 4 || rows > 4096 || (method.PopulationSize != 0 && method.PopulationSize != rows) {
			return invalid("initial population row count must match population size and be in 4..4096")
		}
		if rows*len(x0) > 1_000_000 {
			return invalid("initial population exceeds resource limits")
		}
		for _, row := range method.InitialPopulation {
			if len(row) != len(x0) {
				return invalid("initial population rows must match x0")
			}
			for i, value := range row {
				if math.IsNaN(value) || math.IsInf(value, 0) || value < method.Bounds[i][0] || value > method.Bounds[i][1] {
					return invalid("initial population must be finite and inside bounds")
				}
			}
		}
	}
	if method.PopulationSize*len(x0) > 1_000_000 {
		return invalid("population exceeds resource limits")
	}
	return nil
}

func differentialEvolutionOptions(method DifferentialEvolution) C.ItofinDifferentialEvolutionOptions {
	options := C.ItofinDifferentialEvolutionOptions{
		global: C.ItofinGlobalOptions{seed: C.uint64_t(method.Seed), population_size: C.size_t(method.PopulationSize),
			maxiter: C.size_t(method.MaxIter), maxfev: C.size_t(method.MaxFev)},
	}
	if method.XAtol != nil {
		options.global.xatol, options.global.has_xatol = C.double(*method.XAtol), true
	}
	if method.FAtol != nil {
		options.global.fatol, options.global.has_fatol = C.double(*method.FAtol), true
	}
	if method.Mutation != nil {
		options.mutation, options.has_mutation = C.double(*method.Mutation), true
	}
	if method.Recombination != nil {
		options.recombination, options.has_recombination = C.double(*method.Recombination), true
	}
	return options
}

func runDifferentialEvolution(objective *C.ItofinObjective, x0 []float64, method DifferentialEvolution, out *C.ItofinOptimizeResult, e *C.ItofinError) C.int32_t {
	lower, upper := make([]float64, len(x0)), make([]float64, len(x0))
	for i, pair := range method.Bounds {
		lower[i], upper[i] = pair[0], pair[1]
	}
	population := make([]float64, 0, len(method.InitialPopulation)*len(x0))
	for _, row := range method.InitialPopulation {
		population = append(population, row...)
	}
	options := differentialEvolutionOptions(method)
	return C.itofin_optimize_differential_evolution(objective,
		(*C.double)(unsafe.Pointer(unsafe.SliceData(x0))), C.size_t(len(x0)),
		(*C.double)(unsafe.Pointer(unsafe.SliceData(lower))), C.size_t(len(lower)),
		(*C.double)(unsafe.Pointer(unsafe.SliceData(upper))), C.size_t(len(upper)),
		(*C.double)(unsafe.Pointer(unsafe.SliceData(population))), C.size_t(len(method.InitialPopulation)), C.size_t(len(population)),
		&options, out, e)
}
