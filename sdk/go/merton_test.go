package itofin

import (
	"errors"
	"math"
	"testing"
)

type mertonMarket struct {
	s              *Session
	dc             *DayCounter
	settings       *Settings
	today, expiry  Date
	quotes         [7]*SimpleQuote
	risk, dividend *YieldTermStructure
	vol            *BlackVolTermStructure
	process        *Merton76Process
	engine         *JumpDiffusionEngine
	option         *VanillaOption
}

func newMertonMarket(t *testing.T) *mertonMarket {
	t.Helper()
	m := &mertonMarket{s: pricingMust(NewSession())}
	t.Cleanup(func() { pricingOK(t, m.s.Close()) })
	m.today = pricingMust(NewDate(2, 10, 2026))
	m.expiry = pricingMust(m.today.AddDays(365))
	m.dc = pricingMust(m.s.Actual365Fixed())
	m.settings = pricingMust(m.s.NewSettings())
	pricingOK(t, m.settings.SetEvaluationDate(m.today))
	for i, v := range [7]float64{100, .05, .02, .2, 1, -.1, .3} {
		m.quotes[i] = pricingMust(m.s.NewSimpleQuote(v))
	}
	m.risk = pricingMust(m.s.NewFlatForwardFromQuote(m.today, m.quotes[1], m.dc))
	m.dividend = pricingMust(m.s.NewFlatForwardFromQuote(m.today, m.quotes[2], m.dc))
	m.vol = pricingMust(m.s.NewBlackConstantVolFromQuote(m.today, m.quotes[3], m.dc, nil))
	m.process = pricingMust(m.s.NewMerton76Process(m.quotes[0], m.risk, m.dividend, m.vol, m.quotes[4], m.quotes[5], m.quotes[6]))
	m.engine = pricingMust(m.s.NewJumpDiffusionEngine(m.process, 1e-12, 4096))
	m.option = pricingMust(m.s.NewVanillaOption(Call, 100, m.expiry, m.settings))
	pricingOK(t, m.option.SetJumpDiffusionEngine(m.engine))
	return m
}

func TestMertonOracleAndAllLiveInputs(t *testing.T) {
	m := newMertonMarket(t)
	baseline := pricingMust(m.option.NPV())
	pricingNear(t, baseline, 14.586956184779012, 1e-9)
	pricingNear(t, pricingMust(m.option.PriceJumpDiffusion(m.engine)), baseline, 0)
	for _, test := range []struct {
		get  func() (float64, error)
		want float64
	}{
		{m.process.Spot, 100}, {m.process.JumpIntensity, 1}, {m.process.LogMeanJump, -.1}, {m.process.LogJumpVolatility, .3},
		{m.option.Delta, .6175760317781934}, {m.option.Gamma, .012393297925041149}, {m.option.Theta, -8.239240639178727},
		{m.option.Vega, 24.78659585008232}, {m.option.Rho, 47.17064699304032}, {m.option.DividendRho, -61.75760317781934},
	} {
		pricingNear(t, pricingMust(test.get()), test.want, 1e-9)
	}
	pricingNear(t, pricingMust(m.process.Time(m.expiry)), 1, 0)
	for i, pair := range [][2]float64{{100, 105}, {.05, .06}, {.02, .03}, {.2, .25}, {1, 1.3}, {-.1, -.2}, {.3, .4}} {
		pricingOK(t, m.quotes[i].SetValue(pair[1]))
		if pricingMust(m.option.IsCalculated()) {
			t.Fatal("live quote did not invalidate option")
		}
		if math.Abs(pricingMust(m.option.NPV())-baseline) < 1e-7 {
			t.Fatal("live quote did not change price")
		}
		pricingOK(t, m.quotes[i].SetValue(pair[0]))
		pricingNear(t, pricingMust(m.option.NPV()), baseline, 0)
	}
	for _, q := range m.quotes {
		pricingOK(t, q.Close())
	}
	pricingOK(t, m.risk.Close())
	pricingOK(t, m.dividend.Close())
	pricingOK(t, m.vol.Close())
	pricingOK(t, m.process.Close())
	pricingOK(t, m.engine.Close())
	pricingOK(t, m.option.Calculate())
	pricingNear(t, pricingMust(m.option.NPV()), baseline, 0)
	if _, err := m.process.Spot(); err == nil {
		t.Fatal("closed process accepted")
	}
}

