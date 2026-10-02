package itofin

import (
	"errors"
	"math"
	"reflect"
	"testing"
)

type batesMarket struct {
	s              *Session
	settings       *Settings
	dc             *DayCounter
	today, expiry  Date
	quotes         [3]*SimpleQuote
	risk, dividend *YieldTermStructure
	process        *BatesProcess
	model          *BatesModel
	engine         *BatesEngine
	option         *VanillaOption
	cfg            BatesProcessConfig
}

func newBatesMarket(t *testing.T) *batesMarket {
	t.Helper()
	m := &batesMarket{s: pricingMust(NewSession())}
	t.Cleanup(func() { pricingOK(t, m.s.Close()) })
	m.today = pricingMust(NewDate(2, 10, 2026))
	m.expiry = pricingMust(m.today.AddDays(365))
	m.dc = pricingMust(m.s.Actual365Fixed())
	m.settings = pricingMust(m.s.NewSettings())
	pricingOK(t, m.settings.SetEvaluationDate(m.today))
	for i, v := range [3]float64{100, .05, .02} {
		m.quotes[i] = pricingMust(m.s.NewSimpleQuote(v))
	}
	m.risk = pricingMust(m.s.NewFlatForwardFromQuote(m.today, m.quotes[1], m.dc))
	m.dividend = pricingMust(m.s.NewFlatForwardFromQuote(m.today, m.quotes[2], m.dc))
	m.cfg = BatesProcessConfig{Spot: m.quotes[0], RiskFree: m.risk, Dividend: m.dividend, V0: .04, Kappa: 1.5, Theta: .04, Sigma: .3, Rho: -.7, Lambda: .5, Nu: -.1, Delta: .2}
	m.process = pricingMust(m.s.NewBatesProcess(m.cfg))
	m.model = pricingMust(m.s.NewBatesModel(m.process))
	m.engine = pricingMust(m.s.NewBatesEngine(m.model, 144))
	m.option = pricingMust(m.s.NewVanillaOption(Call, 100, m.expiry, m.settings))
	pricingOK(t, m.option.SetBatesEngine(m.engine))
	return m
}

func TestBatesParametersLiveInputsAndRetainedGraph(t *testing.T) {
	m := newBatesMarket(t)
	baseline := pricingMust(m.option.NPV())
	if baseline <= 0 || math.IsNaN(baseline) {
		t.Fatal("invalid price")
	}
	pricingNear(t, pricingMust(m.option.PriceBates(m.engine)), baseline, 0)
	getters := []func() (float64, error){m.process.V0, m.process.Kappa, m.process.Theta, m.process.Sigma, m.process.Rho, m.process.Lambda, m.process.Nu, m.process.Delta, m.model.V0, m.model.Kappa, m.model.Theta, m.model.Sigma, m.model.Rho, m.model.Lambda, m.model.Nu, m.model.Delta}
	for i, get := range getters {
		pricingNear(t, pricingMust(get()), []float64{.04, 1.5, .04, .3, -.7, .5, -.1, .2}[i%8], 0)
	}
	pricingNear(t, pricingMust(m.process.Spot()), 100, 0)
	pricingNear(t, pricingMust(m.process.Time(m.expiry)), 1, 0)
	initial := pricingMust(m.process.InitialValues())
	if initial != [2]float64{100, .04} {
		t.Fatal(initial)
	}
	initial[0] = 7
	if pricingMust(m.process.InitialValues())[0] != 100 {
		t.Fatal("initial values aliased native state")
	}
	params := pricingMust(m.model.Params())
	want := []float64{.04, 1.5, .3, -.7, .04, -.1, .2, .5}
	if !reflect.DeepEqual(params, want) {
		t.Fatal(params)
	}
	params[7] = .9
	pricingOK(t, m.model.SetParams(params))
	params[7] = 5
	pricingNear(t, pricingMust(m.model.Lambda()), .9, 0)
	if math.Abs(pricingMust(m.option.NPV())-baseline) < 1e-6 {
		t.Fatal("model update ignored")
	}
	pricingOK(t, m.model.SetParams(want))
	pricingNear(t, pricingMust(m.option.NPV()), baseline, 0)
	for i, pair := range [][2]float64{{100, 105}, {.05, .06}, {.02, .03}} {
		pricingOK(t, m.quotes[i].SetValue(pair[1]))
		if pricingMust(m.option.IsCalculated()) {
			t.Fatal("live market update ignored")
		}
		if math.Abs(pricingMust(m.option.NPV())-baseline) < 1e-6 {
			t.Fatal("price did not change")
		}
		pricingOK(t, m.quotes[i].SetValue(pair[0]))
		pricingNear(t, pricingMust(m.option.NPV()), baseline, 0)
	}
	for _, q := range m.quotes {
		pricingOK(t, q.Close())
	}
	pricingOK(t, m.risk.Close())
	pricingOK(t, m.dividend.Close())
	pricingOK(t, m.process.Close())
	pricingOK(t, m.model.Close())
	pricingOK(t, m.engine.Close())
	pricingOK(t, m.option.Calculate())
	pricingNear(t, pricingMust(m.option.NPV()), baseline, 0)
	if _, err := m.process.V0(); err == nil {
		t.Fatal("closed process accepted")
	}
	if _, err := m.model.Params(); err == nil {
		t.Fatal("closed model accepted")
	}
	if _, err := m.option.PriceBates(m.engine); err == nil {
		t.Fatal("closed engine accepted")
	}
}

