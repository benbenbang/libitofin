package itofin

import (
	"errors"
	"math"
	"reflect"
	"sync"
	"testing"
)

type gjrPricingMarket struct {
	*gjrMarket
	settings *Settings
	model    *GJRModel
	engine   *AnalyticGJREngine
	option   *VanillaOption
}

func newGJRPricingMarket(t *testing.T) *gjrPricingMarket {
	t.Helper()
	m := &gjrPricingMarket{gjrMarket: newGJRMarket(t, GJRReflection)}
	m.settings = pricingMust(m.s.NewSettings())
	pricingOK(t, m.settings.SetEvaluationDate(m.today))
	m.model = pricingMust(m.s.NewGJRModel(m.process))
	m.engine = pricingMust(m.s.NewAnalyticGJREngine(m.model))
	m.option = pricingMust(m.s.NewVanillaOption(Call, 100, m.expiry, m.settings))
	pricingOK(t, m.option.SetAnalyticGJREngine(m.engine))
	return m
}

func TestGJRModelParametersLiveInputsAndRetainedGraph(t *testing.T) {
	m := newGJRPricingMarket(t)
	baseline := pricingMust(m.option.NPV())
	pricingNear(t, pricingMust(m.option.PriceAnalyticGJR(m.engine)), baseline, 0)
	want := []float64{m.params.Omega, m.params.Alpha, m.params.Beta, m.params.Gamma, m.params.Lambda, m.params.DailyVariance}
	if got := pricingMust(m.model.Params()); !reflect.DeepEqual(got, want) {
		t.Fatal(got)
	}
	oldProcess := pricingMust(m.model.Process())
	if pricingMust(oldProcess.Discretization()) != GJRFullTruncation || pricingMust(m.process.Discretization()) != GJRReflection {
		t.Fatal("model did not reset discretization")
	}
	if pricingMust(oldProcess.Parameters()) != m.params {
		t.Fatal("daily parameter semantics changed")
	}
	params := pricingMust(m.model.Params())
	params[5] *= 1.5
	pricingOK(t, m.model.SetParams(params))
	params[5] = 7
	current := pricingMust(m.model.Process())
	pricingNear(t, pricingMust(current.Parameters()).DailyVariance, want[5]*1.5, 0)
	pricingNear(t, pricingMust(oldProcess.Parameters()).DailyVariance, want[5], 0)
	if math.Abs(pricingMust(m.option.NPV())-baseline) < 1e-6 {
		t.Fatal("model update ignored")
	}
	pricingOK(t, m.model.SetParams(want))
	pricingNear(t, pricingMust(m.option.NPV()), baseline, 0)
	for i, value := range []float64{105, .06, .03} {
		pricingOK(t, m.quotes[i].SetValue(value))
		if pricingMust(m.option.IsCalculated()) {
			t.Fatal("market update did not invalidate cache")
		}
		if math.Abs(pricingMust(m.option.NPV())-baseline) < 1e-6 {
			t.Fatal("live market update ignored")
		}
		pricingOK(t, m.quotes[i].SetValue([]float64{100, .05, .02}[i]))
		pricingNear(t, pricingMust(m.option.NPV()), baseline, 0)
	}
	for _, q := range m.quotes[1:] {
		pricingOK(t, q.Close())
	}
	for _, close := range []func() error{m.risk.Close, m.dividend.Close, m.process.Close, oldProcess.Close, current.Close, m.model.Close, m.engine.Close, m.settings.Close} {
		pricingOK(t, close())
	}
	pricingOK(t, m.quotes[0].SetValue(105))
	if pricingMust(m.option.IsCalculated()) {
		t.Fatal("post-close quote update did not invalidate analytic cache")
	}
	reference := newGJRPricingMarket(t)
	pricingOK(t, reference.quotes[0].SetValue(105))
	fresh := pricingMust(reference.option.NPV())
	if fresh == baseline {
		t.Fatal("fresh analytic reference ignored spot update")
	}
	pricingNear(t, pricingMust(m.option.NPV()), fresh, 0)
	pricingOK(t, m.quotes[0].Close())
	if _, err := m.model.Params(); err == nil {
		t.Fatal("closed model accepted")
	}
	if _, err := m.option.PriceAnalyticGJR(m.engine); err == nil {
		t.Fatal("closed engine accepted")
	}
	if _, err := m.model.Process(); err == nil {
		t.Fatal("closed model process accepted")
	}
}

