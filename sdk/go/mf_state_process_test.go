package itofin

import (
	"errors"
	"math"
	"os"
	"strconv"
	"strings"
	"sync"
	"testing"
)

func mfNear(t *testing.T, got, want float64) {
	t.Helper()
	if math.IsNaN(got) || math.IsInf(got, 0) || math.IsNaN(want) || math.IsInf(want, 0) || math.Abs(got-want) > 2e-13*math.Max(math.Abs(want), math.SmallestNonzeroFloat64*(1<<52)) {
		t.Fatalf("%.17g != %.17g", got, want)
	}
}

func TestMfStateProcess(t *testing.T) {
	s := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()) })
	times, vols := []float64{0, 1, 2}, []float64{.9, .2, .3, .4}
	p := pricingMust(s.NewMfStateProcess(.7, times, vols))
	times[1], vols[1] = 9, 9
	if pricingMust(p.Reversion()) != .7 || pricingMust(p.X0()) != 0 || pricingMust(p.Drift(2, 7)) != 0 {
		t.Fatal("parameters")
	}
	for _, pair := range [][2]float64{{0, .2}, {.999, .2}, {1, .3}, {2, .4}, {10, .4}} {
		if pricingMust(p.Diffusion(pair[0], -7)) != pair[1] {
			t.Fatal("right-continuous raw diffusion")
		}
	}
	copiedTimes, copiedVols := pricingMust(p.Times()), pricingMust(p.Volatilities())
	copiedTimes[0], copiedVols[1] = 9, 9
	if pricingMust(p.Times())[0] != 0 || pricingMust(p.Volatilities())[1] != .2 {
		t.Fatal("grid getters alias process")
	}
	if pricingMust(p.Expectation(.5, -.2, 2)) != -.2 {
		t.Fatal("mean")
	}
	v := pricingMust(p.Variance(.5, -.2, 2))
	mfNear(t, pricingMust(p.StdDeviation(.5, -.2, 2)), math.Sqrt(v))
	mfNear(t, pricingMust(p.Evolve(.5, -.2, 2, -.7)), -.2-math.Sqrt(v)*.7)
	if pricingMust(p.Evolve(0, .3, 0, 5)) != .3 {
		t.Fatal("zero step")
	}
	for _, a := range []float64{-.4, 0, .4} {
		empty := pricingMust(s.NewMfStateProcess(a, nil, []float64{0}))
		if len(pricingMust(empty.Times())) != 0 || pricingMust(empty.Diffusion(.7, 0)) != 0 {
			t.Fatal("empty grid")
		}
		want := .2
		if a != 0 {
			want = math.Exp(2*a*.7) * math.Expm1(2*a*.2) / (2 * a)
		}
		mfNear(t, pricingMust(empty.Variance(.7, 0, .2)), want)
		if pricingMust(empty.Variance(0, 0, math.Nextafter(1, 2)-1)) == 0 {
			t.Fatal("epsilon threshold")
		}
		if pricingMust(empty.Variance(0, 0, (math.Nextafter(1, 2)-1)/2)) != 0 {
			t.Fatal("sub-epsilon step")
		}
		pricingOK(t, empty.Close())
	}
}

func mfRows(t *testing.T, name string) [][]string {
	t.Helper()
	b, err := os.ReadFile("testdata/" + name)
	pricingOK(t, err)
	var result [][]string
	for _, line := range strings.Split(strings.TrimSpace(string(b)), "\n")[1:] {
		result = append(result, strings.Split(line, "\t"))
	}
	return result
}

func mfFloat(v string) float64 { return pricingMust(strconv.ParseFloat(v, 64)) }
func mfArray(v string) []float64 {
	if v == "-" {
		return nil
	}
	var out []float64
	for _, x := range strings.Split(v, ";") {
		out = append(out, mfFloat(x))
	}
	return out
}