func TestMertonBoundariesAndFailureRecovery(t *testing.T) {
	m := newMertonMarket(t)
	other := pricingMust(NewSession())
	defer other.Close()
	if _, err := other.NewMerton76Process(m.quotes[0], m.risk, m.dividend, m.vol, m.quotes[4], m.quotes[5], m.quotes[6]); !errors.Is(err, ErrSessionMismatch) {
		t.Fatalf("cross session: %v", err)
	}
	if _, err := m.s.NewMerton76Process(nil, m.risk, m.dividend, m.vol, m.quotes[4], m.quotes[5], m.quotes[6]); err == nil {
		t.Fatal("nil spot accepted")
	}
	if _, err := m.s.NewMerton76Process(m.quotes[0], m.risk, m.dividend, nil, m.quotes[4], m.quotes[5], m.quotes[6]); err == nil {
		t.Fatal("nil volatility accepted")
	}
	if _, err := other.NewJumpDiffusionEngine(m.process, 1e-4, 100); !errors.Is(err, ErrSessionMismatch) {
		t.Fatalf("cross session engine: %v", err)
	}
	for _, cfg := range []struct {
		accuracy   float64
		iterations int
	}{{0, 100}, {math.NaN(), 100}, {math.Inf(1), 100}, {1e-4, -1}, {1e-4, 0}, {1e-4, 100001}} {
		if _, err := m.s.NewJumpDiffusionEngine(m.process, cfg.accuracy, cfg.iterations); err == nil {
			t.Fatal("invalid engine configuration accepted")
		}
	}
	if _, err := m.s.NewBlackConstantVolFromQuote(m.today, nil, m.dc, nil); err == nil {
		t.Fatal("nil quote accepted")
	}
	if _, err := other.NewBlackConstantVolFromQuote(m.today, m.quotes[3], m.dc, nil); !errors.Is(err, ErrSessionMismatch) {
		t.Fatalf("cross session volatility: %v", err)
	}
	var nilProcess *Merton76Process
	if _, err := nilProcess.Spot(); err == nil {
		t.Fatal("nil process accepted")
	}
	if _, err := (&Merton76Process{}).Spot(); err == nil {
		t.Fatal("zero process accepted")
	}
	if _, err := m.option.PriceJumpDiffusion(nil); err == nil {
		t.Fatal("nil engine accepted")
	}
	baseline := pricingMust(m.option.NPV())
	for _, bad := range []struct {
		index          int
		value, restore float64
	}{{0, 0, 100}, {4, -1, 1}, {5, math.NaN(), -.1}, {6, -.1, .3}, {3, math.Inf(1), .2}} {
		pricingOK(t, m.quotes[bad.index].SetValue(bad.value))
		if _, err := m.option.NPV(); err == nil {
			t.Fatal("invalid live quote accepted")
		}
		pricingOK(t, m.quotes[bad.index].SetValue(bad.restore))
		pricingNear(t, pricingMust(m.option.NPV()), baseline, 0)
	}
	short := pricingMust(m.s.NewJumpDiffusionEngine(m.process, 1e-14, 1))
	if _, err := m.option.PriceJumpDiffusion(short); err == nil {
		t.Fatal("unconverged series accepted")
	}
	pricingNear(t, pricingMust(m.option.PriceJumpDiffusion(m.engine)), baseline, 0)
	american := pricingMust(m.s.NewAmericanOption(Put, 100, m.today, m.expiry, m.settings))
	if _, err := american.PriceJumpDiffusion(m.engine); err == nil {
		t.Fatal("American exercise accepted")
	}
	pricingOK(t, m.s.Close())
	if _, err := m.process.Spot(); !errors.Is(err, ErrClosed) {
		t.Fatalf("closed session: %v", err)
	}
}

func TestMertonPutZeroJumpAndDeterministicLimits(t *testing.T) {
	m := newMertonMarket(t)
	put := pricingMust(m.s.NewVanillaOption(Put, 100, m.expiry, m.settings))
	pricingNear(t, pricingMust(put.PriceJumpDiffusion(m.engine)), 11.690031304174994, 1e-9)
	pricingOK(t, m.quotes[4].SetValue(0))
	bs := pricingMust(m.s.NewBlackScholesProcess(BlackScholesConfig{Spot: 100, RiskFreeRate: .05, DividendYield: .02, Volatility: .2, ReferenceDate: m.today, DayCounter: m.dc}))
	analytic := pricingMust(m.s.NewVanillaOption(Call, 100, m.expiry, m.settings))
	pricingNear(t, pricingMust(m.option.NPV()), pricingMust(analytic.Price(bs)), 1e-12)
	pricingOK(t, m.quotes[3].SetValue(0))
	pricingNear(t, pricingMust(m.option.NPV()), 100*math.Exp(-.02)-100*math.Exp(-.05), 1e-12)
	for _, value := range []float64{-1, math.NaN(), math.Inf(1)} {
		bad := pricingMust(m.s.NewSimpleQuote(value))
		if _, err := m.s.NewBlackConstantVolFromQuote(m.today, bad, m.dc, nil); err == nil {
			t.Fatal("invalid volatility quote accepted")
		}
		pricingOK(t, bad.Close())
	}
}
