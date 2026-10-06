package itofin

/*
#include "itofin.h"
*/
import "C"

// GeometricBrownianMotionProcess uses scalar coefficients and Euler transitions.
// Signed finite states are supported. This is not exact lognormal sampling.
type GeometricBrownianMotionProcess struct{ object }

// NewGeometricBrownianMotionProcess requires finite initial and mu, and finite
// nonnegative volatility. The immutable process belongs to this session.
func (s *Session) NewGeometricBrownianMotionProcess(initial, mu, volatility float64) (*GeometricBrownianMotionProcess, error) {
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_geometric_brownian_new(s.ctx, C.double(initial), C.double(mu), C.double(volatility), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &GeometricBrownianMotionProcess{object{s, uint64(id)}}, nil
}

func (p *GeometricBrownianMotionProcess) query(kind int32, t, state, dt, draw float64) (float64, error) {
	if p == nil {
		return 0, errNilArgument("process")
	}
	if err := sameSession(p.session, p.object); err != nil {
		return 0, err
	}
	var out C.double
	err := p.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_geometric_brownian_query(p.session.ctx, C.uint64_t(p.id), C.int32_t(kind), C.double(t), C.double(state), C.double(dt), C.double(draw), &out, &e), &e)
	})
	return float64(out), err
}

// X0 returns the constructor's initial state.
func (p *GeometricBrownianMotionProcess) X0() (float64, error) {
	return p.query(0, 0, 0, 0, 0)
}

// Mu returns the constant proportional drift.
func (p *GeometricBrownianMotionProcess) Mu() (float64, error) {
	return p.query(1, 0, 0, 0, 0)
}

// Volatility returns the nonnegative proportional diffusion coefficient.
func (p *GeometricBrownianMotionProcess) Volatility() (float64, error) {
	return p.query(2, 0, 0, 0, 0)
}

// Drift returns mu times the state, with finite nonnegative time.
func (p *GeometricBrownianMotionProcess) Drift(t, state float64) (float64, error) {
	return p.query(3, t, state, 0, 0)
}

// Diffusion returns volatility times the signed state.
func (p *GeometricBrownianMotionProcess) Diffusion(t, state float64) (float64, error) {
	return p.query(4, t, state, 0, 0)
}

// Expectation returns state plus mu*state*dt, not the exact exponential mean.
func (p *GeometricBrownianMotionProcess) Expectation(t, state, dt float64) (float64, error) {
	return p.query(5, t, state, dt, 0)
}

// Variance returns (volatility*state)^2*dt.
func (p *GeometricBrownianMotionProcess) Variance(t, state, dt float64) (float64, error) {
	return p.query(6, t, state, dt, 0)
}

// StdDeviation returns volatility*state*sqrt(dt), preserving the state's sign.
func (p *GeometricBrownianMotionProcess) StdDeviation(t, state, dt float64) (float64, error) {
	return p.query(7, t, state, dt, 0)
}

// Evolve adds StdDeviation times a finite standard Gaussian draw to Expectation.
// Euler transitions may cross zero. Times and dt must be finite and nonnegative.
func (p *GeometricBrownianMotionProcess) Evolve(t, state, dt, draw float64) (float64, error) {
	return p.query(8, t, state, dt, draw)
}
