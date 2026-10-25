package itofin

import (
	"encoding/csv"
	"math"
	"os"
	"strconv"
	"strings"
	"sync"
	"testing"
)

func TestCashDigitalBlackCalculatorAnalytic(t *testing.T) {
	s := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()) })
	cdf := func(x float64) float64 { return math.Erfc(-x/math.Sqrt2) / 2 }
	for _, option := range []OptionType{Call, Put} {
		sign := 1.
		if option == Put {
			sign = -1
		}
		for _, strike := range []float64{80, 100, 120} {
			for _, forward := range []float64{90, 110} {
				for _, deviation := range []float64{.15, .5, 1.2} {
					for _, cash := range []float64{-7, 0, 13} {
						discount, spot, maturity := .94, 97., 2.
						d1 := math.Log(forward/strike)/deviation + deviation/2
						d2 := d1 - deviation
						density := math.Exp(-d2*d2/2) / math.Sqrt(2*math.Pi)
						beta := cdf(sign * d2)
						value := discount * cash * beta
						delta := discount * cash * sign * density / (deviation * forward)
						gamma := -delta / forward * (1 + d2/deviation)
						theta := -(math.Log(discount)*value + math.Log(forward/spot)*forward*delta + .5*deviation*deviation*forward*forward*gamma) / maturity
						elasticity := 0.
						if value > math.Nextafter(1, 2)-1 {
							elasticity = delta / value * forward
						} else if math.Abs(delta) >= math.Nextafter(1, 2)-1 {
							elasticity = math.Copysign(math.MaxFloat64, delta)
						}
						c := pricingMust(s.NewCashDigitalBlackCalculator(option, strike, cash, forward, deviation, discount))
						blackCalculatorNear(t, pricingMust(c.Value()), value)
						blackCalculatorNear(t, pricingMust(c.Alpha()), 0)
						blackCalculatorNear(t, pricingMust(c.Beta()), beta)
						blackCalculatorNear(t, pricingMust(c.DeltaForward()), delta)
						blackCalculatorNear(t, pricingMust(c.Delta(spot)), delta*forward/spot)
						blackCalculatorNear(t, pricingMust(c.GammaForward()), gamma)
						blackCalculatorNear(t, pricingMust(c.Gamma(spot)), gamma*math.Pow(forward/spot, 2))
						blackCalculatorNear(t, pricingMust(c.ElasticityForward()), elasticity)
						blackCalculatorNear(t, pricingMust(c.Elasticity(spot)), elasticity)
						blackCalculatorNear(t, pricingMust(c.Theta(spot, maturity)), theta)
						blackCalculatorNear(t, pricingMust(c.ThetaPerDay(spot, maturity)), theta/365)
						blackCalculatorNear(t, pricingMust(c.Vega(maturity)), discount*cash*sign*density*(math.Log(strike/forward)/(deviation*deviation)-.5)*math.Sqrt(maturity))
						blackCalculatorNear(t, pricingMust(c.Rho(maturity)), maturity*(forward*delta-value))
						blackCalculatorNear(t, pricingMust(c.DividendRho(maturity)), -maturity*forward*delta)
						blackCalculatorNear(t, pricingMust(c.ITMCashProbability()), cdf(d2))
						blackCalculatorNear(t, pricingMust(c.ITMAssetProbability()), cdf(d1))
						blackCalculatorNear(t, pricingMust(c.StrikeSensitivity()), -delta*forward/strike)
						pricingOK(t, c.Close())
					}
				}
			}
		}
	}
}

