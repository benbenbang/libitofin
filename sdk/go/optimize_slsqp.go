package itofin

import (
	"fmt"
	"math"
)

// SLSQPConstraintKind selects an equality c(x)=0 or inequality c(x)>=0.
type SLSQPConstraintKind int32

const (
	SLSQPEquality   SLSQPConstraintKind = 0
	SLSQPInequality SLSQPConstraintKind = 1
)

// SLSQPConstraint describes a vector-valued constraint. Fun returns Dimension
// values; Jac, when supplied, returns Dimension rows of len(x) derivatives.
// A zero Dimension infers the arity by calling Fun once at x0 before solving.
type SLSQPConstraint struct {
	Kind      SLSQPConstraintKind
	Dimension int
	Fun       func([]float64) ([]float64, error)
	Jac       func([]float64) ([][]float64, error)
}

// SLSQP configures constrained minimization. Bounds pairs use negative and
// positive infinity for open sides; nil Bounds leave all coordinates open.
// A zero option field selects the solver default.
type SLSQP struct {
	Constraints     []SLSQPConstraint
	Bounds          [][2]float64
	Gradient        func(x, out []float64) error
	FTol            float64
	MaxIter, MaxFev int
}

func (SLSQP) optimizeMethod() {}

func validateSLSQP(method SLSQP, x0 []float64) ([]int, error) {
	if method.MaxIter < 0 || method.MaxFev < 0 || !validNonnegative(method.FTol) {
		return nil, fmt.Errorf("%w: invalid SLSQP option", ErrInvalidArgument)
	}
	if method.Bounds != nil && len(method.Bounds) != len(x0) {
		return nil, fmt.Errorf("%w: bounds length must match x0", ErrInvalidArgument)
	}
	for _, pair := range method.Bounds {
		if math.IsNaN(pair[0]) || math.IsNaN(pair[1]) || pair[0] > pair[1] ||
			math.IsInf(pair[0], 1) || math.IsInf(pair[1], -1) {
			return nil, fmt.Errorf("%w: invalid SLSQP bounds", ErrInvalidArgument)
		}
	}
	if len(x0) == 0 {
		return nil, fmt.Errorf("%w: x0 must not be empty", ErrInvalidArgument)
	}
	probeX := append([]float64(nil), x0...)
	for i, value := range probeX {
		if math.IsNaN(value) || math.IsInf(value, 0) {
			return nil, fmt.Errorf("%w: x0 must be finite", ErrInvalidArgument)
		}
		if method.Bounds != nil {
			probeX[i] = math.Max(method.Bounds[i][0], math.Min(value, method.Bounds[i][1]))
		}
	}
	dimensions := make([]int, len(method.Constraints))
	for i, constraint := range method.Constraints {
		if constraint.Kind != SLSQPEquality && constraint.Kind != SLSQPInequality {
			return nil, fmt.Errorf("%w: invalid SLSQP constraint kind", ErrInvalidArgument)
		}
		if constraint.Fun == nil || constraint.Dimension < 0 {
			return nil, fmt.Errorf("%w: invalid SLSQP constraint", ErrInvalidArgument)
		}
		dimension := constraint.Dimension
		if dimension == 0 {
			values, err := probeSLSQPConstraint(constraint.Fun, append([]float64(nil), probeX...))
			if err != nil {
				return nil, err
			}
			dimension = len(values)
		}
		if dimension == 0 || dimension > int(^uint(0)>>1)/len(x0) {
			return nil, fmt.Errorf("%w: invalid SLSQP constraint dimension", ErrInvalidArgument)
		}
		dimensions[i] = dimension
	}
	return dimensions, nil
}

func probeSLSQPConstraint(fun func([]float64) ([]float64, error), x []float64) (values []float64, err error) {
	defer func() {
		if value := recover(); value != nil {
			err = fmt.Errorf("itofin: constraint panic: %v", value)
		}
	}()
	return fun(x)
}
