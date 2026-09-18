package itofin

import (
	"encoding/json"
	"fmt"
	"os"
	"testing"
)

type calibrationCompletionCase struct {
	Parameters []float64 `json:"parameters"`
	Errors     []float64 `json:"errors"`
}

type calibrationCompletionOracle struct {
	Version   string                               `json:"quantlib_version"`
	Quotes    [][3]float64                         `json:"quotes"`
	Heston    map[string]calibrationCompletionCase `json:"heston"`
	HullWhite map[string]calibrationCompletionCase `json:"hull_white"`
}

func loadCalibrationCompletionOracle(t *testing.T) calibrationCompletionOracle {
	t.Helper()
	data, err := os.ReadFile("testdata/calibration_completion_oracle.json")
	pricingOK(t, err)
	var oracle calibrationCompletionOracle
	pricingOK(t, json.Unmarshal(data, &oracle))
	if oracle.Version != "1.43" || len(oracle.Quotes) != 9 || len(oracle.Heston) != 2 || len(oracle.HullWhite) != 4 {
		t.Fatalf("incomplete calibration oracle: %+v", oracle)
	}
	return oracle
}

func TestHestonCalibrationPriceAndImpliedVolOracles(t *testing.T) {
	oracle := loadCalibrationCompletionOracle(t)
	for _, variant := range []struct {
		name string
		kind CalibrationErrorType
	}{{"price", PriceError}, {"implied_vol", ImpliedVolError}} {
		t.Run(variant.name, func(t *testing.T) {
			s := pricingMust(NewSession())
			defer s.Close()
			ref := pricingMust(NewDate(15, 1, 2026))
			dc := pricingMust(s.Actual365Fixed())
			cal := pricingMust(s.NullCalendar())
			settings := pricingMust(s.NewSettings())
			pricingOK(t, settings.SetEvaluationDate(ref))
			var helpers []*HestonModelHelper
			for _, quote := range oracle.Quotes {
				helpers = append(helpers, pricingMust(s.NewHestonModelHelper(HestonHelperConfig{Maturity: Period{int32(quote[0]), Months}, Calendar: cal, Spot: 100, Strike: quote[1], Volatility: quote[2], RiskFreeRate: .03, DividendYield: .01, ErrorType: variant.kind, ReferenceDate: ref, DayCounter: dc, Settings: settings})))
			}
			process := pricingMust(s.NewHestonProcess(HestonProcessConfig{RiskFreeRate: .03, DividendYield: .01, Spot: 100, V0: .035, Kappa: 1, Theta: .045, Sigma: .25, Rho: -.4, ReferenceDate: ref, DayCounter: dc}))
			model := pricingMust(s.NewHestonModel(process))
			method := pricingMust(s.NewLevenbergMarquardt(nil))
			criteria := pricingMust(s.NewEndCriteria(EndCriteriaConfig{MaxIterations: 1000, MaxStationaryStateIterations: pricingPtr(uint(100)), RootEpsilon: 1e-8, FunctionEpsilon: 1e-8, GradientNormEpsilon: pricingPtr(1e-8)}))
			pricingOK(t, model.Calibrate(helpers, method, criteria, 96))
			want := oracle.Heston[variant.name]
			if len(want.Parameters) != 5 || len(want.Errors) != len(helpers) {
				t.Fatal("incomplete Heston case")
			}
			for i, get := range []func() (float64, error){model.V0, model.Kappa, model.Theta, model.Sigma, model.Rho} {
				pricingNear(t, pricingMust(get()), want.Parameters[i], 3e-3)
			}
			for i, helper := range helpers {
				pricingNear(t, pricingMust(helper.CalibrationError()), want.Errors[i], 1e-8)
			}
		})
	}
}

func TestHullWhiteCalibrationFixedReversionAndOptionalCombinations(t *testing.T) {
	oracle := loadCalibrationCompletionOracle(t)
	for _, variant := range []struct {
		name string
		kind CalibrationErrorType
	}{{"price", PriceError}, {"implied_vol", ImpliedVolError}} {
		for _, fixed := range []bool{false, true} {
			for options := 0; options < 8; options++ {
				if !fixed && (options == 0 || options == 7) {
					continue
				}
				t.Run(fmt.Sprintf("%s/fixed_%t/options_%d", variant.name, fixed, options), func(t *testing.T) {
					s := pricingMust(NewSession())
					defer s.Close()
					today := pricingMust(NewDate(15, 2, 2002))
					settlement := pricingMust(NewDate(19, 2, 2002))
					settings := pricingMust(s.NewSettings())
					pricingOK(t, settings.SetEvaluationDate(today))
					dc := pricingMust(s.Actual365Fixed())
					fixedDC := pricingMust(s.Thirty360BondBasis())
					floatDC := pricingMust(s.Actual360())
					curve := pricingMust(s.NewFlatForward(settlement, .04875825, dc))
					index := pricingMust(s.NewEuriborSixMonths(curve, settings))
					var helpers []*SwaptionHelper
					for i, vol := range []float64{.1148, .1108, .1070, .1021, .1000} {
						helpers = append(helpers, pricingMust(s.NewSwaptionHelper(SwaptionHelperConfig{Maturity: Period{int32(i + 1), Years}, Length: Period{int32(5 - i), Years}, FixedLegTenor: Period{1, Years}, Volatility: vol, Nominal: 1, Index: index, FixedLegDayCounter: fixedDC, FloatingLegDayCounter: floatDC, Curve: curve, ErrorType: variant.kind})))
					}
					var methodConfig *LevenbergMarquardtConfig
					criteriaConfig := EndCriteriaConfig{MaxIterations: 10000, RootEpsilon: 1e-6, FunctionEpsilon: 1e-8}
					if options&1 != 0 {
						methodConfig = &LevenbergMarquardtConfig{EpsFcn: 1e-8, XTol: 1e-8, GTol: 1e-8}
					}
					if options&2 != 0 {
						criteriaConfig.MaxStationaryStateIterations = pricingPtr(uint(100))
					}
					if options&4 != 0 {
						criteriaConfig.GradientNormEpsilon = pricingPtr(1e-8)
					}
					method := pricingMust(s.NewLevenbergMarquardt(methodConfig))
					criteria := pricingMust(s.NewEndCriteria(criteriaConfig))
					a, suffix := .1, "_free"
					if fixed {
						a, suffix = .05, "_fixed"
					}
					model := pricingMust(s.NewHullWhite(curve, a, .01))
					pricingOK(t, model.Calibrate(helpers, method, criteria, fixed))
					want := oracle.HullWhite[variant.name+suffix]
					if len(want.Parameters) != 2 || len(want.Errors) != len(helpers) {
						t.Fatal("incomplete Hull-White case")
					}
					pricingNear(t, pricingMust(model.A()), want.Parameters[0], 1.3e-5)
					pricingNear(t, pricingMust(model.Sigma()), want.Parameters[1], 1.3e-5)
					if fixed {
						pricingNear(t, pricingMust(model.A()), .05, 1e-15)
					}
					for i, helper := range helpers {
						pricingNear(t, pricingMust(helper.CalibrationError()), want.Errors[i], 1e-8)
					}
				})
			}
		}
	}
}
