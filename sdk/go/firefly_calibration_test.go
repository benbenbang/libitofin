package itofin

import (
	"errors"
	"math"
	"testing"
)

func fireflyCalibrationOptions() Firefly {
	return Firefly{Bounds: [][2]float64{{.001, .03}}, Seed: 42, PopulationSize: 8,
		XAtol: pricingPtr(1e-7), FAtol: pricingPtr(1e-12)}
}

func TestFireflyCalibrationFixedParametersAndResultOwnership(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	method := pricingMust(s.NewFirefly(fireflyCalibrationOptions()))
	if _, err := method.LastResult(); err == nil {
		t.Fatal("missing result accepted")
	}
	model, helpers, criteria := globalCalibrationMarket(t, s)
	pricingOK(t, model.Calibrate(helpers, method, criteria, true))
	result := pricingMust(method.LastResult())
	if !result.Success || result.Nfev <= 8 || result.Njev != 0 {
		t.Fatalf("unexpected global result: %+v", result)
	}
	if pricingMust(model.A()) != .05 {
		t.Fatal("fixed reversion changed")
	}
	pricingNear(t, pricingMust(model.Sigma()), .00585858, 1e-5)
	if result.X[0] != pricingMust(model.Sigma()) {
		t.Fatal("result differs from projected model parameter")
	}
	result.X[0] = 999
	if pricingMust(method.LastResult()).X[0] == 999 {
		t.Fatal("copied result mutated solver state")
	}
	before := pricingMust(model.Sigma())
	if err := model.Calibrate(helpers, method, criteria, false); err == nil {
		t.Fatal("wrong free dimension accepted")
	}
	if _, err := method.LastResult(); err == nil {
		t.Fatal("stale result survived dimension failure")
	}
	if pricingMust(model.Sigma()) != before || pricingMust(model.A()) != .05 {
		t.Fatal("failed calibration changed model")
	}
}

func TestFireflyCalibrationBudgetAndCrossSession(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	model, helpers, criteria := globalCalibrationMarket(t, s)
	options := fireflyCalibrationOptions()
	options.MaxFev = 1
	method := pricingMust(s.NewFirefly(options))
	pricingOK(t, model.Calibrate(helpers, method, criteria, true))
	result := pricingMust(method.LastResult())
	if result.Success || result.Status != OptimizeMaxEvaluations || result.Nfev != 1 {
		t.Fatalf("exhaustion reported as convergence: %+v", result)
	}
	if pricingMust(model.EndCriteriaType()) != EndCriteriaUnknown {
		t.Fatal("evaluation exhaustion claimed a legacy convergence")
	}
	foreign := pricingMust(NewSession())
	foreignMethod := pricingMust(foreign.NewFirefly(options))
	if err := model.Calibrate(helpers, foreignMethod, criteria, true); !errors.Is(err, ErrSessionMismatch) {
		t.Fatalf("foreign method: %v", err)
	}
	pricingOK(t, foreign.Close())
	if _, err := foreignMethod.LastResult(); err == nil {
		t.Fatal("closed session result accepted")
	}
	var nilMethod *FireflyMethod
	if _, err := optimizationMethodObject(nilMethod); err == nil {
		t.Fatal("typed nil accepted")
	}
}

func TestFireflyCalibrationConstructorValidation(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	for _, options := range []Firefly{
		{}, {Bounds: [][2]float64{{0, math.Inf(1)}}},
		{Bounds: [][2]float64{{1, 0}}}, {Bounds: [][2]float64{{0, 1}}, MaxFev: 10_000_001},
		{Bounds: [][2]float64{{0, 1}}, InitialPopulation: [][]float64{{0}, {1}}},
	} {
		if method, err := s.NewFirefly(options); err == nil || method != nil {
			t.Fatalf("invalid constructor accepted: %+v", options)
		}
	}
}

func TestFireflyCalibrationNilAndClosedSession(t *testing.T) {
	var session *Session
	if method, err := session.NewFirefly(fireflyCalibrationOptions()); err == nil || method != nil {
		t.Fatal("nil session accepted")
	}
	var method *FireflyMethod
	if _, err := method.LastResult(); err == nil {
		t.Fatal("nil result accepted")
	}
	session = pricingMust(NewSession())
	pricingOK(t, session.Close())
	if method, err := session.NewFirefly(fireflyCalibrationOptions()); !errors.Is(err, ErrClosed) || method != nil {
		t.Fatalf("closed session constructor: %v", err)
	}
}
