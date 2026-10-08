package itofin

/*
#include "itofin.h"
*/
import "C"

import (
	"fmt"
	"unsafe"
)

// ParticleSwarm selects seeded synchronous global-best particle swarm.
// Bounds are mandatory and finite. Population and budgets follow DifferentialEvolution.
// Nil coefficients select inertia .7, cognitive/social 1.4 and velocity clamp .2.
// Explicit zero coefficients are preserved. Convergence is a heuristic, not proof
// that the returned best evaluated point is a global optimum.
type ParticleSwarm struct {
	Bounds                                                  [][2]float64
	Seed                                                    uint64
	PopulationSize                                          int
	InitialPopulation                                       [][]float64
	MaxIter, MaxFev                                         int
	XAtol, FAtol, Inertia, Cognitive, Social, VelocityClamp *float64
}

func (ParticleSwarm) optimizeMethod() {}

func (method ParticleSwarm) globalOptions() DifferentialEvolution {
	return DifferentialEvolution{Bounds: method.Bounds, Seed: method.Seed,
		PopulationSize: method.PopulationSize, InitialPopulation: method.InitialPopulation,
		MaxIter: method.MaxIter, MaxFev: method.MaxFev, XAtol: method.XAtol, FAtol: method.FAtol}
}

func validateParticleSwarm(method ParticleSwarm, x0 []float64) error {
	if err := validateDifferentialEvolution(method.globalOptions(), x0); err != nil {
		return err
	}
	for _, coefficient := range []struct {
		value   *float64
		maximum float64
		name    string
	}{
		{method.Inertia, 1, "inertia"}, {method.Cognitive, 4, "cognitive"}, {method.Social, 4, "social"},
	} {
		if coefficient.value != nil && (!validNonnegative(*coefficient.value) || *coefficient.value > coefficient.maximum) {
			return fmt.Errorf("%w: %s must be finite and in [0, %g]", ErrInvalidArgument, coefficient.name, coefficient.maximum)
		}
	}
	if method.VelocityClamp != nil && (!validNonnegative(*method.VelocityClamp) || *method.VelocityClamp == 0 || *method.VelocityClamp > 1) {
		return fmt.Errorf("%w: velocity clamp must be in (0, 1]", ErrInvalidArgument)
	}
	return nil
}

func particleSwarmOptions(method ParticleSwarm) C.ItofinParticleSwarmOptions {
	options := C.ItofinParticleSwarmOptions{global: differentialEvolutionOptions(method.globalOptions()).global}
	if method.Inertia != nil {
		options.inertia, options.has_inertia = C.double(*method.Inertia), true
	}
	if method.Cognitive != nil {
		options.cognitive, options.has_cognitive = C.double(*method.Cognitive), true
	}
	if method.Social != nil {
		options.social, options.has_social = C.double(*method.Social), true
	}
	if method.VelocityClamp != nil {
		options.velocity_clamp, options.has_velocity_clamp = C.double(*method.VelocityClamp), true
	}
	return options
}

func runParticleSwarm(objective *C.ItofinObjective, x0 []float64, method ParticleSwarm, out *C.ItofinOptimizeResult, e *C.ItofinError) C.int32_t {
	lower, upper := make([]float64, len(x0)), make([]float64, len(x0))
	for i, pair := range method.Bounds {
		lower[i], upper[i] = pair[0], pair[1]
	}
	population := make([]float64, 0, len(method.InitialPopulation)*len(x0))
	for _, row := range method.InitialPopulation {
		population = append(population, row...)
	}
	options := particleSwarmOptions(method)
	return C.itofin_optimize_particle_swarm(objective,
		(*C.double)(unsafe.Pointer(unsafe.SliceData(x0))), C.size_t(len(x0)),
		ratesPtr(lower), C.size_t(len(lower)), ratesPtr(upper), C.size_t(len(upper)),
		ratesPtr(population), C.size_t(len(method.InitialPopulation)), C.size_t(len(population)), &options, out, e)
}
