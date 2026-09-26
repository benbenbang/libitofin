package itofin

import (
	"context"
	"errors"
	"math"
	"strings"
	"testing"
)

var _ OptimizeMethod = SLSQP{Constraints: []SLSQPConstraint{{Kind: SLSQPEquality}}}

func TestMinimizeSLSQPVectorArityAndProjectedProbe(t *testing.T) {
	probe := math.NaN()
	constraints := []SLSQPConstraint{{
		Kind: SLSQPInequality,
		Fun: func(x []float64) ([]float64, error) {
			if math.IsNaN(probe) {
				probe = x[0]
			}
			return []float64{x[0] - 1, 2 - x[0]}, nil
		},
		Jac: func([]float64) ([][]float64, error) { return [][]float64{{1}, {-1}}, nil },
	}}
	objective := func(x []float64) (float64, error) { return math.Pow(x[0]-3, 2), nil }
	result, err := Minimize(context.Background(), objective, []float64{5}, SLSQP{
		Bounds: [][2]float64{{0, 2}}, Constraints: constraints,
	})
	ratesOK(t, err)
	if probe != 2 || !result.Success || math.Abs(result.X[0]-2) > 1e-6 {
		t.Fatalf("probe=%g result=%+v", probe, result)
	}
	called := false
	constraints[0].Fun = func([]float64) ([]float64, error) { called = true; return []float64{1}, nil }
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	_, err = Minimize(ctx, objective, []float64{0}, SLSQP{Constraints: constraints})
	if !errors.Is(err, context.Canceled) || called {
		t.Fatalf("pre-cancelled context invoked constraint: %v", err)
	}
	_, err = Minimize(context.Background(), objective, []float64{math.NaN()}, SLSQP{Constraints: constraints})
	if !errors.Is(err, ErrInvalidArgument) || called {
		t.Fatalf("nonfinite x0 invoked constraint: %v", err)
	}
}

func TestMinimizeSLSQPHockSchittkowski71(t *testing.T) {
	objective := func(x []float64) (float64, error) {
		return x[0]*x[3]*(x[0]+x[1]+x[2]) + x[2], nil
	}
	method := SLSQP{
		Bounds: [][2]float64{{1, 5}, {1, 5}, {1, 5}, {1, 5}},
		Constraints: []SLSQPConstraint{
			{Kind: SLSQPInequality,
				Fun: func(x []float64) ([]float64, error) {
					return []float64{x[0]*x[1]*x[2]*x[3] - 25}, nil
				},
				Jac: func(x []float64) ([][]float64, error) {
					return [][]float64{{x[1] * x[2] * x[3], x[0] * x[2] * x[3], x[0] * x[1] * x[3], x[0] * x[1] * x[2]}}, nil
				}},
			{Kind: SLSQPEquality,
				Fun: func(x []float64) ([]float64, error) {
					return []float64{x[0]*x[0] + x[1]*x[1] + x[2]*x[2] + x[3]*x[3] - 40}, nil
				},
				Jac: func(x []float64) ([][]float64, error) {
					return [][]float64{{2 * x[0], 2 * x[1], 2 * x[2], 2 * x[3]}}, nil
				}},
		},
		FTol: 1e-10,
	}
	result, err := Minimize(context.Background(), objective, []float64{1, 5, 5, 1}, method)
	ratesOK(t, err)
	if !result.Success || result.Status != OptimizeConvergedFTol || math.Abs(result.Fun-17.0140173) > 1e-5 || result.Nfev == 0 || result.Njev == 0 {
		t.Fatalf("HS71: %+v", result)
	}
	if math.Abs(result.X[0]*result.X[1]*result.X[2]*result.X[3]-25) > 1e-5 ||
		math.Abs(result.X[0]*result.X[0]+result.X[1]*result.X[1]+result.X[2]*result.X[2]+result.X[3]*result.X[3]-40) > 1e-5 {
		t.Fatalf("HS71 feasibility: %+v", result)
	}
	pointer, err := Minimize(context.Background(), objective, []float64{1, 5, 5, 1}, &method)
	ratesOK(t, err)
	if !pointer.Success || math.Abs(pointer.Fun-result.Fun) > 1e-7 {
		t.Fatalf("pointer SLSQP: %+v", pointer)
	}
}

