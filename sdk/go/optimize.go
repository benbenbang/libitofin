package itofin

/*
#include "itofin.h"
typedef int32_t (*goOptimizeValueFn)(size_t, const double*, size_t, double*, ItofinError*);
typedef int32_t (*goOptimizeCallbackFn)(size_t, const ItofinIterationState*, bool*, ItofinError*);
typedef void (*goOptimizeDrop)(size_t);
extern int32_t goOptimizeValue(uintptr_t, double*, size_t, double*, ItofinError*);
extern int32_t goOptimizeCallback(uintptr_t, ItofinIterationState*, bool*, ItofinError*);
extern void goOptimizeRelease(uintptr_t);
*/
import "C"

import (
	"context"
	"errors"
	"fmt"
	"runtime/cgo"
	"unsafe"
)

// OptimizeStatus says why a Minimize run stopped. Values match Python
// itofin.optimize.Status and are append-only; 7 and 8 are reserved for the
// line-search and constrained solvers.
type OptimizeStatus int32

const (
	OptimizeConvergedXTol  OptimizeStatus = C.ITOFIN_OPTIMIZE_CONVERGED_XTOL
	OptimizeConvergedFTol  OptimizeStatus = C.ITOFIN_OPTIMIZE_CONVERGED_FTOL
	OptimizeConvergedGTol  OptimizeStatus = C.ITOFIN_OPTIMIZE_CONVERGED_GTOL
	OptimizeMaxIterations  OptimizeStatus = C.ITOFIN_OPTIMIZE_MAX_ITERATIONS
	OptimizeMaxEvaluations OptimizeStatus = C.ITOFIN_OPTIMIZE_MAX_EVALUATIONS
	OptimizeCancelled      OptimizeStatus = C.ITOFIN_OPTIMIZE_CANCELLED
	OptimizeNonfinite      OptimizeStatus = C.ITOFIN_OPTIMIZE_NONFINITE
)

var optimizeMessages = map[OptimizeStatus]string{
	OptimizeConvergedXTol:  "converged: step below the x tolerance",
	OptimizeConvergedFTol:  "converged: change below the f tolerance",
	OptimizeConvergedGTol:  "converged: gradient below the g tolerance",
	OptimizeMaxIterations:  "maximum number of iterations reached",
	OptimizeMaxEvaluations: "maximum number of function evaluations reached",
	OptimizeCancelled:      "stopped by the callback",
	OptimizeNonfinite:      "a nonfinite value ended the search",
}

// String returns the same human reading as the Rust and Python results.
func (s OptimizeStatus) String() string {
	if message, ok := optimizeMessages[s]; ok {
		return message
	}
	return fmt.Sprintf("OptimizeStatus(%d)", int32(s))
}

// NelderMeadOptions configures Minimize. A zero field keeps the solver
// default: budgets of 200 per coordinate and tolerances of 1e-4.
type NelderMeadOptions struct {
	MaxIter, MaxFev int
	XAtol, FAtol    float64
	Adaptive        bool
}

// OptimizeResult is the best point Minimize found and why it stopped.
type OptimizeResult struct {
	X               []float64
	Fun             float64
	Nit, Nfev, Njev int
	Status          OptimizeStatus
	Success         bool
	Message         string
}

type optimizeState struct {
	ctx context.Context
	fn  func([]float64) (float64, error)
	err error
}

// Minimize runs Nelder-Mead on fn from x0 on the calling goroutine, outside
// any Session, so fn may call Session methods. An error returned by fn, or a
// panic in it, ends the run and is returned as is. Cancelling ctx stops the
// run at the end of the current iteration and returns the partial result
// together with ctx.Err().
func Minimize(ctx context.Context, fn func(x []float64) (float64, error), x0 []float64, opts NelderMeadOptions) (OptimizeResult, error) {
	if ctx == nil || fn == nil {
		return OptimizeResult{}, errNilArgument("context and objective")
	}
	if opts.MaxIter < 0 || opts.MaxFev < 0 {
		return OptimizeResult{}, errors.New("itofin: MaxIter and MaxFev must not be negative")
	}
	if err := ctx.Err(); err != nil {
		return OptimizeResult{}, err
	}
	state := &optimizeState{ctx: ctx, fn: fn}
	objective := C.ItofinObjective{
		userdata: C.size_t(cgo.NewHandle(state)),
		value:    (C.goOptimizeValueFn)(C.goOptimizeValue),
		callback: (C.goOptimizeCallbackFn)(C.goOptimizeCallback),
		release:  (C.goOptimizeDrop)(C.goOptimizeRelease),
	}
	options := C.ItofinOptimizeOptions{
		maxiter: C.size_t(opts.MaxIter), maxfev: C.size_t(opts.MaxFev),
		xatol: C.double(opts.XAtol), fatol: C.double(opts.FAtol), adaptive: C.bool(opts.Adaptive),
	}
	x := make([]float64, len(x0))
	var pins runtimePinner
	pins.pin(x)
	defer pins.unpin()
	var out C.ItofinOptimizeResult
	out.x = (*C.double)(unsafe.Pointer(unsafe.SliceData(x)))
	var e C.ItofinError
	status := C.itofin_optimize_nelder_mead(&objective, (*C.double)(unsafe.Pointer(unsafe.SliceData(x0))), C.size_t(len(x0)), &options, &out, &e)
	if state.err != nil {
		return OptimizeResult{}, state.err
	}
	if err := ffiError(status, &e); err != nil {
		return OptimizeResult{}, err
	}
	result := OptimizeResult{
		X: x, Fun: float64(out.fun), Nit: int(out.nit), Nfev: int(out.nfev), Njev: int(out.njev),
		Status: OptimizeStatus(out.status), Success: bool(out.success),
	}
	result.Message = result.Status.String()
	if result.Status == OptimizeCancelled {
		return result, ctx.Err()
	}
	return result, nil
}

func evaluateObjective(state *optimizeState, point []float64, out *C.double) (status C.int32_t) {
	defer func() {
		if value := recover(); value != nil {
			state.err = fmt.Errorf("itofin: objective panic: %v", value)
			status = C.ITOFIN_CORE_ERROR
		}
	}()
	value, err := state.fn(point)
	if err != nil {
		state.err = err
		return C.ITOFIN_CORE_ERROR
	}
	*out = C.double(value)
	return 0
}

//export goOptimizeValue
func goOptimizeValue(handle C.uintptr_t, x *C.double, n C.size_t, out *C.double, e *C.ItofinError) C.int32_t {
	point := append([]float64(nil), unsafe.Slice((*float64)(unsafe.Pointer(x)), int(n))...)
	return evaluateObjective(cgo.Handle(handle).Value().(*optimizeState), point, out)
}

//export goOptimizeCallback
func goOptimizeCallback(handle C.uintptr_t, state *C.ItofinIterationState, stop *C.bool, e *C.ItofinError) C.int32_t {
	*stop = C.bool(cgo.Handle(handle).Value().(*optimizeState).ctx.Err() != nil)
	return 0
}

//export goOptimizeRelease
func goOptimizeRelease(handle C.uintptr_t) { cgo.Handle(handle).Delete() }
