package itofin

/*
#include "itofin.h"
*/
import "C"
import (
	"fmt"
	"runtime"
)

// Constraint is a reusable, session-owned calibration constraint.
type Constraint struct{ object }

func (s *Session) NewNoConstraint() (*Constraint, error) {
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_no_constraint_new(s.ctx, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &Constraint{object{s, uint64(id)}}, nil
}

func (s *Session) NewPositiveConstraint() (*Constraint, error) {
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_positive_constraint_new(s.ctx, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &Constraint{object{s, uint64(id)}}, nil
}

// NewBoundaryConstraint applies the inclusive bounds to every model parameter.
func (s *Session) NewBoundaryConstraint(low, high float64) (*Constraint, error) {
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_boundary_constraint_new(s.ctx, C.double(low), C.double(high), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &Constraint{object{s, uint64(id)}}, nil
}

// NewCompositeConstraint copies both children; closing them leaves the composite usable.
func (s *Session) NewCompositeConstraint(left, right *Constraint) (*Constraint, error) {
	if left == nil || right == nil {
		return nil, errNilArgument("constraint")
	}
	if err := sameSession(s, left.object, right.object); err != nil {
		return nil, err
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_composite_constraint_new(s.ctx, C.uint64_t(left.id), C.uint64_t(right.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &Constraint{object{s, uint64(id)}}, nil
}

// CalibrationOptions apply to one fit. An empty slice means the core default.
// Heston fixed-parameter order is theta, kappa, sigma, rho, v0; Hull-White is a, sigma.
// Levenberg-Marquardt rejects infeasible trial points rather than projecting them;
// an infeasible starting point returns an error.
type CalibrationOptions struct {
	Constraint    *Constraint
	Weights       []float64
	FixParameters []bool
}

type calibrationArgs struct {
	cfg     C.ItofinCalibrationOptions
	weights []C.double
	fixed   []C.uint8_t
	pin     runtime.Pinner
}

func newCalibrationArgs(s *Session, provided []*CalibrationOptions, fixReversion bool) (*calibrationArgs, error) {
	if len(provided) > 1 {
		return nil, fmt.Errorf("itofin: expected at most one calibration options argument")
	}
	args := new(calibrationArgs)
	if len(provided) == 0 || provided[0] == nil {
		return args, nil
	}
	opts := provided[0]
	if fixReversion && len(opts.FixParameters) != 0 {
		return nil, fmt.Errorf("itofin: fixReversion and FixParameters cannot both be set")
	}
	if opts.Constraint != nil {
		if err := sameSession(s, opts.Constraint.object); err != nil {
			return nil, err
		}
		args.cfg.constraint = C.uint64_t(opts.Constraint.id)
	}
	args.weights = make([]C.double, len(opts.Weights))
	for i, weight := range opts.Weights {
		args.weights[i] = C.double(weight)
	}
	args.fixed = make([]C.uint8_t, len(opts.FixParameters))
	for i, fixed := range opts.FixParameters {
		if fixed {
			args.fixed[i] = 1
		}
	}
	if len(args.weights) != 0 {
		args.pin.Pin(&args.weights[0])
		args.cfg.weights = &args.weights[0]
		args.cfg.weights_len = C.size_t(len(args.weights))
	}
	if len(args.fixed) != 0 {
		args.pin.Pin(&args.fixed[0])
		args.cfg.fix_parameters = &args.fixed[0]
		args.cfg.fix_parameters_len = C.size_t(len(args.fixed))
	}
	args.pin.Pin(&args.cfg)
	return args, nil
}

func (a *calibrationArgs) release() {
	a.pin.Unpin()
	runtime.KeepAlive(a)
}
