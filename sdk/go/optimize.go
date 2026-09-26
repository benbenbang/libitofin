package itofin

/*
#include "itofin.h"
typedef int32_t (*goOptimizeValueFn)(size_t, const double*, size_t, double*, ItofinError*);
typedef int32_t (*goOptimizeGradientFn)(size_t, const double*, size_t, double*, ItofinError*);
typedef int32_t (*goOptimizeCallbackFn)(size_t, const ItofinIterationState*, bool*, ItofinError*);
typedef void (*goOptimizeDrop)(size_t);
extern int32_t goOptimizeValue(uintptr_t, double*, size_t, double*, ItofinError*);
extern int32_t goOptimizeGradient(uintptr_t, double*, size_t, double*, ItofinError*);
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
// itofin.optimize.Status and are append-only; 8 is reserved for a
// constrained solver.
type OptimizeStatus int32

const (
	OptimizeConvergedXTol    OptimizeStatus = C.ITOFIN_OPTIMIZE_CONVERGED_XTOL
	OptimizeConvergedFTol    OptimizeStatus = C.ITOFIN_OPTIMIZE_CONVERGED_FTOL
	OptimizeConvergedGTol    OptimizeStatus = C.ITOFIN_OPTIMIZE_CONVERGED_GTOL
	OptimizeMaxIterations    OptimizeStatus = C.ITOFIN_OPTIMIZE_MAX_ITERATIONS
	OptimizeMaxEvaluations   OptimizeStatus = C.ITOFIN_OPTIMIZE_MAX_EVALUATIONS
	OptimizeCancelled        OptimizeStatus = C.ITOFIN_OPTIMIZE_CANCELLED
	OptimizeNonfinite        OptimizeStatus = C.ITOFIN_OPTIMIZE_NONFINITE
	OptimizeLineSearchFailed OptimizeStatus = C.ITOFIN_OPTIMIZE_LINE_SEARCH_FAILED
)

var optimizeMessages = map[OptimizeStatus]string{
	OptimizeConvergedXTol:    "converged: step below the x tolerance",
	OptimizeConvergedFTol:    "converged: change below the f tolerance",
	OptimizeConvergedGTol:    "converged: gradient below the g tolerance",
	OptimizeMaxIterations:    "maximum number of iterations reached",
	OptimizeMaxEvaluations:   "maximum number of function evaluations reached",
	OptimizeCancelled:        "stopped by the callback",
	OptimizeNonfinite:        "a nonfinite value ended the search",
	OptimizeLineSearchFailed: "the line search failed",
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

// OptimizeMethod selects a solver for Minimize.
type OptimizeMethod interface{ optimizeMethod() }

// NelderMead is the short name for the existing NelderMeadOptions.
type NelderMead = NelderMeadOptions

func (NelderMeadOptions) optimizeMethod() {}

// BFGS configures the gradient solver. Eps is an absolute finite-difference
// step when Gradient is nil; zero selects the solver default.
type BFGS struct {
	Gradient          func(x, out []float64) error
	GTol, Eps         float64
	MaxIter           int
	CentralDifference bool
	Bounds            *OptimizeBounds
}

// OptimizeBounds reserves box bounds for methods that support them.
type OptimizeBounds struct{ Lower, Upper []float64 }

func (BFGS) optimizeMethod() {}

// ErrInvalidArgument marks a method option that cannot be accepted.
var ErrInvalidArgument = errors.New("itofin: invalid optimizer argument")

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
	ctx      context.Context
	fn       func([]float64) (float64, error)
	gradient func([]float64, []float64) error
	err      error
}

// Minimize runs the selected method on fn from x0 on the calling goroutine, outside
// any Session, so fn may call Session methods. An error returned by fn, or a
// panic in it, ends the run and is returned as is. Cancelling ctx stops the
// run at the end of the current iteration and returns the partial result
// together with ctx.Err().
func Minimize(ctx context.Context, fn func(x []float64) (float64, error), x0 []float64, method OptimizeMethod) (OptimizeResult, error) {
	if ctx == nil || fn == nil {
		return OptimizeResult{}, errNilArgument("context and objective")
	}
	var nm NelderMeadOptions
	var bfgs BFGS
	isBFGS := false
	switch selected := method.(type) {
	case NelderMeadOptions:
		nm = selected
	case *NelderMeadOptions:
		if selected == nil {
			return OptimizeResult{}, fmt.Errorf("%w: nil method", ErrInvalidArgument)
		}
		nm = *selected
	case BFGS:
		isBFGS = true
		bfgs = selected
	case *BFGS:
		if selected == nil {
			return OptimizeResult{}, fmt.Errorf("%w: nil method", ErrInvalidArgument)
		}
		isBFGS = true
		bfgs = *selected
	default:
		return OptimizeResult{}, fmt.Errorf("%w: unknown method", ErrInvalidArgument)
	}
	if isBFGS {
		if bfgs.Bounds != nil || bfgs.MaxIter < 0 || bfgs.GTol < 0 || bfgs.Eps < 0 {
			return OptimizeResult{}, fmt.Errorf("%w: unsupported bounds or invalid BFGS option", ErrInvalidArgument)
		}
	} else if nm.MaxIter < 0 || nm.MaxFev < 0 {
		return OptimizeResult{}, fmt.Errorf("%w: negative budget", ErrInvalidArgument)
	}
	if err := ctx.Err(); err != nil {
		return OptimizeResult{}, err
	}
	state := &optimizeState{ctx: ctx, fn: fn, gradient: bfgs.Gradient}
	objective := C.ItofinObjective{
		userdata: C.size_t(cgo.NewHandle(state)),
		value:    (C.goOptimizeValueFn)(C.goOptimizeValue),
		gradient: nil,
		callback: (C.goOptimizeCallbackFn)(C.goOptimizeCallback),
		release:  (C.goOptimizeDrop)(C.goOptimizeRelease),
	}
	if bfgs.Gradient != nil {
		objective.gradient = (C.goOptimizeGradientFn)(C.goOptimizeGradient)
	}
	x := make([]float64, len(x0))
	var pins runtimePinner
	pins.pin(x)
	defer pins.unpin()
	var out C.ItofinOptimizeResult
	out.x = (*C.double)(unsafe.Pointer(unsafe.SliceData(x)))
	var e C.ItofinError
	var status C.int32_t
	if isBFGS {
		fd := C.int32_t(0)
		if bfgs.CentralDifference {
			fd = 1
		}
		options := C.ItofinBfgsOptions{gtol: C.double(bfgs.GTol), eps: C.double(bfgs.Eps), finite_difference: fd, maxiter: C.size_t(bfgs.MaxIter)}
		status = C.itofin_optimize_bfgs(&objective, (*C.double)(unsafe.Pointer(unsafe.SliceData(x0))), C.size_t(len(x0)), &options, &out, &e)
	} else {
		options := C.ItofinOptimizeOptions{maxiter: C.size_t(nm.MaxIter), maxfev: C.size_t(nm.MaxFev),
			xatol: C.double(nm.XAtol), fatol: C.double(nm.FAtol), adaptive: C.bool(nm.Adaptive)}
		status = C.itofin_optimize_nelder_mead(&objective, (*C.double)(unsafe.Pointer(unsafe.SliceData(x0))), C.size_t(len(x0)), &options, &out, &e)
	}
	if state.err != nil {
		return OptimizeResult{}, state.err
	}
	if err := ffiError(status, &e); err != nil {
		if status == C.ITOFIN_INVALID_ARGUMENT {
			return OptimizeResult{}, fmt.Errorf("%w: %v", ErrInvalidArgument, err)
		}
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

//export goOptimizeGradient
func goOptimizeGradient(handle C.uintptr_t, x *C.double, n C.size_t, out *C.double, e *C.ItofinError) (status C.int32_t) {
	state := cgo.Handle(handle).Value().(*optimizeState)
	defer func() {
		if value := recover(); value != nil {
			state.err = fmt.Errorf("itofin: gradient panic: %v", value)
			status = C.ITOFIN_CORE_ERROR
		}
	}()
	point := append([]float64(nil), unsafe.Slice((*float64)(unsafe.Pointer(x)), int(n))...)
	gradient := make([]float64, int(n))
	if err := state.gradient(point, gradient); err != nil {
		state.err = err
		return C.ITOFIN_CORE_ERROR
	}
	copy(unsafe.Slice((*float64)(unsafe.Pointer(out)), int(n)), gradient)
	return 0
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
