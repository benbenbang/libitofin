package itofin

import (
	"math"
	"sync"
	"testing"
)

func TestMCVarianceSwapStatisticsLiveInputsRetentionAndExpiry(t *testing.T) {
	m := newVarianceSwapMarket(t)
	engine := pricingMust(m.s.NewMCVarianceSwapEngine(m.process, MCVarianceSwapConfig{Steps: 12, Samples: 32, Seed: 42}))
	pricingOK(t, m.swap.SetMCEngine(engine))
	variance := pricingMust(m.swap.Variance())
	varianceSwapNear(t, variance, .04, 1e-14)
	varianceSwapNear(t, pricingMust(m.swap.VarianceError()), 0, 1e-14)
	varianceSwapNear(t, pricingMust(m.swap.ErrorEstimate()), 0, 1e-12)
	if pricingMust(m.swap.Samples()) != 32 || len(pricingMust(m.swap.OptionWeights())) != 0 {
		t.Fatal("MC statistics or replication portfolio")
	}
	for _, update := range []struct {
		index int
		value float64
	}{{0, .08}, {1, .02}, {2, .25}} {
		old := pricingMust(m.quotes[update.index].Value())
		pricingOK(t, m.quotes[update.index].SetValue(update.value))
		if pricingMust(m.swap.IsCalculated()) {
			t.Fatal("live market did not invalidate MC")
		}
		varianceSwapNear(t, pricingMust(m.swap.Variance()), math.Pow(pricingMust(m.quotes[2].Value()), 2), 1e-14)
		pricingOK(t, m.quotes[update.index].SetValue(old))
	}
	varianceSwapNear(t, pricingMust(m.swap.Variance()), variance, 0)
	pricingOK(t, m.quotes[2].SetValue(math.NaN()))
	if _, err := m.swap.VarianceError(); err == nil {
		t.Fatal("invalid market returned stale statistic")
	}
	if pricingMust(m.swap.IsCalculated()) {
		t.Fatal("failed calculation cached")
	}
	pricingOK(t, m.quotes[2].SetValue(.2))
	varianceSwapNear(t, pricingMust(m.swap.Variance()), variance, 0)
	for _, q := range m.quotes {
		pricingOK(t, q.Close())
	}
	for _, o := range []object{m.risk.object, m.dividend.object, m.vol.object, m.process.object, engine.object, m.settings.object} {
		pricingOK(t, o.Close())
	}
	pricingOK(t, m.swap.Recalculate())
	varianceSwapNear(t, pricingMust(m.swap.Variance()), variance, 0)
	if pricingMust(m.swap.Samples()) != 32 {
		t.Fatal("retained samples")
	}
	pricingOK(t, m.swap.Close())
	if _, err := m.swap.Samples(); err == nil {
		t.Fatal("closed swap")
	}
}

func TestMCVarianceSwapEngineReplacementToleranceAndConcurrency(t *testing.T) {
	m := newVarianceSwapMarket(t)
	if _, err := m.swap.Samples(); err == nil {
		t.Fatal("replication sample count")
	}
	if _, err := m.swap.VarianceError(); err == nil {
		t.Fatal("replication sampling error")
	}
	mc := pricingMust(m.s.NewMCVarianceSwapEngine(m.process, MCVarianceSwapConfig{StepsPerYear: 12, AbsoluteTolerance: .01, Seed: 42}))
	pricingOK(t, m.swap.SetMCEngine(mc))
	if pricingMust(m.swap.Samples()) != 1023 {
		t.Fatal("native initial tolerance batch")
	}
	var wg sync.WaitGroup
	for range 8 {
		wg.Go(func() {
			for range 4 {
				if len(pricingMust(m.swap.OptionWeights())) != 0 || pricingMust(m.swap.Samples()) != 1023 {
					t.Error("concurrent MC snapshot")
				}
			}
		})
	}
	wg.Wait()
	pricingOK(t, m.swap.SetEngine(m.engine))
	if len(pricingMust(m.swap.OptionWeights())) != 6 {
		t.Fatal("replication engine replacement")
	}
	if _, err := m.swap.Samples(); err == nil {
		t.Fatal("stale MC count")
	}
	pricingOK(t, m.swap.SetMCEngine(mc))
	pricingOK(t, m.settings.SetEvaluationDate(m.maturity))
	if pricingMust(m.swap.NPV()) != 0 || pricingMust(m.swap.ErrorEstimate()) != 0 {
		t.Fatal("expired NPV")
	}
	if _, err := m.swap.VarianceError(); err == nil {
		t.Fatal("expired variance error")
	}
	if _, err := m.swap.Samples(); err == nil {
		t.Fatal("expired sample count")
	}
}

func TestMCVarianceSwapInvalidConfigurationAndSessions(t *testing.T) {
	m := newVarianceSwapMarket(t)
	invalid := []MCVarianceSwapConfig{
		{}, {Steps: 1}, {Steps: 1, Samples: 1}, {Steps: 1, StepsPerYear: 1, Samples: 2},
		{Steps: 1, Samples: 2, AbsoluteTolerance: .01},
		{Steps: 1, AbsoluteTolerance: math.NaN()}, {Steps: 1, AbsoluteTolerance: math.Inf(1)},
		{Steps: 1, AbsoluteTolerance: -1}, {Steps: 1, AbsoluteTolerance: .01, MaxSamples: 1022},
		{Steps: 100001, Samples: 2}, {Steps: 100000, Samples: 1000}, {Steps: 1, Samples: 2, Seed: 1 << 32}, {Steps: 1, Samples: 1000001}, {Steps: 1, Samples: 32, MaxSamples: 1},
	}
	for _, cfg := range invalid {
		if engine, err := m.s.NewMCVarianceSwapEngine(m.process, cfg); err == nil || engine != nil {
			t.Fatalf("invalid config %+v", cfg)
		}
	}
	cfg := MCVarianceSwapConfig{Steps: 12, Samples: 32, Seed: 42}
	if _, err := m.s.NewMCVarianceSwapEngine(nil, cfg); err == nil {
		t.Fatal("nil process")
	}
	other := newVarianceSwapMarket(t)
	if _, err := other.s.NewMCVarianceSwapEngine(m.process, cfg); err == nil {
		t.Fatal("foreign process")
	}
	mc := pricingMust(m.s.NewMCVarianceSwapEngine(m.process, cfg))
	if err := other.swap.SetMCEngine(mc); err == nil {
		t.Fatal("foreign engine")
	}
	if err := m.swap.SetMCEngine(nil); err == nil {
		t.Fatal("nil engine")
	}
	var nilSwap *VarianceSwap
	if err := nilSwap.SetMCEngine(mc); err == nil {
		t.Fatal("nil swap engine")
	}
	if _, err := nilSwap.Samples(); err == nil {
		t.Fatal("nil swap samples")
	}
	if _, err := nilSwap.VarianceError(); err == nil {
		t.Fatal("nil swap variance error")
	}
	pricingOK(t, mc.Close())
	if err := m.swap.SetMCEngine(mc); err == nil {
		t.Fatal("closed engine")
	}
	pricingOK(t, m.process.Close())
	if _, err := m.s.NewMCVarianceSwapEngine(m.process, cfg); err == nil {
		t.Fatal("closed process")
	}
	pricingOK(t, m.s.Close())
	if _, err := m.swap.Samples(); err == nil {
		t.Fatal("closed session")
	}
}
