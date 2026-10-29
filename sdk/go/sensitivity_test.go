package itofin

import (
	"math"
	"testing"
)

type sensitivityPortfolio struct {
	quotes      []*SimpleQuote
	instruments []Instrument
	npvs        []func() (float64, error)
}

func sensitivityFixture(t *testing.T, s *Session) sensitivityPortfolio {
	t.Helper()
	bond, quote, settings, _, _, _, curve, _ := bondFixture(t, s, "fixed", 100)
	effective := testDate(t, 17, 1, 2024)
	rate := .045
	ibor, e := s.NewEuriborSixMonths(curve, settings)
	bondCheck(t, e)
	swap, e := s.MakeVanillaSwap(MakeVanillaSwapConfig{Tenor: Period{5, Years}, Index: ibor, Settings: settings, EffectiveDate: &effective, FixedRate: &rate})
	bondCheck(t, e)
	overnight, e := s.NewEstr(curve, settings)
	bondCheck(t, e)
	ois, e := s.MakeOis(MakeOisConfig{Tenor: Period{2, Years}, Index: overnight, Settings: settings, EffectiveDate: &effective, FixedRate: &rate})
	bondCheck(t, e)
	return sensitivityPortfolio{[]*SimpleQuote{quote}, []Instrument{bond, swap, ois}, []func() (float64, error){bond.NPV, swap.NPV, ois.NPV}}
}

func (p sensitivityPortfolio) npvAt(t *testing.T, bump float64, quantities []float64) float64 {
	t.Helper()
	originals := make([]float64, len(p.quotes))
	for i, q := range p.quotes {
		v, e := q.Value()
		bondCheck(t, e)
		originals[i] = v
		bondCheck(t, q.SetValue(v+bump))
	}
	total := 0.0
	for i, npv := range p.npvs {
		v, e := npv()
		bondCheck(t, e)
		total += float64(quantities[i] * v)
	}
	for i, q := range p.quotes {
		bondCheck(t, q.SetValue(originals[i]))
	}
	return total
}

func (p sensitivityPortfolio) checkManual(t *testing.T, s *Session, quantities []float64) {
	t.Helper()
	h := DefaultShift
	before, e := p.quotes[0].Value()
	bondCheck(t, e)
	reference := p.npvAt(t, 0, quantities)
	up := p.npvAt(t, h, quantities)
	down := p.npvAt(t, -h, quantities)
	centered, e := s.ParallelAnalysis(p.quotes, p.instruments, quantities, h, Centered, nil)
	bondCheck(t, e)
	if centered.First != (up-down)/(2.0*h) || centered.Second == nil || *centered.Second != (up-float64(2.0*reference)+down)/(h*h) {
		t.Fatalf("centered %v vs manual", centered)
	}
	oneSide, e := s.ParallelAnalysis(p.quotes, p.instruments, quantities, h, OneSide, &reference)
	bondCheck(t, e)
	if oneSide.First != (up-reference)/h || oneSide.Second != nil {
		t.Fatalf("one-sided %v vs manual", oneSide)
	}
	if after, _ := p.quotes[0].Value(); after != before {
		t.Fatalf("quote not restored: %g", after)
	}
}

func TestSensitivityParallelMatchesManualBump(t *testing.T) {
	s, e := NewSession()
	bondCheck(t, e)
	defer s.Close()
	sensitivityFixture(t, s).checkManual(t, s, []float64{1, 2, -1})
}

func TestSensitivityParallelZeroBondAnalyticDuration(t *testing.T) {
	s, e := NewSession()
	bondCheck(t, e)
	defer s.Close()
	bond, quote, _, _, _, _, _, _ := bondFixture(t, s, "zero", 100)
	npv, e := bond.NPV()
	bondCheck(t, e)
	years := float64(testDate(t, 15, 1, 2027).Serial()-testDate(t, 15, 1, 2024).Serial()) / 365
	got, e := s.ParallelAnalysis([]*SimpleQuote{quote}, []Instrument{bond}, nil, DefaultShift, Centered, nil)
	bondCheck(t, e)
	if math.Abs(got.First/(-years*npv)-1) > 1e-6 || math.Abs(*got.Second/(years*years*npv)-1) > 1e-6 {
		t.Fatalf("%v vs duration %g convexity %g", got, -years*npv, years*years*npv)
	}
}

func TestSensitivityFloatingBondRunsThePreflight(t *testing.T) {
	s, e := NewSession()
	bondCheck(t, e)
	defer s.Close()
	var historical floatingCase
	for _, c := range floatingOracles(t) {
		if c.Name == "historical" {
			historical = c
		}
	}
	m := floatingFixture(t, s, historical, true)
	p := sensitivityPortfolio{[]*SimpleQuote{m.rate, m.discount}, []Instrument{m.bond}, []func() (float64, error){m.bond.NPV}}
	if _, e := s.ParallelAnalysis(p.quotes, p.instruments, nil, DefaultShift, Centered, nil); e == nil {
		t.Fatal("missing fixing passed the preflight")
	}
	bondCheck(t, m.index.AddFixing(testDate(t, 13, 7, 2024), .0275))
	p.checkManual(t, s, []float64{1})
}
