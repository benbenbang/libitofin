package itofin

import (
	"encoding/json"
	"math"
	"os"
	"os/exec"
	"testing"
)

type optimizationMethodsCoreResult struct {
	Params [5]float64      `json:"params"`
	End    EndCriteriaType `json:"end"`
}

func optimizationMethodsCoreOracle(t *testing.T) map[string]optimizationMethodsCoreResult {
	t.Helper()
	var data []byte
	var err error
	if path := os.Getenv("ITOFIN_OPTIMIZATION_METHODS_CORE_ORACLE_JSON"); path != "" {
		data, err = os.ReadFile(path)
	} else {
		cmd := exec.Command("cargo", "run", "--quiet", "--release", "-p", "libitofin-ffi", "--example", "optimization_methods_oracle")
		cmd.Dir = "../.."
		data, err = cmd.Output()
	}
	if err != nil {
		t.Fatalf("load Rust optimization-method oracle: %v", err)
	}
	var results map[string]optimizationMethodsCoreResult
	if err := json.Unmarshal(data, &results); err != nil {
		t.Fatalf("decode Rust optimization-method oracle: %v", err)
	}
	if len(results) != 3 {
		t.Fatalf("Rust optimization-method oracle contains %d methods, want 3", len(results))
	}
	return results
}

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
	core := optimizationMethodsCoreOracle(t)
	for _, variant := range []struct {
		name string
		new  func(*Session) (OptimizationMethod, error)
		end  EndCriteriaType
	}{
		{"simplex", func(s *Session) (OptimizationMethod, error) { return s.NewSimplex(0.1) }, EndCriteriaStationaryPoint},
		{"conjugate_gradient", func(s *Session) (OptimizationMethod, error) { return s.NewConjugateGradient() }, EndCriteriaStationaryFunctionValue},
		{"steepest_descent", func(s *Session) (OptimizationMethod, error) { return s.NewSteepestDescent() }, EndCriteriaMaxIterations},
	} {
		t.Run(variant.name, func(t *testing.T) {
			want, ok := core[variant.name]
			if !ok {
				t.Fatalf("Rust optimization-method oracle missing %s", variant.name)
			}
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
				if math.IsInf(value, 0) || math.IsNaN(value) {
					t.Fatalf("parameter %d is not finite: %.17g", i, value)
				}
				if math.Abs(value-want.Params[i]) > 1e-12 {
					t.Fatalf("parameter %d: Go %.17g, Rust core %.17g", i, value, want.Params[i])
				}
			}
			if result[3] >= 0.3 {
				t.Fatalf("sigma did not fall from initial 0.3: %.17g", result[3])
			}
			if kind := pricingMust(model.EndCriteriaType()); kind != variant.end || kind != want.End {
				t.Fatalf("end criterion: Go %v, Rust core %v, regression %v", kind, want.End, variant.end)
			}
		})
	}
}
