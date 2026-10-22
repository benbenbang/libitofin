package itofin

import (
	"encoding/json"
	"errors"
	"math"
	"os"
	"sync"
	"testing"
)

func blackDeltaNear(t *testing.T, got, want, tolerance float64) {
	t.Helper()
	if math.IsNaN(got) || math.Abs(got-want) > tolerance {
		t.Fatalf("%.17g != %.17g", got, want)
	}
}

func TestBlackDeltaFixturesAndConventions(t *testing.T) {
	raw := pricingMust(os.ReadFile("testdata/black_delta.json"))
	var fixture struct {
		Rows []struct {
			Option        OptionType
			Convention    DeltaType
			Delta, Strike float64
		}
	}
	pricingOK(t, json.Unmarshal(raw, &fixture))
	if len(fixture.Rows) != 8 {
		t.Fatal("fixture rows")
	}
	s := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()) })
	for _, r := range fixture.Rows {
		c := pricingMust(s.NewBlackDeltaCalculator(r.Option, r.Convention, 1.421, .997306, .992266, .1180654))
		blackDeltaNear(t, pricingMust(c.DeltaFromStrike(1.6)), r.Delta, 1e-14)
		delta := .15
		if r.Option == Put {
			delta = -.15
		}
		strike := pricingMust(c.StrikeFromDelta(delta))
		blackDeltaNear(t, strike, r.Strike, 1e-10)
		blackDeltaNear(t, pricingMust(c.DeltaFromStrike(strike)), delta, 1e-8*math.Abs(delta))
		for atm := AtmNull; atm <= AtmPutCall50; atm++ {
			actual, err := c.AtmStrike(atm)
			if atm == AtmNull || (atm == AtmPutCall50 && r.Convention != DeltaFwd) {
				if err == nil {
					t.Fatal("invalid ATM accepted", atm, r.Convention)
				}
				continue
			}
			pricingOK(t, err)
			want := 1.4237071536606858
			switch atm {
			case AtmSpot:
				want = 1.421
			case AtmFwd:
				want = 1.413818813884605
			case AtmDeltaNeutral:
				if r.Convention >= DeltaPaSpot {
					want = 1.4039991534456169
				}
			}
			blackDeltaNear(t, actual, want, 1e-14)
		}
		pricingOK(t, c.Close())
	}
}

func TestBlackDeltaBranchesDomainsAndZeroVol(t *testing.T) {
	s := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()) })
	for convention := DeltaSpot; convention <= DeltaPaFwd; convention++ {
		call := pricingMust(s.NewBlackDeltaCalculator(Call, convention, 1.421, .997306, .992266, .1180654))
		put := pricingMust(s.NewBlackDeltaCalculator(Put, convention, 1.421, .997306, .992266, .1180654))
		scale := 1.
		if convention == DeltaSpot || convention == DeltaPaSpot {
			scale = .992266
		}
		want := scale
		if convention >= DeltaPaSpot {
			want *= 1.6 / 1.413818813884605
		}
		blackDeltaNear(t, pricingMust(call.DeltaFromStrike(1.6))-pricingMust(put.DeltaFromStrike(1.6)), want, 1e-14)
		if convention >= DeltaPaSpot {
			low := pricingMust(call.DeltaFromStrike(.5))
			high := pricingMust(call.StrikeFromDelta(low))
			if high <= 1.4 {
				t.Fatal("wrong premium-adjusted call branch")
			}
			blackDeltaNear(t, pricingMust(call.DeltaFromStrike(high)), low, 1e-10)
			if _, err := call.StrikeFromDelta(.9); err == nil {
				t.Fatal("infeasible delta")
			}
		}
		zero := pricingMust(s.NewBlackDeltaCalculator(Put, convention, 1, 1, 1, 0))
		blackDeltaNear(t, pricingMust(zero.DeltaFromStrike(1)), -.5, 0)
		want = -1.
		if convention >= DeltaPaSpot {
			want = -2.
		}
		blackDeltaNear(t, pricingMust(zero.DeltaFromStrike(2)), want, 0)
		if _, err := zero.StrikeFromDelta(-.25); err == nil {
			t.Fatal("zero-vol inverse")
		}
	}
	for i := 0; i < 4; i++ {
		for _, bad := range []float64{math.NaN(), math.Inf(1), math.Inf(-1), -1} {
			args := []float64{1, 1, 1, .1}
			args[i] = bad
			if _, err := s.NewBlackDeltaCalculator(Call, DeltaSpot, args[0], args[1], args[2], args[3]); err == nil {
				t.Fatal("invalid constructor", i, bad)
			}
		}
	}
	for _, pair := range [][2]int32{{2, 0}, {-1, 0}, {0, 4}, {0, -1}} {
		if _, err := s.NewBlackDeltaCalculator(OptionType(pair[0]), DeltaType(pair[1]), 1, 1, 1, .1); err == nil {
			t.Fatal("invalid enum")
		}
	}
	c := pricingMust(s.NewBlackDeltaCalculator(Call, DeltaFwd, 1, 1, 1, .1))
	for _, bad := range []float64{math.NaN(), math.Inf(1), -1} {
		if _, err := c.DeltaFromStrike(bad); err == nil {
			t.Fatal("invalid strike")
		}
	}
	for _, bad := range []float64{math.NaN(), math.Inf(1), -.1, 0, 1, 2} {
		if _, err := c.StrikeFromDelta(bad); err == nil {
			t.Fatal("invalid inverse")
		}
	}
	if _, err := c.AtmStrike(AtmType(99)); err == nil {
		t.Fatal("invalid ATM enum")
	}
}

