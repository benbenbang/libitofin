package itofin

import (
	"encoding/json"
	"errors"
	"math"
	"os"
	"sync"
	"testing"
)

func g2Near(t *testing.T, got, want float64) {
	t.Helper()
	if math.Abs(got-want) > 8e-14*math.Max(math.Abs(want), 1e-300) {
		t.Fatalf("%.17g != %.17g", got, want)
	}
}

func TestG2ExactTransitions(t *testing.T) {
	s := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()) })
	p := pricingMust(s.NewG2Process(.1, .03, .4, .007, -.6))
	if pricingMust(p.Size()) != 2 || pricingMust(p.Factors()) != 2 {
		t.Fatal("dimensions")
	}
	values := []float64{pricingMust(p.X0()), pricingMust(p.Y0()), pricingMust(p.A()), pricingMust(p.Sigma()), pricingMust(p.B()), pricingMust(p.Eta()), pricingMust(p.Rho())}
	want := []float64{0, 0, .1, .03, .4, .007, -.6}
	for i := range want {
		if values[i] != want[i] {
			t.Fatal("coefficient", i)
		}
	}
	if pricingMust(p.InitialValues()) != [2]float64{} {
		t.Fatal("initial values")
	}
	if pricingMust(p.Apply([2]float64{1, -2}, [2]float64{3, 4})) != [2]float64{4, 2} {
		t.Fatal("apply")
	}
	x := [2]float64{.2, -.3}
	drift := pricingMust(p.Drift(1, x))
	g2Near(t, drift[0], -.1*x[0])
	g2Near(t, drift[1], -.4*x[1])
	d := pricingMust(p.Diffusion(0, x))
	g2Near(t, d[1][0], -.6*.007)
	g2Near(t, d[1][0]*d[1][0]+d[1][1]*d[1][1], .007*.007)
	g2Near(t, d[0][0]*d[1][0], -.6*.03*.007)
	dt := .8
	integral := func(k float64) float64 { return -math.Expm1(-k*dt) / k }
	cov := pricingMust(p.Covariance(1, x, dt))
	root := pricingMust(p.StdDeviation(1, x, dt))
	expected := [2][2]float64{{.03 * .03 * integral(.2), -.6 * .03 * .007 * integral(.5)}, {-.6 * .03 * .007 * integral(.5), .007 * .007 * integral(.8)}}
	for i := range cov {
		for j := range cov[i] {
			g2Near(t, cov[i][j], expected[i][j])
			g2Near(t, root[i][0]*root[j][0]+root[i][1]*root[j][1], expected[i][j])
		}
	}
	mean := pricingMust(p.Expectation(1, x, dt))
	g2Near(t, mean[0], x[0]*math.Exp(-.1*dt))
	g2Near(t, mean[1], x[1]*math.Exp(-.4*dt))
	draws := [2]float64{.75, -.4}
	next := pricingMust(p.Evolve(1, x, dt, draws))
	for i := range next {
		g2Near(t, next[i], mean[i]+root[i][0]*draws[0]+root[i][1]*draws[1])
	}
	if pricingMust(p.Evolve(0, x, 0, draws)) != x {
		t.Fatal("zero step")
	}
}

func TestG2QuantLibOracleAndDiffusionQualification(t *testing.T) {
	raw := pricingMust(os.ReadFile("testdata/g2_process.json"))
	type row struct {
		Parameters   [5]float64    `json:"parameters"`
		T            float64       `json:"t"`
		Dt           float64       `json:"dt"`
		State        [2]float64    `json:"state"`
		Expectation  [2]float64    `json:"expectation"`
		Covariance   [2][2]float64 `json:"covariance"`
		StdDeviation [2][2]float64 `json:"std_deviation"`
		RawDiffusion [2][2]float64 `json:"raw_diffusion"`
	}
	var source struct {
		Rows []row `json:"rows"`
	}
	pricingOK(t, json.Unmarshal(raw, &source))
	if len(source.Rows) != 4 {
		t.Fatal("oracle row count")
	}
	s := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()) })
	for _, r := range source.Rows {
		a := r.Parameters
		p := pricingMust(s.NewG2Process(a[0], a[1], a[2], a[3], a[4]))
		e := pricingMust(p.Expectation(r.T, r.State, r.Dt))
		c := pricingMust(p.Covariance(r.T, r.State, r.Dt))
		std := pricingMust(p.StdDeviation(r.T, r.State, r.Dt))
		for i := 0; i < 2; i++ {
			g2Near(t, e[i], r.Expectation[i])
			for j := 0; j < 2; j++ {
				g2Near(t, c[i][j], r.Covariance[i][j])
				g2Near(t, std[i][j], r.StdDeviation[i][j])
			}
		}
		d := pricingMust(p.Diffusion(r.T, r.State))
		g2Near(t, r.RawDiffusion[1][0], a[4]*a[1])
		g2Near(t, d[1][0], a[4]*a[3])
		if a[1] != a[3] && a[4] != 0 && d[1][0] == r.RawDiffusion[1][0] {
			t.Fatal("upstream diffusion defect copied")
		}
	}
}

