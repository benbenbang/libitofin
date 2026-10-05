package itofin

import (
	"errors"
	"math"
	"sync"
	"testing"
)

type varianceSwapMarket struct {
	s               *Session
	settings        *Settings
	today, maturity Date
	quotes          [3]*SimpleQuote
	risk, dividend  *YieldTermStructure
	vol             *BlackVolTermStructure
	process         *BlackScholesProcess
	engine          *ReplicatingVarianceSwapEngine
	swap            *VarianceSwap
}

func newVarianceSwapMarket(t *testing.T) *varianceSwapMarket {
	t.Helper()
	m := &varianceSwapMarket{s: pricingMust(NewSession())}
	t.Cleanup(func() { pricingOK(t, m.s.Close()) })
	m.today = pricingMust(NewDate(5, 10, 2026))
	m.maturity = pricingMust(m.today.AddDays(365))
	dc := pricingMust(m.s.Actual365Fixed())
	m.settings = pricingMust(m.s.NewSettings())
	pricingOK(t, m.settings.SetEvaluationDate(m.today))
	for i, v := range []float64{.05, 0, .2} {
		m.quotes[i] = pricingMust(m.s.NewSimpleQuote(v))
	}
	m.risk = pricingMust(m.s.NewFlatForwardFromQuote(m.today, m.quotes[0], dc))
	m.dividend = pricingMust(m.s.NewFlatForwardFromQuote(m.today, m.quotes[1], dc))
	m.vol = pricingMust(m.s.NewBlackConstantVolFromQuote(m.today, m.quotes[2], dc, nil))
	m.process = pricingMust(m.s.NewBlackScholesProcessFromCurves(100, m.risk, m.dividend, m.vol))
	m.engine = pricingMust(m.s.NewReplicatingVarianceSwapEngine(ReplicatingVarianceSwapEngineConfig{m.process, 5, []float64{100, 110, 120}, []float64{80, 90, 100}}))
	m.swap = pricingMust(m.s.NewVarianceSwap(VarianceSwapConfig{PositionLong, .04, 1000, m.today, m.maturity, m.settings}))
	pricingOK(t, m.swap.SetEngine(m.engine))
	return m
}
func varianceSwapNear(t *testing.T, got, want, tolerance float64) {
	t.Helper()
	if math.IsNaN(got) || math.IsInf(got, 0) || math.IsNaN(want) || math.IsInf(want, 0) || math.Abs(got-want) > tolerance {
		t.Fatalf("got %.17g want %.17g tolerance %g", got, want, tolerance)
	}
}
func TestVarianceSwapTermsLiveUpdatesAndRetainedOwners(t *testing.T) {
	m := newVarianceSwapMarket(t)
	if pricingMust(m.swap.Position()) != PositionLong || pricingMust(m.swap.StartDate()) != m.today || pricingMust(m.swap.MaturityDate()) != m.maturity {
		t.Fatal("immutable terms")
	}
	varianceSwapNear(t, pricingMust(m.swap.Strike()), .04, 0)
	varianceSwapNear(t, pricingMust(m.swap.Notional()), 1000, 0)
	if pricingMust(m.swap.IsCalculated()) || pricingMust(m.swap.IsExpired()) {
		t.Fatal("initial lifecycle")
	}
	variance := pricingMust(m.swap.Variance())
	npv := pricingMust(m.swap.NPV())
	varianceSwapNear(t, npv, math.Exp(-.05)*1000*(variance-.04), 1e-12)
	short := pricingMust(m.s.NewVarianceSwap(VarianceSwapConfig{PositionShort, .04, 1000, m.today, m.maturity, m.settings}))
	pricingOK(t, short.SetEngine(m.engine))
	varianceSwapNear(t, pricingMust(short.NPV()), -npv, 0)
	for i, pair := range [][2]float64{{.05, .08}, {0, .02}, {.2, .25}} {
		pricingOK(t, m.quotes[i].SetValue(pair[1]))
		if pricingMust(m.swap.IsCalculated()) {
			t.Fatal("live input did not invalidate")
		}
		if pricingMust(m.swap.Variance()) == variance {
			t.Fatal("live input did not reprice")
		}
		pricingOK(t, m.quotes[i].SetValue(pair[0]))
		varianceSwapNear(t, pricingMust(m.swap.Variance()), variance, 0)
	}
	pricingOK(t, m.quotes[2].SetValue(math.Inf(1)))
	if _, e := m.swap.Variance(); e == nil {
		t.Fatal("nonfinite volatility accepted")
	}
	if pricingMust(m.swap.IsCalculated()) {
		t.Fatal("failed price cached")
	}
	pricingOK(t, m.quotes[2].SetValue(.2))
	varianceSwapNear(t, pricingMust(m.swap.Variance()), variance, 0)
	for _, q := range m.quotes {
		pricingOK(t, q.Close())
	}
	for _, o := range []object{m.risk.object, m.dividend.object, m.vol.object, m.process.object, m.engine.object, m.settings.object} {
		pricingOK(t, o.Close())
	}
	pricingOK(t, m.swap.Recalculate())
	varianceSwapNear(t, pricingMust(m.swap.NPV()), npv, 0)
	if _, e := m.swap.Variance(); e != nil {
		t.Fatal(e)
	}
	pricingOK(t, m.swap.Close())
	if _, e := m.swap.NPV(); e == nil {
		t.Fatal("closed swap accepted")
	}
}
func TestVarianceSwapSnapshotsEngineReplacementAndConcurrency(t *testing.T) {
	m := newVarianceSwapMarket(t)
	w := pricingMust(m.swap.OptionWeights())
	if len(w) != 6 {
		t.Fatal("weight count")
	}
	for i, weight := range w {
		want := Call
		if i >= 3 {
			want = Put
		}
		if weight.OptionType != want {
			t.Fatal("weight order")
		}
		if math.IsNaN(weight.Weight) || math.IsInf(weight.Weight, 0) {
			t.Fatal("nonfinite weight")
		}
	}
	original := w[0]
	w[0].Weight = 91
	again := pricingMust(m.swap.OptionWeights())
	if again[0] != original {
		t.Fatal("aliased weight snapshot")
	}
	calls := []float64{120, 100, 110, 100}
	puts := []float64{100, 90, 80, 100}
	engine := pricingMust(m.s.NewReplicatingVarianceSwapEngine(ReplicatingVarianceSwapEngineConfig{m.process, 5, calls, puts}))
	calls[0] = math.NaN()
	puts[0] = 0
	pricingOK(t, m.swap.SetEngine(engine))
	if len(pricingMust(m.swap.OptionWeights())) != 6 {
		t.Fatal("strip input retained by alias")
	}
	engine2 := pricingMust(m.s.NewReplicatingVarianceSwapEngine(ReplicatingVarianceSwapEngineConfig{m.process, 5, []float64{100, 105, 110, 120}, []float64{90, 100}}))
	pricingOK(t, m.swap.SetEngine(engine2))
	if pricingMust(m.swap.IsCalculated()) {
		t.Fatal("replacement did not invalidate")
	}
	var wg sync.WaitGroup
	for i := 0; i < 8; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for j := 0; j < 10; j++ {
				if _, e := m.swap.OptionWeights(); e != nil {
					t.Error(e)
				}
			}
		}()
	}
	wg.Wait()
	pricingOK(t, m.settings.SetEvaluationDate(pricingMust(m.maturity.AddDays(1))))
	if !pricingMust(m.swap.IsExpired()) {
		t.Fatal("not expired")
	}
	varianceSwapNear(t, pricingMust(m.swap.NPV()), 0, 0)
	if _, e := m.swap.Variance(); e == nil {
		t.Fatal("expired variance provided")
	}
	if _, e := m.swap.OptionWeights(); e == nil {
		t.Fatal("expired weights provided")
	}
}
func TestVarianceSwapInvalidBoundariesAndSessionLifecycle(t *testing.T) {
	m := newVarianceSwapMarket(t)
	other := pricingMust(NewSession())
	defer other.Close()
	cfg := VarianceSwapConfig{PositionLong, .04, 1000, m.today, m.maturity, m.settings}
	if _, e := other.NewVarianceSwap(cfg); !errors.Is(e, ErrSessionMismatch) {
		t.Fatalf("session mismatch %v", e)
	}
	ec := ReplicatingVarianceSwapEngineConfig{m.process, 5, []float64{100, 110}, []float64{90, 100}}
	if _, e := other.NewReplicatingVarianceSwapEngine(ec); !errors.Is(e, ErrSessionMismatch) {
		t.Fatalf("engine session mismatch %v", e)
	}
	for _, dk := range []float64{0, -1, math.NaN(), math.Inf(1), 90} {
		bad := ec
		bad.Dk = dk
		if _, e := m.s.NewReplicatingVarianceSwapEngine(bad); e == nil {
			t.Fatal("bad dk")
		}
	}
	for _, strip := range [][]float64{nil, {100}, {100, 100}, {100, math.NaN()}, {100, math.Inf(1)}, {0, 100}, make([]float64, 4097)} {
		bad := ec
		bad.CallStrikes = strip
		if _, e := m.s.NewReplicatingVarianceSwapEngine(bad); e == nil {
			t.Fatal("bad strip")
		}
	}
	for _, x := range []float64{0, -1, math.NaN(), math.Inf(1)} {
		bad := cfg
		bad.Strike = x
		if _, e := m.s.NewVarianceSwap(bad); e == nil {
			t.Fatal("bad strike")
		}
		bad = cfg
		bad.Notional = x
		if _, e := m.s.NewVarianceSwap(bad); e == nil {
			t.Fatal("bad notional")
		}
	}
	for _, start := range []Date{pricingMust(m.today.AddDays(-1)), pricingMust(m.today.AddDays(1))} {
		bad := cfg
		bad.StartDate = start
		swap := pricingMust(m.s.NewVarianceSwap(bad))
		pricingOK(t, swap.SetEngine(m.engine))
		if _, e := swap.NPV(); e == nil {
			t.Fatal("unsupported start")
		}
	}
	bad := cfg
	bad.Position = Position(99)
	if _, e := m.s.NewVarianceSwap(bad); e == nil {
		t.Fatal("bad position")
	}
	bad = cfg
	bad.MaturityDate = m.today
	if _, e := m.s.NewVarianceSwap(bad); e == nil {
		t.Fatal("bad dates")
	}
	bad = cfg
	bad.Settings = nil
	if _, e := m.s.NewVarianceSwap(bad); e == nil {
		t.Fatal("nil settings")
	}
	ec.Process = nil
	if _, e := m.s.NewReplicatingVarianceSwapEngine(ec); e == nil {
		t.Fatal("nil process")
	}
	if e := m.swap.SetEngine(nil); e == nil {
		t.Fatal("nil engine")
	}
	var nilSwap *VarianceSwap
	if _, e := nilSwap.NPV(); e == nil {
		t.Fatal("nil swap")
	}
	if _, e := nilSwap.OptionWeights(); e == nil {
		t.Fatal("nil snapshot")
	}
	if e := nilSwap.Recalculate(); e == nil {
		t.Fatal("nil recalculate")
	}
	if _, e := (&VarianceSwap{}).Variance(); e == nil {
		t.Fatal("zero swap")
	}
	pricingOK(t, m.engine.Close())
	if e := m.swap.SetEngine(m.engine); e == nil {
		t.Fatal("released engine")
	}
}