func TestGJRModelBoundariesAndAtomicFailures(t *testing.T) {
	m := newGJRPricingMarket(t)
	other := pricingMust(NewSession())
	defer other.Close()
	if _, err := other.NewGJRModel(m.process); !errors.Is(err, ErrSessionMismatch) {
		t.Fatal(err)
	}
	if _, err := other.NewAnalyticGJREngine(m.model); !errors.Is(err, ErrSessionMismatch) {
		t.Fatal(err)
	}
	if _, err := other.NewMCGJREngine(m.process, GJRMCConfig{Steps: 1, Samples: 2}); !errors.Is(err, ErrSessionMismatch) {
		t.Fatal(err)
	}
	var nilModel *GJRModel
	for _, model := range []*GJRModel{nilModel, {}} {
		if _, err := model.Params(); err == nil {
			t.Fatal("nil or zero model accepted")
		}
		if _, err := model.Process(); err == nil {
			t.Fatal("nil or zero model process accepted")
		}
		if err := model.SetParams(nil); err == nil {
			t.Fatal("nil or zero model setter accepted")
		}
	}
	for _, err := range []error{m.option.SetAnalyticGJREngine(nil), m.option.SetMCGJREngine(nil)} {
		if err == nil {
			t.Fatal("nil engine accepted")
		}
	}
	if _, err := m.option.PriceAnalyticGJR(nil); err == nil {
		t.Fatal("nil pricing engine accepted")
	}
	if _, err := m.option.PriceMCGJR(nil); err == nil {
		t.Fatal("nil MC pricing engine accepted")
	}
	if _, err := m.s.NewGJRModel(nil); err == nil {
		t.Fatal("nil process accepted")
	}
	if _, err := m.s.NewAnalyticGJREngine(nil); err == nil {
		t.Fatal("nil model accepted")
	}
	if _, err := m.s.NewMCGJREngine(nil, GJRMCConfig{}); err == nil {
		t.Fatal("nil MC process accepted")
	}
	want := pricingMust(m.model.Params())
	baseline := pricingMust(m.option.NPV())
	bad := [][]float64{nil, want[:5], append(append([]float64(nil), want...), 1)}
	for i, value := range []float64{-1, -1, -1, -1, math.NaN(), 0} {
		p := append([]float64(nil), want...)
		p[i] = value
		bad = append(bad, p)
	}
	for _, p := range bad {
		if err := m.model.SetParams(p); err == nil {
			t.Fatal("invalid parameters accepted", p)
		}
		if !reflect.DeepEqual(pricingMust(m.model.Params()), want) {
			t.Fatal("partial update")
		}
		pricingNear(t, pricingMust(m.option.NPV()), baseline, 0)
	}
	if _, err := m.option.Delta(); err == nil {
		t.Fatal("fabricated Greek")
	}
	if _, err := m.option.ErrorEstimate(); err == nil {
		t.Fatal("fabricated analytic error estimate")
	}
	american := pricingMust(m.s.NewAmericanOption(Call, 100, m.today, m.expiry, m.settings))
	if _, err := american.PriceAnalyticGJR(m.engine); err == nil {
		t.Fatal("American option accepted")
	}
	pricingOK(t, m.s.Close())
	if _, err := m.model.Params(); !errors.Is(err, ErrClosed) {
		t.Fatal(err)
	}
}

func TestGJRModelConcurrentClose(t *testing.T) {
	m := newGJRPricingMarket(t)
	var group sync.WaitGroup
	for i := 0; i < 4; i++ {
		group.Go(func() {
			for j := 0; j < 16; j++ {
				_, err := m.model.Params()
				if err != nil && !errors.Is(err, ErrClosed) {
					t.Error(err)
				}
			}
		})
	}
	group.Go(func() { pricingOK(t, m.s.Close()) })
	group.Wait()
}

func TestGJRModelRejectsWrongNativeTypes(t *testing.T) {
	m := newGJRPricingMarket(t)
	wrongModel := &GJRModel{m.process.object}
	for _, call := range []func() error{
		func() error { _, err := wrongModel.Params(); return err },
		func() error { _, err := wrongModel.Process(); return err },
		func() error { return wrongModel.SetParams([]float64{.000002, .04, .9, .06, .1, .00016}) },
		func() error { _, err := m.s.NewGJRModel(&GJRProcess{m.model.object}); return err },
		func() error { _, err := m.s.NewAnalyticGJREngine(wrongModel); return err },
		func() error {
			_, err := m.s.NewMCGJREngine(&GJRProcess{m.model.object}, GJRMCConfig{Steps: 1, Samples: 2})
			return err
		},
	} {
		if err := call(); err == nil {
			t.Fatal("incorrect native object type accepted")
		}
	}
	foreign := newGJRPricingMarket(t)
	if err := m.option.SetAnalyticGJREngine(foreign.engine); !errors.Is(err, ErrSessionMismatch) {
		t.Fatal(err)
	}
	engine := pricingMust(foreign.s.NewMCGJREngine(foreign.process, GJRMCConfig{Steps: 1, Samples: 2}))
	if err := m.option.SetMCGJREngine(engine); !errors.Is(err, ErrSessionMismatch) {
		t.Fatal(err)
	}
}
