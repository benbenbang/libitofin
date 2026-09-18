package consumer_test

import (
	"errors"
	"math"
	"os"
	"reflect"
	"strconv"
	"testing"

	itofin "github.com/benbenbang/libitofin/sdk/go"
)

func TestExternalNativeVersion(t *testing.T) {
	want := os.Getenv("ITOFIN_EXPECTED_VERSION")
	if want == "" || itofin.Version() != want {
		t.Fatalf("native version %q, package version %q", itofin.Version(), want)
	}
}

func portfolio() itofin.GBMConfig {
	return itofin.GBMConfig{
		Initial: []float64{6000, 4000}, Drift: []float64{.06, .03},
		Volatility: []float64{.2, .1}, Correlation: []float64{1, .3, .3, 1},
		Horizon: 1, Steps: 12, Paths: 1000, Seed: 42,
	}
}

func simulate(t testing.TB, c itofin.GBMConfig) *itofin.Simulation {
	t.Helper()
	r, err := itofin.SimulateGBM(c)
	if err != nil {
		t.Fatal(err)
	}
	return r
}

func TestExternalPortfolioLayoutAndReplay(t *testing.T) {
	c := portfolio()
	full := simulate(t, c)
	if full.Paths != c.Paths || full.Times != c.Steps+1 || full.Assets != 2 || full.TerminalOnly {
		t.Fatalf("unexpected full-path shape: %d x %d x %d", full.Paths, full.Times, full.Assets)
	}
	if len(full.Values) != c.Paths*(c.Steps+1)*2 {
		t.Fatal("unexpected full-path buffer length")
	}
	c.TerminalOnly = true
	terminal := simulate(t, c)
	if terminal.Times != 1 || !terminal.TerminalOnly || len(terminal.Values) != c.Paths*2 {
		t.Fatal("unexpected terminal shape")
	}
	if !reflect.DeepEqual(terminal.Values, simulate(t, c).Values) {
		t.Fatal("seeded replay changed")
	}
	for path := range c.Paths {
		start := path * full.Times * full.Assets
		if !reflect.DeepEqual(full.Values[start:start+2], c.Initial) {
			t.Fatal("allocated initial values changed")
		}
		last := start + c.Steps*2
		if !reflect.DeepEqual(full.Values[last:last+2], terminal.Values[path*2:path*2+2]) {
			t.Fatal("terminal and full-path results disagree")
		}
	}
}

func TestExternalAllocatedPortfolioAndSingleAsset(t *testing.T) {
	c := portfolio()
	c.Volatility = []float64{0, 0}
	c.TerminalOnly = true
	r := simulate(t, c)
	want := 6000*math.Exp(.06) + 4000*math.Exp(.03)
	for path := range c.Paths {
		total := r.Values[path*2] + r.Values[path*2+1]
		if math.IsNaN(total) || math.Abs(total-want) > 1e-8 {
			t.Fatalf("allocated holdings must be summed without reweighting: %.12g != %.12g", total, want)
		}
	}
	single := itofin.GBMConfig{
		Initial: []float64{10000}, Drift: []float64{.06}, Volatility: []float64{0},
		Horizon: 1, Steps: 12, Paths: 3, Seed: 42, TerminalOnly: true,
	}
	for _, value := range simulate(t, single).Values {
		if math.IsNaN(value) || math.Abs(value-10000*math.Exp(.06)) > 1e-8 {
			t.Fatalf("single-asset growth: %.12g", value)
		}
	}
}

func TestExternalCorrelationAndOutputBound(t *testing.T) {
	c := portfolio()
	c.Steps, c.Paths, c.TerminalOnly = 1, 30000, true
	r := simulate(t, c)
	var sx, sy, sxx, syy, sxy float64
	for path := range c.Paths {
		x := math.Log(r.Values[path*2] / c.Initial[0])
		y := math.Log(r.Values[path*2+1] / c.Initial[1])
		sx += x
		sy += y
		sxx += x * x
		syy += y * y
		sxy += x * y
	}
	n := float64(c.Paths)
	corr := (sxy - sx*sy/n) / math.Sqrt((sxx-sx*sx/n)*(syy-sy*sy/n))
	if math.IsNaN(corr) || math.Abs(corr-.3) > .02 {
		t.Fatalf("log-return correlation: %g", corr)
	}
	c.MaxOutputValues = 10
	if _, err := itofin.SimulateGBM(c); err == nil {
		t.Fatal("oversized output accepted")
	}
}

func TestExternalConcurrentSessionsAndErrors(t *testing.T) {
	for range 4 {
		t.Run("session", func(t *testing.T) {
			t.Parallel()
			s, err := itofin.NewSession()
			if err != nil {
				t.Fatal(err)
			}
			t.Cleanup(func() {
				if err := s.Close(); err != nil {
					t.Error(err)
				}
			})
			q, err := s.NewSimpleQuote(42)
			if err != nil {
				t.Fatal(err)
			}
			value, err := q.Value()
			if err != nil || value != 42 {
				t.Fatalf("quote: %g, %v", value, err)
			}
			if err := s.Close(); err != nil {
				t.Fatal(err)
			}
			if _, err := q.Value(); !errors.Is(err, itofin.ErrClosed) {
				t.Fatalf("closed session: %v", err)
			}
		})
	}
}

func BenchmarkExternalPortfolio(b *testing.B) {
	for _, paths := range []int{1000, 10000} {
		for _, terminal := range []bool{false, true} {
			name := "full"
			if terminal {
				name = "terminal"
			}
			b.Run(name+"/"+strconv.Itoa(paths), func(b *testing.B) {
				c := portfolio()
				c.Paths, c.Steps, c.TerminalOnly = paths, 252, terminal
				b.ReportAllocs()
				for b.Loop() {
					r := simulate(b, c)
					if len(r.Values) == 0 {
						b.Fatal("empty simulation")
					}
				}
			})
		}
	}
}
