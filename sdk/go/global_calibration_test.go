package itofin

import (
	"errors"
	"math"
	"testing"
)

func globalCalibrationMarket(t *testing.T, s *Session) (*HullWhite, []*SwaptionHelper, *EndCriteria) {
	t.Helper()
	settings := pricingMust(s.NewSettings())
	pricingOK(t, settings.SetEvaluationDate(pricingMust(NewDate(15, 2, 2002))))
	dc := pricingMust(s.Actual365Fixed())
	curve := pricingMust(s.NewFlatForward(pricingMust(NewDate(19, 2, 2002)), .04875825, dc))
	index := pricingMust(s.NewEuriborSixMonths(curve, settings))
	fixedDC, floatDC := pricingMust(s.Thirty360BondBasis()), pricingMust(s.Actual360())
	var helpers []*SwaptionHelper
	for i, vol := range []float64{.1148, .1108, .1070, .1021, .1000} {
		helpers = append(helpers, pricingMust(s.NewSwaptionHelper(SwaptionHelperConfig{
			Maturity: Period{int32(i + 1), Years}, Length: Period{int32(5 - i), Years},
			FixedLegTenor: Period{1, Years}, Volatility: vol, Nominal: 1, Index: index,
			FixedLegDayCounter: fixedDC, FloatingLegDayCounter: floatDC, Curve: curve, ErrorType: RelativePriceError,
		})))
	}
	model := pricingMust(s.NewHullWhite(curve, .05, .01))
	criteria := pricingMust(s.NewEndCriteria(EndCriteriaConfig{MaxIterations: 500,
		MaxStationaryStateIterations: pricingPtr(uint(50)), RootEpsilon: 1e-8, FunctionEpsilon: 1e-8}))
	return model, helpers, criteria
}

func globalCalibrationOptions() DifferentialEvolution {
	return DifferentialEvolution{Bounds: [][2]float64{{.001, .03}}, Seed: 42, PopulationSize: 8,
		XAtol: pricingPtr(1e-7), FAtol: pricingPtr(1e-12)}
}

func TestGlobalCalibrationFixedParametersAndResultOwnership(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	method := pricingMust(s.NewDifferentialEvolution(globalCalibrationOptions()))
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

func TestGlobalCalibrationBudgetAndCrossSession(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	model, helpers, criteria := globalCalibrationMarket(t, s)
	options := globalCalibrationOptions()
	options.MaxFev = 1
	method := pricingMust(s.NewDifferentialEvolution(options))
	pricingOK(t, model.Calibrate(helpers, method, criteria, true))
	result := pricingMust(method.LastResult())
	if result.Success || result.Status != OptimizeMaxEvaluations || result.Nfev != 1 {
		t.Fatalf("exhaustion reported as convergence: %+v", result)
	}
	if pricingMust(model.EndCriteriaType()) != EndCriteriaUnknown {
		t.Fatal("evaluation exhaustion claimed a legacy convergence")
	}
	foreign := pricingMust(NewSession())
	foreignMethod := pricingMust(foreign.NewDifferentialEvolution(options))
	if err := model.Calibrate(helpers, foreignMethod, criteria, true); !errors.Is(err, ErrSessionMismatch) {
		t.Fatalf("foreign method: %v", err)
	}
	pricingOK(t, foreign.Close())
	if _, err := foreignMethod.LastResult(); err == nil {
		t.Fatal("closed session result accepted")
	}
	var nilMethod *DifferentialEvolutionMethod
	if _, err := optimizationMethodObject(nilMethod); err == nil {
		t.Fatal("typed nil accepted")
	}
}

func TestGlobalCalibrationConstructorValidation(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	for _, options := range []DifferentialEvolution{
		{}, {Bounds: [][2]float64{{0, math.Inf(1)}}},
		{Bounds: [][2]float64{{1, 0}}}, {Bounds: [][2]float64{{0, 1}}, MaxFev: 10_000_001},
		{Bounds: [][2]float64{{0, 1}}, InitialPopulation: [][]float64{{0}, {1}}},
	} {
		if method, err := s.NewDifferentialEvolution(options); err == nil || method != nil {
			t.Fatalf("invalid constructor accepted: %+v", options)
		}
	}
}
