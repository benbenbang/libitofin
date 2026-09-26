package itofin

/*
#include "itofin.h"
typedef int32_t (*goOptimizeValueFn)(size_t, const double*, size_t, double*, ItofinError*);
typedef int32_t (*goOptimizeGradientFn)(size_t, const double*, size_t, double*, ItofinError*);
typedef int32_t (*goOptimizeCallbackFn)(size_t, const ItofinIterationState*, bool*, ItofinError*);
typedef int32_t (*goOptimizeConstraintFn)(size_t, const double*, size_t, double*, size_t, ItofinError*);
typedef void (*goOptimizeDrop)(size_t);
extern int32_t goOptimizeValue(uintptr_t, double*, size_t, double*, ItofinError*);
extern int32_t goOptimizeGradient(uintptr_t, double*, size_t, double*, ItofinError*);
extern int32_t goOptimizeCallback(uintptr_t, ItofinIterationState*, bool*, ItofinError*);
extern int32_t goOptimizeConstraintValue(uintptr_t, double*, size_t, double*, size_t, ItofinError*);
extern int32_t goOptimizeConstraintJac(uintptr_t, double*, size_t, double*, size_t, ItofinError*);
extern void goOptimizeRelease(uintptr_t);
*/
import "C"

import (
	"context"
	"errors"
	"fmt"
	"math"
	"runtime/cgo"
	"unsafe"
)

// OptimizeStatus says why a Minimize run stopped. Values match Python
// itofin.optimize.Status and are append-only.
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
	OptimizeInfeasible       OptimizeStatus = C.ITOFIN_OPTIMIZE_INFEASIBLE
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
	OptimizeInfeasible:       "the constraints are infeasible",
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

// LBFGSB configures the box-constrained gradient solver. Each Bounds pair is
// [lower, upper]; math.Inf(-1) and math.Inf(1) leave the respective side open.
// A nil Bounds slice leaves every coordinate unbounded.
type LBFGSB struct {
	Bounds          [][2]float64
	Gradient        func(x, out []float64) error
	MaxCor          int
	FTol, GTol, Eps float64
	MaxIter, MaxFev int
}

func (LBFGSB) optimizeMethod() {}

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

type optimizeConstraintState struct {
	parent     *optimizeState
	constraint SLSQPConstraint
	dimension  int
}

