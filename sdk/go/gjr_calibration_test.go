package itofin

import (
	"errors"
	"math"
	"reflect"
	"testing"
)

func gjrCalibrationHelpers(t *testing.T, m *gjrPricingMarket) []*HestonModelHelper {
	t.Helper()
	calendar := pricingMust(m.s.NullCalendar())
	dc := pricingMust(m.s.Actual365Fixed())
	var helpers []*HestonModelHelper
	for _, strike := range []float64{90, 100, 110} {
		helpers = append(helpers, pricingMust(m.s.NewHestonModelHelper(HestonHelperConfig{Maturity: Period{12, Months}, Calendar: calendar, Spot: 100, Strike: strike, Volatility: .2, RiskFreeRate: .05, DividendYield: .02, ErrorType: RelativePriceError, ReferenceDate: m.today, DayCounter: dc, Settings: m.settings})))
	}
	return helpers
}

func TestGJRCalibrationValidationAndAtomicFailures(t *testing.T) {
	m := newGJRPricingMarket(t)
	helpers := gjrCalibrationHelpers(t, m)
	method := pricingMust(m.s.NewLevenbergMarquardt(nil))
	criteria := pricingMust(m.s.NewEndCriteria(EndCriteriaConfig{MaxIterations: 50, RootEpsilon: 1e-8, FunctionEpsilon: 1e-8}))
	params := pricingMust(m.model.Params())
	baseline := pricingMust(m.option.NPV())
	for _, opts := range []*CalibrationOptions{
		{Weights: []float64{1}}, {Weights: []float64{-1, 1, 1}},
		{Weights: []float64{math.NaN(), 1, 1}}, {Weights: []float64{math.Inf(1), 1, 1}},
		{FixParameters: []bool{true}},
	} {
		if err := m.model.Calibrate(helpers, method, criteria, opts); err == nil {
			t.Fatal("invalid calibration options accepted")
		}
		if !reflect.DeepEqual(pricingMust(m.model.Params()), params) {
			t.Fatal("failed calibration mutated model")
		}
		pricingNear(t, pricingMust(m.option.NPV()), baseline, 0)
	}
	for _, call := range []func() error{
		func() error { return m.model.Calibrate(nil, method, criteria) },
		func() error { return m.model.Calibrate([]*HestonModelHelper{nil}, method, criteria) },
		func() error { return m.model.Calibrate(helpers, nil, criteria) },
		func() error { return m.model.Calibrate(helpers, method, nil) },
		func() error { return m.model.Calibrate(helpers, method, criteria, nil, nil) },
	} {
		if err := call(); err == nil {
			t.Fatal("invalid calibration arguments accepted")
		}
	}
	var nilModel *GJRModel
	if err := nilModel.Calibrate(helpers, method, criteria); err == nil {
		t.Fatal("nil model accepted")
	}
	other := pricingMust(NewSession())
	defer other.Close()
	foreignMethod := pricingMust(other.NewLevenbergMarquardt(nil))
	if err := m.model.Calibrate(helpers, foreignMethod, criteria); !errors.Is(err, ErrSessionMismatch) {
		t.Fatal(err)
	}
	foreignConstraint := pricingMust(other.NewBoundaryConstraint(0, 1))
	if err := m.model.Calibrate(helpers, method, criteria, &CalibrationOptions{Constraint: foreignConstraint}); !errors.Is(err, ErrSessionMismatch) {
		t.Fatal(err)
	}
	pricingOK(t, helpers[0].Close())
	if err := m.model.Calibrate(helpers, method, criteria); err == nil {
		t.Fatal("closed helper accepted")
	}
	if !reflect.DeepEqual(pricingMust(m.model.Params()), params) {
		t.Fatal("invalid helper mutated model")
	}
	pricingOK(t, m.model.Close())
	if err := m.model.Calibrate(helpers[1:], method, criteria); err == nil {
		t.Fatal("closed model accepted")
	}
}
