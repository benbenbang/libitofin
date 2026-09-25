package itofin

import (
	"context"
	"errors"
	"math"
	"strings"
	"testing"
	"time"
)

func rosenbrock(x []float64) (float64, error) {
	return 100*math.Pow(x[1]-x[0]*x[0], 2) + math.Pow(1-x[0], 2), nil
}

func TestMinimizeNelderMeadRosenbrock(t *testing.T) {
	result, err := Minimize(context.Background(), rosenbrock, []float64{-1.2, 1}, NelderMeadOptions{XAtol: 1e-8, FAtol: 1e-8})
	ratesOK(t, err)
	if !result.Success || math.Abs(result.X[0]-1) > 1e-6 || math.Abs(result.X[1]-1) > 1e-6 || result.Fun > 1e-10 {
		t.Fatalf("%+v", result)
	}
	if result.Nit == 0 || result.Nfev <= result.Nit || result.Njev != 0 || result.Message != result.Status.String() {
		t.Fatalf("%+v", result)
	}
	result, err = Minimize(context.Background(), rosenbrock, []float64{-1.2, 1}, NelderMeadOptions{MaxIter: 5})
	ratesOK(t, err)
	if result.Status != OptimizeMaxIterations || result.Nit != 5 || result.Success {
		t.Fatalf("%+v", result)
	}
	if _, err = Minimize(context.Background(), rosenbrock, nil, NelderMeadOptions{}); err == nil || !strings.Contains(err.Error(), "x0 must not be empty") {
		t.Fatalf("empty x0 accepted: %v", err)
	}
	if _, err = Minimize(context.Background(), rosenbrock, []float64{1}, NelderMeadOptions{MaxFev: -1}); err == nil {
		t.Fatal("negative budget accepted")
	}
}

func TestMinimizeStatusValues(t *testing.T) {
	statuses := []OptimizeStatus{OptimizeConvergedXTol, OptimizeConvergedFTol, OptimizeConvergedGTol,
		OptimizeMaxIterations, OptimizeMaxEvaluations, OptimizeCancelled, OptimizeNonfinite}
	for want, status := range statuses {
		if int(status) != want || strings.HasPrefix(status.String(), "OptimizeStatus(") {
			t.Fatalf("%d: %d %q", want, status, status)
		}
	}
}

func TestMinimizeObjectiveCallsSessionPricing(t *testing.T) {
	s, e := NewSession()
	ratesOK(t, e)
	defer s.Close()
	today, e := NewDate(7, 7, 2026)
	ratesOK(t, e)
	settings, e := s.NewSettings()
	ratesOK(t, e)
	ratesOK(t, settings.SetEvaluationDate(today))
	dc, e := s.Actual360()
	ratesOK(t, e)
	rate, e := s.NewSimpleQuote(.05)
	ratesOK(t, e)
	curve, e := s.NewFlatForwardFromQuote(today, rate, dc)
	ratesOK(t, e)
	index, e := s.NewEuriborSixMonths(curve, settings)
	ratesOK(t, e)
	fixed := .03
	swap, e := s.MakeVanillaSwap(MakeVanillaSwapConfig{Tenor: Period{5, Years}, Index: index, Settings: settings, FixedRate: &fixed})
	ratesOK(t, e)
	npv := func(x []float64) (float64, error) {
		scratch, err := s.NewSimpleQuote(x[0])
		if err != nil {
			return 0, err
		}
		defer scratch.Close()
		if err := rate.SetValue(x[0]); err != nil {
			return 0, err
		}
		value, err := swap.NPV()
		return value * value * 1e8, err
	}
	result, err := Minimize(context.Background(), npv, []float64{.05}, NelderMeadOptions{XAtol: 1e-10, FAtol: 1e-12})
	ratesOK(t, err)
	value, e := swap.NPV()
	ratesOK(t, e)
	if !result.Success || math.Abs(value) > 1e-6 || math.Abs(result.X[0]-.03) > 5e-3 {
		t.Fatalf("%+v npv=%g", result, value)
	}
}

func TestMinimizeObjectiveClosesSession(t *testing.T) {
	s, e := NewSession()
	ratesOK(t, e)
	quote, e := s.NewSimpleQuote(1)
	ratesOK(t, e)
	calls := 0
	_, err := Minimize(context.Background(), func(x []float64) (float64, error) {
		calls++
		if calls == 3 {
			if err := s.Close(); err != nil {
				return 0, err
			}
		}
		v, err := quote.Value()
		return v * x[0] * x[0], err
	}, []float64{1}, NelderMeadOptions{})
	if !errors.Is(err, ErrClosed) || calls != 3 {
		t.Fatalf("calls=%d err=%v", calls, err)
	}
	if err := s.Close(); err != nil {
		t.Fatalf("second close: %v", err)
	}
}

func TestMinimizeErrorsCancellationAndPanics(t *testing.T) {
	sentinel := errors.New("objective failed")
	_, err := Minimize(context.Background(), func([]float64) (float64, error) { return 0, sentinel }, []float64{1}, NelderMeadOptions{})
	if err != sentinel {
		t.Fatalf("error identity lost: %v", err)
	}
	_, err = Minimize(context.Background(), func([]float64) (float64, error) { panic("boom") }, []float64{1}, NelderMeadOptions{})
	if err == nil || !strings.Contains(err.Error(), "boom") {
		t.Fatalf("panic not surfaced: %v", err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	calls := 0
	result, err := Minimize(ctx, func(x []float64) (float64, error) {
		if calls++; calls == 10 {
			cancel()
		}
		return rosenbrock(x)
	}, []float64{-1.2, 1}, NelderMeadOptions{})
	if !errors.Is(err, context.Canceled) || result.Status != OptimizeCancelled || result.Success || len(result.X) != 2 || result.Nit == 0 {
		t.Fatalf("%+v %v", result, err)
	}
	deadline, stop := context.WithTimeout(context.Background(), time.Millisecond)
	defer stop()
	result, err = Minimize(deadline, func(x []float64) (float64, error) {
		time.Sleep(time.Millisecond)
		return rosenbrock(x)
	}, []float64{-1.2, 1}, NelderMeadOptions{})
	if !errors.Is(err, context.DeadlineExceeded) || result.Status != OptimizeCancelled {
		t.Fatalf("%+v %v", result, err)
	}
}
