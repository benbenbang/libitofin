package itofin

import (
	"bufio"
	"context"
	"encoding/json"
	"errors"
	"math"
	"os"
	"testing"
)

type fireflyFixtureInput struct {
	Objective         string
	X0                []float64
	Bounds            [][2]float64
	MaxIter           int         `json:"maxiter"`
	MaxFev            int         `json:"maxfev"`
	PopulationSize    int         `json:"population_size"`
	InitialPopulation [][]float64 `json:"initial_population"`
	Options           struct {
		Seed                uint64
		Alpha, Beta0, Gamma *float64
		AlphaDecay          *float64 `json:"alpha_decay"`
		XAtol, FAtol        *float64
	}
	FailAtEvaluation      int `json:"fail_at_evaluation"`
	FailAtCallback        int `json:"fail_at_callback"`
	NonfiniteAtEvaluation int `json:"nonfinite_at_evaluation"`
	CancelAfterGeneration int `json:"cancel_after_generation"`
}

type fireflyFixturePoint struct {
	X               []float64
	Fun             *float64
	Nit, Nfev, Njev int
	Status          string
	Success         bool
}

func fireflyFixtureValue(name string, x []float64) float64 {
	switch name {
	case "signed_quadratic":
		return math.Pow(x[0]-1.25, 2) + 4*math.Pow(x[1]+.75, 2) - 3
	case "boundary_quadratic":
		return math.Pow(x[0]-3, 2) - 20
	case "quadratic":
		return x[0] * x[0]
	case "linear":
		return x[0]
	case "constant":
		return -7
	case "multimodal_polynomial":
		return math.Pow(x[0]-1, 2) * (math.Pow(x[0]+2, 2) + .1)
	case "himmelblau":
		return math.Pow(x[0]*x[0]+x[1]-11, 2) + math.Pow(x[0]+x[1]*x[1]-7, 2)
	case "shifted_rastrigin":
		result := 15.0
		for _, v := range x {
			result += v*v - 10*math.Cos(2*math.Pi*v)
		}
		return result
	default:
		panic("unknown independent fixture objective")
	}
}

func fireflyFixtureNear(t *testing.T, got, want []float64) {
	t.Helper()
	if len(got) != len(want) {
		t.Fatalf("shape %v != %v", got, want)
	}
	for i := range got {
		if math.Max(math.Abs(got[i]), math.Abs(want[i])) < 1e-50 {
			if got[i] != want[i] {
				t.Fatalf("microscopic values %v != %v", got, want)
			}
			continue
		}
		if math.IsNaN(got[i]) || math.IsInf(got[i], 0) || math.Abs(got[i]-want[i]) > 3e-12*math.Max(1, math.Abs(want[i])) {
			t.Fatalf("values %v != %v", got, want)
		}
	}
}

func TestFireflyIndependentPolicyFixtures(t *testing.T) {
	file, err := os.Open("../../crates/itofin-optimize/tests/fixtures/firefly.jsonl")
	ratesOK(t, err)
	defer file.Close()
	type fixture struct {
		input       fireflyFixtureInput
		result      fireflyFixturePoint
		evaluations []fireflyFixturePoint
	}
	cases := make(map[string]*fixture)
	var names []string
	scanner := bufio.NewScanner(file)
	for scanner.Scan() {
		var record struct {
			Kind, Name string
			Value      json.RawMessage
		}
		ratesOK(t, json.Unmarshal(scanner.Bytes(), &record))
		if record.Kind == "input" {
			cases[record.Name] = &fixture{}
			names = append(names, record.Name)
		}
		current := cases[record.Name]
		if current == nil {
			continue
		}
		switch record.Kind {
		case "input":
			ratesOK(t, json.Unmarshal(record.Value, &current.input))
		case "result":
			ratesOK(t, json.Unmarshal(record.Value, &current.result))
		case "evaluation":
			var point fireflyFixturePoint
			ratesOK(t, json.Unmarshal(record.Value, &point))
			current.evaluations = append(current.evaluations, point)
		}
	}
	ratesOK(t, scanner.Err())
	if len(names) != 28 {
		t.Fatalf("fixture count %d", len(names))
	}
	for _, name := range names {
		t.Run(name, func(t *testing.T) {
			current := cases[name]
			input, expected := current.input, current.result
			if input.FailAtCallback != 0 {
				t.Skip("Go Minimize exposes context cancellation, not a user iteration callback; C ABI tests cover this case")
			}
			options := input.Options
			method := Firefly{Bounds: input.Bounds, Seed: options.Seed, MaxIter: input.MaxIter, MaxFev: input.MaxFev,
				PopulationSize: input.PopulationSize, InitialPopulation: input.InitialPopulation,
				XAtol: options.XAtol, FAtol: options.FAtol, Alpha: options.Alpha,
				Beta0: options.Beta0, Gamma: options.Gamma, AlphaDecay: options.AlphaDecay}
			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()
			sentinel := errors.New("independent objective failure")
			calls := 0
			var trace []fireflyFixturePoint
			result, err := Minimize(ctx, func(x []float64) (float64, error) {
				calls++
				if calls == input.FailAtEvaluation {
					value := math.NaN()
					trace = append(trace, fireflyFixturePoint{X: append([]float64(nil), x...), Fun: &value})
					return 0, sentinel
				}
				value := fireflyFixtureValue(input.Objective, x)
				if calls == input.NonfiniteAtEvaluation {
					value = math.NaN()
				}
				if input.CancelAfterGeneration == 1 && calls == 11 {
					cancel()
				}
				trace = append(trace, fireflyFixturePoint{X: append([]float64(nil), x...), Fun: &value})
				return value, nil
			}, input.X0, method)
			if expected.Status == "objective_error" {
				if err != sentinel || calls != expected.Nfev {
					t.Fatalf("typed error/count: %v, %d != %d", err, calls, expected.Nfev)
				}
			} else {
				if expected.Status == "cancelled" {
					if !errors.Is(err, context.Canceled) {
						t.Fatalf("cancel: %v", err)
					}
				} else {
					ratesOK(t, err)
				}
				fireflyFixtureNear(t, result.X, expected.X)
				if expected.Fun == nil {
					if !math.IsNaN(result.Fun) {
						t.Fatalf("nonfinite value %g", result.Fun)
					}
				} else {
					fireflyFixtureNear(t, []float64{result.Fun}, []float64{*expected.Fun})
				}
				status := map[string]OptimizeStatus{"converged": OptimizeConvergedXTol, "max_iterations": OptimizeMaxIterations, "max_evaluations": OptimizeMaxEvaluations, "cancelled": OptimizeCancelled, "nonfinite": OptimizeNonfinite}[expected.Status]
				if result.Status != status || result.Success != expected.Success || result.Nit != expected.Nit || result.Nfev != expected.Nfev || result.Njev != expected.Njev {
					t.Fatalf("result %+v != %+v", result, expected)
				}
			}
			if len(current.evaluations) != 0 {
				if len(trace) != len(current.evaluations) {
					t.Fatalf("trace length %d != %d", len(trace), len(current.evaluations))
				}
				for i, point := range trace {
					want := current.evaluations[i]
					fireflyFixtureNear(t, point.X, want.X)
					if want.Fun == nil {
						if !math.IsNaN(*point.Fun) {
							t.Fatal("expected nonfinite evaluation")
						}
					} else {
						fireflyFixtureNear(t, []float64{*point.Fun}, []float64{*want.Fun})
					}
				}
			}
		})
	}
}
