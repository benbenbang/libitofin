package itofin

import (
	"encoding/json"
	"os"
	"testing"
)

func TestMertonRejectsUnderflowedAdjustedMean(t *testing.T) {
	option, _ := mertonOracleOption(t, mertonOracleInput{
		OptionType: "put", Spot: 100, Strike: 100, JumpIntensity: 1,
		LogMeanJump: -744, MaturityDays: 1, RelativeAccuracy: 1e-12, MaxIterations: 1000,
	})
	if _, err := option.NPV(); err == nil {
		t.Fatal("positive intensity with unrepresentable mean was treated as zero jumps")
	}
}

type mertonOracleInput struct {
	OptionType        string  `json:"option_type"`
	Spot              float64 `json:"spot"`
	Strike            float64 `json:"strike"`
	DividendYield     float64 `json:"dividend_yield"`
	RiskFreeRate      float64 `json:"risk_free_rate"`
	Volatility        float64 `json:"volatility"`
	JumpIntensity     float64 `json:"jump_intensity"`
	LogMeanJump       float64 `json:"log_mean_jump"`
	LogJumpVolatility float64 `json:"log_jump_volatility"`
	MaturityDays      int     `json:"maturity_days"`
	RelativeAccuracy  float64 `json:"relative_accuracy"`
	MaxIterations     int     `json:"max_iterations"`
	ClockVariant      int     `json:"clock_variant"`
}

type mertonOracleCase struct {
	Name     string             `json:"name"`
	Input    mertonOracleInput  `json:"input"`
	Expected map[string]float64 `json:"expected"`
}

func mertonOracleOption(t *testing.T, in mertonOracleInput) (*VanillaOption, *JumpDiffusionEngine) {
	t.Helper()
	s := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()) })
	today := pricingMust(NewDate(2, 10, 2026))
	expiry := pricingMust(today.AddDays(int64(in.MaturityDays)))
	dc := pricingMust(s.Actual360())
	volDC := dc
	volDate := today
	if in.ClockVariant == 1 {
		dc = pricingMust(s.Actual365Fixed())
		volDate = pricingMust(today.AddDays(-30))
	} else if in.ClockVariant != 0 {
		t.Fatalf("unsupported oracle clock variant %d", in.ClockVariant)
	}
	settings := pricingMust(s.NewSettings())
	pricingOK(t, settings.SetEvaluationDate(today))
	var quotes [7]*SimpleQuote
	for i, value := range [7]float64{in.Spot, in.RiskFreeRate, in.DividendYield, in.Volatility, in.JumpIntensity, in.LogMeanJump, in.LogJumpVolatility} {
		quotes[i] = pricingMust(s.NewSimpleQuote(value))
	}
	risk := pricingMust(s.NewFlatForwardFromQuote(today, quotes[1], dc))
	dividend := pricingMust(s.NewFlatForwardFromQuote(today, quotes[2], dc))
	vol := pricingMust(s.NewBlackConstantVolFromQuote(volDate, quotes[3], volDC, nil))
	process := pricingMust(s.NewMerton76Process(quotes[0], risk, dividend, vol, quotes[4], quotes[5], quotes[6]))
	engine := pricingMust(s.NewJumpDiffusionEngine(process, in.RelativeAccuracy, in.MaxIterations))
	var kind OptionType
	switch in.OptionType {
	case "call":
		kind = Call
	case "put":
		kind = Put
	default:
		t.Fatalf("unknown oracle option type %q", in.OptionType)
	}
	option := pricingMust(s.NewVanillaOption(kind, in.Strike, expiry, settings))
	pricingOK(t, option.SetJumpDiffusionEngine(engine))
	return option, engine
}

func TestMertonNativeQuantLibOracle(t *testing.T) {
	data := pricingMust(os.ReadFile("testdata/merton76-oracle.json"))
	var fixture struct {
		Cases []mertonOracleCase `json:"cases"`
	}
	pricingOK(t, json.Unmarshal(data, &fixture))
	if len(fixture.Cases) != 12 {
		t.Fatalf("expected 12 independent oracle cases, got %d", len(fixture.Cases))
	}
	for _, test := range fixture.Cases {
		t.Run(test.Name, func(t *testing.T) {
			option, _ := mertonOracleOption(t, test.Input)
			for _, field := range []struct {
				name string
				get  func() (float64, error)
			}{
				{"value", option.NPV}, {"delta", option.Delta}, {"gamma", option.Gamma},
				{"theta", option.Theta}, {"vega", option.Vega}, {"rho", option.Rho},
				{"dividend_rho", option.DividendRho},
			} {
				want, exists := test.Expected[field.name]
				if !exists {
					t.Fatalf("missing oracle field %s", field.name)
				}
				pricingNear(t, pricingMust(field.get()), want, 1e-8)
			}
		})
	}
}

func TestMertonZeroPricePrefixDoesNotTruncatePositiveTail(t *testing.T) {
	option, _ := mertonOracleOption(t, mertonOracleInput{
		OptionType: "call", Spot: 100, Strike: 1000, DividendYield: .02,
		RiskFreeRate: .05, Volatility: .001, JumpIntensity: .01,
		LogMeanJump: 1, LogJumpVolatility: 0, MaturityDays: 360,
		RelativeAccuracy: 1e-12, MaxIterations: 4096,
	})
	pricingNear(t, pricingMust(option.NPV()), .00016415889568634478, 1e-12)
}

func TestMertonHighPoissonMeanRespectsIterationBudget(t *testing.T) {
	option, _ := mertonOracleOption(t, mertonOracleInput{
		OptionType: "call", Spot: 100, Strike: 100, DividendYield: .02,
		RiskFreeRate: .05, Volatility: .2, JumpIntensity: 800,
		LogMeanJump: 0, LogJumpVolatility: .02, MaturityDays: 360,
		RelativeAccuracy: 1e-12, MaxIterations: 100,
	})
	if _, err := option.NPV(); err == nil {
		t.Fatal("Poisson mode beyond the iteration budget was accepted")
	}
}