func TestG2LimitsAndLifecycle(t *testing.T) {
	s := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()) })
	for _, vols := range [][2]float64{{0, 0}, {0, .007}, {.03, 0}, {.03, .007}} {
		p := pricingMust(s.NewG2Process(0, vols[0], 0, vols[1], -1))
		c := pricingMust(p.Covariance(0, [2]float64{}, .7))
		g2Near(t, c[0][0], vols[0]*vols[0]*.7)
		g2Near(t, c[1][1], vols[1]*vols[1]*.7)
		g2Near(t, c[0][1], -vols[0]*vols[1]*.7)
	}
	p := pricingMust(s.NewG2Process(.1, .03, .2, .007, 1))
	root := pricingMust(p.StdDeviation(0, [2]float64{}, 1e-8))
	if root[1][1] <= 0 {
		t.Fatal("independent shock lost")
	}
	g2Near(t, root[1][1], 2.3094010744491024e-16*(.007/.008))
	q := pricingMust(s.NewG2Process(1e200, 1e200, 1e200, 1e200, .5))
	cov := pricingMust(q.Covariance(0, [2]float64{}, 1))
	g2Near(t, cov[0][0], 5e199)
	var wg sync.WaitGroup
	for range 8 {
		wg.Add(1)
		go func() {
			defer wg.Done()
			if _, err := p.Covariance(0, [2]float64{}, .2); err != nil {
				t.Error(err)
			}
		}()
	}
	wg.Wait()
	copy := *p
	pricingOK(t, p.Close())
	pricingOK(t, copy.Close())
	if _, err := copy.X0(); err == nil {
		t.Fatal("stale handle")
	}
	other := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, other.Close()) })
	if _, err := (&G2Process{object{other, q.id}}).X0(); err == nil {
		t.Fatal("foreign handle")
	}
	quote := pricingMust(s.NewSimpleQuote(1))
	if _, err := (&G2Process{quote.object}).X0(); err == nil {
		t.Fatal("wrong type")
	}
	var nilP *G2Process
	if _, err := nilP.InitialValues(); err == nil {
		t.Fatal("nil receiver")
	}
	var nilS *Session
	if _, err := nilS.NewG2Process(0, 0, 0, 0, 0); err == nil {
		t.Fatal("nil session")
	}
	pricingOK(t, s.Close())
	if _, err := q.X0(); !errors.Is(err, ErrClosed) {
		t.Fatal(err)
	}
}

func TestG2InvalidDomains(t *testing.T) {
	s := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()) })
	for i := 0; i < 5; i++ {
		for _, bad := range []float64{math.NaN(), math.Inf(1), math.Inf(-1)} {
			a := [5]float64{.1, .03, .2, .007, .5}
			a[i] = bad
			if _, err := s.NewG2Process(a[0], a[1], a[2], a[3], a[4]); err == nil {
				t.Fatal("nonfinite constructor")
			}
		}
	}
	for _, a := range [][5]float64{{0, -1, 0, 0, 0}, {0, 0, 0, -1, 0}, {0, 0, 0, 0, 1.01}} {
		if _, err := s.NewG2Process(a[0], a[1], a[2], a[3], a[4]); err == nil {
			t.Fatal("constructor domain")
		}
	}
	p := pricingMust(s.NewG2Process(.1, .03, .2, .007, -.6))
	for _, bad := range []float64{-1, math.NaN(), math.Inf(1)} {
		if _, err := p.Drift(bad, [2]float64{}); err == nil {
			t.Fatal("time")
		}
		if _, err := p.Covariance(0, [2]float64{}, bad); err == nil {
			t.Fatal("step")
		}
		if _, err := p.Expectation(0, [2]float64{}, bad); err == nil {
			t.Fatal("step")
		}
	}
	for _, bad := range []float64{math.NaN(), math.Inf(1)} {
		if _, err := p.Diffusion(0, [2]float64{bad, 0}); err == nil {
			t.Fatal("state")
		}
		if _, err := p.Evolve(0, [2]float64{}, 1, [2]float64{0, bad}); err == nil {
			t.Fatal("draw")
		}
		if _, err := p.Apply([2]float64{}, [2]float64{bad, 0}); err == nil {
			t.Fatal("change")
		}
	}
	if _, err := p.Apply([2]float64{math.MaxFloat64, 0}, [2]float64{math.MaxFloat64, 0}); err == nil {
		t.Fatal("overflow")
	}
}
