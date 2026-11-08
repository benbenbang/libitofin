package itofin

import (
	"math"
	"strings"
	"testing"
)

type localVolMarket struct {
	s                  *Session
	today              Date
	dc                 *DayCounter
	settings           *Settings
	spot               *SimpleQuote
	riskFree, dividend *YieldTermStructure
}

func newLocalVolMarket(t *testing.T) *localVolMarket {
	s := pricingMust(NewSession())
	t.Cleanup(func() { s.Close() })
	m := &localVolMarket{s: s, today: pricingMust(NewDate(15, 6, 2026)), dc: pricingMust(s.Actual360())}
	m.settings = pricingMust(s.NewSettings())
	pricingOK(t, m.settings.SetEvaluationDate(m.today))
	m.spot = pricingMust(s.NewSimpleQuote(100))
	m.riskFree = pricingMust(s.NewFlatForward(m.today, .05, m.dc))
	m.dividend = pricingMust(s.NewFlatForward(m.today, .02, m.dc))
	return m
}

func (m *localVolMarket) dates(days ...int64) []Date {
	out := make([]Date, len(days))
	for i, d := range days {
		out[i] = pricingMust(m.today.AddDays(d))
	}
	return out
}

func (m *localVolMarket) curve(vols []float64, monotone bool, days ...int64) BlackVarianceCurveConfig {
	return BlackVarianceCurveConfig{ReferenceDate: m.today, Dates: m.dates(days...), Volatilities: vols, DayCounter: m.dc, ForceMonotoneVariance: monotone}
}

func (m *localVolMarket) smile() *BlackVolTermStructure {
	var strikes []float64
	var vols [][]float64
	for k := -7; k <= 7; k++ {
		x := 0.2 * float64(k)
		v := .2 - .1*x + .05*x*x
		strikes, vols = append(strikes, 100*math.Exp(x)), append(vols, []float64{v, v, v})
	}
	return pricingMust(m.s.BlackVarianceSurface(BlackVarianceSurfaceConfig{ReferenceDate: m.today, Dates: m.dates(60, 120, 240), Strikes: strikes, Volatilities: vols, DayCounter: m.dc}))
}

func (m *localVolMarket) process(black *BlackVolTermStructure) (*LocalVolTermStructure, *BlackScholesProcess) {
	local := pricingMust(m.s.NewLocalVolSurface(black, m.riskFree, m.dividend, m.spot))
	return local, pricingMust(m.s.NewBlackScholesProcessWithLocalVol(m.spot, m.riskFree, m.dividend, black, local))
}

func (m *localVolMarket) option(kind OptionType, strike float64, days int64) *VanillaOption {
	return pricingMust(m.s.NewVanillaOption(kind, strike, pricingMust(m.today.AddDays(days)), m.settings))
}

func (m *localVolMarket) mc(p *BlackScholesProcess, kind OptionType, strike float64, days int64) (*VanillaOption, float64, float64, error) {
	o := m.option(kind, strike, days)
	cfg := MCConfig{StepsPerYear: pricingPtr(uint(12)), Samples: pricingPtr(uint(20000)), Seed: pricingPtr(uint32(42))}
	if err := o.SetMCEngine(pricingMust(m.s.NewMCEuropeanEngine(p, cfg))); err != nil {
		return nil, 0, 0, err
	}
	npv, err := o.NPV()
	if err != nil {
		return nil, 0, 0, err
	}
	return o, npv, pricingMust(o.ErrorEstimate()), nil
}

func localVolBlack(call bool, strike, variance float64) float64 {
	forward, sd := 100*math.Exp(.03), math.Sqrt(variance)
	d1 := math.Log(forward/strike)/sd + sd/2
	n := func(x float64) float64 { return .5 * math.Erfc(-x/math.Sqrt2) }
	if call {
		return math.Exp(-.05) * (forward*n(d1) - strike*n(d1-sd))
	}
	return math.Exp(-.05) * (strike*n(sd-d1) - forward*n(-d1))
}

func TestLocalVolTimeOnlyMatchesAnalytic(t *testing.T) {
	m := newLocalVolMarket(t)
	black := pricingMust(m.s.BlackVarianceCurve(m.curve([]float64{.15, .2, .25}, true, 119, 269, 360)))
	local, p := m.process(black)
	var vEuler float64
	for i := 0; i < 12; i++ {
		v := pricingMust(local.LocalVol(float64(i)/12, 100, true))
		vEuler += v * v / 12
	}
	for _, kind := range []OptionType{Call, Put} {
		for _, strike := range []float64{90, 100, 110} {
			analytic := pricingMust(m.option(kind, strike, 360).Price(p))
			_, mc, se, err := m.mc(p, kind, strike, 360)
			pricingOK(t, err)
			euler := localVolBlack(kind == Call, strike, vEuler)
			budget := math.Abs(euler - analytic)
			if se <= 0 || math.Abs(mc-euler) >= 3*se || budget >= .03 || math.Abs(mc-analytic) >= 3*se+budget {
				t.Fatalf("%v K=%g: mc %g euler %g analytic %g se %g", kind, strike, mc, euler, analytic, se)
			}
		}
	}
}

