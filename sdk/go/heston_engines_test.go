package itofin

import (
	"encoding/json"
	"math"
	"os"
	"testing"
)

func TestCosHestonQuantLibRetentionAndErrors(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	ref := pricingMust(NewDate(7, 2, 2017))
	settings := pricingMust(s.NewSettings())
	pricingOK(t, settings.SetEvaluationDate(ref))
	dc := pricingMust(s.Actual365Fixed())
	p := pricingMust(s.NewHestonProcess(HestonProcessConfig{RiskFreeRate: .15, DividendYield: .07, Spot: 100, V0: .1, Kappa: 4, Theta: .22, Sigma: 1.8, Rho: -.75, ReferenceDate: ref, DayCounter: dc}))
	m := pricingMust(s.NewHestonModel(p))
	for _, cfg := range []struct {
		l float64
		n uint
	}{{0, 200}, {math.NaN(), 200}, {16, 0}} {
		if _, err := s.NewCosHestonEngine(m, cfg.l, cfg.n); err == nil {
			t.Fatal("invalid COS config accepted")
		}
	}
	e := pricingMust(s.NewCosHestonEngine(m, 25, 600))
	var option *VanillaOption
	for _, tc := range []struct {
		kind             OptionType
		strike, expected float64
	}{{Call, 120, 9.364410588426075}, {Call, 250, .01036797658132471}, {Put, 80, 5.319092971836708}, {Put, 10, .01032681906278383}} {
		option = pricingMust(s.NewVanillaOption(tc.kind, tc.strike, pricingMust(NewDate(7, 2, 2018)), settings))
		pricingNear(t, pricingMust(option.PriceCosHeston(e)), tc.expected, 1e-10)
	}
	pricingOK(t, option.SetCosHestonEngine(e))
	pricingOK(t, p.Close())
	pricingOK(t, m.Close())
	pricingOK(t, e.Close())
	pricingNear(t, pricingMust(option.NPV()), .01032681906278383, 1e-10)
	if err := option.SetCosHestonEngine(e); err == nil {
		t.Fatal("released engine accepted")
	}
	if err := option.SetCosHestonEngine(nil); err == nil {
		t.Fatal("nil engine accepted")
	}
}

func TestExponentialFittingHestonQuantLibGrid(t *testing.T) {
	data, err := os.ReadFile("testdata/heston_exponential.json")
	pricingOK(t, err)
	var fixture struct {
		Moneyness []float64
		Days      []int
		Prices    []float64
	}
	pricingOK(t, json.Unmarshal(data, &fixture))
	s := pricingMust(NewSession())
	defer s.Close()
	ref := pricingMust(NewDate(13, 5, 2020))
	dc := pricingMust(s.Actual365Fixed())
	settings := pricingMust(s.NewSettings())
	pricingOK(t, settings.SetEvaluationDate(ref))
	p := pricingMust(s.NewHestonProcess(HestonProcessConfig{RiskFreeRate: .0507, DividendYield: .0469, Spot: 1, V0: .04, Kappa: 2.5, Theta: .06, Sigma: .75, Rho: -.6, ReferenceDate: ref, DayCounter: dc}))
	m := pricingMust(s.NewHestonModel(p))
	e := pricingMust(s.NewExponentialFittingHestonEngine(m, nil))
	var option *VanillaOption
	var expected float64
	for i, days := range fixture.Days {
		tau := float64(days) / 365
		discount := math.Exp(-.0507 * tau)
		forward := math.Exp((.0507 - .0469) * tau)
		for j, moneyness := range fixture.Moneyness {
			strike := math.Exp(-moneyness*math.Sqrt(.06*tau)) * forward
			for _, kind := range []OptionType{Call, Put} {
				intrinsic := (forward - strike) * discount
				if kind == Put {
					intrinsic = -intrinsic
				}
				expected = fixture.Prices[i*11+j] + math.Max(intrinsic, 0)
				option = pricingMust(s.NewVanillaOption(kind, strike, pricingMust(ref.AddDays(int64(days))), settings))
				pricingNear(t, pricingMust(option.PriceExponentialFittingHeston(e)), expected, 1e-8)
			}
		}
	}
	pricingOK(t, option.SetExponentialFittingHestonEngine(e))
	pricingOK(t, p.Close())
	pricingOK(t, m.Close())
	pricingOK(t, e.Close())
	pricingNear(t, pricingMust(option.NPV()), expected, 1e-8)
	if err := option.SetExponentialFittingHestonEngine(e); err == nil {
		t.Fatal("released engine accepted")
	}
}

