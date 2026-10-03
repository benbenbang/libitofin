package itofin

import (
	"math"
	"reflect"
	"testing"
)

func TestGJRIndependentWeightedFixedCalibrationOracle(t *testing.T) {
	fixture := gjrReadOracle[struct {
		Input         gjrModelOracleInput
		Initial       []float64 `json:"initial_params"`
		Fitted        []float64 `json:"calibrated_params"`
		Fixed         []bool    `json:"fixed_params"`
		Weights       []float64
		Lambda        float64 `json:"simplex_lambda"`
		MaxIterations uint    `json:"max_iterations"`
		Stationary    uint    `json:"stationary_iterations"`
		Tolerances    []float64
		Rows          []struct {
			Maturity int32 `json:"maturity_days"`
			Strike   float64
			Vol      float64 `json:"market_vol"`
			Price    float64 `json:"model_price"`
		}
	}](t, "synthetic")
	if len(fixture.Rows) != 6 || len(fixture.Fitted) != 6 || len(fixture.Weights) != 6 {
		t.Fatal("incomplete independent calibration oracle")
	}
	input := fixture.Input
	input.DailyVariance = fixture.Initial[5]
	s, settings, process, today := gjrOracleMarket(t, input, false)
	model := pricingMust(s.NewGJRModel(process))
	engine := pricingMust(s.NewAnalyticGJREngine(model))
	calendar := pricingMust(s.NullCalendar())
	dc := pricingMust(s.Actual365Fixed())
	var helpers []*HestonModelHelper
	for _, r := range fixture.Rows {
		helpers = append(helpers, pricingMust(s.NewHestonModelHelper(HestonHelperConfig{Maturity: Period{r.Maturity, Days}, Calendar: calendar, Spot: input.Spot, Strike: r.Strike, Volatility: r.Vol, RiskFreeRate: input.Risk, DividendYield: input.Dividend, ErrorType: ImpliedVolError, ReferenceDate: today, DayCounter: dc, Settings: settings})))
	}
	method := pricingMust(s.NewSimplex(fixture.Lambda))
	criteria := pricingMust(s.NewEndCriteria(EndCriteriaConfig{MaxIterations: fixture.MaxIterations, MaxStationaryStateIterations: &fixture.Stationary, RootEpsilon: fixture.Tolerances[0], FunctionEpsilon: fixture.Tolerances[1], GradientNormEpsilon: &fixture.Tolerances[2]}))
	option := pricingMust(s.NewVanillaOption(Call, 110, pricingMust(today.AddDays(63)), settings))
	baseline := pricingMust(option.PriceAnalyticGJR(engine))
	pricingOK(t, model.Calibrate(helpers, method, criteria, &CalibrationOptions{Weights: fixture.Weights, FixParameters: fixture.Fixed}))
	params := pricingMust(model.Params())
	for i, want := range fixture.Fitted {
		pricingNear(t, params[i], want, 2e-10)
		if fixture.Fixed[i] && params[i] != fixture.Initial[i] {
			t.Fatalf("fixed parameter %d changed", i)
		}
	}
	if math.Abs(pricingMust(option.NPV())-baseline) < 1e-6 {
		t.Fatal("existing engine ignored calibration")
	}
	for i, helper := range helpers {
		row := fixture.Rows[i]
		kind := Call
		if row.Strike < input.Spot {
			kind = Put
		}
		option := pricingMust(s.NewVanillaOption(kind, row.Strike, pricingMust(today.AddDays(int64(row.Maturity))), settings))
		pricingNear(t, pricingMust(option.PriceAnalyticGJR(engine)), row.Price, 3e-8)
		if math.Abs(pricingMust(helper.CalibrationError())) > 5e-8 {
			t.Fatal("independent helper quote not fitted", i)
		}
	}
	before := pricingMust(helpers[0].CalibrationError())
	changed := append([]float64(nil), params...)
	changed[5] *= 1.1
	pricingOK(t, model.SetParams(changed))
	if pricingMust(helpers[0].CalibrationError()) == before {
		t.Fatal("calibration helper engine did not retain live model")
	}
	pricingOK(t, model.SetParams(params))
	allFixed := []bool{true, true, true, true, true, true}
	if err := model.Calibrate(helpers, method, criteria, &CalibrationOptions{FixParameters: allFixed}); err == nil {
		t.Fatal("all-fixed fit without a free parameter accepted")
	}
	if !reflect.DeepEqual(pricingMust(model.Params()), params) {
		t.Fatal("all-fixed failed calibration mutated model")
	}
	pricingOK(t, model.Close())
	pricingOK(t, process.Close())
	pricingOK(t, engine.Close())
	pricingOK(t, option.Calculate())
	pricingNear(t, pricingMust(option.NPV()), fixture.Rows[len(fixture.Rows)-1].Price, 3e-8)
	pricingNear(t, pricingMust(helpers[0].CalibrationError()), before, 0)
}
