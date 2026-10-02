package itofin

import (
	"encoding/json"
	"fmt"
	"math"
	"os"
	"reflect"
	"testing"
)

type batesCalibrationFit struct {
	Name                 string
	Start, Fitted        []float64
	Fixed                []bool
	MaxIterations        uint    `json:"max_iterations"`
	StationaryIterations uint    `json:"stationary_iterations"`
	OptimizerTolerance   float64 `json:"optimizer_tolerance"`
}
type batesOracle struct {
	Cases          [][]json.RawMessage
	PriceTolerance float64 `json:"price_absolute_tolerance"`
	Calibration    struct {
		Reference          []int
		RiskFree           float64 `json:"risk_free"`
		Dividend, Spot     float64
		Helpers            [][4]float64
		Fits               []batesCalibrationFit
		ParameterTolerance float64 `json:"parameter_tolerance"`
		RelativeTolerance  float64 `json:"max_relative_error_tolerance"`
		Order              uint    `json:"integration_order"`
	}
}

func loadBatesOracle(t *testing.T) batesOracle {
	t.Helper()
	data := pricingMust(os.ReadFile("testdata/bates-oracle.json"))
	var out batesOracle
	pricingOK(t, json.Unmarshal(data, &out))
	if len(out.Cases) != 52 || len(out.Calibration.Helpers) != 20 || len(out.Calibration.Fits) != 2 {
		t.Fatal("incomplete oracle")
	}
	return out
}
func batesDecode[T any](t *testing.T, raw json.RawMessage) T {
	t.Helper()
	var out T
	pricingOK(t, json.Unmarshal(raw, &out))
	return out
}
func batesDate(t *testing.T, parts []int) Date {
	t.Helper()
	return pricingMust(NewDate(parts[0], parts[1], parts[2]))
}
func TestBatesIndependentPriceOracles(t *testing.T) {
	oracle := loadBatesOracle(t)
	for _, row := range oracle.Cases {
		name := batesDecode[string](t, row[0])
		t.Run(name, func(t *testing.T) {
			s := pricingMust(NewSession())
			defer s.Close()
			ref := batesDate(t, batesDecode[[]int](t, row[1]))
			expiry := batesDate(t, batesDecode[[]int](t, row[2]))
			dc := pricingMust(s.Actual365Fixed())
			if batesDecode[string](t, row[3]) == "ActualActualISDA" {
				dc = pricingMust(s.ActualActualISDA())
			}
			settings := pricingMust(s.NewSettings())
			pricingOK(t, settings.SetEvaluationDate(ref))
			risk := pricingMust(s.NewFlatForward(ref, batesDecode[float64](t, row[4]), dc))
			dividend := pricingMust(s.NewFlatForward(ref, batesDecode[float64](t, row[5]), dc))
			spot := pricingMust(s.NewSimpleQuote(batesDecode[float64](t, row[6])))
			p := batesDecode[[]float64](t, row[9])
			process := pricingMust(s.NewBatesProcess(BatesProcessConfig{Spot: spot, RiskFree: risk, Dividend: dividend, V0: p[4], Kappa: p[1], Theta: p[0], Sigma: p[2], Rho: p[3], Nu: p[5], Delta: p[6], Lambda: p[7]}))
			model := pricingMust(s.NewBatesModel(process))
			engine := pricingMust(s.NewBatesEngine(model, batesDecode[uint](t, row[10])))
			kind := Call
			if batesDecode[string](t, row[8]) == "put" {
				kind = Put
			}
			option := pricingMust(s.NewVanillaOption(kind, batesDecode[float64](t, row[7]), expiry, settings))
			pricingNear(t, pricingMust(option.PriceBates(engine)), batesDecode[float64](t, row[12]), oracle.PriceTolerance)
		})
	}
}
func TestBatesIndependentCalibrationOracles(t *testing.T) {
	oracle := loadBatesOracle(t)
	c := oracle.Calibration
	for _, fit := range c.Fits {
		t.Run(fit.Name, func(t *testing.T) {
			s := pricingMust(NewSession())
			defer s.Close()
			ref := batesDate(t, c.Reference)
			dc := pricingMust(s.Actual365Fixed())
			cal := pricingMust(s.NullCalendar())
			settings := pricingMust(s.NewSettings())
			pricingOK(t, settings.SetEvaluationDate(ref))
			var helpers []*HestonModelHelper
			for _, row := range c.Helpers {
				helpers = append(helpers, pricingMust(s.NewHestonModelHelper(HestonHelperConfig{Maturity: Period{int32(row[0]), Months}, Calendar: cal, Spot: c.Spot, Strike: row[1], Volatility: row[2], RiskFreeRate: c.RiskFree, DividendYield: c.Dividend, ErrorType: RelativePriceError, ReferenceDate: ref, DayCounter: dc, Settings: settings})))
			}
			risk := pricingMust(s.NewFlatForward(ref, c.RiskFree, dc))
			dividend := pricingMust(s.NewFlatForward(ref, c.Dividend, dc))
			spot := pricingMust(s.NewSimpleQuote(c.Spot))
			p := fit.Start
			process := pricingMust(s.NewBatesProcess(BatesProcessConfig{Spot: spot, RiskFree: risk, Dividend: dividend, V0: p[4], Kappa: p[1], Theta: p[0], Sigma: p[2], Rho: p[3], Nu: p[5], Delta: p[6], Lambda: p[7]}))
			model := pricingMust(s.NewBatesModel(process))
			method := pricingMust(s.NewLevenbergMarquardt(&LevenbergMarquardtConfig{EpsFcn: fit.OptimizerTolerance, XTol: fit.OptimizerTolerance, GTol: fit.OptimizerTolerance}))
			criteria := pricingMust(s.NewEndCriteria(EndCriteriaConfig{MaxIterations: fit.MaxIterations, MaxStationaryStateIterations: &fit.StationaryIterations, RootEpsilon: fit.OptimizerTolerance, FunctionEpsilon: fit.OptimizerTolerance, GradientNormEpsilon: &fit.OptimizerTolerance}))
			constraint := pricingMust(s.NewBoundaryConstraint(-1, 2))
			weights := make([]float64, len(helpers))
			for i := range weights {
				weights[i] = 1
			}
			pricingOK(t, model.Calibrate(helpers, method, criteria, c.Order, &CalibrationOptions{Constraint: constraint, Weights: weights, FixParameters: fit.Fixed}))
			params := pricingMust(model.Params())
			for i, expected := range fit.Fitted {
				pricingNear(t, params[i], expected, c.ParameterTolerance)
				if fit.Fixed[i] && params[i] != fit.Start[i] {
					t.Fatalf("fixed parameter %d changed", i)
				}
			}
			for i, h := range helpers {
				if math.Abs(pricingMust(h.CalibrationError())) > c.RelativeTolerance {
					t.Fatal(fmt.Sprintf("helper %d residual exceeded", i))
				}
			}
			badOptions := []*CalibrationOptions{{Weights: []float64{1}}, {Weights: append([]float64{-1}, weights[1:]...)}, {Weights: append([]float64{math.NaN()}, weights[1:]...)}, {Weights: append([]float64{math.Inf(1)}, weights[1:]...)}, {FixParameters: []bool{true}}}
			for _, bad := range badOptions {
				if err := model.Calibrate(helpers, method, criteria, c.Order, bad); err == nil {
					t.Fatal("invalid options accepted")
				}
				if !reflect.DeepEqual(pricingMust(model.Params()), params) {
					t.Fatal("invalid fit partially mutated model")
				}
			}
			if err := model.Calibrate(nil, method, criteria, c.Order); err == nil {
				t.Fatal("empty helper set accepted")
			}
			if err := model.Calibrate([]*HestonModelHelper{nil}, method, criteria, c.Order); err == nil {
				t.Fatal("nil helper accepted")
			}
			if err := model.Calibrate(helpers, nil, criteria, c.Order); err == nil {
				t.Fatal("nil method accepted")
			}
			if err := model.Calibrate(helpers, method, nil, c.Order); err == nil {
				t.Fatal("nil criteria accepted")
			}
			if err := model.Calibrate(helpers, method, criteria, c.Order, nil, nil); err == nil {
				t.Fatal("multiple options accepted")
			}
		})
	}
}
