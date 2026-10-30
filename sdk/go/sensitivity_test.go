package itofin

import (
	"math"
	"strings"
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

func TestSensitivityParallelEqualsTheSumOfBuckets(t *testing.T) {
	s, e := NewSession()
	bondCheck(t, e)
	defer s.Close()
	settings, e := s.NewSettings()
	bondCheck(t, e)
	bondCheck(t, settings.SetEvaluationDate(testDate(t, 15, 6, 2026)))
	var quotes []*SimpleQuote
	cfg := PiecewiseCurveConfig{ReferenceDate: testDate(t, 17, 6, 2026), DayCounter: sessionMust(s.Actual365Fixed())}
	for i, months := range []int32{3, 6, 12, 24} {
		quotes = append(quotes, sessionMust(s.NewSimpleQuote(.03+.002*float64(i))))
		index := sessionMust(s.NewEuribor(Period{months, Months}, nil, settings))
		cfg.Helpers = append(cfg.Helpers, sessionMust(s.NewDepositRateHelper(quotes[i], index)))
	}
	curve := sessionMust(s.NewPiecewiseLinearZero(cfg))
	ibor := sessionMust(s.NewEuriborSixMonths(curve, settings))
	rate := .04
	swap := sessionMust(s.MakeVanillaSwap(MakeVanillaSwapConfig{Tenor: Period{2, Years}, Index: ibor, Settings: settings, FixedRate: &rate}))
	instruments := []Instrument{swap}
	parallel, e := s.ParallelAnalysis(quotes, instruments, nil, DefaultShift, Centered, nil)
	bondCheck(t, e)
	buckets, e := s.BucketAnalysis(quotes, instruments, nil, DefaultShift, Centered)
	bondCheck(t, e)
	sum := 0.0
	for _, b := range buckets {
		if b.Second == nil {
			t.Fatal("centered bucket without second order")
		}
		sum += b.First
	}
	if len(buckets) != len(quotes) || math.Abs(parallel.First) < 1e-3 || math.Abs(sum/parallel.First-1) > 1e-6 {
		t.Fatalf("buckets %v sum %g vs parallel %g", buckets, sum, parallel.First)
	}
	single, e := s.BucketAnalysis(quotes[3:], instruments, nil, DefaultShift, OneSide)
	bondCheck(t, e)
	alone, e := s.ParallelAnalysis(quotes[3:], instruments, nil, DefaultShift, OneSide, nil)
	bondCheck(t, e)
	if math.Abs(single[0].First/alone.First-1) > 1e-9 || single[0].Second != nil {
		t.Fatalf("single bucket %v vs parallel %v", single[0], alone)
	}
}

func TestSensitivityErrorsLeaveTheQuotes(t *testing.T) {
	s, e := NewSession()
	bondCheck(t, e)
	defer s.Close()
	p := sensitivityFixture(t, s)
	bond, _, settings, cal, _, _, _, _ := bondFixture(t, s, "zero", 100)
	unpriced := sessionMust(s.NewZeroCouponBond(ZeroCouponBondConfig{BondConfig: BondConfig{FaceAmount: 100, Settings: settings, Calendar: cal}, MaturityDate: testDate(t, 15, 1, 2027)}))
	cases := []struct {
		instruments []Instrument
		shift       float64
		want        string
	}{
		{p.instruments, 0, "zero shift not allowed"},
		{nil, DefaultShift, "empty instrument vector"},
		{[]Instrument{(*Bond)(nil)}, DefaultShift, "uninitialized object"},
		{[]Instrument{nil}, DefaultShift, "must not be nil"},
		{[]Instrument{bond, unpriced}, DefaultShift, "pricing engine"},
	}
	reference := 100.0
	for _, c := range cases {
		_, e := s.ParallelAnalysis(p.quotes, c.instruments, nil, c.shift, Centered, &reference)
		_, be := s.BucketAnalysis(p.quotes, c.instruments, nil, c.shift, Centered)
		for _, err := range []error{e, be} {
			if err == nil || !strings.Contains(err.Error(), c.want) {
				t.Fatalf("%q: got %v", c.want, err)
			}
		}
		if v, _ := p.quotes[0].Value(); v != .04 {
			t.Fatalf("quote moved: %g", v)
		}
	}
	other := sessionMust(NewSession())
	defer other.Close()
	if _, e := other.BucketAnalysis(p.quotes, p.instruments, nil, DefaultShift, Centered); e == nil {
		t.Fatal("cross-session analysis accepted")
	}
}
