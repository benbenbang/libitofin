package itofin

import (
	"context"
	"errors"
	"math"
	"reflect"
	"strings"
	"testing"
)

var _ OptimizeMethod = ParticleSwarm{}
var _ OptimizeMethod = (*ParticleSwarm)(nil)

func TestParticleSwarmNegativeSeededObjective(t *testing.T) {
	fn := func(x []float64) (float64, error) { return math.Pow(x[0]-0.25, 2) - 3, nil }
	method := ParticleSwarm{Bounds: [][2]float64{{-2, 2}}}
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

func TestParticleSwarmMultimodalBudgetAndInitialRows(t *testing.T) {
	zero := 0.0
	fn := func(x []float64) (float64, error) {
		return 20 + x[0]*x[0] + x[1]*x[1] - 10*(math.Cos(2*math.Pi*x[0])+math.Cos(2*math.Pi*x[1])), nil
	}
	method := ParticleSwarm{Bounds: [][2]float64{{-5.12, 5.12}, {-5.12, 5.12}}, Seed: 42, PopulationSize: 40}
	result, err := Minimize(context.Background(), fn, []float64{2, 2}, method)
	ratesOK(t, err)
	if !result.Success || result.Fun > 1e-6 || result.Nfev > 1_000_000 {
		t.Fatalf("multimodal result: %+v", result)
	}
	method.MaxIter, method.XAtol, method.FAtol = 2, &zero, &zero
	result, err = Minimize(context.Background(), fn, []float64{2, 2}, method)
	ratesOK(t, err)
	if result.Status != OptimizeMaxIterations || result.Nit != 2 || result.Nfev != 120 {
		t.Fatalf("generation budget: %+v", result)
	}
	initial := [][]float64{{-1}, {0.25}, {0.5}, {1}}
	var calls [][]float64
	method = ParticleSwarm{Bounds: [][2]float64{{-1, 1}}, InitialPopulation: initial, MaxFev: 4, Cognitive: &zero}
	result, err = Minimize(context.Background(), func(x []float64) (float64, error) {
		calls = append(calls, append([]float64(nil), x...))
		return math.Pow(x[0]-0.25, 2) - 3, nil
	}, []float64{0}, method)
	ratesOK(t, err)
	if result.Status != OptimizeMaxEvaluations || result.Nfev != 4 || result.X[0] != 0.25 || !reflect.DeepEqual(calls, initial) {
		t.Fatalf("initial preservation: result=%+v calls=%v", result, calls)
	}
}

func TestParticleSwarmInvalidOptionsNeverEvaluate(t *testing.T) {
	negative, nan, infinity, tooHigh, zero := -1.0, math.NaN(), math.Inf(1), 1.1, 0.0
	bounds := [][2]float64{{-1, 1}}
	invalid := []ParticleSwarm{
		{}, {Bounds: bounds, PopulationSize: 3}, {Bounds: bounds, PopulationSize: 4097},
		{Bounds: bounds, MaxIter: -1}, {Bounds: bounds, MaxFev: -1},
		{Bounds: bounds, MaxIter: 1_000_001}, {Bounds: bounds, MaxFev: 10_000_001},
		{Bounds: bounds, XAtol: &negative}, {Bounds: bounds, FAtol: &nan},
		{Bounds: bounds, VelocityClamp: &infinity}, {Bounds: bounds, VelocityClamp: &zero},
		{Bounds: bounds, Inertia: &tooHigh}, {Bounds: bounds, Cognitive: &negative}, {Bounds: bounds, Social: &infinity},
		{Bounds: [][2]float64{{math.Inf(-1), 1}}}, {Bounds: [][2]float64{{-math.MaxFloat64, math.MaxFloat64}}},
		{Bounds: [][2]float64{{2, 1}}}, {Bounds: bounds, InitialPopulation: [][]float64{}},
		{Bounds: bounds, InitialPopulation: [][]float64{{0}, {0}, {0}, {math.NaN()}}},
		{Bounds: bounds, InitialPopulation: [][]float64{{0}, {0}, {0}, {2}}},
		{Bounds: bounds, InitialPopulation: [][]float64{{0}, {0}, {0}, {0, 1}}},
		{Bounds: bounds, PopulationSize: 5, InitialPopulation: [][]float64{{0}, {0}, {0}, {0}}},
	}
	calls := 0
	fn := func([]float64) (float64, error) { calls++; return 0, nil }
	for _, method := range invalid {
		if _, err := Minimize(context.Background(), fn, []float64{0}, method); !errors.Is(err, ErrInvalidArgument) {
			t.Fatalf("invalid %+v: %v", method, err)
		}
	}
	for _, x0 := range [][]float64{nil, {2}, {math.NaN()}, make([]float64, 257)} {
		if _, err := Minimize(context.Background(), fn, x0, ParticleSwarm{Bounds: bounds}); !errors.Is(err, ErrInvalidArgument) {
			t.Fatalf("invalid x0: %v", err)
		}
	}
	if _, err := Minimize(context.Background(), fn, []float64{0}, (*ParticleSwarm)(nil)); !errors.Is(err, ErrInvalidArgument) {
		t.Fatalf("nil method: %v", err)
	}
	if calls != 0 {
		t.Fatalf("invalid inputs made %d objective calls", calls)
	}
}

func TestParticleSwarmErrorPanicCancellationAndReentry(t *testing.T) {
	method := ParticleSwarm{Bounds: [][2]float64{{-1, 1}}}
	sentinel := errors.New("PSO objective failed")
	_, err := Minimize(context.Background(), func([]float64) (float64, error) { return 0, sentinel }, []float64{0}, method)
	if err != sentinel {
		t.Fatalf("error identity: %v", err)
	}
	_, err = Minimize(context.Background(), func([]float64) (float64, error) { panic("PSO panic") }, []float64{0}, method)
	if err == nil || !strings.Contains(err.Error(), "PSO panic") {
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

func TestParticleSwarmFixedCoordinatesAndInputCopies(t *testing.T) {
	initial := [][]float64{{-1, 2}, {0, 2}, {0.5, 2}, {1, 2}}
	method := ParticleSwarm{Bounds: [][2]float64{{-1, 1}, {2, 2}}, InitialPopulation: initial, MaxFev: 4}
	result, err := Minimize(context.Background(), func(x []float64) (float64, error) {
		v := math.Pow(x[0]-0.5, 2) + math.Pow(x[1]-2, 2)
		x[0], x[1] = 100, 100
		return v, nil
	}, []float64{0, 2}, method)
	ratesOK(t, err)
	if result.X[0] != 0.5 || result.X[1] != 2 || initial[0][0] != -1 || initial[0][1] != 2 {
		t.Fatalf("borrowed inputs mutated: %+v, %v", result, initial)
	}
}

func TestParticleSwarmAllFixedAndZeroCoefficients(t *testing.T) {
	zero := 0.0
	calls := 0
	result, err := Minimize(context.Background(), func(x []float64) (float64, error) {
		calls++
		return math.Pow(x[0]-.25, 2) - 3, nil
	}, []float64{.25}, ParticleSwarm{Bounds: [][2]float64{{.25, .25}}, Inertia: &zero, Cognitive: &zero, Social: &zero})
	ratesOK(t, err)
	if !result.Success || result.Fun != -3 || result.Nit != 0 || result.Nfev != 1 || result.Njev != 0 || calls != 1 {
		t.Fatalf("all-fixed: %+v calls=%d", result, calls)
	}
	initial := [][]float64{{-1}, {.25}, {.5}, {1}}
	result, err = Minimize(context.Background(), func(x []float64) (float64, error) {
		return math.Pow(x[0]-.25, 2) - 3, nil
	}, []float64{0}, ParticleSwarm{Bounds: [][2]float64{{-1, 1}}, InitialPopulation: initial, MaxFev: 5, Inertia: &zero, Cognitive: &zero, Social: &zero})
	ratesOK(t, err)
	if result.Status != OptimizeMaxEvaluations || result.Nit != 0 || result.Nfev != 5 || result.X[0] != .25 {
		t.Fatalf("partial generation: %+v", result)
	}
}

func TestParticleSwarmNonfiniteIsNotCallbackError(t *testing.T) {
	result, err := Minimize(context.Background(), func([]float64) (float64, error) {
		return math.NaN(), nil
	}, []float64{0}, ParticleSwarm{Bounds: [][2]float64{{-1, 1}}})
	ratesOK(t, err)
	if result.Status != OptimizeNonfinite || result.Success || result.Nfev != 1 {
		t.Fatalf("nonfinite: %+v", result)
	}
}
