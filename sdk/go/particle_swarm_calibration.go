package itofin

/*
#include "itofin.h"
*/
import "C"

import "fmt"

// ParticleSwarmMethod is a session-owned bounded calibration optimizer.
// Its box and population follow the projected free-parameter order.
type ParticleSwarmMethod struct {
	object
	dimension int
}

func (m *ParticleSwarmMethod) optimizationObject() object {
	if m == nil {
		return object{}
	}
	return m.object
}

// NewParticleSwarm creates a deterministic global calibration optimizer.
// Supply a wholly feasible finite Bounds box; invalid model candidates abort
// before pricing. ParticleSwarm options are shared with scalar Minimize.
func (s *Session) NewParticleSwarm(options ParticleSwarm) (*ParticleSwarmMethod, error) {
	n := len(options.Bounds)
	if n == 0 || n > 256 {
		return nil, fmt.Errorf("%w: global dimensions must be in 1..256", ErrInvalidArgument)
	}
	lower, upper := make([]float64, n), make([]float64, n)
	for i, pair := range options.Bounds {
		lower[i], upper[i] = pair[0], pair[1]
	}
	if err := validateParticleSwarm(options, lower); err != nil {
		return nil, err
	}
	population := make([]float64, 0, len(options.InitialPopulation)*n)
	for _, row := range options.InitialPopulation {
		population = append(population, row...)
	}
	encoded := particleSwarmOptions(options)
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_particle_swarm_new(s.ctx,
			ratesPtr(lower), C.size_t(n), ratesPtr(upper), C.size_t(n), &encoded,
			ratesPtr(population), C.size_t(len(options.InitialPopulation)), C.size_t(len(population)), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &ParticleSwarmMethod{object{s, uint64(id)}, n}, nil
}

// LastResult copies the last global outcome without repricing or mutating a model.
// It errors when no completed global invocation exists, including after invalid
// input or a failed objective inside the solver. Earlier model preflight failures
// retain the previous diagnostics. Exhaustion returns a nonsuccessful result.
func (m *ParticleSwarmMethod) LastResult() (OptimizeResult, error) {
	if m == nil || m.session == nil {
		return OptimizeResult{}, errNilArgument("method")
	}
	x := make([]float64, m.dimension)
	var pins runtimePinner
	pins.pin(x)
	defer pins.unpin()
	var native C.ItofinOptimizeResult
	native.x = ratesPtr(x)
	err := m.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_particle_swarm_result(m.session.ctx, C.uint64_t(m.id),
			C.size_t(m.dimension), &native, &e), &e)
	})
	if err != nil {
		return OptimizeResult{}, err
	}
	if native.nit > C.size_t(^uint(0)>>1) || native.nfev > C.size_t(^uint(0)>>1) {
		return OptimizeResult{}, fmt.Errorf("%w: result counts exceed Go int", ErrInvalidArgument)
	}
	status := OptimizeStatus(native.status)
	return OptimizeResult{X: x, Fun: float64(native.fun), Nit: int(native.nit), Nfev: int(native.nfev),
		Njev: int(native.njev), Status: status, Success: bool(native.success), Message: status.String()}, nil
}
