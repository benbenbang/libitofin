package itofin

import (
	"context"
	"errors"
	"math"
	"reflect"
	"strings"
	"testing"
)

var _ OptimizeMethod = HybridSimulatedAnnealing{}
var _ OptimizeMethod = (*HybridSimulatedAnnealing)(nil)

func TestHybridSimulatedAnnealingNegativeSeededObjective(t *testing.T) {
	fn := func(x []float64) (float64, error) { return math.Pow(x[0]-0.25, 2) - 3, nil }
	method := HybridSimulatedAnnealing{Bounds: [][2]float64{{-2, 2}}}
	first, err := Minimize(context.Background(), fn, []float64{0}, method)
	ratesOK(t, err)
	second, err := Minimize(context.Background(), fn, []float64{0}, &method)
	ratesOK(t, err)
	if !first.Success || math.Abs(first.X[0]-0.25) > 1e-5 || first.Fun >= -2.999999 || first.Njev != 0 {
		t.Fatalf("negative minimum: %+v", first)
	}
	if !reflect.DeepEqual(first, second) {
		t.Fatalf("deterministic seed: %+v != %+v", first, second)
	}
}

func TestHybridSimulatedAnnealingErrorPanicCancellationAndReentry(t *testing.T) {
	method := HybridSimulatedAnnealing{Bounds: [][2]float64{{-1, 1}}}
	sentinel := errors.New("annealing objective failed")
	_, err := Minimize(context.Background(), func([]float64) (float64, error) { return 0, sentinel }, []float64{0}, method)
	if err != sentinel {
		t.Fatalf("error identity: %v", err)
	}
	_, err = Minimize(context.Background(), func([]float64) (float64, error) { panic("annealing panic") }, []float64{0}, method)
	if err == nil || !strings.Contains(err.Error(), "annealing panic") {
		t.Fatalf("panic propagation: %v", err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	calls := 0
	_, err = Minimize(ctx, func([]float64) (float64, error) { calls++; return 0, nil }, []float64{0}, method)
	if !errors.Is(err, context.Canceled) || calls != 0 {
		t.Fatalf("pre-cancel: %v, %d calls", err, calls)
	}
	ctx, cancel = context.WithCancel(context.Background())
	defer cancel()
	result, err := Minimize(ctx, func(x []float64) (float64, error) {
		cancel()
		return x[0] * x[0], nil
	}, []float64{0.5}, method)
	if !errors.Is(err, context.Canceled) || result.Status != OptimizeCancelled || len(result.X) != 1 {
		t.Fatalf("partial cancellation: %+v, %v", result, err)
	}
	session, err := NewSession()
	ratesOK(t, err)
	defer session.Close()
	quote, err := session.NewSimpleQuote(0.25)
	ratesOK(t, err)
	defer quote.Close()
	result, err = Minimize(context.Background(), func(x []float64) (float64, error) {
		v, err := quote.Value()
		return math.Pow(x[0]-v, 2), err
	}, []float64{0}, method)
	ratesOK(t, err)
	if !result.Success || math.Abs(result.X[0]-0.25) > 1e-5 {
		t.Fatalf("session reentry: %+v", result)
	}
}

func TestHybridSimulatedAnnealingNonfiniteIsNotCallbackError(t *testing.T) {
	result, err := Minimize(context.Background(), func([]float64) (float64, error) {
		return math.NaN(), nil
	}, []float64{0}, HybridSimulatedAnnealing{Bounds: [][2]float64{{-1, 1}}})
	ratesOK(t, err)
	if result.Status != OptimizeNonfinite || result.Success || result.Nfev != 1 {
		t.Fatalf("nonfinite: %+v", result)
	}
}

func TestHybridSimulatedAnnealingBudgetsFixedAndCopies(t *testing.T) {
	zero := 0.0
	method := HybridSimulatedAnnealing{Bounds: [][2]float64{{-1, 1}, {2, 2}}, MaxFev: 1, XAtol: &zero, FAtol: &zero}
	x0 := []float64{.5, 2}
	result, err := Minimize(context.Background(), func(x []float64) (float64, error) {
		value := x[0]*x[0] - 3
		x[0], x[1] = 100, 100
		return value, nil
	}, x0, method)
	ratesOK(t, err)
	if result.Status != OptimizeMaxEvaluations || result.Nfev != 1 || result.Nit != 0 || result.Njev != 0 || result.X[0] != .5 || result.X[1] != 2 || x0[0] != .5 {
		t.Fatalf("budget/copies: %+v x0=%v", result, x0)
	}
	method = HybridSimulatedAnnealing{Bounds: [][2]float64{{.25, .25}}}
	calls := 0
	result, err = Minimize(context.Background(), func(x []float64) (float64, error) { calls++; return -3, nil }, []float64{.25}, method)
	ratesOK(t, err)
	if !result.Success || result.Nit != 0 || result.Nfev != 1 || calls != 1 || result.Fun != -3 {
		t.Fatalf("fixed: %+v", result)
	}
	method = HybridSimulatedAnnealing{Bounds: [][2]float64{{-1, 1}}, MaxIter: 2, XAtol: &zero, FAtol: &zero}
	result, err = Minimize(context.Background(), func(x []float64) (float64, error) { return x[0] * x[0], nil }, []float64{.5}, method)
	ratesOK(t, err)
	if result.Status != OptimizeMaxIterations || result.Nit != 2 || result.Nfev != 3 {
		t.Fatalf("iterations: %+v", result)
	}
}

func TestHybridSimulatedAnnealingInvalidOptionsNeverEvaluate(t *testing.T) {
	negative, nan, infinity, one, zero := -1.0, math.NaN(), math.Inf(1), 1.0, 0.0
	zeroInt, many, tooManySteps := 0, 1_000_001, 257
	bounds := [][2]float64{{-1, 1}}
	invalid := []HybridSimulatedAnnealing{
		{}, {Bounds: bounds, MaxIter: -1}, {Bounds: bounds, MaxFev: -1},
		{Bounds: bounds, MaxIter: 1_000_001}, {Bounds: bounds, MaxFev: 10_000_001},
		{Bounds: bounds, XAtol: &negative}, {Bounds: bounds, FAtol: &nan},
		{Bounds: bounds, InitialTemperature: &infinity}, {Bounds: bounds, InitialTemperature: &zero},
		{Bounds: bounds, CoolingRate: &one}, {Bounds: bounds, CoolingRate: &zero},
		{Bounds: bounds, StepSize: &zero}, {Bounds: bounds, StepSize: &infinity},
		{Bounds: bounds, LocalSearchInterval: &zeroInt}, {Bounds: bounds, LocalSearchInterval: &many},
		{Bounds: bounds, LocalSearchSteps: &tooManySteps}, {Bounds: bounds, ReannealInterval: &zeroInt},
		{Bounds: [][2]float64{{math.Inf(-1), 1}}}, {Bounds: [][2]float64{{-math.MaxFloat64, math.MaxFloat64}}},
		{Bounds: [][2]float64{{2, 1}}},
	}
	calls := 0
	fn := func([]float64) (float64, error) { calls++; return 0, nil }
	for _, method := range invalid {
		if _, err := Minimize(context.Background(), fn, []float64{0}, method); !errors.Is(err, ErrInvalidArgument) {
			t.Fatalf("invalid %+v: %v", method, err)
		}
	}
	for _, x0 := range [][]float64{nil, {2}, {math.NaN()}, make([]float64, 257)} {
		if _, err := Minimize(context.Background(), fn, x0, HybridSimulatedAnnealing{Bounds: bounds}); !errors.Is(err, ErrInvalidArgument) {
			t.Fatalf("invalid x0: %v", err)
		}
	}
	if _, err := Minimize(context.Background(), fn, []float64{0}, (*HybridSimulatedAnnealing)(nil)); !errors.Is(err, ErrInvalidArgument) {
		t.Fatalf("nil: %v", err)
	}
	if calls != 0 {
		t.Fatalf("invalid inputs made %d calls", calls)
	}
}
