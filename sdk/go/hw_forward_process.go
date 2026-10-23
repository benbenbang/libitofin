package itofin

/*
#include "itofin.h"
*/
import "C"

// HullWhiteForwardProcess retains a live curve under a mutable forward measure.
// X0 is captured at construction. This is not a calibrated Hull-White model.
type HullWhiteForwardProcess struct{ object }

// NewHullWhiteForwardProcess retains curve, accepts nonnegative a and sigma,
// and starts with horizon zero. Times are process years, not dates.
func (s *Session) NewHullWhiteForwardProcess(curve *YieldTermStructure, a, sigma float64) (*HullWhiteForwardProcess, error) {
	if curve == nil {
		return nil, errNilArgument("curve")
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		if err := sameSession(s, curve.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_hw_forward_process_new(s.ctx, C.uint64_t(curve.id), C.double(a), C.double(sigma), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &HullWhiteForwardProcess{object{s, uint64(id)}}, nil
}

func (p *HullWhiteForwardProcess) query(kind int32, t, x, dt, dw float64) (float64, error) {
	if p == nil {
		return 0, errNilArgument("process")
	}
	var out C.double
	err := p.session.invoke(func() error {
		if err := sameSession(p.session, p.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_hw_forward_process_query(p.session.ctx, C.uint64_t(p.id), C.int32_t(kind), C.double(t), C.double(x), C.double(dt), C.double(dw), &out, &e), &e)
	})
	return float64(out), err
}

// SetForwardMeasureTime notifies through the native setter. Invalid values preserve state.
func (p *HullWhiteForwardProcess) SetForwardMeasureTime(horizon float64) error {
	if p == nil {
		return errNilArgument("process")
	}
	return p.session.invoke(func() error {
		if err := sameSession(p.session, p.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_hw_forward_process_set_forward_measure_time(p.session.ctx, C.uint64_t(p.id), C.double(horizon), &e), &e)
	})
}

// X0 returns the short rate captured at construction, even after curve updates.
func (p *HullWhiteForwardProcess) X0() (float64, error) { return p.query(0, 0, 0, 0, 0) }

// A returns constant nonnegative mean reversion.
func (p *HullWhiteForwardProcess) A() (float64, error) { return p.query(1, 0, 0, 0, 0) }

// Sigma returns constant nonnegative volatility.
func (p *HullWhiteForwardProcess) Sigma() (float64, error) { return p.query(2, 0, 0, 0, 0) }

// ForwardMeasureTime returns the current finite nonnegative horizon.
func (p *HullWhiteForwardProcess) ForwardMeasureTime() (float64, error) {
	return p.query(3, 0, 0, 0, 0)
}

// Alpha returns the fitting shift from the live curve at t.
func (p *HullWhiteForwardProcess) Alpha(t float64) (float64, error) { return p.query(4, t, 0, 0, 0) }

// B returns the affine kernel for absolute endpoints, including maturity before t.
func (p *HullWhiteForwardProcess) B(t, maturity float64) (float64, error) {
	return p.query(5, t, 0, maturity, 0)
}

// MT returns the integrated adjustment for s <= t and an absolute maturity.
func (p *HullWhiteForwardProcess) MT(s, t, maturity float64) (float64, error) {
	return p.query(6, s, t, maturity, 0)
}

// Drift returns forward-measure drift using the live curve and current horizon.
func (p *HullWhiteForwardProcess) Drift(t, x float64) (float64, error) { return p.query(7, t, x, 0, 0) }

// Diffusion returns instantaneous volatility at valid t and x.
func (p *HullWhiteForwardProcess) Diffusion(t, x float64) (float64, error) {
	return p.query(8, t, x, 0, 0)
}

// Expectation returns the exact conditional mean using the live curve.
func (p *HullWhiteForwardProcess) Expectation(t, x, dt float64) (float64, error) {
	return p.query(9, t, x, dt, 0)
}

// StdDeviation returns conditional standard deviation.
func (p *HullWhiteForwardProcess) StdDeviation(t, x, dt float64) (float64, error) {
	return p.query(10, t, x, dt, 0)
}

// Variance retains the native Ornstein-Uhlenbeck small-speed branch.
func (p *HullWhiteForwardProcess) Variance(t, x, dt float64) (float64, error) {
	return p.query(11, t, x, dt, 0)
}

// Evolve advances with one finite standard Gaussian draw.
func (p *HullWhiteForwardProcess) Evolve(t, x, dt, dw float64) (float64, error) {
	return p.query(12, t, x, dt, dw)
}
