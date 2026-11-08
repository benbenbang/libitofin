package itofin

import (
	"strings"
	"testing"
)

type matrixMarket struct {
	quotes      [][]*SimpleQuote
	instruments []Instrument
	npvs        []func() (float64, error)
}

func matrixFixture(t *testing.T, s *Session) matrixMarket {
	t.Helper()
	m := optionFixture(t, s)
	cfg := PiecewiseCurveConfig{ReferenceDate: testDate(t, 17, 6, 2026), DayCounter: m.dc}
	var deposits []*SimpleQuote
	for i, months := range []int32{3, 6, 12, 24} {
		deposits = append(deposits, sessionMust(s.NewSimpleQuote(.03+.002*float64(i))))
		index := sessionMust(s.NewEuribor(Period{months, Months}, nil, m.settings))
		cfg.Helpers = append(cfg.Helpers, sessionMust(s.NewDepositRateHelper(deposits[i], index)))
	}
	curve := sessionMust(s.NewPiecewiseLinearZero(cfg))
	ibor := sessionMust(s.NewEuriborSixMonths(curve, m.settings))
	rate := .04
	swap := sessionMust(s.MakeVanillaSwap(MakeVanillaSwapConfig{Tenor: Period{2, Years}, Index: ibor, Settings: m.settings, FixedRate: &rate}))
	return matrixMarket{
		[][]*SimpleQuote{{deposits[2], m.rate}, {}, {m.spot, deposits[3]}, {m.vol}},
		[]Instrument{swap, m.option},
		[]func() (float64, error){swap.NPV, m.option.NPV},
	}
}

func TestSensitivityBucketMatrixMatchesManualBump(t *testing.T) {
	s, e := NewSession()
	bondCheck(t, e)
	defer s.Close()
	m := matrixFixture(t, s)
	quantities := []float64{2, -1}
	h := DefaultShift
	whole := sensitivityPortfolio{nil, m.instruments, m.npvs}
	reference := whole.npvAt(t, 0, quantities)
	centered, e := s.BucketAnalysisMatrix(m.quotes, m.instruments, quantities, h, Centered)
	bondCheck(t, e)
	oneSideReference := whole.npvAt(t, 0, quantities)
	oneSide, e := s.BucketAnalysisMatrix(m.quotes, m.instruments, quantities, h, OneSide)
	bondCheck(t, e)
	var flat []*SimpleQuote
	for r, row := range m.quotes {
		if len(centered[r]) != len(row) || len(oneSide[r]) != len(row) {
			t.Fatalf("row %d shape %d/%d vs %d", r, len(centered[r]), len(oneSide[r]), len(row))
		}
		for c, q := range row {
			p := sensitivityPortfolio{[]*SimpleQuote{q}, m.instruments, m.npvs}
			up := p.npvAt(t, h, quantities)
			down := p.npvAt(t, -h, quantities)
			got := centered[r][c]
			if got.First == 0 || got.First != (up-down)/(2.0*h) || got.Second == nil || *got.Second != (up-float64(2.0*reference)+down)/(h*h) {
				t.Fatalf("centered [%d][%d] %v vs manual", r, c, got)
			}
			if oneSide[r][c].First != (up-oneSideReference)/h || oneSide[r][c].Second != nil {
				t.Fatalf("one-sided [%d][%d] %v vs manual", r, c, oneSide[r][c])
			}
		}
		flat = append(flat, row...)
	}
	buckets, e := s.BucketAnalysis(flat, m.instruments, quantities, h, Centered)
	bondCheck(t, e)
	k := 0
	for _, row := range centered {
		for _, got := range row {
			if got.First != buckets[k].First || buckets[k].Second == nil {
				t.Fatalf("matrix %v vs bucket %v", got, buckets[k])
			}
			k++
		}
	}
	empty, e := s.BucketAnalysisMatrix([][]*SimpleQuote{{}}, m.instruments, quantities, h, Centered)
	bondCheck(t, e)
	if len(empty) != 1 || len(empty[0]) != 0 {
		t.Fatalf("all-empty rows %v", empty)
	}
}

func TestSensitivityBucketMatrixErrors(t *testing.T) {
	s, e := NewSession()
	bondCheck(t, e)
	defer s.Close()
	m := matrixFixture(t, s)
	spot := m.quotes[2][0]
	cases := []struct {
		quotes      [][]*SimpleQuote
		instruments []Instrument
		want        string
	}{
		{nil, m.instruments, "empty SimpleQuote range"},
		{[][]*SimpleQuote{}, m.instruments, "empty SimpleQuote range"},
		{m.quotes, []Instrument{&VanillaOption{spot.object}}, "sensitivity analysis supports"},
		{[][]*SimpleQuote{{&SimpleQuote{m.instruments[1].instrumentObject()}}}, m.instruments, "wrong-type handle"},
		{[][]*SimpleQuote{{nil}}, m.instruments, "must not be nil"},
	}
	for _, c := range cases {
		_, e := s.BucketAnalysisMatrix(c.quotes, c.instruments, nil, DefaultShift, Centered)
		if e == nil || !strings.Contains(e.Error(), c.want) {
			t.Fatalf("%v: got %v, want %q", c.quotes, e, c.want)
		}
	}
	if v, _ := spot.Value(); v != 100 {
		t.Fatalf("spot not restored: %g", v)
	}
}
