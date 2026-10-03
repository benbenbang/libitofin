package itofin

import (
	"encoding/json"
	"fmt"
	"os"
	"testing"
)

type gjrModelOracleInput struct {
	Spot                      float64
	DailyVariance             float64 `json:"daily_variance"`
	Risk                      float64 `json:"risk_free_rate"`
	Dividend                  float64 `json:"dividend_yield"`
	Omega, Alpha, Beta, Gamma float64
	Lambda                    float64   `json:"lambda_parameter"`
	Days                      float64   `json:"days_per_year"`
	Scheme                    GJRScheme `json:"discretization"`
}

func gjrReadOracle[T any](t *testing.T, name string) T {
	t.Helper()
	data := pricingMust(os.ReadFile("testdata/gjrgarch-model-" + name + ".json"))
	var out T
	pricingOK(t, json.Unmarshal(data, &out))
	return out
}

func gjrOracleMarket(t *testing.T, input gjrModelOracleInput, actualActual bool) (*Session, *Settings, *GJRProcess, Date) {
	t.Helper()
	s := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()) })
	today := pricingMust(NewDate(15, 1, 2026))
	settings := pricingMust(s.NewSettings())
	pricingOK(t, settings.SetEvaluationDate(today))
	dc := pricingMust(s.Actual365Fixed())
	if actualActual {
		dc = pricingMust(s.ActualActualISDA())
	}
	spot := pricingMust(s.NewSimpleQuote(input.Spot))
	risk := pricingMust(s.NewFlatForward(today, input.Risk, dc))
	dividend := pricingMust(s.NewFlatForward(today, input.Dividend, dc))
	params := GJRParameters{input.DailyVariance, input.Omega, input.Alpha, input.Beta, input.Gamma, input.Lambda, input.Days}
	process := pricingMust(s.NewGJRProcess(spot, risk, dividend, params, input.Scheme))
	return s, settings, process, today
}

func TestGJRIndependentAnalyticOracles(t *testing.T) {
	type row struct {
		Name              string
		Maturity          int `json:"maturity_days"`
		Strike, Call, Put float64
		Changes           map[string]float64
	}
	type fixture struct {
		Input gjrModelOracleInput
		Rows  []row
	}
	base := gjrReadOracle[fixture](t, "analytic")
	if len(base.Rows) != 12 {
		t.Fatal("incomplete analytic oracle")
	}
	for _, r := range base.Rows {
		t.Run(r.Name, func(t *testing.T) {
			input := base.Input
			data := pricingMust(json.Marshal(input))
			var values map[string]any
			pricingOK(t, json.Unmarshal(data, &values))
			for key, value := range r.Changes {
				values[key] = value
			}
			pricingOK(t, json.Unmarshal(pricingMust(json.Marshal(values)), &input))
			s, settings, process, today := gjrOracleMarket(t, input, false)
			model := pricingMust(s.NewGJRModel(process))
			engine := pricingMust(s.NewAnalyticGJREngine(model))
			expiry := pricingMust(today.AddDays(int64(r.Maturity)))
			for i, kind := range []OptionType{Call, Put} {
				option := pricingMust(s.NewVanillaOption(kind, r.Strike, expiry, settings))
				pricingNear(t, pricingMust(option.PriceAnalyticGJR(engine)), []float64{r.Call, r.Put}[i], 2e-10)
			}
		})
	}
	for n := 0; n < 3; n++ {
		matrix := gjrReadOracle[fixture](t, fmt.Sprintf("matrix-%d", n))
		if len(matrix.Rows) != 12 {
			t.Fatal("incomplete cached-price matrix")
		}
		s, settings, process, today := gjrOracleMarket(t, matrix.Input, true)
		engine := pricingMust(s.NewAnalyticGJREngine(pricingMust(s.NewGJRModel(process))))
		for _, r := range matrix.Rows {
			expiry := pricingMust(today.AddDays(int64(r.Maturity)))
			for i, kind := range []OptionType{Call, Put} {
				option := pricingMust(s.NewVanillaOption(kind, r.Strike, expiry, settings))
				pricingNear(t, pricingMust(option.PriceAnalyticGJR(engine)), []float64{r.Call, r.Put}[i], 2e-10)
			}
		}
	}
}

func TestGJRIndependentSeededMCOracles(t *testing.T) {
	type row struct {
		Input                gjrModelOracleInput
		Maturity             int `json:"maturity_days"`
		Kind                 int `json:"option_type"`
		Strike, Price, Error float64
		Settings             struct {
			Steps      uint `json:"timeSteps"`
			Samples    uint `json:"requiredSamples"`
			Seed       uint64
			Antithetic bool `json:"antitheticVariate"`
		}
	}
	fixture := gjrReadOracle[struct{ Rows []row }](t, "mc")
	if len(fixture.Rows) != 12 {
		t.Fatal("incomplete native MC oracle")
	}
	for i, r := range fixture.Rows {
		t.Run(fmt.Sprint(i), func(t *testing.T) {
			s, settings, process, today := gjrOracleMarket(t, r.Input, false)
			engine := pricingMust(s.NewMCGJREngine(process, GJRMCConfig{Steps: r.Settings.Steps, Samples: r.Settings.Samples, Seed: r.Settings.Seed, Antithetic: r.Settings.Antithetic}))
			kind := Call
			if r.Kind == -1 {
				kind = Put
			}
			option := pricingMust(s.NewVanillaOption(kind, r.Strike, pricingMust(today.AddDays(int64(r.Maturity))), settings))
			pricingNear(t, pricingMust(option.PriceMCGJR(engine)), r.Price, 2e-11)
			pricingNear(t, pricingMust(option.ErrorEstimate()), r.Error, 2e-11)
		})
	}
}

func TestGJRIndependentToleranceSamplingOracle(t *testing.T) {
	fixture := gjrReadOracle[struct {
		Tolerance struct {
			Input                gjrModelOracleInput
			Maturity             int  `json:"maturity_days"`
			Samples              uint `json:"effective_samples"`
			Strike, Price, Error float64
			Settings             struct {
				Steps      uint    `json:"timeSteps"`
				Tolerance  float64 `json:"requiredTolerance"`
				MaxSamples uint    `json:"maxSamples"`
				Seed       uint64
			}
		}
	}](t, "mc").Tolerance
	if fixture.Samples < 1023 || fixture.Settings.Tolerance <= 0 {
		t.Fatal("incomplete tolerance oracle")
	}
	s, settings, process, today := gjrOracleMarket(t, fixture.Input, false)
	config := GJRMCConfig{Steps: fixture.Settings.Steps, AbsoluteTolerance: fixture.Settings.Tolerance, MaxSamples: fixture.Settings.MaxSamples, Seed: fixture.Settings.Seed}
	engine := pricingMust(s.NewMCGJREngine(process, config))
	option := pricingMust(s.NewVanillaOption(Call, fixture.Strike, pricingMust(today.AddDays(int64(fixture.Maturity))), settings))
	pricingNear(t, pricingMust(option.PriceMCGJR(engine)), fixture.Price, 2e-11)
	pricingNear(t, pricingMust(option.ErrorEstimate()), fixture.Error, 2e-11)
	config.AbsoluteTolerance, config.MaxSamples, config.Samples = 0, 0, fixture.Samples
	replay := pricingMust(s.NewMCGJREngine(process, config))
	pricingNear(t, pricingMust(option.PriceMCGJR(replay)), fixture.Price, 2e-11)
	pricingNear(t, pricingMust(option.ErrorEstimate()), fixture.Error, 2e-11)
}
