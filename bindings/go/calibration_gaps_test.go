package itofin

import "testing"

func TestHullWhiteCalibrationErrorVariantsAndDefaults(t *testing.T) {
	for _, variant := range []struct {
		name     string
		kind     CalibrationErrorType
		a, sigma float64
		errors   [5]float64
	}{
		{"price", PriceError, .0828005393776294, .006597107155359615, [5]float64{.00045815819750159685, .00015315001161751197, -.00017254604939047347, -.00044079247082854083, -.00032439541496136247}},
		{"implied_vol", ImpliedVolError, .04935928714375431, .005878002798658328, [5]float64{-.008043442517224747, -.003963550573927166, -.00012281601897717875, .004884685434162622, .007171699362487946}},
	} {
		t.Run(variant.name, func(t *testing.T) {
			for _, explicit := range []bool{false, true} {
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
				var config *LevenbergMarquardtConfig
				criteriaConfig := EndCriteriaConfig{MaxIterations: 10000, RootEpsilon: 1e-6, FunctionEpsilon: 1e-8}
				if explicit {
					config = &LevenbergMarquardtConfig{EpsFcn: 1e-8, XTol: 1e-8, GTol: 1e-8}
					criteriaConfig.MaxStationaryStateIterations = pricingPtr(uint(100))
					criteriaConfig.GradientNormEpsilon = pricingPtr(1e-8)
				}
				method := pricingMust(s.NewLevenbergMarquardt(config))
				criteria := pricingMust(s.NewEndCriteria(criteriaConfig))
				model := pricingMust(s.NewHullWhite(curve, .1, .01))
				pricingOK(t, model.Calibrate(helpers, method, criteria, false))
				pricingNear(t, pricingMust(model.A()), variant.a, 1.3e-5)
				pricingNear(t, pricingMust(model.Sigma()), variant.sigma, 1.3e-5)
				for i, helper := range helpers {
					pricingNear(t, pricingMust(helper.CalibrationError()), variant.errors[i], 1e-8)
				}
			}
		})
	}
}
