package itofin

import (
	"strings"
	"testing"
)

const (
	parameterX = 0.75
	parameterH = 0.0625
)

func parameterFixture(t *testing.T, s *Session) (*SimpleQuote, []Quote) {
	t.Helper()
	q := pricingMust(s.NewSimpleQuote(parameterX))
	square := pricingMust(s.NewCompositeQuote(q, q, CompositeMultiply))
	triple := pricingMust(s.NewDerivedQuote(q, 3, 0))
	return q, []Quote{square, triple}
}

func optional(v float64) *float64 { return &v }

func parameterSlots(t *testing.T, got []ParameterSensitivity, want [][2]*float64) {
	t.Helper()
	if len(got) != len(want) {
		t.Fatalf("got %d slots want %d", len(got), len(want))
	}
	same := func(a, b *float64) bool { return (a == nil) == (b == nil) && (a == nil || *a == *b) }
	for i, w := range want {
		if !same(got[i].First, w[0]) || !same(got[i].Second, w[1]) {
			t.Fatalf("slot %d: got %v want %v", i, got[i], w)
		}
	}
}

func parameterRestored(t *testing.T, q *SimpleQuote, want float64) {
	t.Helper()
	if v := pricingMust(q.Value()); v != want {
		t.Fatalf("quote %v not restored to %v", v, want)
	}
}

// Analytic oracle of crates/libitofin/tests/parameter_sensitivity.rs; there is
// no QuantLib test-suite case, the parameter forms were removed upstream.
func TestParameterSensitivityAnalyticOracle(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	q, parameters := parameterFixture(t, s)
	centered := pricingMust(s.ParameterSensitivity(q, parameters, nil, parameterH, Centered))
	parameterSlots(t, centered, [][2]*float64{{optional(1.5), optional(2)}, {optional(3), optional(0)}})
	oneSide := pricingMust(s.ParameterSensitivity(q, parameters, nil, parameterH, OneSide))
	parameterSlots(t, oneSide, [][2]*float64{{optional(1.5625), nil}, {optional(3), nil}})
	references := []*float64{optional(parameterX * parameterX), nil}
	skipped := pricingMust(s.ParameterSensitivity(q, parameters, references, parameterH, Centered))
	parameterSlots(t, skipped, [][2]*float64{{optional(1.5), optional(2)}, {nil, nil}})
	parameterRestored(t, q, parameterX)
}

func TestParameterSensitivityVectorIsOneRowPerQuote(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	q1 := pricingMust(s.NewSimpleQuote(parameterX))
	q2 := pricingMust(s.NewSimpleQuote(0.5))
	parameters := []Quote{pricingMust(s.NewCompositeQuote(q1, q1, CompositeMultiply)), pricingMust(s.NewDerivedQuote(q2, 5, 0))}
	rows := pricingMust(s.ParameterSensitivityVector([]*SimpleQuote{q1, q2}, parameters, parameterH, Centered))
	want := [][]float64{{1.5, 0}, {0, 5}}
	for r, q := range []*SimpleQuote{q1, q2} {
		for c := range parameters {
			if got := *rows[r][c].First; got != want[r][c] {
				t.Fatalf("row %d col %d: got %v want %v", r, c, got, want[r][c])
			}
		}
		single := pricingMust(s.ParameterSensitivity(q, parameters, nil, parameterH, Centered))
		parameterSlots(t, rows[r], [][2]*float64{{single[0].First, single[0].Second}, {single[1].First, single[1].Second}})
	}
	parameterRestored(t, q1, parameterX)
	parameterRestored(t, q2, 0.5)
}

func TestParameterSensitivityErrorsLeaveTheQuote(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	q, parameters := parameterFixture(t, s)
	cases := []struct {
		name string
		run  func() error
		want string
	}{
		{"zero shift", func() error {
			_, e := s.ParameterSensitivity(q, parameters, nil, 0, Centered)
			return e
		}, "zero shift not allowed"},
		{"no parameters", func() error {
			_, e := s.ParameterSensitivity(q, nil, nil, parameterH, Centered)
			return e
		}, "empty parameters vector"},
		{"reference size", func() error {
			_, e := s.ParameterSensitivity(q, parameters, []*float64{nil}, parameterH, Centered)
			return e
		}, "referenceValues has size 1, instead of 2"},
		{"empty references", func() error {
			_, e := s.ParameterSensitivity(q, parameters, []*float64{}, parameterH, Centered)
			return e
		}, "referenceValues has size 0, instead of 2"},
		{"kind", func() error {
			_, e := s.ParameterSensitivity(q, parameters, nil, parameterH, SensitivityAnalysis(2))
			return e
		}, "invalid sensitivity analysis kind"},
		{"no quotes", func() error {
			_, e := s.ParameterSensitivityVector(nil, parameters, parameterH, Centered)
			return e
		}, "empty SimpleQuote vector"},
		{"nil parameter", func() error {
			_, e := s.ParameterSensitivity(q, []Quote{nil}, nil, parameterH, Centered)
			return e
		}, "parameter must not be nil"},
	}
	for _, c := range cases {
		if e := c.run(); e == nil || !strings.HasSuffix(e.Error(), c.want) {
			t.Fatalf("%s: got %v want %q", c.name, e, c.want)
		}
	}
	other := pricingMust(NewSession())
	defer other.Close()
	if _, e := other.ParameterSensitivity(q, parameters, nil, parameterH, Centered); e != ErrSessionMismatch {
		t.Fatalf("got %v want ErrSessionMismatch", e)
	}
	parameterRestored(t, q, parameterX)
}
