package itofin

import "testing"

// TestHullWhiteCachedNoStartDelayAfterInputsClosed ports QuantLib's
// testCachedHullWhite2 (shortratemodels.cpp:229-306), using its PAR cached values.
func TestHullWhiteCachedNoStartDelayAfterInputsClosed(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	settings := pricingMust(s.NewSettings())
	pricingOK(t, settings.SetEvaluationDate(pricingMust(NewDate(15, 2, 2002))))
	dc := pricingMust(s.Actual365Fixed())
	fixedDC := pricingMust(s.Thirty360BondBasis())
	floatDC := pricingMust(s.Actual360())
	curve := pricingMust(s.NewFlatForward(pricingMust(NewDate(19, 2, 2002)), .04875825, dc))
	model := pricingMust(s.NewHullWhite(curve, .1, .01))
	currency := pricingMust(s.EUR())
	calendar := pricingMust(s.Target())
	index := pricingMust(s.NewIborIndex(IborIndexConfig{
		FamilyName: "Euribor", Tenor: Period{6, Months}, SettlementDays: 0,
		Currency: currency, FixingCalendar: calendar, Convention: ModifiedFollowing,
		EndOfMonth: true, DayCounter: floatDC, Forwarding: curve, Settings: settings,
	}))
	var helpers []*SwaptionHelper
	for i, vol := range []float64{.1148, .1108, .1070, .1021, .1000} {
		helpers = append(helpers, pricingMust(s.NewSwaptionHelper(SwaptionHelperConfig{
			Maturity: Period{int32(i + 1), Years}, Length: Period{int32(5 - i), Years},
			FixedLegTenor: Period{1, Years}, Volatility: vol, Nominal: 1, Index: index,
			FixedLegDayCounter: fixedDC, FloatingLegDayCounter: floatDC,
			Curve: curve, ErrorType: RelativePriceError,
		})))
	}
	for _, closeInput := range []func() error{
		index.Close, curve.Close, settings.Close, currency.Close, calendar.Close,
		dc.Close, fixedDC.Close, floatDC.Close,
	} {
		pricingOK(t, closeInput())
	}
	method := pricingMust(s.NewLevenbergMarquardt(&LevenbergMarquardtConfig{
		EpsFcn: 1e-8, XTol: 1e-8, GTol: 1e-8,
	}))
	criteria := pricingMust(s.NewEndCriteria(EndCriteriaConfig{
		MaxIterations: 10000, MaxStationaryStateIterations: pricingPtr(uint(100)),
		RootEpsilon: 1e-6, FunctionEpsilon: 1e-8, GradientNormEpsilon: pricingPtr(1e-8),
	}))
	pricingOK(t, model.Calibrate(helpers, method, criteria, false))
	pricingNear(t, pricingMust(model.A()), .0482063, 1e-5)
	pricingNear(t, pricingMust(model.Sigma()), .00582687, 1e-5)
}
