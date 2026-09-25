package main

import (
	"encoding/json"
	"math"
	"os"

	itofin "github.com/benbenbang/libitofin/sdk/go"
)

func must[T any](value T, err error) T {
	if err != nil {
		panic(err)
	}
	return value
}

func calibrate(create func(*itofin.Session) (itofin.OptimizationMethod, error)) [5]float64 {
	s := must(itofin.NewSession())
	defer s.Close()
	reference := must(itofin.NewDate(15, 1, 2026))
	dayCounter := must(s.Actual360())
	calendar := must(s.NullCalendar())
	settings := must(s.NewSettings())
	if err := settings.SetEvaluationDate(reference); err != nil {
		panic(err)
	}
	maturities := []itofin.Period{
		{Length: 1, Unit: itofin.Months}, {Length: 2, Unit: itofin.Months},
		{Length: 3, Unit: itofin.Months}, {Length: 6, Unit: itofin.Months},
		{Length: 9, Unit: itofin.Months}, {Length: 1, Unit: itofin.Years},
		{Length: 2, Unit: itofin.Years},
	}
	strikes := [7][3]uint64{
		{0x3fefac53e80821cf, 0x3feec1d93a138c3f, 0x3fedde266b06edcb},
		{0x3feee6fc4e5c066b, 0x3fedad1ea01315b6, 0x3fec7fb4cb280859},
		{0x3fedfc74e7ab4083, 0x3fec86124a9aed52, 0x3feb21f1fa31573d},
		{0x3feb422c40cceb6b, 0x3fe96481fc737590, 0x3fe7a78a19df52fa},
		{0x3fe8a167781bf00b, 0x3fe6938b2fef7da4, 0x3fe4b18a04b47ab2},
		{0x3fe632e5c3c88016, 0x3fe4128a7c687e73, 0x3fe22653d92d71bf},
		{0x3fdd08fc2bc78913, 0x3fd92e6fb34bcd0d, 0x3fd5d6d3fbc52310},
	}
	var helpers []*itofin.HestonModelHelper
	for i, maturity := range maturities {
		for _, bits := range strikes[i] {
			helpers = append(helpers, must(s.NewHestonModelHelper(itofin.HestonHelperConfig{
				Maturity: maturity, Calendar: calendar, Spot: 1, Strike: math.Float64frombits(bits),
				Volatility: .1, RiskFreeRate: .04, DividendYield: .50,
				ErrorType: itofin.RelativePriceError, ReferenceDate: reference,
				DayCounter: dayCounter, Settings: settings,
			})))
		}
	}
	process := must(s.NewHestonProcess(itofin.HestonProcessConfig{
		RiskFreeRate: .04, DividendYield: .50, Spot: 1, V0: .01,
		Kappa: .2, Theta: .02, Sigma: .3, Rho: -.75,
		ReferenceDate: reference, DayCounter: dayCounter,
	}))
	model := must(s.NewHestonModel(process))
	method := must(create(s))
	criteria := must(s.NewEndCriteria(itofin.EndCriteriaConfig{
		MaxIterations: 400, MaxStationaryStateIterations: uintPointer(40),
		RootEpsilon: 1e-8, FunctionEpsilon: 1e-8, GradientNormEpsilon: floatPointer(1e-8),
	}))
	if err := model.Calibrate(helpers, method, criteria, 96); err != nil {
		panic(err)
	}
	return [5]float64{must(model.V0()), must(model.Kappa()), must(model.Theta()), must(model.Sigma()), must(model.Rho())}
}

func uintPointer(value uint) *uint        { return &value }
func floatPointer(value float64) *float64 { return &value }

func main() {
	results := make(map[string][5]float64)
	for _, row := range []struct {
		name   string
		create func(*itofin.Session) (itofin.OptimizationMethod, error)
	}{
		{"Simplex", func(s *itofin.Session) (itofin.OptimizationMethod, error) { return s.NewSimplex(0.1) }},
		{"ConjugateGradient", func(s *itofin.Session) (itofin.OptimizationMethod, error) { return s.NewConjugateGradient() }},
		{"SteepestDescent", func(s *itofin.Session) (itofin.OptimizationMethod, error) { return s.NewSteepestDescent() }},
	} {
		results[row.name] = calibrate(row.create)
	}
	if err := json.NewEncoder(os.Stdout).Encode(results); err != nil {
		panic(err)
	}
}
