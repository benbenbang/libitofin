package itofin

import (
	"math"
	"testing"
)

func TestOptimizationMethodConstructorsAndOwnership(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	for _, lambda := range []float64{0, -1, math.NaN(), math.Inf(1)} {
		if method, err := s.NewSimplex(lambda); err == nil || method != nil {
			t.Fatalf("invalid simplex lambda %v accepted: %v", lambda, method)
		}
	}
	for _, create := range []func() (OptimizationMethod, error){
		func() (OptimizationMethod, error) { return s.NewLevenbergMarquardt(nil) },
		func() (OptimizationMethod, error) { return s.NewSimplex(.1) },
		func() (OptimizationMethod, error) { return s.NewConjugateGradient() },
		func() (OptimizationMethod, error) { return s.NewSteepestDescent() },
	} {
		method, err := create()
		pricingOK(t, err)
		if _, err := optimizationMethodObject(method); err != nil {
			t.Fatal(err)
		}
	}
	var nilSimplex *Simplex
	if _, err := optimizationMethodObject(nilSimplex); err == nil {
		t.Fatal("typed nil method accepted")
	}
	ref := pricingMust(NewDate(15, 1, 2026))
	dc := pricingMust(s.Actual365Fixed())
	curve := pricingMust(s.NewFlatForward(ref, .03, dc))
	model := pricingMust(s.NewHullWhite(curve, .05, .01))
	if kind := pricingMust(model.EndCriteriaType()); kind != EndCriteriaNone {
		t.Fatalf("uncalibrated Hull-White reason: %v", kind)
	}
}

func TestCalibrationOptimizationMethods(t *testing.T) {
	for _, variant := range []struct {
		name   string
		new    func(*Session) (OptimizationMethod, error)
		params [5]float64
		end    EndCriteriaType
	}{
		{"simplex", func(s *Session) (OptimizationMethod, error) { return s.NewSimplex(0.1) }, [5]float64{0.01000000000107512, 0.1483535700303218, 0.01000000000679777, 5.030267915404000e-11, -0.4546235848312395}, EndCriteriaStationaryPoint},
		{"conjugate_gradient", func(s *Session) (OptimizationMethod, error) { return s.NewConjugateGradient() }, [5]float64{0.008655748461209384, 0.20302231484372477, 0.03241055121479137, 0.05583023664447662, -0.36675046501506725}, EndCriteriaStationaryFunctionValue},
		{"steepest_descent", func(s *Session) (OptimizationMethod, error) { return s.NewSteepestDescent() }, [5]float64{0.009279646471165427, 0.2003282187710104, 0.01137238386842711, 0.27757131213875286, -0.708882936730623}, EndCriteriaMaxIterations},
	} {
		t.Run(variant.name, func(t *testing.T) {
			s := pricingMust(NewSession())
			defer s.Close()
			ref := pricingMust(NewDate(15, 1, 2026))
			dc := pricingMust(s.Actual360())
			cal := pricingMust(s.NullCalendar())
			settings := pricingMust(s.NewSettings())
			pricingOK(t, settings.SetEvaluationDate(ref))
			maturities := []Period{{1, Months}, {2, Months}, {3, Months}, {6, Months}, {9, Months}, {1, Years}, {2, Years}}
			strikes := [7][3]uint64{
				{0x3fefac53e80821cf, 0x3feec1d93a138c3f, 0x3fedde266b06edcb},
				{0x3feee6fc4e5c066b, 0x3fedad1ea01315b6, 0x3fec7fb4cb280859},
				{0x3fedfc74e7ab4083, 0x3fec86124a9aed52, 0x3feb21f1fa31573d},
				{0x3feb422c40cceb6b, 0x3fe96481fc737590, 0x3fe7a78a19df52fa},
				{0x3fe8a167781bf00b, 0x3fe6938b2fef7da4, 0x3fe4b18a04b47ab2},
				{0x3fe632e5c3c88016, 0x3fe4128a7c687e73, 0x3fe22653d92d71bf},
				{0x3fdd08fc2bc78913, 0x3fd92e6fb34bcd0d, 0x3fd5d6d3fbc52310},
			}
			var helpers []*HestonModelHelper
			for i, maturity := range maturities {
				for j := range strikes[i] {
					strike := math.Float64frombits(strikes[i][j])
					helpers = append(helpers, pricingMust(s.NewHestonModelHelper(HestonHelperConfig{Maturity: maturity, Calendar: cal, Spot: 1, Strike: strike, Volatility: .1, RiskFreeRate: .04, DividendYield: .50, ErrorType: RelativePriceError, ReferenceDate: ref, DayCounter: dc, Settings: settings})))
				}
			}
			process := pricingMust(s.NewHestonProcess(HestonProcessConfig{RiskFreeRate: .04, DividendYield: .50, Spot: 1, V0: .01, Kappa: .2, Theta: .02, Sigma: .3, Rho: -.75, ReferenceDate: ref, DayCounter: dc}))
			model := pricingMust(s.NewHestonModel(process))
			if kind := pricingMust(model.EndCriteriaType()); kind != EndCriteriaNone {
				t.Fatalf("uncalibrated Heston reason: %v", kind)
			}
			method, err := variant.new(s)
			pricingOK(t, err)
			criteria := pricingMust(s.NewEndCriteria(EndCriteriaConfig{MaxIterations: 400, MaxStationaryStateIterations: pricingPtr(uint(40)), RootEpsilon: 1e-8, FunctionEpsilon: 1e-8, GradientNormEpsilon: pricingPtr(1e-8)}))
			pricingOK(t, model.Calibrate(helpers, method, criteria, 96))
			result := [5]float64{pricingMust(model.V0()), pricingMust(model.Kappa()), pricingMust(model.Theta()), pricingMust(model.Sigma()), pricingMust(model.Rho())}
			for i, value := range result {
				if math.Abs(value-variant.params[i]) > 1e-12 {
					t.Fatalf("parameter %d: got %.17g, Rust core %.17g", i, value, variant.params[i])
				}
			}
			if kind := pricingMust(model.EndCriteriaType()); kind != variant.end {
				t.Fatalf("end criterion: got %v, Rust core %v", kind, variant.end)
			}
		})
	}
}
