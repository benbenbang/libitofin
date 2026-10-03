package itofin

import (
	"encoding/json"
	"math"
	"os"
	"testing"
)

type gjrOracleInput struct {
	Spot           float64
	DailyVariance  float64 `json:"daily_variance"`
	RiskFreeRate   float64 `json:"risk_free_rate"`
	DividendYield  float64 `json:"dividend_yield"`
	Omega          float64
	Alpha          float64
	Beta           float64
	Gamma          float64
	Lambda         float64
	DaysPerYear    float64 `json:"days_per_year"`
	Discretization GJRScheme
	Horizon        float64
	Steps, Paths   int
	Seed           uint32
}

func (c gjrOracleInput) parameters() GJRParameters {
	return GJRParameters{c.DailyVariance, c.Omega, c.Alpha, c.Beta, c.Gamma, c.Lambda, c.DaysPerYear}
}

func gjrOracleRead(t *testing.T, path string, out any) {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if err := json.Unmarshal(data, out); err != nil {
		t.Fatal(err)
	}
}

func gjrOracleNear(t *testing.T, got, want []float64) {
	t.Helper()
	if len(got) != len(want) {
		t.Fatalf("oracle shape: got %d want %d", len(got), len(want))
	}
	for i, value := range got {
		if math.IsNaN(value) || math.IsInf(value, 0) || math.Abs(value-want[i]) > 1e-12+2e-13*math.Abs(want[i]) {
			t.Fatalf("oracle value %d: got %.17g want %.17g", i, value, want[i])
		}
	}
}

func TestGJRQuantLibTransitionFixtures(t *testing.T) {
	count := 0
	for _, group := range []string{"0", "1", "2", "edge"} {
		var fixture struct {
			Cases []struct {
				Name                              string
				Input                             gjrOracleInput
				State, DW, Initial, Drift, Evolve [2]float64
				DT                                float64
				Diffusion                         [2][2]float64
			}
		}
		gjrOracleRead(t, "testdata/gjrgarch-transitions-"+group+".json", &fixture)
		count += len(fixture.Cases)
		for _, c := range fixture.Cases {
			t.Run(c.Name, func(t *testing.T) {
				s := pricingMust(NewSession())
				defer s.Close()
				today := pricingMust(NewDate(3, 10, 2026))
				dc := pricingMust(s.Actual365Fixed())
				spot := pricingMust(s.NewSimpleQuote(c.Input.Spot))
				risk := pricingMust(s.NewFlatForward(today, c.Input.RiskFreeRate, dc))
				dividend := pricingMust(s.NewFlatForward(today, c.Input.DividendYield, dc))
				p := pricingMust(s.NewGJRProcess(spot, risk, dividend, c.Input.parameters(), c.Input.Discretization))
				initial := pricingMust(p.InitialValues())
				drift := pricingMust(p.Drift(0, c.State))
				diffusion := pricingMust(p.Diffusion(0, c.State))
				evolve := pricingMust(p.Evolve(0, c.State, c.DT, c.DW))
				gjrOracleNear(t, initial[:], c.Initial[:])
				gjrOracleNear(t, drift[:], c.Drift[:])
				gjrOracleNear(t, diffusion[0][:], c.Diffusion[0][:])
				gjrOracleNear(t, diffusion[1][:], c.Diffusion[1][:])
				gjrOracleNear(t, evolve[:], c.Evolve[:])
			})
		}
	}
	if count != 27 {
		t.Fatalf("missing transition cases: %d", count)
	}
}

func TestGJRQuantLibSeededPathFixtures(t *testing.T) {
	var fixture struct {
		Cases []struct {
			Name     string
			Input    gjrOracleInput
			Full     []float64 `json:"expected_full"`
			Terminal []float64 `json:"expected_terminal"`
		}
	}
	gjrOracleRead(t, "testdata/gjrgarch-paths.json", &fixture)
	if len(fixture.Cases) != 6 {
		t.Fatalf("missing path cases: %d", len(fixture.Cases))
	}
	for _, c := range fixture.Cases {
		t.Run(c.Name, func(t *testing.T) {
			input := c.Input
			config := GJRConfig{
				Spot: input.Spot, DailyVariance: input.DailyVariance, RiskFreeRate: input.RiskFreeRate, DividendYield: input.DividendYield,
				Omega: input.Omega, Alpha: input.Alpha, Beta: input.Beta, Gamma: input.Gamma, Lambda: input.Lambda, DaysPerYear: input.DaysPerYear,
				Horizon: input.Horizon, Steps: input.Steps, Paths: input.Paths, Seed: input.Seed, Scheme: input.Discretization,
			}
			gjrOracleNear(t, pricingMust(SimulateGJR(config)).Values, c.Full)
			config.TerminalOnly = true
			gjrOracleNear(t, pricingMust(SimulateGJR(config)).Values, c.Terminal)
		})
	}
}
