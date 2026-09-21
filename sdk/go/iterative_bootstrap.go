package itofin

/*
#include "itofin.h"
*/
import "C"
import "unsafe"

// IterativeBootstrapOptions controls yield-curve retries and optional approximate fallback.
// Start with DefaultIterativeBootstrapOptions. Nil scalar pointers use curve/trait defaults.
// DontThrow can return a curve that does not reprice its helpers exactly.
type IterativeBootstrapOptions struct {
	Accuracy, MinValue, MaxValue   *float64
	MaxAttempts                    uint
	MaxFactor, MinFactor           float64
	DontThrow                      bool
	DontThrowSteps, MaxEvaluations uint
}

func DefaultIterativeBootstrapOptions() IterativeBootstrapOptions {
	return IterativeBootstrapOptions{MaxAttempts: 1, MaxFactor: 2, MinFactor: 2, DontThrowSteps: 10, MaxEvaluations: 100}
}

func (s *Session) iterativeCurve(cfg PiecewiseCurveConfig, kind int, ids []C.uint64_t, out *C.uint64_t) error {
	x := cfg.IterativeOptions
	opts := C.ItofinIterativeBootstrapOptions{max_attempts: C.size_t(x.MaxAttempts), max_factor: C.double(x.MaxFactor), min_factor: C.double(x.MinFactor), dont_throw_steps: C.size_t(x.DontThrowSteps), max_evaluations: C.size_t(x.MaxEvaluations)}
	if x.Accuracy != nil {
		opts.accuracy, opts.has_accuracy = C.double(*x.Accuracy), 1
	}
	if x.MinValue != nil {
		opts.min_value, opts.has_min_value = C.double(*x.MinValue), 1
	}
	if x.MaxValue != nil {
		opts.max_value, opts.has_max_value = C.double(*x.MaxValue), 1
	}
	if x.DontThrow {
		opts.dont_throw = 1
	}
	var e C.ItofinError
	return ffiError(C.itofin_piecewise_curve_new_with_options(s.ctx, C.int32_t(cfg.ReferenceDate.Serial()), unsafe.SliceData(ids), C.size_t(len(ids)), C.uint64_t(cfg.DayCounter.id), C.int32_t(kind), &opts, out, &e), &e)
}