func TestExponentialFittingHestonVariatesAndErrors(t *testing.T) {
	data, err := os.ReadFile("testdata/heston_control_variates.json")
	pricingOK(t, err)
	var fixture struct {
		Prices            []float64
		FixedScalingPrice float64 `json:"fixed_scaling_price"`
	}
	pricingOK(t, json.Unmarshal(data, &fixture))
	s := pricingMust(NewSession())
	defer s.Close()
	ref := pricingMust(NewDate(7, 2, 2017))
	dc := pricingMust(s.Actual365Fixed())
	settings := pricingMust(s.NewSettings())
	pricingOK(t, settings.SetEvaluationDate(ref))
	p := pricingMust(s.NewHestonProcess(HestonProcessConfig{RiskFreeRate: .15, DividendYield: .07, Spot: 100, V0: .1, Kappa: 4, Theta: .22, Sigma: 1.8, Rho: -.75, ReferenceDate: ref, DayCounter: dc}))
	m := pricingMust(s.NewHestonModel(p))
	option := pricingMust(s.NewVanillaOption(Call, 120, pricingMust(NewDate(7, 2, 2018)), settings))
	for i, cv := range []ExponentialFittingControlVariate{HestonOptimal, HestonAndersenPiterbarg, HestonAndersenPiterbargOptCV, HestonAsymptoticChF, HestonAngledContour, HestonAngledContourNoCV} {
		if int32(cv) != int32(i) {
			t.Fatal("control variate discriminant mismatch")
		}
		engine := pricingMust(s.NewExponentialFittingHestonEngine(m, &ExponentialFittingHestonConfig{ControlVariate: cv, Alpha: -.5}))
		pricingNear(t, pricingMust(option.PriceExponentialFittingHeston(engine)), fixture.Prices[i], 1e-10)
	}
	for _, cfg := range []ExponentialFittingHestonConfig{{ControlVariate: 99, Alpha: -.5}, {Scaling: pricingPtr(0.), Alpha: -.5}, {Scaling: pricingPtr(math.NaN()), Alpha: -.5}, {ControlVariate: HestonAsymptoticChF, Alpha: -.3}, {Alpha: math.NaN()}, {Alpha: math.Inf(1)}} {
		if _, err := s.NewExponentialFittingHestonEngine(m, &cfg); err == nil {
			t.Fatal("invalid exponential config accepted")
		}
	}
	e := pricingMust(s.NewExponentialFittingHestonEngine(m, &ExponentialFittingHestonConfig{Scaling: pricingPtr(1.), Alpha: -.5}))
	pricingNear(t, pricingMust(option.PriceExponentialFittingHeston(e)), fixture.FixedScalingPrice, 1e-10)
	other := pricingMust(NewSession())
	defer other.Close()
	if _, err := other.NewCosHestonEngine(m, 16, 200); err == nil {
		t.Fatal("foreign model accepted")
	}
	if _, err := s.NewExponentialFittingHestonEngine(nil, nil); err == nil {
		t.Fatal("nil model accepted")
	}
}

