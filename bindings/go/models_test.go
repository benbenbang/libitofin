package itofin

import (
	"math"
	"testing"
)

func TestHestonCalibratesFlatSurface(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	ref := pricingMust(NewDate(15, 1, 2026))
	dc := pricingMust(s.Actual360())
	cal := pricingMust(s.NullCalendar())
	settings := pricingMust(s.NewSettings())
	pricingOK(t, settings.SetEvaluationDate(ref))
	taus := []float64{.086111111111111, .163888888888889, .25, .502777777777778, .758333333333333, 1.013888888888889, 2.027777777777778}
	maturities := []Period{{1, Months}, {2, Months}, {3, Months}, {6, Months}, {9, Months}, {1, Years}, {2, Years}}
	var helpers []*HestonModelHelper
	for i, maturity := range maturities {
		tau := taus[i]
		for _, moneyness := range []float64{-1, 0, 1} {
			strike := math.Exp((.04-.50)*tau) * math.Exp(-moneyness*.1*math.Sqrt(tau))
			helpers = append(helpers, pricingMust(s.NewHestonModelHelper(HestonHelperConfig{Maturity: maturity, Calendar: cal, Spot: 1, Strike: strike, Volatility: .1, RiskFreeRate: .04, DividendYield: .50, ErrorType: RelativePriceError, ReferenceDate: ref, DayCounter: dc, Settings: settings})))
		}
	}
	for _, sigma := range []float64{.1, .3, .5} {
		proc := pricingMust(s.NewHestonProcess(HestonProcessConfig{RiskFreeRate: .04, DividendYield: .50, Spot: 1, V0: .01, Kappa: .2, Theta: .02, Sigma: sigma, Rho: -.75, ReferenceDate: ref, DayCounter: dc}))
		model := pricingMust(s.NewHestonModel(proc))
		option := pricingMust(s.NewVanillaOption(Call, .7, pricingMust(ref.AddDays(360)), settings))
		before := pricingMust(option.PriceHeston(model, 96))
		if !pricingMust(option.IsCalculated()) {
			t.Fatal("precalibration cache invalid")
		}
		method := pricingMust(s.NewLevenbergMarquardt(nil))
		criteria := pricingMust(s.NewEndCriteria(EndCriteriaConfig{MaxIterations: 400, MaxStationaryStateIterations: pricingPtr(uint(40)), RootEpsilon: 1e-8, FunctionEpsilon: 1e-8, GradientNormEpsilon: pricingPtr(1e-8)}))
		if err := model.Calibrate(nil, method, criteria, 96); err == nil {
			t.Fatal("empty helpers accepted")
		}
		pricingOK(t, model.Calibrate(helpers, method, criteria, 96))
		if pricingMust(option.IsCalculated()) {
			t.Fatal("calibration failed to invalidate live option")
		}
		if after := pricingMust(option.NPV()); after == before {
			t.Fatal("calibration did not change price")
		}
		if got := pricingMust(model.Sigma()); got >= 3e-3 {
			t.Fatalf("sigma %g", got)
		}
		pricingNear(t, pricingMust(model.Kappa())*(pricingMust(model.Theta())-.01), 0, 3e-3)
		pricingNear(t, pricingMust(model.V0()), .01, 3e-3)
		for _, h := range helpers {
			if got := pricingMust(h.CalibrationError()); got >= 1e-2 {
				t.Fatalf("helper error %g", got)
			}
		}
	}
}