// Minimize runs the selected method on fn from x0 on the calling goroutine, outside
// any Session, so fn may call Session methods. An error returned by fn, or a
// panic in it, ends the run and is returned as is. Cancelling ctx stops the
// run at the end of the current iteration and returns the partial result
// together with ctx.Err(). Constraint errors are returned unchanged too.
func Minimize(ctx context.Context, fn func(x []float64) (float64, error), x0 []float64, method OptimizeMethod) (OptimizeResult, error) {
	if ctx == nil || fn == nil {
		return OptimizeResult{}, errNilArgument("context and objective")
	}
	var nm NelderMeadOptions
	var bfgs BFGS
	var lbfgsb LBFGSB
	var slsqp SLSQP
	isBFGS := false
	isLBFGSB := false
	isSLSQP := false
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
	case LBFGSB:
		isLBFGSB = true
		lbfgsb = selected
	case *LBFGSB:
		if selected == nil {
			return OptimizeResult{}, fmt.Errorf("%w: nil method", ErrInvalidArgument)
		}
		isLBFGSB = true
		lbfgsb = *selected
	case SLSQP:
		isSLSQP = true
		slsqp = selected
	case *SLSQP:
		if selected == nil {
			return OptimizeResult{}, fmt.Errorf("%w: nil method", ErrInvalidArgument)
		}
		isSLSQP = true
		slsqp = *selected
	default:
		return OptimizeResult{}, fmt.Errorf("%w: unknown method", ErrInvalidArgument)
	}
	if err := ctx.Err(); err != nil {
		return OptimizeResult{}, err
	}
	var dimensions []int
	if isBFGS {
		if bfgs.Bounds != nil || bfgs.MaxIter < 0 || bfgs.GTol < 0 || bfgs.Eps < 0 {
			return OptimizeResult{}, fmt.Errorf("%w: unsupported bounds or invalid BFGS option", ErrInvalidArgument)
		}
	} else if isLBFGSB {
		if lbfgsb.MaxCor < 0 || lbfgsb.MaxIter < 0 || lbfgsb.MaxFev < 0 ||
			!validNonnegative(lbfgsb.FTol) || !validNonnegative(lbfgsb.GTol) || !validNonnegative(lbfgsb.Eps) {
			return OptimizeResult{}, fmt.Errorf("%w: invalid L-BFGS-B option", ErrInvalidArgument)
		}
		if lbfgsb.Bounds != nil && len(lbfgsb.Bounds) != len(x0) {
			return OptimizeResult{}, fmt.Errorf("%w: bounds length must match x0", ErrInvalidArgument)
		}
		for _, pair := range lbfgsb.Bounds {
			if math.IsNaN(pair[0]) || math.IsNaN(pair[1]) || pair[0] > pair[1] ||
				math.IsInf(pair[0], 1) || math.IsInf(pair[1], -1) {
				return OptimizeResult{}, fmt.Errorf("%w: invalid L-BFGS-B bounds", ErrInvalidArgument)
			}
		}
	} else if isSLSQP {
		var err error
		dimensions, err = validateSLSQP(slsqp, x0)
		if err != nil {
			return OptimizeResult{}, err
		}
	} else if nm.MaxIter < 0 || nm.MaxFev < 0 {
		return OptimizeResult{}, fmt.Errorf("%w: negative budget", ErrInvalidArgument)
	}
	gradient := bfgs.Gradient
	if isLBFGSB {
		gradient = lbfgsb.Gradient
	} else if isSLSQP {
		gradient = slsqp.Gradient
	}
	state := &optimizeState{ctx: ctx, fn: fn, gradient: gradient}
	objective := C.ItofinObjective{
		userdata: C.size_t(cgo.NewHandle(state)),
		value:    (C.goOptimizeValueFn)(C.goOptimizeValue),
		gradient: nil,
		callback: (C.goOptimizeCallbackFn)(C.goOptimizeCallback),
		release:  (C.goOptimizeDrop)(C.goOptimizeRelease),
	}
	if gradient != nil {
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
	} else if isLBFGSB {
		options := C.ItofinLbfgsbOptions{maxcor: C.size_t(lbfgsb.MaxCor), ftol: C.double(lbfgsb.FTol),
			gtol: C.double(lbfgsb.GTol), eps: C.double(lbfgsb.Eps), maxiter: C.size_t(lbfgsb.MaxIter), maxfev: C.size_t(lbfgsb.MaxFev)}
		var lower, upper []float64
		if lbfgsb.Bounds != nil {
			lower = make([]float64, len(lbfgsb.Bounds))
			upper = make([]float64, len(lbfgsb.Bounds))
			for i, pair := range lbfgsb.Bounds {
				lower[i], upper[i] = pair[0], pair[1]
			}
		}
		status = C.itofin_optimize_lbfgsb(&objective,
			(*C.double)(unsafe.Pointer(unsafe.SliceData(x0))), C.size_t(len(x0)),
			(*C.double)(unsafe.Pointer(unsafe.SliceData(lower))), C.size_t(len(lower)),
			(*C.double)(unsafe.Pointer(unsafe.SliceData(upper))), C.size_t(len(upper)),
			&options, &out, &e)
	} else if isSLSQP {
		options := C.ItofinSlsqpOptions{ftol: C.double(slsqp.FTol), maxiter: C.size_t(slsqp.MaxIter), maxfev: C.size_t(slsqp.MaxFev)}
		var lower, upper []float64
		if slsqp.Bounds != nil {
			lower = make([]float64, len(slsqp.Bounds))
			upper = make([]float64, len(slsqp.Bounds))
			for i, pair := range slsqp.Bounds {
				lower[i], upper[i] = pair[0], pair[1]
			}
		}
		constraints := make([]C.ItofinConstraint, len(slsqp.Constraints))
		for i, constraint := range slsqp.Constraints {
			bridge := &optimizeConstraintState{parent: state, constraint: constraint, dimension: dimensions[i]}
			constraints[i] = C.ItofinConstraint{
				kind: C.int32_t(constraint.Kind), dimension: C.size_t(dimensions[i]),
				userdata: C.size_t(cgo.NewHandle(bridge)),
				fun:      (C.goOptimizeConstraintFn)(C.goOptimizeConstraintValue),
				release:  (C.goOptimizeDrop)(C.goOptimizeRelease),
			}
			if constraint.Jac != nil {
				constraints[i].jac = (C.goOptimizeConstraintFn)(C.goOptimizeConstraintJac)
			}
		}
		status = C.itofin_optimize_slsqp(&objective,
			(*C.double)(unsafe.Pointer(unsafe.SliceData(x0))), C.size_t(len(x0)),
			(*C.double)(unsafe.Pointer(unsafe.SliceData(lower))), C.size_t(len(lower)),
			(*C.double)(unsafe.Pointer(unsafe.SliceData(upper))), C.size_t(len(upper)),
			(*C.ItofinConstraint)(unsafe.Pointer(unsafe.SliceData(constraints))), C.size_t(len(constraints)),
			&options, &out, &e)
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

func validNonnegative(value float64) bool {
	return !math.IsNaN(value) && !math.IsInf(value, 0) && value >= 0
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

//export goOptimizeConstraintValue
func goOptimizeConstraintValue(handle C.uintptr_t, x *C.double, n C.size_t, out *C.double, dimension C.size_t, e *C.ItofinError) (status C.int32_t) {
	bridge := cgo.Handle(handle).Value().(*optimizeConstraintState)
	defer func() {
		if value := recover(); value != nil {
			bridge.parent.err = fmt.Errorf("itofin: constraint panic: %v", value)
			status = C.ITOFIN_CORE_ERROR
		}
	}()
	point := append([]float64(nil), unsafe.Slice((*float64)(unsafe.Pointer(x)), int(n))...)
	values, err := bridge.constraint.Fun(point)
	if err != nil {
		bridge.parent.err = err
		return C.ITOFIN_CORE_ERROR
	}
	if len(values) != bridge.dimension || int(dimension) != bridge.dimension {
		bridge.parent.err = fmt.Errorf("%w: constraint returned the wrong vector length", ErrInvalidArgument)
		return C.ITOFIN_CORE_ERROR
	}
	copy(unsafe.Slice((*float64)(unsafe.Pointer(out)), bridge.dimension), values)
	return 0
}

//export goOptimizeConstraintJac
func goOptimizeConstraintJac(handle C.uintptr_t, x *C.double, n C.size_t, out *C.double, length C.size_t, e *C.ItofinError) (status C.int32_t) {
	bridge := cgo.Handle(handle).Value().(*optimizeConstraintState)
	defer func() {
		if value := recover(); value != nil {
			bridge.parent.err = fmt.Errorf("itofin: constraint Jacobian panic: %v", value)
			status = C.ITOFIN_CORE_ERROR
		}
	}()
	point := append([]float64(nil), unsafe.Slice((*float64)(unsafe.Pointer(x)), int(n))...)
	rows, err := bridge.constraint.Jac(point)
	if err != nil {
		bridge.parent.err = err
		return C.ITOFIN_CORE_ERROR
	}
	if len(rows) != bridge.dimension || int(length) != bridge.dimension*len(point) {
		bridge.parent.err = fmt.Errorf("%w: constraint Jacobian returned the wrong shape", ErrInvalidArgument)
		return C.ITOFIN_CORE_ERROR
	}
	output := unsafe.Slice((*float64)(unsafe.Pointer(out)), int(length))
	for i, row := range rows {
		if len(row) != len(point) {
			bridge.parent.err = fmt.Errorf("%w: constraint Jacobian returned the wrong shape", ErrInvalidArgument)
			return C.ITOFIN_CORE_ERROR
		}
		copy(output[i*len(point):(i+1)*len(point)], row)
	}
	return 0
}

//export goOptimizeRelease
func goOptimizeRelease(handle C.uintptr_t) { cgo.Handle(handle).Delete() }