func TestCashDigitalBlackCalculatorDomainsAndLifecycle(t *testing.T) {
	s := pricingMust(NewSession())
	for parameter := 0; parameter < 5; parameter++ {
		for _, bad := range []float64{math.NaN(), math.Inf(1), math.Inf(-1)} {
			args := []float64{100, 7, 110, .2, .95}
			args[parameter] = bad
			if _, err := s.NewCashDigitalBlackCalculator(Call, args[0], args[1], args[2], args[3], args[4]); err == nil {
				t.Fatal("nonfinite", parameter, bad)
			}
		}
	}
	for _, bad := range []float64{-1, 0, 1e-20, math.Nextafter(1, 2) - 1} {
		if _, err := s.NewCashDigitalBlackCalculator(Call, 100, 7, 110, bad, .95); err == nil {
			t.Fatal("degenerate deviation", bad)
		}
	}
	for _, option := range []OptionType{OptionType(-1), OptionType(2)} {
		if _, err := s.NewCashDigitalBlackCalculator(option, 100, 7, 110, .2, .95); err == nil {
			t.Fatal("invalid option")
		}
	}
	for _, args := range [][5]float64{{-1, 7, 110, .2, .95}, {100, 7, 0, .2, .95}, {100, 7, -1, .2, .95}, {100, 7, 110, .2, 0}} {
		if _, err := s.NewCashDigitalBlackCalculator(Call, args[0], args[1], args[2], args[3], args[4]); err == nil {
			t.Fatal("invalid domain")
		}
	}
	epsilon := math.Nextafter(1, 2) - 1
	tiny := pricingMust(s.NewCashDigitalBlackCalculator(Call, 100, 7, 110, math.Nextafter(epsilon, math.Inf(1)), .95))
	blackCalculatorNear(t, pricingMust(tiny.Value()), 6.65)
	pricingOK(t, tiny.Close())
	zero := pricingMust(s.NewCashDigitalBlackCalculator(Call, 0, 7, 110, .2, .95))
	blackCalculatorNear(t, pricingMust(zero.Value()), 6.65)
	for _, query := range []func() (float64, error){zero.GammaForward, zero.StrikeSensitivity, func() (float64, error) { return zero.Gamma(100) }, func() (float64, error) { return zero.Vega(2) }, func() (float64, error) { return zero.Theta(100, 2) }} {
		if _, err := query(); err == nil {
			t.Fatal("nonfinite result")
		}
	}
	blackCalculatorNear(t, pricingMust(zero.Theta(math.NaN(), 0)), 0)
	huge := pricingMust(s.NewCashDigitalBlackCalculator(Call, 100, 1e308, 110, .2, 10))
	if _, err := huge.Value(); err == nil {
		t.Fatal("overflow")
	}
	c := pricingMust(s.NewCashDigitalBlackCalculator(Put, 100, 7, 110, .2, .95))
	var group sync.WaitGroup
	for i := 0; i < 8; i++ {
		group.Add(1)
		go func() {
			defer group.Done()
			if _, err := c.Value(); err != nil {
				t.Error(err)
			}
		}()
	}
	group.Wait()
	pricingOK(t, c.Close())
	if _, err := c.Value(); err == nil {
		t.Fatal("released calculator")
	}
	pricingOK(t, zero.Close())
	pricingOK(t, huge.Close())
	pricingOK(t, s.Close())
	if _, err := s.NewCashDigitalBlackCalculator(Call, 100, 7, 110, .2, .95); err == nil {
		t.Fatal("closed session")
	}
	var missing *Session
	if _, err := missing.NewCashDigitalBlackCalculator(Call, 100, 7, 110, .2, .95); err == nil {
		t.Fatal("nil session")
	}
}

func TestCashDigitalBlackCalculatorNativeQueries(t *testing.T) {
	data, err := os.ReadFile("testdata/cash_digital_black_calculator.tsv")
	if err != nil {
		t.Fatal(err)
	}
	var lines []string
	for _, line := range strings.Split(string(data), "\n") {
		if line != "" && !strings.HasPrefix(line, "#") {
			lines = append(lines, line)
		}
	}
	reader := csv.NewReader(strings.NewReader(strings.Join(lines, "\n")))
	reader.Comma = '\t'
	rows, err := reader.ReadAll()
	if err != nil {
		t.Fatal(err)
	}
	headers := rows[0]
	s := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()) })
	for _, fields := range rows[1:] {
		row := map[string]string{}
		for i, key := range headers {
			row[key] = fields[i]
		}
		t.Run(row["id"], func(t *testing.T) {
			number := func(key string) float64 {
				x, e := strconv.ParseFloat(row[key], 64)
				if e != nil {
					t.Fatal(e)
				}
				return x
			}
			option := Call
			if row["option"] == "1" {
				option = Put
			}
			c, e := s.NewCashDigitalBlackCalculator(option, number("strike"), number("cash_payoff"), number("forward"), number("std_dev"), number("discount"))
			if row["domain"] == "diagnostic_rejected" {
				if e == nil {
					t.Fatal("degenerate native diagnostic accepted")
				}
				return
			}
			if e != nil {
				t.Fatal(e)
			}
			defer func() { pricingOK(t, c.Close()) }()
			spot, maturity := number("spot"), number("maturity")
			queries := map[string]func() (float64, error){
				"value": c.Value, "alpha": c.Alpha, "beta": c.Beta, "delta_forward": c.DeltaForward,
				"delta":              func() (float64, error) { return c.Delta(spot) },
				"elasticity_forward": c.ElasticityForward, "elasticity": func() (float64, error) { return c.Elasticity(spot) },
				"gamma_forward": c.GammaForward, "gamma": func() (float64, error) { return c.Gamma(spot) },
				"theta": func() (float64, error) { return c.Theta(spot, maturity) }, "theta_per_day": func() (float64, error) { return c.ThetaPerDay(spot, maturity) },
				"vega": func() (float64, error) { return c.Vega(maturity) }, "rho": func() (float64, error) { return c.Rho(maturity) },
				"dividend_rho": func() (float64, error) { return c.DividendRho(maturity) }, "strike_sensitivity": c.StrikeSensitivity,
				"itm_cash_probability": c.ITMCashProbability, "itm_asset_probability": c.ITMAssetProbability,
			}
			for name, query := range queries {
				actual, e := query()
				expected := row[name]
				if expected == "nan" || expected == "+inf" || expected == "-inf" || expected == "error" {
					if e == nil {
						t.Fatal("native nonfinite result accepted", name)
					}
					continue
				}
				if e != nil {
					t.Fatal(name, e)
				}
				blackCalculatorNear(t, actual, number(name))
			}
		})
	}
}
