package itofin

import (
	"errors"
	"math"
	"testing"
)

type gjrMarket struct {
	s              *Session
	today, expiry  Date
	quotes         [3]*SimpleQuote
	risk, dividend *YieldTermStructure
	process        *GJRProcess
	params         GJRParameters
}

func newGJRMarket(t *testing.T, scheme GJRScheme) *gjrMarket {
	t.Helper()
	m := &gjrMarket{s: pricingMust(NewSession())}
	t.Cleanup(func() { pricingOK(t, m.s.Close()) })
	m.today = pricingMust(NewDate(3, 10, 2026))
	m.expiry = pricingMust(m.today.AddDays(365))
	dc := pricingMust(m.s.Actual365Fixed())
	for i, value := range [3]float64{100, .05, .02} {
		m.quotes[i] = pricingMust(m.s.NewSimpleQuote(value))
	}
	m.risk = pricingMust(m.s.NewFlatForwardFromQuote(m.today, m.quotes[1], dc))
	m.dividend = pricingMust(m.s.NewFlatForwardFromQuote(m.today, m.quotes[2], dc))
	m.params = GJRParameters{DailyVariance: .00016, Omega: .000002, Alpha: .04, Beta: .9, Gamma: .06, Lambda: .1, DaysPerYear: 252}
	m.process = pricingMust(m.s.NewGJRProcess(m.quotes[0], m.risk, m.dividend, m.params, scheme))
	return m
}

func TestGJRLiveMarketInputsAndRetainedOwnership(t *testing.T) {
	m := newGJRMarket(t, GJRFullTruncation)
	initial := pricingMust(m.process.InitialValues())
	if initial != [2]float64{100, m.params.DailyVariance * m.params.DaysPerYear} {
		t.Fatalf("initial values: %v", initial)
	}
	if params := pricingMust(m.process.Parameters()); params != m.params {
		t.Fatalf("parameter order: %+v", params)
	}
	pricingNear(t, pricingMust(m.process.Time(m.expiry)), 1, 0)
	baseline := pricingMust(m.process.Drift(.25, initial))
	pricingOK(t, m.quotes[0].SetValue(105))
	if pricingMust(m.process.InitialValues())[0] != 105 {
		t.Fatal("process did not observe spot quote")
	}
	for _, test := range []struct {
		index int
		value float64
		sign  float64
	}{{1, .06, 1}, {2, .03, -1}} {
		pricingOK(t, m.quotes[test.index].SetValue(test.value))
		updated := pricingMust(m.process.Drift(.25, initial))
		pricingNear(t, updated[0]-baseline[0], test.sign*.01, 1e-10)
		pricingOK(t, m.quotes[test.index].SetValue([3]float64{100, .05, .02}[test.index]))
	}
	for _, q := range m.quotes {
		pricingOK(t, q.Close())
	}
	pricingOK(t, m.risk.Close())
	pricingOK(t, m.dividend.Close())
	if pricingMust(m.process.InitialValues())[0] != 105 {
		t.Fatal("retained spot lost after handle close")
	}
	if got := pricingMust(m.process.Drift(.25, initial)); got != baseline {
		t.Fatal("retained curves changed after handle close")
	}
	pricingOK(t, m.process.Close())
	if _, err := m.process.InitialValues(); err == nil {
		t.Fatal("closed process accepted")
	}
	pricingOK(t, m.process.Close())
}

func TestGJRProcessLifecycleAndRecovery(t *testing.T) {
	m := newGJRMarket(t, GJRFullTruncation)
	other := pricingMust(NewSession())
	defer other.Close()
	if _, err := other.NewGJRProcess(m.quotes[0], m.risk, m.dividend, m.params, GJRFullTruncation); !errors.Is(err, ErrSessionMismatch) {
		t.Fatalf("cross-session market: %v", err)
	}
	for _, input := range []struct {
		spot *SimpleQuote
		rf   *YieldTermStructure
		div  *YieldTermStructure
	}{{nil, m.risk, m.dividend}, {m.quotes[0], nil, m.dividend}, {m.quotes[0], m.risk, nil}} {
		if _, err := m.s.NewGJRProcess(input.spot, input.rf, input.div, m.params, GJRFullTruncation); err == nil {
			t.Fatal("nil market accepted")
		}
	}
	var nilProcess *GJRProcess
	for _, p := range []*GJRProcess{nilProcess, {}} {
		if _, err := p.InitialValues(); err == nil {
			t.Fatal("uninitialized process accepted")
		}
		if _, err := p.Evolve(0, [2]float64{100, .00016}, .1, [2]float64{}); err == nil {
			t.Fatal("uninitialized evolve accepted")
		}
		if _, err := p.Time(m.expiry); err == nil {
			t.Fatal("uninitialized time accepted")
		}
	}
	initial := pricingMust(m.process.InitialValues())
	for _, value := range []float64{0, -1, math.NaN(), math.Inf(1)} {
		pricingOK(t, m.quotes[0].SetValue(value))
		if _, err := m.process.InitialValues(); err == nil {
			t.Fatal("invalid live spot accepted")
		}
		pricingOK(t, m.quotes[0].SetValue(100))
		if pricingMust(m.process.InitialValues()) != initial {
			t.Fatal("invalid spot permanently poisoned process")
		}
	}
	pricingOK(t, m.s.Close())
	if _, err := m.process.InitialValues(); !errors.Is(err, ErrClosed) {
		t.Fatalf("closed session: %v", err)
	}
}