func TestMfStateNativeAndHighPrecisionFixtures(t *testing.T) {
	s := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()) })
	for _, r := range mfRows(t, "mf_state_native.tsv") {
		times, vols := []float64{.5, 1.5}, []float64{.2, .4, .1}
		if r[6] == "1" {
			times, vols = nil, []float64{.2}
		}
		p := pricingMust(s.NewMfStateProcess(mfFloat(r[0]), times, vols))
		x, dt := mfFloat(r[1]), mfFloat(r[2])
		mfNear(t, pricingMust(p.Diffusion(x, 3)), mfFloat(r[3]))
		mfNear(t, pricingMust(p.Variance(x, 3, dt)), mfFloat(r[4]))
		mfNear(t, pricingMust(p.StdDeviation(x, 3, dt)), mfFloat(r[5]))
		pricingOK(t, p.Close())
	}
	for _, r := range mfRows(t, "mf_state_reference.tsv") {
		p := pricingMust(s.NewMfStateProcess(mfFloat(r[1]), mfArray(r[2]), mfArray(r[3])))
		x, dt := mfFloat(r[4]), mfFloat(r[5])
		mfNear(t, pricingMust(p.Variance(x, 3, dt)), mfFloat(r[6]))
		mfNear(t, pricingMust(p.StdDeviation(x, -2, dt)), mfFloat(r[7]))
		mfNear(t, pricingMust(p.Evolve(x, 0, dt, 1)), mfFloat(r[7]))
		pricingOK(t, p.Close())
	}
}

func TestMfStateLifecycleAndInvalidInputs(t *testing.T) {
	s := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()) })
	for _, a := range []float64{math.NaN(), math.Inf(1), math.Inf(-1)} {
		if _, err := s.NewMfStateProcess(a, nil, []float64{.2}); err == nil {
			t.Fatal("nonfinite reversion")
		}
	}
	for _, pair := range [][2][]float64{{nil, nil}, {{1}, {.2}}, {{1, 1}, {.2, .3, .4}}, {{2, 1}, {.2, .3, .4}}, {{-1}, {.2, .3}}, {{math.NaN()}, {.2, .3}}, {{math.Inf(1)}, {.2, .3}}, {nil, {-.2}}, {nil, {math.Inf(1)}}, {nil, {math.NaN()}}} {
		if _, err := s.NewMfStateProcess(0, pair[0], pair[1]); err == nil {
			t.Fatal("invalid grid accepted")
		}
	}
	p := pricingMust(s.NewMfStateProcess(.1, []float64{1}, []float64{.2, .3}))
	for _, f := range []func() (float64, error){func() (float64, error) { return p.Drift(-1, 0) }, func() (float64, error) { return p.Diffusion(0, math.NaN()) }, func() (float64, error) { return p.Expectation(0, 0, -1) }, func() (float64, error) { return p.StdDeviation(0, math.Inf(1), 1) }, func() (float64, error) { return p.Variance(math.MaxFloat64, 0, math.MaxFloat64) }, func() (float64, error) { return p.Evolve(0, 0, 1, math.NaN()) }} {
		if _, err := f(); err == nil {
			t.Fatal("invalid query accepted")
		}
	}
	var wg sync.WaitGroup
	for i := 0; i < 8; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			if _, err := p.Variance(0, 0, 2); err != nil {
				t.Error(err)
			}
		}()
	}
	wg.Wait()
	other := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, other.Close()) })
	foreign := &MfStateProcess{object{other, p.id}}
	if _, err := foreign.Times(); err == nil {
		t.Fatal("foreign handle")
	}
	pricingOK(t, p.Close())
	if _, err := p.X0(); err == nil {
		t.Fatal("released handle")
	}
	if _, err := p.Volatilities(); err == nil {
		t.Fatal("released grid")
	}
	var nilProcess *MfStateProcess
	var zero MfStateProcess
	for _, bad := range []*MfStateProcess{nilProcess, &zero} {
		if _, err := bad.X0(); err == nil {
			t.Fatal("nil/zero process")
		}
		if _, err := bad.Times(); err == nil {
			t.Fatal("nil/zero grid")
		}
	}
	q := pricingMust(s.NewMfStateProcess(0, nil, []float64{0}))
	pricingOK(t, s.Close())
	if _, err := q.Times(); !errors.Is(err, ErrClosed) {
		t.Fatal(err)
	}
}
