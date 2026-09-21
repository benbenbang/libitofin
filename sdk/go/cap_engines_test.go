package itofin

import (
	"math"
	"testing"
)

func capNear(t *testing.T, got, want, tol float64) {
	t.Helper()
	if math.IsNaN(got) || math.IsInf(got, 0) || math.Abs(got-want) > tol {
		t.Fatalf("got %.17g want %.17g tolerance %g", got, want, tol)
	}
}
func TestCapNormalAndTreeOracles(t *testing.T) {
	for _, tc := range []struct {
		rate, strike, vol float64
		prices, vegas     [3]float64
	}{
		{.03, .04, .01, [3]float64{.6034628980651433, 1.053253364838137, -.4497904667729936}, [3]float64{127.78090454050886, 154.68272470527324, -26.901820164764377}},
		{-.01, -.005, .007, [3]float64{.6750024800533693, .29261507158291433, .38238740847045494}, [3]float64{164.10370412870708, 113.36514792045139, 50.73855620825567}},
		{.03, .04, 0, [3]float64{}, [3]float64{}},
	} {
		s := sessionMust(NewSession())
		defer s.Close()
		today := sessionMust(NewDate(15, 1, 2026))
		settings := sessionMust(s.NewSettings())
		ratesOK(t, settings.SetEvaluationDate(today))
		dc := sessionMust(s.Actual365Fixed())
		couponDC := sessionMust(s.Actual360())
		cal := sessionMust(s.Target())
		curve := sessionMust(s.NewFlatForward(today, tc.rate, dc))
		index := sessionMust(s.NewEuriborSixMonths(curve, settings))
		term := Unadjusted
		schedule := sessionMust(s.NewSchedule(ScheduleConfig{Start: sessionMust(NewDate(15, 1, 2027)), End: sessionMust(NewDate(15, 1, 2030)), Frequency: Semiannual, Calendar: cal, Convention: Unadjusted, TerminationConvention: &term}))
		leg := sessionMust(s.NewIborLeg(schedule, index))
		leg = sessionMust(leg.WithNotional(100))
		leg = sessionMust(leg.WithPaymentDayCounter(couponDC))
		leg = sessionMust(leg.WithPaymentAdjustment(Unadjusted))
		leg = sessionMust(leg.WithFixingDays(0))
		options := []*CapFloor{sessionMust(s.NewCap(leg, []float64{tc.strike}, settings)), sessionMust(s.NewFloor(leg, []float64{tc.strike - .015}, settings)), sessionMust(s.NewCollar(leg, []float64{tc.strike}, []float64{tc.strike - .015}, settings))}
		quote := sessionMust(s.NewSimpleQuote(tc.vol))
		engine := sessionMust(s.NewBachelierCapFloorEngineFlat(RateEngineFlatVolConfig{Discount: curve, Volatility: quote, DayCounter: dc, Settings: settings}))
		surface := sessionMust(s.ConstantOptionletVolatility(ConstantRateVolConfig{ReferenceDate: today, Calendar: cal, Convention: Following, Volatility: tc.vol, DayCounter: dc, VolatilityType: Normal}))
		surfaceEngine := sessionMust(s.NewBachelierCapFloorEngine(BachelierCapFloorEngineConfig{Volatility: surface, Discount: curve}))
		capNear(t, sessionMust(options[0].PriceBachelier(surfaceEngine)), tc.prices[0], 1e-11)
		for i, o := range options {
			ratesOK(t, o.SetBachelierEngine(engine))
			capNear(t, sessionMust(o.PriceBachelier(engine)), tc.prices[i], 1e-11)
			r := sessionMust(o.Results())
			capNear(t, r.AdditionalResults["vega"], tc.vegas[i], 1e-10)
		}
		if tc.rate == .03 && tc.vol == .01 {
			model := sessionMust(s.NewHullWhite(curve, .05, .01))
			tree := sessionMust(s.NewTreeCapFloorEngine(model, 100))
			for i, want := range []float64{.5187376052783954, .9526665495458903, -.4339289442674949} {
				ratesOK(t, options[i].SetTreeEngine(tree))
				capNear(t, sessionMust(options[i].PriceTree(tree)), want, 1e-11)
			}
			ratesOK(t, tree.Close())
			ratesOK(t, model.Close())
			ratesOK(t, index.Close())
			ratesOK(t, leg.Close())
			ratesOK(t, curve.Close())
			ratesOK(t, settings.SetEvaluationDate(sessionMust(NewDate(16, 1, 2026))))
			capNear(t, sessionMust(options[0].NPV()), .5187376052783954, 1e-11)
		} else {
			ratesOK(t, engine.Close())
			ratesOK(t, quote.SetValue(tc.vol+.001))
			v := sessionMust(options[0].NPV())
			if !math.IsNaN(v) && !math.IsInf(v, 0) && v > tc.prices[0] {
			} else {
				t.Fatal("retained quote update", v)
			}
		}
	}
}
func TestCapHelperNormalCalibrationAndRecovery(t *testing.T) {
	s := sessionMust(NewSession())
	defer s.Close()
	settings := sessionMust(s.NewSettings())
	date := sessionMust(NewDate(15, 1, 2026))
	ratesOK(t, settings.SetEvaluationDate(date))
	dc := sessionMust(s.Actual365Fixed())
	curve := sessionMust(s.NewFlatForward(date, .03, dc))
	index := sessionMust(s.NewEuriborSixMonths(curve, settings))
	quote := sessionMust(s.NewSimpleQuote(.01))
	model := sessionMust(s.NewHullWhite(curve, .05, .01))
	engine := sessionMust(s.NewTreeCapFloorEngine(model, 30))
	cfg := CapHelperConfig{Length: Period{5, Years}, Volatility: quote, Index: index, FixedLegFrequency: Annual, FixedLegDayCounter: dc, Curve: curve, ErrorType: RelativePriceError, VolatilityType: Normal}
	helper := sessionMust(s.NewCapHelper(cfg))
	market := sessionMust(helper.MarketValue())
	capNear(t, market, 0.023719601355028135, 1e-12)
	capNear(t, sessionMust(helper.BlackPrice(.01)), market, 1e-14)
	if _, err := helper.ModelValue(); err == nil {
		t.Fatal("missing engine accepted")
	}
	ratesOK(t, helper.SetTreeEngine(engine))
	before := sessionMust(helper.ModelValue())
	capNear(t, before, 0.021512584929889333, 1e-12)
	if math.IsNaN(before) || math.IsInf(before, 0) || before <= 0 {
		t.Fatal(before)
	}
	times := sessionMust(helper.MandatoryTimes())
	fixed := sessionMust(s.NewTreeCapFloorEngineWithTimeGrid(model, times))
	ratesOK(t, helper.SetTreeEngine(fixed))
	v := sessionMust(helper.ModelValue())
	if math.IsNaN(v) || math.IsInf(v, 0) {
		t.Fatal(v)
	}
	cfg.Length.Length = 0
	if _, err := s.NewCapHelper(cfg); err == nil {
		t.Fatal("invalid tenor accepted")
	}
	cfg.Length.Length = 5
	for _, v := range []float64{-1, math.NaN(), math.Inf(1)} {
		if _, err := helper.BlackPrice(v); err == nil {
			t.Fatal("invalid volatility", v)
		}
	}
	ratesOK(t, quote.SetValue(.02))
	if v := sessionMust(helper.MarketValue()); math.IsNaN(v) || math.IsInf(v, 0) || v <= market {
		t.Fatal(v)
	}
	ratesOK(t, quote.SetValue(.01))
	capNear(t, sessionMust(helper.MarketValue()), market, 1e-14)
	for _, kind := range []CalibrationErrorType{RelativePriceError, PriceError, ImpliedVolError} {
		cfg.ErrorType = kind
		checked := sessionMust(s.NewCapHelper(cfg))
		ratesOK(t, checked.SetTreeEngine(engine))
		expected := sessionMust(checked.CalibrationError())
		for _, invalid := range []float64{math.NaN(), math.Inf(1), -.01} {
			ratesOK(t, quote.SetValue(invalid))
			if _, err := checked.CalibrationError(); err == nil {
				t.Fatal("invalid market quote accepted", kind, invalid)
			}
			ratesOK(t, quote.SetValue(.01))
			capNear(t, sessionMust(checked.CalibrationError()), expected, 1e-14)
		}
	}
	cfg.ErrorType = RelativePriceError

	ratesOK(t, quote.SetValue(.2))
	cfg.VolatilityType = ShiftedLognormal
	for _, tc := range []struct{ shift, expected float64 }{{0, .01372895806987519}, {.01, .018693918169825054}} {
		cfg.Shift = tc.shift
		shifted := sessionMust(s.NewCapHelper(cfg))
		capNear(t, sessionMust(shifted.MarketValue()), tc.expected, 1e-12)
	}
	ratesOK(t, quote.SetValue(.01))
	cfg.VolatilityType, cfg.Shift = Normal, 0

	helpers := []*CapHelper{}
	for _, n := range []int32{2, 3, 4, 5} {
		cfg.Length.Length = n
		helpers = append(helpers, sessionMust(s.NewCapHelper(cfg)))
	}
	method := sessionMust(s.NewLevenbergMarquardt(&LevenbergMarquardtConfig{EpsFcn: 1e-8, XTol: 1e-8, GTol: 1e-8}))
	stationary := uint(100)
	gradient := 1e-8
	criteria := sessionMust(s.NewEndCriteria(EndCriteriaConfig{MaxIterations: 10000, MaxStationaryStateIterations: &stationary, RootEpsilon: 1e-6, FunctionEpsilon: 1e-8, GradientNormEpsilon: &gradient}))
	ratesOK(t, model.CalibrateCaps(helpers, method, criteria, true, 30))
	capNear(t, sessionMust(model.A()), .05, 1e-14)
	capNear(t, sessionMust(model.Sigma()), 0.010705079712885954, 1e-8)
	for _, h := range helpers {
		v := sessionMust(h.CalibrationError())
		if math.IsNaN(v) || math.IsInf(v, 0) {
			t.Fatal(v)
		}
	}
	retained := sessionMust(helpers[0].ModelValue())

	ratesOK(t, index.Close())
	ratesOK(t, curve.Close())
	ratesOK(t, engine.Close())
	ratesOK(t, model.Close())
	capNear(t, sessionMust(helpers[0].ModelValue()), retained, 1e-14)
	ratesOK(t, quote.SetValue(.012))
	if v := sessionMust(helper.MarketValue()); math.IsNaN(v) || math.IsInf(v, 0) || v <= market {
		t.Fatal(v)
	}
}
