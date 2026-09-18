package itofin

import (
	"errors"
	"fmt"
	"math"
	"strings"
	"testing"
	"time"
)

func TestBootstrapCallbackRetryAfterFailure(t *testing.T) {
	for _, phase := range []string{"dates", "penalties"} {
		for _, panics := range []bool{false, true} {
			t.Run(fmt.Sprintf("%s/panic=%v", phase, panics), func(t *testing.T) {
				s, _, quotes, cfg := globalStrip(t)
				fail, calls := true, 0
				callback := func() error {
					calls++
					if fail {
						if panics {
							panic("retry sentinel")
						}
						return errors.New("retry sentinel")
					}
					return nil
				}
				if phase == "dates" {
					cfg.AdditionalDates = func() ([]Date, error) { return nil, callback() }
				} else {
					cfg.AdditionalPenalties = func(BootstrapState) ([]float64, error) { return nil, callback() }
				}
				curve := sessionMust(s.NewPiecewiseYieldCurve(cfg))
				if _, err := curve.Discount(.05, false); err == nil || !strings.Contains(err.Error(), "retry sentinel") {
					t.Fatalf("first calculation: %v", err)
				}
				before := calls
				fail = false
				got, err := curve.DiscountDate(curveDate(t, 17, 7, 2026), false)
				curveNear(t, curveMust(t, got, err), 1/(1+.04*30/360), 1e-9)
				if calls <= before {
					t.Fatal("retry did not invoke callback")
				}
				if err := quotes[0].SetValue(.05); err != nil {
					t.Fatal(err)
				}
				got, err = curve.DiscountDate(curveDate(t, 17, 7, 2026), false)
				curveNear(t, curveMust(t, got, err), 1/(1+.05*30/360), 1e-9)
			})
		}
	}
}

func TestBootstrapCallbackRejectsInvalidResults(t *testing.T) {
	for _, kind := range []string{"nan", "infinity", "changing-length", "invalid-date", "missing-variable-residual"} {
		t.Run(kind, func(t *testing.T) {
			s, _, _, cfg := globalStrip(t)
			want := "finite residuals"
			switch kind {
			case "nan":
				cfg.AdditionalPenalties = func(BootstrapState) ([]float64, error) { return []float64{math.NaN()}, nil }
			case "infinity":
				cfg.AdditionalPenalties = func(BootstrapState) ([]float64, error) { return []float64{math.Inf(1)}, nil }
			case "changing-length":
				want = "residual count changed"
				calls := 0
				cfg.AdditionalPenalties = func(BootstrapState) ([]float64, error) { calls++; return make([]float64, calls%2+1), nil }
			case "invalid-date":
				want = "date"
				cfg.AdditionalDates = func() ([]Date, error) { return []Date{{}}, nil }
			case "missing-variable-residual":
				want = "less functions"
				quote := sessionMust(s.NewSimpleQuote(.01))
				cfg.AdditionalVariables = sessionMust(s.NewSimpleQuoteVariables([]*SimpleQuote{quote}, []float64{.01}, nil))
			}
			curve, err := s.NewPiecewiseYieldCurve(cfg)
			if err == nil {
				_, err = curve.Discount(.05, false)
			}
			if err == nil || !strings.Contains(err.Error(), want) {
				t.Fatalf("invalid %s: %v, want %q", kind, err, want)
			}
		})
	}
}

func TestBootstrapCallbackSnapshotCannotMutateCurve(t *testing.T) {
	s, _, _, cfg := globalStrip(t)
	var previous BootstrapState
	calls := 0
	cfg.AdditionalPenalties = func(state BootstrapState) ([]float64, error) {
		calls++
		if len(state.Times) != 3 || len(state.Data) != 3 || state.Times[0] != 0 || state.Data[0] != 1 {
			return nil, fmt.Errorf("snapshot corruption: %+v", state)
		}
		if calls > 1 && (previous.Times[0] != -100 || previous.Data[0] != -100) {
			return nil, errors.New("a retained snapshot changed after callback return")
		}
		previous = state
		for i := range state.Times {
			state.Times[i], state.Data[i] = -100, -100
		}
		return nil, nil
	}
	curve := sessionMust(s.NewPiecewiseYieldCurve(cfg))
	got, err := curve.DiscountDate(curveDate(t, 17, 7, 2026), false)
	curveNear(t, curveMust(t, got, err), 1/(1+.04*30/360), 1e-9)
	dates, values, err := curve.Nodes()
	if err != nil {
		t.Fatal(err)
	}
	if calls < 2 || len(dates) != 3 || values[0] != 1 || dates[0] != cfg.ReferenceDate {
		t.Fatalf("native state changed: calls=%d dates=%v data=%v", calls, dates, values)
	}
}

func TestBootstrapCallbackRejectsConcurrentSessionCall(t *testing.T) {
	s, _, quotes, cfg := globalStrip(t)
	calls := 0
	cfg.AdditionalPenalties = func(BootstrapState) ([]float64, error) {
		calls++
		result := make(chan error, 2)
		go func() { _, err := quotes[0].Value(); result <- err }()
		go func() { result <- s.Close() }()
		for range 2 {
			select {
			case err := <-result:
				if !errors.Is(err, ErrCallbackReentry) {
					return nil, fmt.Errorf("concurrent session call: %v", err)
				}
			case <-time.After(5 * time.Second):
				return nil, errors.New("concurrent session call blocked inside callback")
			}
		}
		return nil, nil
	}
	curve := sessionMust(s.NewPiecewiseYieldCurve(cfg))
	got, err := curve.DiscountDate(curveDate(t, 17, 7, 2026), false)
	curveNear(t, curveMust(t, got, err), 1/(1+.04*30/360), 1e-9)
	if calls == 0 {
		t.Fatal("callback was not invoked")
	}
	got, err = quotes[0].Value()
	curveNear(t, curveMust(t, got, err), .04, 0)
}
