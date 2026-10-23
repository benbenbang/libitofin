package itofin

/*
#include "itofin.h"
*/
import "C"

// GsrProcess is a standalone constant-parameter Gaussian forward-measure process.
// It is not a fitted GSR model. Times are process years, not dates.
type GsrProcess struct{ object }

// NewGsrProcess copies a validated constant grid. Signed reversion, zero
// volatility and zero horizon are valid. Varying parameters are unsupported.
func (s *Session) NewGsrProcess(times, vols, reversions []float64, tMeasure float64) (*GsrProcess, error) {
	times = append([]float64(nil), times...)
	vols = append([]float64(nil), vols...)
	reversions = append([]float64(nil), reversions...)
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_gsr_process_new(s.ctx, doubles(times), C.size_t(len(times)), doubles(vols), C.size_t(len(vols)), doubles(reversions), C.size_t(len(reversions)), C.double(tMeasure), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &GsrProcess{object{s, uint64(id)}}, nil
}

func (p *GsrProcess) checked() error {
	if p == nil {
		return errNilArgument("process")
	}
	return sameSession(p.session, p.object)
}

func (p *GsrProcess) query(kind int32, t, x, dt, dw float64) (float64, error) {
	if err := p.checked(); err != nil {
		return 0, err
	}
	var out C.double
	err := p.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_gsr_process_query(p.session.ctx, C.uint64_t(p.id), C.int32_t(kind), C.double(t), C.double(x), C.double(dt), C.double(dw), &out, &e), &e)
	})
	return float64(out), err
}

// SetForwardMeasureTime replaces the horizon through the native notifying
// setter. Invalid values preserve the previous horizon and process dynamics.
func (p *GsrProcess) SetForwardMeasureTime(horizon float64) error {
	if err := p.checked(); err != nil {
		return err
	}
	return p.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_gsr_process_set_forward_measure_time(p.session.ctx, C.uint64_t(p.id), C.double(horizon), &e), &e)
	})
}

// X0 returns the zero initial state.
func (p *GsrProcess) X0() (float64, error) { return p.query(0, 0, 0, 0, 0) }

// ForwardMeasureTime returns the current horizon.
func (p *GsrProcess) ForwardMeasureTime() (float64, error) { return p.query(1, 0, 0, 0, 0) }

// Sigma returns constant volatility at a valid time.
func (p *GsrProcess) Sigma(t float64) (float64, error) { return p.query(2, t, 0, 0, 0) }

// Reversion returns signed constant reversion at a valid time.
func (p *GsrProcess) Reversion(t float64) (float64, error) { return p.query(3, t, 0, 0, 0) }

// Y returns the auxiliary variance y(t).
func (p *GsrProcess) Y(t float64) (float64, error) { return p.query(4, t, 0, 0, 0) }

// G returns the affine kernel for ordered endpoints.
func (p *GsrProcess) G(t, end float64) (float64, error) { return p.query(5, t, 0, end, 0) }

// Drift returns the forward-measure drift.
func (p *GsrProcess) Drift(t, x float64) (float64, error) { return p.query(6, t, x, 0, 0) }

// Diffusion returns instantaneous diffusion.
func (p *GsrProcess) Diffusion(t, x float64) (float64, error) { return p.query(7, t, x, 0, 0) }

// Expectation returns the exact conditional mean.
func (p *GsrProcess) Expectation(t, x, dt float64) (float64, error) { return p.query(8, t, x, dt, 0) }

// StdDeviation returns the conditional standard deviation.
func (p *GsrProcess) StdDeviation(t, x, dt float64) (float64, error) { return p.query(9, t, x, dt, 0) }

// Variance returns the conditional variance.
func (p *GsrProcess) Variance(t, x, dt float64) (float64, error) { return p.query(10, t, x, dt, 0) }

// Evolve advances using one finite standard Gaussian draw.
func (p *GsrProcess) Evolve(t, x, dt, dw float64) (float64, error) { return p.query(11, t, x, dt, dw) }