func TestGJRProcessDomainsAndSchemeBoundaries(t *testing.T) {
	m := newGJRMarket(t, GJRFullTruncation)
	for _, change := range []func(*GJRParameters){
		func(p *GJRParameters) { p.DailyVariance = -1 },
		func(p *GJRParameters) { p.Omega = -1 },
		func(p *GJRParameters) { p.Alpha = -1 },
		func(p *GJRParameters) { p.Beta = -1 },
		func(p *GJRParameters) { p.Gamma = -p.Alpha - .01 },
		func(p *GJRParameters) { p.Lambda = math.NaN() },
		func(p *GJRParameters) { p.DaysPerYear = 0 },
		func(p *GJRParameters) { p.DaysPerYear = math.Inf(1) },
	} {
		params := m.params
		change(&params)
		if _, err := m.s.NewGJRProcess(m.quotes[0], m.risk, m.dividend, params, GJRFullTruncation); err == nil {
			t.Fatal("invalid parameter accepted")
		}
	}
	for _, scheme := range []GJRScheme{-1, 3} {
		if _, err := m.s.NewGJRProcess(m.quotes[0], m.risk, m.dividend, m.params, scheme); err == nil {
			t.Fatal("invalid scheme accepted")
		}
	}
	for _, scheme := range []GJRScheme{GJRPartialTruncation, GJRFullTruncation, GJRReflection} {
		p := pricingMust(m.s.NewGJRProcess(m.quotes[0], m.risk, m.dividend, m.params, scheme))
		initial := pricingMust(p.InitialValues())
		if pricingMust(p.Discretization()) != scheme {
			t.Fatal("scheme getter mismatch")
		}
		zeroStep := pricingMust(p.Evolve(0, initial, 0, [2]float64{1, -1}))
		pricingNear(t, zeroStep[0], initial[0], 0)
		pricingNear(t, zeroStep[1], initial[1], 1e-16)
		if got := pricingMust(p.Diffusion(0, initial)); got[0][0] != math.Sqrt(m.params.DailyVariance*m.params.DaysPerYear) || got[0][1] != 0 {
			t.Fatalf("spot diffusion annualization: %v", got)
		}
		negative := [2]float64{100, -.01}
		want := negative
		if scheme == GJRReflection {
			want[1] = .01
		}
		negativeStep := pricingMust(p.Evolve(0, negative, 0, [2]float64{}))
		pricingNear(t, negativeStep[0], want[0], 0)
		pricingNear(t, negativeStep[1], want[1], 1e-16)
		for _, state := range [][2]float64{{0, .00016}, {math.Inf(1), .00016}, {100, math.NaN()}} {
			if _, err := p.Drift(0, state); err == nil {
				t.Fatal("invalid state accepted")
			}
		}
		for _, input := range []struct {
			t, dt float64
			draws [2]float64
		}{{math.NaN(), .1, [2]float64{}}, {0, -.1, [2]float64{}}, {0, math.Inf(1), [2]float64{}}, {0, .1, [2]float64{math.NaN(), 0}}} {
			if _, err := p.Evolve(input.t, initial, input.dt, input.draws); err == nil {
				t.Fatal("invalid evolve input accepted")
			}
		}
		pricingOK(t, p.Close())
	}
}

func TestGJRProcessConcurrentQueriesAndClose(t *testing.T) {
	m := newGJRMarket(t, GJRFullTruncation)
	initial := pricingMust(m.process.InitialValues())
	results := make(chan error, 16)
	start := make(chan struct{})
	for range 16 {
		go func() {
			<-start
			for range 20 {
				got, err := m.process.InitialValues()
				if err != nil {
					var native *Error
					if !errors.As(err, &native) || native.Code != 2 {
						results <- err
						return
					}
				} else if got != initial {
					results <- errors.New("concurrent query changed initial state")
					return
				}
			}
			results <- nil
		}()
	}
	close(start)
	pricingOK(t, m.process.Close())
	for range 16 {
		if err := <-results; err != nil {
			t.Fatal(err)
		}
	}
	if _, err := m.process.InitialValues(); err == nil {
		t.Fatal("query succeeded after concurrent Close completed")
	}
}