func TestBlackDeltaSessionsCloseAndConcurrency(t *testing.T) {
	s := pricingMust(NewSession())
	other := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()); pricingOK(t, other.Close()) })
	c := pricingMust(s.NewBlackDeltaCalculator(Call, DeltaFwd, 1, 1, 1, .1))
	var nilCalc *BlackDeltaCalculator
	if _, err := nilCalc.DeltaFromStrike(1); err == nil {
		t.Fatal("nil calculator")
	}
	var empty BlackDeltaCalculator
	if _, err := empty.AtmStrike(AtmSpot); err == nil {
		t.Fatal("uninitialized calculator")
	}
	if err := sameSession(other, c.object); !errors.Is(err, ErrSessionMismatch) {
		t.Fatal("session mismatch", err)
	}
	forged := &BlackDeltaCalculator{object{other, c.id}}
	if _, err := forged.DeltaFromStrike(1); err == nil {
		t.Fatal("foreign handle")
	}
	wrong := pricingMust(s.NewG2Process(.1, .01, .2, .02, 0))
	wrongType := &BlackDeltaCalculator{wrong.object}
	if _, err := wrongType.DeltaFromStrike(1); err == nil {
		t.Fatal("wrong handle type")
	}
	var wg sync.WaitGroup
	for i := 0; i < 16; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			v, err := c.AtmStrike(AtmFwd)
			if err != nil || v != 1 {
				t.Errorf("concurrent query %v %v", v, err)
			}
		}()
	}
	wg.Wait()
	copyCalc := *c
	pricingOK(t, c.Close())
	pricingOK(t, c.Close())
	if _, err := copyCalc.DeltaFromStrike(1); err == nil {
		t.Fatal("released alias")
	}
	live := pricingMust(s.NewBlackDeltaCalculator(Put, DeltaSpot, 1, 1, 1, .1))
	pricingOK(t, s.Close())
	if _, err := live.AtmStrike(AtmSpot); !errors.Is(err, ErrClosed) {
		t.Fatal("closed session", err)
	}
	pricingOK(t, live.Close())
}

func TestBlackDeltaExtremeFiniteDomains(t *testing.T) {
	s := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()) })
	for _, a := range [][4]float64{{0, 1, 1, .1}, {1, 0, 1, .1}, {1, 1, 0, .1}, {math.MaxFloat64, math.SmallestNonzeroFloat64, 1, .1}, {1, 1, 1, math.MaxFloat64}} {
		if _, err := s.NewBlackDeltaCalculator(Call, DeltaFwd, a[0], a[1], a[2], a[3]); err == nil {
			t.Fatal("extreme domain accepted", a)
		}
	}
	tiny := pricingMust(s.NewBlackDeltaCalculator(Call, DeltaFwd, 1e-300, 1, 1, .1))
	if _, err := tiny.DeltaFromStrike(math.MaxFloat64); err == nil {
		t.Fatal("overflowing ratio accepted")
	}
}