func TestMinimizeSLSQPInfeasibleAndCancellation(t *testing.T) {
	quadratic := func(x []float64) (float64, error) { return x[0]*x[0] + x[1]*x[1], nil }
	method := SLSQP{Constraints: []SLSQPConstraint{
		{Kind: SLSQPInequality, Fun: func(x []float64) ([]float64, error) { return []float64{x[0] - 1}, nil }},
		{Kind: SLSQPInequality, Fun: func(x []float64) ([]float64, error) { return []float64{-x[0]}, nil }},
	}}
	result, err := Minimize(context.Background(), quadratic, []float64{0.5, 0}, method)
	ratesOK(t, err)
	if result.Status != OptimizeInfeasible || result.Success || result.Message != OptimizeInfeasible.String() {
		t.Fatalf("infeasible: %+v", result)
	}
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	called := 0
	result, err = Minimize(ctx, func(x []float64) (float64, error) {
		called++
		cancel()
		return x[0]*x[0] + 2*x[1]*x[1] + 3*x[2]*x[2], nil
	}, []float64{3, 3, 3}, SLSQP{Constraints: []SLSQPConstraint{{Kind: SLSQPEquality, Fun: func(x []float64) ([]float64, error) {
		return []float64{x[0] + x[1] + x[2] - 1}, nil
	}}}})
	if !errors.Is(err, context.Canceled) || result.Status != OptimizeCancelled || result.Success || called == 0 {
		t.Fatalf("cancelled: %+v error=%v calls=%d", result, err, called)
	}
}

func TestMinimizeSLSQPConstraintErrorsAndShapes(t *testing.T) {
	quadratic := func(x []float64) (float64, error) { return (x[0] - 1) * (x[0] - 1), nil }
	sentinel := errors.New("constraint failed")
	for _, dimension := range []int{0, 1} {
		_, err := Minimize(context.Background(), quadratic, []float64{0}, SLSQP{Constraints: []SLSQPConstraint{{
			Kind: SLSQPInequality, Dimension: dimension,
			Fun: func([]float64) ([]float64, error) { return nil, sentinel },
		}}})
		if err != sentinel {
			t.Fatalf("dimension %d lost error identity: %v", dimension, err)
		}
	}
	_, err := Minimize(context.Background(), quadratic, []float64{0}, SLSQP{Constraints: []SLSQPConstraint{{
		Kind: SLSQPInequality, Dimension: 2,
		Fun: func(x []float64) ([]float64, error) { return []float64{x[0]}, nil },
	}}})
	if !errors.Is(err, ErrInvalidArgument) || !strings.Contains(err.Error(), "vector length") {
		t.Fatalf("wrong vector length: %v", err)
	}
	_, err = Minimize(context.Background(), quadratic, []float64{0}, SLSQP{Constraints: []SLSQPConstraint{{
		Kind: SLSQPInequality, Dimension: 1,
		Fun: func(x []float64) ([]float64, error) { return []float64{x[0]}, nil },
		Jac: func([]float64) ([][]float64, error) { return [][]float64{{1, 2}}, nil },
	}}})
	if !errors.Is(err, ErrInvalidArgument) || !strings.Contains(err.Error(), "Jacobian") {
		t.Fatalf("wrong Jacobian shape: %v", err)
	}
	_, err = Minimize(context.Background(), quadratic, []float64{0}, SLSQP{Constraints: []SLSQPConstraint{{
		Kind: SLSQPInequality, Dimension: 1,
		Fun: func(x []float64) ([]float64, error) { return []float64{x[0]}, nil },
		Jac: func([]float64) ([][]float64, error) { return nil, sentinel },
	}}})
	if err != sentinel {
		t.Fatalf("Jacobian error identity: %v", err)
	}
	invalid := []SLSQP{
		{Bounds: [][2]float64{{0, 1}, {0, 1}}},
		{Bounds: [][2]float64{{2, 1}}},
		{FTol: math.NaN()},
		{Constraints: []SLSQPConstraint{{Kind: 9, Fun: func([]float64) ([]float64, error) { return []float64{1}, nil }}}},
		{Constraints: []SLSQPConstraint{{Kind: SLSQPInequality, Dimension: -1, Fun: func([]float64) ([]float64, error) { return []float64{1}, nil }}}},
	}
	for _, method := range invalid {
		if _, err := Minimize(context.Background(), quadratic, []float64{0}, method); !errors.Is(err, ErrInvalidArgument) {
			t.Fatalf("invalid SLSQP method: %v", err)
		}
	}
	if _, err := Minimize(context.Background(), quadratic, []float64{0}, (*SLSQP)(nil)); !errors.Is(err, ErrInvalidArgument) {
		t.Fatalf("nil SLSQP: %v", err)
	}
}
