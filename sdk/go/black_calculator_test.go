package itofin

import (
	"math"
	"sync"
	"testing"
)

func blackCalculatorNear(t *testing.T, got, want float64) {
	t.Helper()
	if math.IsNaN(got) || math.Abs(got-want) > 2e-11*math.Max(1, math.Abs(want)) {
		t.Fatalf("%.17g != %.17g", got, want)
	}
}

func TestBlackCalculatorAnalytic(t *testing.T) {
	s := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()) })
	cdf := func(x float64) float64 { return math.Erfc(-x/math.Sqrt2) / 2 }
	for _, option := range []OptionType{Call, Put} {
		sign := 1.
		if option == Put {
			sign = -1
		}
		for _, forward := range []float64{90, 110} {
			for _, strike := range []float64{80, 100, 120} {
				for _, deviation := range []float64{.15, .5, 1.2} {
					discount, spot, maturity := .94, 97., 2.
					d1 := math.Log(forward/strike)/deviation + deviation/2
					d2 := d1 - deviation
					density := math.Exp(-d1*d1/2) / math.Sqrt(2*math.Pi)
					alpha, beta := sign*cdf(sign*d1), -sign*cdf(sign*d2)
					value := discount * (forward*alpha + strike*beta)
					delta, gamma := discount*alpha, discount*density/(forward*deviation)
					r := -math.Log(discount) / maturity
					q := r - math.Log(forward/spot)/maturity
					theta := -discount*forward*density*deviation/(2*maturity) + q*discount*forward*alpha + r*discount*strike*beta
					c := pricingMust(s.NewBlackCalculator(option, strike, forward, deviation, discount))
					blackCalculatorNear(t, pricingMust(c.Value()), value)
					blackCalculatorNear(t, pricingMust(c.Alpha()), alpha)
					blackCalculatorNear(t, pricingMust(c.Beta()), beta)
					blackCalculatorNear(t, pricingMust(c.DeltaForward()), delta)
					blackCalculatorNear(t, pricingMust(c.Delta(spot)), delta*forward/spot)
					blackCalculatorNear(t, pricingMust(c.GammaForward()), gamma)
					blackCalculatorNear(t, pricingMust(c.Gamma(spot)), gamma*math.Pow(forward/spot, 2))
					blackCalculatorNear(t, pricingMust(c.ElasticityForward()), delta*forward/value)
					blackCalculatorNear(t, pricingMust(c.Elasticity(spot)), delta*forward/value)
					blackCalculatorNear(t, pricingMust(c.Theta(spot, maturity)), theta)
					blackCalculatorNear(t, pricingMust(c.ThetaPerDay(spot, maturity)), theta/365)
					blackCalculatorNear(t, pricingMust(c.Vega(maturity)), discount*forward*density*math.Sqrt(maturity))
					blackCalculatorNear(t, pricingMust(c.Rho(maturity)), -maturity*discount*strike*beta)
					blackCalculatorNear(t, pricingMust(c.DividendRho(maturity)), -maturity*discount*forward*alpha)
					blackCalculatorNear(t, pricingMust(c.ITMCashProbability()), cdf(d2))
					blackCalculatorNear(t, pricingMust(c.ITMAssetProbability()), cdf(d1))
					blackCalculatorNear(t, pricingMust(c.StrikeSensitivity()), discount*beta)
					pricingOK(t, c.Close())
				}
			}
		}
	}
}

func TestBlackCalculatorDomainsAndLifecycle(t *testing.T) {
	s := pricingMust(NewSession())
	for parameter := 0; parameter < 4; parameter++ {
		for _, bad := range []float64{math.NaN(), math.Inf(1), math.Inf(-1), -1} {
			args := []float64{100, 110, .2, .95}
			args[parameter] = bad
			if _, err := s.NewBlackCalculator(Call, args[0], args[1], args[2], args[3]); err == nil {
				t.Fatal("invalid constructor", parameter, bad)
			}
		}
	}
	for _, option := range []OptionType{OptionType(-1), OptionType(2)} {
		if _, err := s.NewBlackCalculator(option, 100, 110, .2, .95); err == nil {
			t.Fatal("invalid option")
		}
	}
	for _, args := range [][4]float64{{100, 0, .2, .95}, {100, 110, .2, 0}} {
		if _, err := s.NewBlackCalculator(Call, args[0], args[1], args[2], args[3]); err == nil {
			t.Fatal("zero forward/discount")
		}
	}
	c := pricingMust(s.NewBlackCalculator(Put, 100, 110, 0, .95))
	blackCalculatorNear(t, pricingMust(c.DeltaForward()), 0)
	blackCalculatorNear(t, pricingMust(c.StrikeSensitivity()), 0)
	blackCalculatorNear(t, pricingMust(c.Theta(math.NaN(), 0)), 0)
	for _, bad := range []float64{0, -1, math.NaN(), math.Inf(1)} {
		if _, err := c.Delta(bad); err == nil {
			t.Fatal("invalid spot")
		}
		if _, err := c.Gamma(bad); err == nil {
			t.Fatal("invalid spot")
		}
		if _, err := c.Elasticity(bad); err == nil {
			t.Fatal("invalid spot")
		}
	}
	for _, bad := range []float64{-1, math.NaN(), math.Inf(1)} {
		if _, err := c.Vega(bad); err == nil {
			t.Fatal("invalid maturity")
		}
		if _, err := c.Rho(bad); err == nil {
			t.Fatal("invalid maturity")
		}
		if _, err := c.DividendRho(bad); err == nil {
			t.Fatal("invalid maturity")
		}
		if _, err := c.Theta(100, bad); err == nil {
			t.Fatal("invalid maturity")
		}
	}
	zero := pricingMust(s.NewBlackCalculator(Call, 0, 100, .2, .95))
	blackCalculatorNear(t, pricingMust(zero.Value()), 95)
	for _, query := range []func() (float64, error){zero.GammaForward, zero.StrikeSensitivity, func() (float64, error) { return zero.Vega(2) }, func() (float64, error) { return zero.Gamma(100) }, func() (float64, error) { return zero.Theta(100, 2) }, func() (float64, error) { return zero.ThetaPerDay(100, 2) }} {
		if _, err := query(); err == nil {
			t.Fatal("nonfinite result accepted")
		}
	}
	huge := pricingMust(s.NewBlackCalculator(Call, 0, 1e308, 0, 10))
	if _, err := huge.Value(); err == nil {
		t.Fatal("overflow result accepted")
	}
	var absent *BlackCalculator
	if _, err := absent.Value(); err == nil {
		t.Fatal("nil receiver")
	}
	var empty BlackCalculator
	if _, err := empty.Value(); err == nil {
		t.Fatal("zero receiver")
	}
	var wg sync.WaitGroup
	for i := 0; i < 8; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			if _, err := c.Value(); err != nil {
				t.Error(err)
			}
		}()
	}
	wg.Wait()
	pricingOK(t, c.Close())
	if _, err := c.Value(); err == nil {
		t.Fatal("closed calculator")
	}
	pricingOK(t, zero.Close())
	pricingOK(t, huge.Close())
	pricingOK(t, s.Close())
	pricingOK(t, s.Close())
	if _, err := s.NewBlackCalculator(Call, 100, 100, .2, 1); err == nil {
		t.Fatal("closed session constructor")
	}
	if _, err := zero.Value(); err == nil {
		t.Fatal("closed session query")
	}
	var absentSession *Session
	if _, err := absentSession.NewBlackCalculator(Call, 100, 100, .2, 1); err == nil {
		t.Fatal("nil session")
	}
}
