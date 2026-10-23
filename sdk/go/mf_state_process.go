package itofin

/*
#include "itofin.h"
*/
import "C"
import "fmt"

// MfStateProcess is an immutable standalone Gaussian state process, not a
// fitted Markov-functional interest-rate model. Times are process years.
type MfStateProcess struct{ object }

// NewMfStateProcess copies a signed reversion and a volatility bucket grid.
// Times must increase and volatility count must exceed time count by one.
// Empty times retain native unit-volatility variance, regardless of vols[0].
func (s *Session) NewMfStateProcess(reversion float64, times, vols []float64) (*MfStateProcess, error) {
	times = append([]float64(nil), times...)
	vols = append([]float64(nil), vols...)
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_mf_state_process_new(s.ctx, C.double(reversion), doubles(times), C.size_t(len(times)), doubles(vols), C.size_t(len(vols)), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &MfStateProcess{object{s, uint64(id)}}, nil
}

func (p *MfStateProcess) checked() error {
	if p == nil {
		return errNilArgument("process")
	}
	return sameSession(p.session, p.object)
}

func (p *MfStateProcess) query(kind int32, t, x, dt, dw float64) (float64, error) {
	if err := p.checked(); err != nil {
		return 0, err
	}
	var out C.double
	err := p.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_mf_state_process_query(p.session.ctx, C.uint64_t(p.id), C.int32_t(kind), C.double(t), C.double(x), C.double(dt), C.double(dw), &out, &e), &e)
	})
	return float64(out), err
}

func (p *MfStateProcess) grid(kind int32) ([]float64, error) {
	if err := p.checked(); err != nil {
		return nil, err
	}
	var values []float64
	err := p.session.invoke(func() error {
		var n C.size_t
		var e C.ItofinError
		if err := ffiError(C.itofin_mf_state_process_grid(p.session.ctx, C.uint64_t(p.id), C.int32_t(kind), nil, 0, &n, &e), &e); err != nil {
			return err
		}
		if uint64(n) > uint64(^uint(0)>>1) {
			return fmt.Errorf("MF state grid exceeds Go slice range")
		}
		values = make([]float64, int(n))
		return ffiError(C.itofin_mf_state_process_grid(p.session.ctx, C.uint64_t(p.id), C.int32_t(kind), doubles(values), n, &n, &e), &e)
	})
	return values, err
}

// Reversion returns the signed exponential reversion parameter.
func (p *MfStateProcess) Reversion() (float64, error) { return p.query(0, 0, 0, 0, 0) }

// X0 returns zero, the initial process state.
func (p *MfStateProcess) X0() (float64, error) { return p.query(1, 0, 0, 0, 0) }

// Times returns a new copy of the right-continuous bucket boundaries.
func (p *MfStateProcess) Times() ([]float64, error) { return p.grid(0) }

// Volatilities returns a new copy of the raw bucket volatilities.
func (p *MfStateProcess) Volatilities() ([]float64, error) { return p.grid(1) }

// Drift returns zero after validating process time and state.
func (p *MfStateProcess) Drift(t, x float64) (float64, error) { return p.query(2, t, x, 0, 0) }

// Diffusion returns raw sigma(t), not sigma(t)*exp(reversion*t).
func (p *MfStateProcess) Diffusion(t, x float64) (float64, error) { return p.query(3, t, x, 0, 0) }

// Expectation returns the unchanged state after validating the interval.
func (p *MfStateProcess) Expectation(t, x, dt float64) (float64, error) {
	return p.query(4, t, x, dt, 0)
}

// StdDeviation returns the exact conditional standard deviation.
func (p *MfStateProcess) StdDeviation(t, x, dt float64) (float64, error) {
	return p.query(5, t, x, dt, 0)
}

// Variance integrates sigma(s)^2*exp(2*reversion*s) over the interval.
func (p *MfStateProcess) Variance(t, x, dt float64) (float64, error) {
	return p.query(6, t, x, dt, 0)
}

// Evolve advances using one finite standard Gaussian draw.
func (p *MfStateProcess) Evolve(t, x, dt, dw float64) (float64, error) {
	return p.query(7, t, x, dt, dw)
}