func TestAlternativeHestonCalibrationAndLiveModel(t *testing.T) {
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
	for _, useCos := range []bool{true, false} {
		sigma := .1
		proc := pricingMust(s.NewHestonProcess(HestonProcessConfig{RiskFreeRate: .04, DividendYield: .50, Spot: 1, V0: .01, Kappa: .2, Theta: .02, Sigma: sigma, Rho: -.75, ReferenceDate: ref, DayCounter: dc}))
		model := pricingMust(s.NewHestonModel(proc))
		option := pricingMust(s.NewVanillaOption(Call, .7, pricingMust(ref.AddDays(360)), settings))
		var before float64
		var cosEngine *CosHestonEngine
		var beforeC2 float64
		if useCos {
			cosEngine = pricingMust(s.NewCosHestonEngine(model, 25, 600))
			beforeC2 = pricingMust(cosEngine.C2(1))
			before = pricingMust(option.PriceCosHeston(cosEngine))
		} else {
			before = pricingMust(option.PriceExponentialFittingHeston(pricingMust(s.NewExponentialFittingHestonEngine(model, nil))))
		}
		if !pricingMust(option.IsCalculated()) {
			t.Fatal("precalibration cache invalid")
		}
		method := pricingMust(s.NewLevenbergMarquardt(nil))
		criteria := pricingMust(s.NewEndCriteria(EndCriteriaConfig{MaxIterations: 400, MaxStationaryStateIterations: pricingPtr(uint(40)), RootEpsilon: 1e-8, FunctionEpsilon: 1e-8, GradientNormEpsilon: pricingPtr(1e-8)}))
		calibrate := func(h []*HestonModelHelper) error {
			if useCos {
				return model.CalibrateCOS(h, method, criteria, 16, 200)
			}
			return model.CalibrateExponentialFitting(h, method, criteria, nil)
		}
		if err := calibrate(nil); err == nil {
			t.Fatal("empty helpers accepted")
		}
		pricingOK(t, calibrate(helpers))
		if useCos && pricingMust(cosEngine.C2(1)) == beforeC2 {
			t.Fatal("COS inspector ignored calibrated model")
		}
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

func TestCosHestonInspectorsQuantLibRetentionAndErrors(t *testing.T) {
	data, err := os.ReadFile("testdata/cos_heston_inspectors.json")
	pricingOK(t, err)
	var fixture struct {
		Cumulants      [][]float64
		Characteristic [][]float64
		Mu             [][]float64
	}
	pricingOK(t, json.Unmarshal(data, &fixture))
	s := pricingMust(NewSession())
	defer s.Close()
	ref := pricingMust(NewDate(7, 2, 2017))
	dc := pricingMust(s.Actual365Fixed())
	p := pricingMust(s.NewHestonProcess(HestonProcessConfig{RiskFreeRate: .15, DividendYield: .075, Spot: 100, V0: .1, Kappa: 4, Theta: .25, Sigma: .4, Rho: -.75, ReferenceDate: ref, DayCounter: dc}))
	m := pricingMust(s.NewHestonModel(p))
	e := pricingMust(s.NewCosHestonEngine(m, 16, 200))
	pricingOK(t, p.Close())
	pricingOK(t, m.Close())
	if len(fixture.Cumulants) != 13 || len(fixture.Characteristic) != 16 || len(fixture.Mu) != 4 {
		t.Fatal("incomplete COS inspector fixture")
	}
	inspectors := []func(float64) (float64, error){e.C1, e.C2, e.C3, e.C4}
	for _, row := range fixture.Cumulants {
		for i, fn := range inspectors {
			pricingNear(t, pricingMust(fn(row[0])), row[i+1], 1e-10)
		}
	}
	for _, row := range fixture.Characteristic {
		v := pricingMust(e.CHF(row[1], row[0]))
		pricingNear(t, real(v), row[2], 1e-12)
		pricingNear(t, imag(v), row[3], 1e-12)
	}
	for _, row := range fixture.Mu {
		pricingNear(t, pricingMust(e.MuT(row[0])), row[1], 1e-12)
	}
	for _, fn := range append(inspectors, e.MuT) {
		if pricingMust(fn(0)) != 0 {
			t.Fatal("nonzero initial cumulant/drift")
		}
		for _, t0 := range []float64{-1, math.NaN(), math.Inf(1)} {
			if _, err := fn(t0); err == nil {
				t.Fatal("invalid time accepted")
			}
		}
	}
	for _, pair := range [][2]float64{{math.NaN(), 1}, {math.Inf(1), 1}, {1, -1}, {1, math.Inf(1)}} {
		if _, err := e.CHF(pair[0], pair[1]); err == nil {
			t.Fatal("invalid characteristic argument accepted")
		}
	}
	if _, err := e.C4(1e6); err == nil {
		t.Fatal("nonfinite cumulant accepted")
	}
	pricingNear(t, pricingMust(e.C4(fixture.Cumulants[0][0])), fixture.Cumulants[0][4], 1e-10)
	if pricingMust(e.CHF(0, 1)) != 1 {
		t.Fatal("characteristic function not normalized")
	}
	pricingOK(t, e.Close())
	if _, err := e.C1(1); err == nil {
		t.Fatal("released inspector accepted")
	}
	var absent *CosHestonEngine
	if _, err := absent.C1(1); err == nil {
		t.Fatal("nil inspector accepted")
	}
}