// QuantLib testCachedHullWhite: the par-coupon and fixed-reversion arms.
func TestHullWhiteCalibrationCachedOracles(t *testing.T) {
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
		helpers = append(helpers, pricingMust(s.NewSwaptionHelper(SwaptionHelperConfig{Maturity: Period{int32(i + 1), Years}, Length: Period{int32(5 - i), Years}, FixedLegTenor: Period{1, Years}, Volatility: vol, Nominal: 1, Index: index, FixedLegDayCounter: fixedDC, FloatingLegDayCounter: floatDC, Curve: curve, ErrorType: RelativePriceError})))
	}
	for _, fixed := range []bool{false, true} {
		a, max, stationary, root := .1, uint(10000), uint(100), 1e-6
		if fixed {
			a, max, stationary, root = .05, 1000, 500, 1e-8
		}
		model := pricingMust(s.NewHullWhite(curve, a, .01))
		method := pricingMust(s.NewLevenbergMarquardt(nil))
		criteria := pricingMust(s.NewEndCriteria(EndCriteriaConfig{MaxIterations: max, MaxStationaryStateIterations: &stationary, RootEpsilon: root, FunctionEpsilon: 1e-8, GradientNormEpsilon: pricingPtr(1e-8)}))
		if err := model.Calibrate(nil, method, criteria, fixed); err == nil {
			t.Fatal("empty helpers accepted")
		}
		pricingOK(t, model.Calibrate(helpers, method, criteria, fixed))
		if fixed {
			pricingNear(t, pricingMust(model.A()), .05, 1e-15)
			pricingNear(t, pricingMust(model.Sigma()), .00585858, 1e-5)
		} else {
			pricingNear(t, pricingMust(model.A()), .0464041, 1.3e-5)
			pricingNear(t, pricingMust(model.Sigma()), .00579912, 1.3e-5)
		}
		for _, h := range helpers {
			if v := pricingMust(h.CalibrationError()); math.IsNaN(v) || math.IsInf(v, 0) {
				t.Fatalf("helper error %g", v)
			}
		}
	}
}

func TestModelConstraintsParametersAndBondOption(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	ref := pricingMust(NewDate(15, 1, 2026))
	dc := pricingMust(s.Actual365Fixed())
	cfg := HestonProcessConfig{RiskFreeRate: .03, DividendYield: .01, Spot: 100, V0: .04, Kappa: 1, Theta: .04, Sigma: .2, Rho: -.5, ReferenceDate: ref, DayCounter: dc}
	process := pricingMust(s.NewHestonProcess(cfg))
	model := pricingMust(s.NewHestonModel(process))
	for i, fn := range []func() (float64, error){process.V0, process.Kappa, process.Theta, process.Sigma, process.Rho, model.V0, model.Kappa, model.Theta, model.Sigma, model.Rho} {
		pricingNear(t, pricingMust(fn()), []float64{.04, 1, .04, .2, -.5}[i%5], 1e-15)
	}
	cfg.V0 = -.1
	bad := pricingMust(s.NewHestonProcess(cfg))
	if _, err := s.NewHestonModel(bad); err == nil {
		t.Fatal("negative variance accepted")
	}
	curve := pricingMust(s.NewFlatForward(ref, .03, dc))
	hw := pricingMust(s.NewHullWhite(curve, .05, .01))
	pricingNear(t, pricingMust(hw.A()), .05, 1e-12)
	pricingNear(t, pricingMust(hw.Sigma()), .01, 1e-12)
	pricingNear(t, pricingMust(hw.R0()), .03, 1e-12)
	itm := pricingMust(hw.DiscountBondOption(Call, .8, 1, 3))
	otm := pricingMust(hw.DiscountBondOption(Call, 1.1, 1, 3))
	if !(itm > otm && otm >= 0) {
		t.Fatalf("bond options %g %g", itm, otm)
	}
	if _, err := s.NewHullWhite(curve, .05, -.01); err == nil {
		t.Fatal("negative sigma accepted")
	}
	if _, err := s.NewEndCriteria(EndCriteriaConfig{MaxIterations: 10, MaxStationaryStateIterations: pricingPtr(uint(10))}); err == nil {
		t.Fatal("invalid stationary iterations accepted")
	}
	if _, err := s.NewEndCriteria(EndCriteriaConfig{MaxIterations: 100, RootEpsilon: math.NaN()}); err == nil {
		t.Fatal("NaN epsilon accepted")
	}
}
