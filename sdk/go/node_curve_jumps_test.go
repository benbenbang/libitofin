package itofin

import "testing"

type nodeJumpKind struct {
	name   string
	values []float64
	build  func(*Session, NodeCurveConfig) (*YieldTermStructure, error)
	interp string
}

func nodeJumpKinds() []nodeJumpKind {
	rates := []float64{.03, .031, .032, .033, .034}
	discounts := []float64{1, .992, .984, .967, .935}
	return []nodeJumpKind{
		{"linear-zero", rates, (*Session).NewZeroCurve, "Linear"},
		{"cubic-zero", rates, (*Session).NewZeroCurve, "Cubic"},
		{"loglinear-discount", discounts, (*Session).NewDiscountCurve, "LogLinear"},
		{"cubic-discount", discounts, (*Session).NewDiscountCurve, "Cubic"},
		{"backward-flat-forward", rates, (*Session).NewForwardCurve, ""},
	}
}

// Node curves keep their nodes under jumps, so on every kind the jumped
// discount is the plain discount times the jumps strictly before the query
// date: a reference-date jump never applies, nor does a jump at the query date.
func TestNodeCurveJumpsScalePlainDiscount(t *testing.T) {
	for _, k := range nodeJumpKinds() {
		t.Run(k.name, func(t *testing.T) {
			s := sessionMust(NewSession())
			t.Cleanup(func() { _ = s.Close() })
			cfg := NodeCurveConfig{
				Dates:         []Date{curveDate(t, 17, 6, 2026), curveDate(t, 17, 9, 2026), curveDate(t, 17, 12, 2026), curveDate(t, 17, 6, 2027), curveDate(t, 17, 6, 2028)},
				Values:        k.values,
				DayCounter:    sessionMust(s.Actual365Fixed()),
				Calendar:      sessionMust(s.Target()),
				Interpolation: k.interp,
			}
			plain := sessionMust(k.build(s, cfg))
			jumpDates := []Date{cfg.Dates[0], curveDate(t, 1, 8, 2026), curveDate(t, 1, 3, 2027)}
			values := []float64{.9, .995, .99}
			for _, v := range values {
				cfg.JumpQuotes = append(cfg.JumpQuotes, sessionMust(s.NewSimpleQuote(v)))
			}
			cfg.JumpDates = jumpDates
			jumped := sessionMust(k.build(s, cfg))
			if got := sessionMust(jumped.JumpDates()); len(got) != 3 || got[0] != jumpDates[0] || got[1] != jumpDates[1] || got[2] != jumpDates[2] {
				t.Fatalf("jump dates %v", got)
			}
			for _, q := range []Date{cfg.Dates[0], curveDate(t, 20, 7, 2026), jumpDates[1], curveDate(t, 2, 8, 2026), jumpDates[2], curveDate(t, 1, 6, 2027), curveDate(t, 1, 1, 2029)} {
				product := 1.0
				for i, d := range jumpDates {
					if cfg.Dates[0].Serial() < d.Serial() && d.Serial() < q.Serial() {
						product *= values[i]
					}
				}
				p, j := sessionMust(plain.DiscountDate(q, true)), sessionMust(jumped.DiscountDate(q, true))
				curveNear(t, j, p*product, 1e-15)
				if q == jumpDates[1] && j != p {
					t.Fatalf("jump applied at its own date: %v vs %v", j, p)
				}
			}
			before := sessionMust(jumped.Discount(1.5, true))
			curveNear(t, before, sessionMust(plain.Discount(1.5, true))*.995*.99, 1e-15)
			if err := cfg.JumpQuotes[2].SetValue(.98); err != nil {
				t.Fatal(err)
			}
			curveNear(t, sessionMust(jumped.Discount(1.5, true)), before*.98/.99, 1e-15)
		})
	}
}

func TestNodeCurveJumpsGenerateYearEnds(t *testing.T) {
	for _, k := range nodeJumpKinds() {
		t.Run(k.name, func(t *testing.T) {
			s := sessionMust(NewSession())
			t.Cleanup(func() { _ = s.Close() })
			cfg := NodeCurveConfig{
				Dates:         []Date{curveDate(t, 17, 6, 2026), curveDate(t, 17, 9, 2026), curveDate(t, 17, 12, 2026), curveDate(t, 17, 6, 2027), curveDate(t, 17, 6, 2028)},
				Values:        k.values,
				DayCounter:    sessionMust(s.Actual365Fixed()),
				Interpolation: k.interp,
				JumpQuotes:    []*SimpleQuote{sessionMust(s.NewSimpleQuote(.995)), sessionMust(s.NewSimpleQuote(.99))},
			}
			got := sessionMust(sessionMust(k.build(s, cfg)).JumpDates())
			if len(got) != 2 || got[0] != curveDate(t, 31, 12, 2026) || got[1] != curveDate(t, 31, 12, 2027) {
				t.Fatalf("generated jump dates %v", got)
			}
		})
	}
}

func TestNodeCurveJumpsInvalidArguments(t *testing.T) {
	s := sessionMust(NewSession())
	t.Cleanup(func() { _ = s.Close() })
	other := sessionMust(NewSession())
	t.Cleanup(func() { _ = other.Close() })
	cfg := NodeCurveConfig{
		Dates:      []Date{curveDate(t, 17, 6, 2026), curveDate(t, 17, 6, 2027)},
		Values:     []float64{.03, .031},
		DayCounter: sessionMust(s.Actual365Fixed()),
	}
	q := sessionMust(s.NewSimpleQuote(.99))
	for name, c := range map[string]NodeCurveConfig{
		"nil quote":     {JumpQuotes: []*SimpleQuote{nil}},
		"other session": {JumpQuotes: []*SimpleQuote{sessionMust(other.NewSimpleQuote(.99))}},
		"mismatch":      {JumpQuotes: []*SimpleQuote{q}, JumpDates: []Date{cfg.Dates[1], cfg.Dates[1]}},
		"dates only":    {JumpDates: []Date{cfg.Dates[1]}},
	} {
		c.Dates, c.Values, c.DayCounter = cfg.Dates, cfg.Values, cfg.DayCounter
		if _, err := s.NewZeroCurve(c); err == nil {
			t.Fatalf("%s: expected an error", name)
		}
	}
}