func TestLocalVolSmileMovesPrice(t *testing.T) {
	m := newLocalVolMarket(t)
	_, p := m.process(m.smile())
	flat := pricingMust(m.s.NewBlackScholesProcess(BlackScholesConfig{Spot: 100, RiskFreeRate: .05, DividendYield: .02, Volatility: .2, ReferenceDate: m.today, DayCounter: m.dc}))
	for _, c := range []struct {
		kind         OptionType
		strike, sign float64
	}{{Call, 115, -1}, {Put, 85, 1}} {
		base := pricingMust(m.option(c.kind, c.strike, 180).Price(flat))
		_, mc, se, err := m.mc(p, c.kind, c.strike, 180)
		pricingOK(t, err)
		implied := pricingMust(m.option(c.kind, c.strike, 180).Price(p))
		if c.sign*(mc-base) <= 5*se || math.Abs(mc-implied) >= 3*se+.01 {
			t.Fatalf("K=%g: mc %g flat %g implied %g se %g", c.strike, mc, base, implied, se)
		}
	}
}

func TestLocalVolQuoteRepricesAndHandlesOutliveCallers(t *testing.T) {
	m := newLocalVolMarket(t)
	black := pricingMust(m.s.BlackVarianceCurve(m.curve([]float64{.15, .2, .25}, true, 119, 269, 360)))
	local, p := m.process(black)
	o, before, _, err := m.mc(p, Call, 100, 360)
	pricingOK(t, err)
	pricingOK(t, m.spot.SetValue(105))
	if calculated := pricingMust(o.IsCalculated()); calculated {
		t.Fatal("quote change did not invalidate the option")
	}
	after := pricingMust(o.NPV())
	if after <= before {
		t.Fatalf("spot bump: %g -> %g", before, after)
	}
	for _, c := range []interface{ Close() error }{black, local, m.spot, m.riskFree, m.dividend} {
		pricingOK(t, c.Close())
	}
	if _, again, _, err := m.mc(p, Call, 100, 360); err != nil || again != after {
		t.Fatalf("after release: %g, %v; want %g", again, err, after)
	}
}

func TestLocalVolCoreErrors(t *testing.T) {
	m := newLocalVolMarket(t)
	local, _ := m.process(pricingMust(m.s.BlackVarianceCurve(m.curve([]float64{.15, .2, .25}, true, 119, 269, 360))))
	if _, err := local.LocalVol(1.5, 100, false); err == nil || !strings.Contains(err.Error(), "past max curve time") {
		t.Fatalf("out of range: %v", err)
	}
	_, p := m.process(pricingMust(m.s.BlackVarianceCurve(m.curve([]float64{.3, .1}, false, 102, 252))))
	if _, _, _, err := m.mc(p, Call, 100, 180); err == nil || !strings.Contains(err.Error(), "decreasing variance") {
		t.Fatalf("decreasing variance: %v", err)
	}
	plain := pricingMust(m.s.NewBlackScholesProcessFromQuote(m.spot, m.riskFree, m.dividend, m.smile()))
	if _, _, _, err := m.mc(plain, Call, 100, 180); err == nil || !strings.Contains(err.Error(), "only constant Black volatilities") {
		t.Fatalf("surface without local vol: %v", err)
	}
	if _, err := m.s.NewLocalVolSurface(nil, m.riskFree, m.dividend, m.spot); err == nil {
		t.Fatal("nil black vol accepted")
	}
}

func TestLocalVolCurveQueriesAndPrices(t *testing.T) {
	m := newLocalVolMarket(t)
	cfg := m.curve([]float64{.15, .2, .25}, true, 119, 269, 360)
	black := pricingMust(m.s.BlackVarianceCurve(cfg))
	curve := pricingMust(m.s.NewLocalVolCurve(cfg))
	if v := pricingMust(curve.LocalVol(.1, 100, false)); math.Abs(v-.15) > 1e-12 {
		t.Fatalf("first-segment local vol %g", v)
	}
	if _, err := curve.LocalVol(1.5, 100, false); err == nil || !strings.Contains(err.Error(), "past max curve time") {
		t.Fatalf("out of range: %v", err)
	}
	p := pricingMust(m.s.NewBlackScholesProcessWithLocalVol(m.spot, m.riskFree, m.dividend, black, curve))
	pricingOK(t, curve.Close())
	analytic := pricingMust(m.option(Call, 100, 360).Price(p))
	if _, mc, se, err := m.mc(p, Call, 100, 360); err != nil || math.Abs(mc-analytic) >= 3*se+.03 {
		t.Fatalf("curve mc %g analytic %g se %g: %v", mc, analytic, se, err)
	}
	if _, err := m.s.NewLocalVolCurve(BlackVarianceCurveConfig{}); err == nil {
		t.Fatal("missing day counter accepted")
	}
}
