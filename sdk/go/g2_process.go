package itofin

/*
#include "itofin.h"
*/
import "C"

// G2Process is an immutable two-factor Gaussian process, initially x=y=0.
// It is not a fitted short-rate model or tree. Matrices use state rows and
// independent-normal columns; instantaneous diffusion uses rho*eta.
type G2Process struct{ object }

// NewG2Process accepts finite signed reversions, nonnegative volatilities and
// correlation in [-1,1]. Zero volatilities and zero reversions are supported.
func (s *Session) NewG2Process(a, sigma, b, eta, rho float64) (*G2Process, error) {
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_g2_process_new(s.ctx, C.double(a), C.double(sigma), C.double(b), C.double(eta), C.double(rho), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &G2Process{object{s, uint64(id)}}, nil
}

func (p *G2Process) checked() error {
	if p == nil {
		return errNilArgument("process")
	}
	return sameSession(p.session, p.object)
}

func g2Vector(x [2]float64) C.ItofinG2Vector {
	return C.ItofinG2Vector{values: [2]C.double{C.double(x[0]), C.double(x[1])}}
}

func (p *G2Process) scalar(kind int32) (float64, error) {
	if err := p.checked(); err != nil {
		return 0, err
	}
	var out C.double
	err := p.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_g2_process_scalar(p.session.ctx, C.uint64_t(p.id), C.int32_t(kind), &out, &e), &e)
	})
	return float64(out), err
}

func (p *G2Process) vector(kind int32, t float64, x [2]float64, dt float64, change [2]float64) ([2]float64, error) {
	if err := p.checked(); err != nil {
		return [2]float64{}, err
	}
	var out C.ItofinG2Vector
	err := p.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_g2_process_vector(p.session.ctx, C.uint64_t(p.id), C.int32_t(kind), C.double(t), g2Vector(x), C.double(dt), g2Vector(change), &out, &e), &e)
	})
	return [2]float64{float64(out.values[0]), float64(out.values[1])}, err
}

func (p *G2Process) matrix(kind int32, t float64, x [2]float64, dt float64) ([2][2]float64, error) {
	if err := p.checked(); err != nil {
		return [2][2]float64{}, err
	}
	var out C.ItofinG2Matrix
	err := p.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_g2_process_matrix(p.session.ctx, C.uint64_t(p.id), C.int32_t(kind), C.double(t), g2Vector(x), C.double(dt), &out, &e), &e)
	})
	return [2][2]float64{{float64(out.values[0]), float64(out.values[1])}, {float64(out.values[2]), float64(out.values[3])}}, err
}

// Size returns the number of state dimensions.
func (p *G2Process) Size() (int, error) { v, err := p.scalar(0); return int(v), err }

// Factors returns the number of independent Gaussian draws.
func (p *G2Process) Factors() (int, error) { v, err := p.scalar(1); return int(v), err }

// X0 returns initial x, always zero.
func (p *G2Process) X0() (float64, error) { return p.scalar(2) }

// Y0 returns initial y, always zero.
func (p *G2Process) Y0() (float64, error) { return p.scalar(3) }

// A returns signed x reversion.
func (p *G2Process) A() (float64, error) { return p.scalar(4) }

// Sigma returns x volatility.
func (p *G2Process) Sigma() (float64, error) { return p.scalar(5) }

// B returns signed y reversion.
func (p *G2Process) B() (float64, error) { return p.scalar(6) }

// Eta returns y volatility.
func (p *G2Process) Eta() (float64, error) { return p.scalar(7) }

// Rho returns instantaneous correlation.
func (p *G2Process) Rho() (float64, error) { return p.scalar(8) }

// InitialValues returns a copied [0,0] state.
func (p *G2Process) InitialValues() ([2]float64, error) {
	return p.vector(0, 0, [2]float64{}, 0, [2]float64{})
}

// Drift returns [-a*x,-b*y] at finite nonnegative time.
func (p *G2Process) Drift(t float64, x [2]float64) ([2]float64, error) {
	return p.vector(1, t, x, 0, [2]float64{})
}

// Expectation returns exact OU conditional means.
func (p *G2Process) Expectation(t float64, x [2]float64, dt float64) ([2]float64, error) {
	return p.vector(2, t, x, dt, [2]float64{})
}

// Apply adds finite component changes and rejects nonfinite sums.
func (p *G2Process) Apply(x, change [2]float64) ([2]float64, error) {
	return p.vector(3, 0, x, 0, change)
}

// Evolve advances exactly using two independent standard Gaussian draws.
func (p *G2Process) Evolve(t float64, x [2]float64, dt float64, draws [2]float64) ([2]float64, error) {
	return p.vector(4, t, x, dt, draws)
}

// Diffusion returns the instantaneous two-by-two loading matrix.
func (p *G2Process) Diffusion(t float64, x [2]float64) ([2][2]float64, error) {
	return p.matrix(0, t, x, 0)
}

// StdDeviation returns the exact step root mapping independent standard normals.
func (p *G2Process) StdDeviation(t float64, x [2]float64, dt float64) ([2][2]float64, error) {
	return p.matrix(1, t, x, dt)
}

// Covariance returns the exact conditional covariance matrix.
func (p *G2Process) Covariance(t float64, x [2]float64, dt float64) ([2][2]float64, error) {
	return p.matrix(2, t, x, dt)
}