func TestBatesBoundariesAndAtomicParameterFailure(t *testing.T) {
	m := newBatesMarket(t)
	other := pricingMust(NewSession())
	defer other.Close()
	if _, err := other.NewBatesProcess(m.cfg); !errors.Is(err, ErrSessionMismatch) {
		t.Fatal(err)
	}
	if _, err := other.NewBatesModel(m.process); !errors.Is(err, ErrSessionMismatch) {
		t.Fatal(err)
	}
	if _, err := other.NewBatesEngine(m.model, 144); !errors.Is(err, ErrSessionMismatch) {
		t.Fatal(err)
	}
	cfg := m.cfg
	cfg.Spot = nil
	if _, err := m.s.NewBatesProcess(cfg); err == nil {
		t.Fatal("nil spot accepted")
	}
	cfg = m.cfg
	cfg.RiskFree = nil
	if _, err := m.s.NewBatesProcess(cfg); err == nil {
		t.Fatal("nil curve accepted")
	}
	for _, change := range []func(*BatesProcessConfig){
		func(c *BatesProcessConfig) { c.V0 = 0 }, func(c *BatesProcessConfig) { c.Kappa = -1 },
		func(c *BatesProcessConfig) { c.Theta = math.NaN() }, func(c *BatesProcessConfig) { c.Sigma = math.Inf(1) },
		func(c *BatesProcessConfig) { c.Rho = 1.01 }, func(c *BatesProcessConfig) { c.Lambda = -1 },
		func(c *BatesProcessConfig) { c.Nu = 1000 }, func(c *BatesProcessConfig) { c.Delta = -1 },
	} {
		config := m.cfg
		change(&config)
		if _, err := m.s.NewBatesProcess(config); err == nil {
			t.Fatal("invalid constructor parameters accepted")
		}
	}
	for _, order := range []uint{0, 193} {
		if _, err := m.s.NewBatesEngine(m.model, order); err == nil {
			t.Fatal("invalid order accepted")
		}
	}
	var process *BatesProcess
	var model *BatesModel
	for _, get := range []func() (float64, error){process.V0, model.V0, (&BatesProcess{}).Spot, (&BatesModel{}).Lambda} {
		if _, err := get(); err == nil {
			t.Fatal("nil or zero object accepted")
		}
	}
	if _, err := process.InitialValues(); err == nil {
		t.Fatal("nil initial values")
	}
	if _, err := process.Time(m.expiry); err == nil {
		t.Fatal("nil time")
	}
	if _, err := model.Params(); err == nil {
		t.Fatal("nil params")
	}
	if err := model.SetParams(nil); err == nil {
		t.Fatal("nil model")
	}
	if _, err := m.s.NewBatesModel(nil); err == nil {
		t.Fatal("nil process")
	}
	if _, err := m.s.NewBatesEngine(nil, 144); err == nil {
		t.Fatal("nil model")
	}
	if err := m.option.SetBatesEngine(nil); err == nil {
		t.Fatal("nil engine")
	}
	if _, err := m.option.PriceBates(nil); err == nil {
		t.Fatal("nil engine")
	}
	baseline := pricingMust(m.option.NPV())
	want := pricingMust(m.model.Params())
	for field, bad := range []float64{0, -1, math.NaN(), 1.01, math.Inf(1), 1000, -.1, -.1} {
		params := append([]float64(nil), want...)
		params[field] = bad
		if err := m.model.SetParams(params); err == nil {
			t.Fatalf("invalid field %d accepted", field)
		}
		if !reflect.DeepEqual(pricingMust(m.model.Params()), want) {
			t.Fatal("partial update")
		}
	}
	for _, params := range [][]float64{nil, want[:7], append(append([]float64(nil), want...), 1)} {
		if err := m.model.SetParams(params); err == nil {
			t.Fatal("invalid array length accepted")
		}
	}
	if _, err := m.process.Time(Date{}); err == nil {
		t.Fatal("null date accepted")
	}
	for _, bad := range []float64{0, -1, math.NaN(), math.Inf(1)} {
		pricingOK(t, m.quotes[0].SetValue(bad))
		if _, err := m.process.InitialValues(); err == nil {
			t.Fatal("invalid live spot")
		}
		if _, err := m.option.NPV(); err == nil {
			t.Fatal("invalid live market priced")
		}
		pricingOK(t, m.quotes[0].SetValue(100))
		pricingNear(t, pricingMust(m.option.NPV()), baseline, 0)
	}
	if _, err := m.option.Delta(); err == nil {
		t.Fatal("fabricated Greek")
	}
	cfg = m.cfg
	cfg.Lambda = 0
	cfg.Delta = 0
	if _, err := m.s.NewBatesProcess(cfg); err != nil {
		t.Fatal("inclusive jump boundary rejected", err)
	}
	pricingOK(t, m.s.Close())
	if _, err := m.model.Params(); !errors.Is(err, ErrClosed) {
		t.Fatal(err)
	}
}
