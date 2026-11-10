package itofin

import (
	"math"
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

// Two quotes x = 0.75, y = 0.5 against three parameters x * x, x - y and
// x * y, centered with h = 0.0625. The differences are exact: d/dx gives
// (2x, 2), (1, 0), (y, 0) and d/dy gives (0, 0), (-1, 0), (x, 0). The 2 x 3
// buffer is asymmetric, so a column-major read or swapped counts fail.
func TestParameterSensitivityVectorIsOneRowPerQuote(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	q1 := pricingMust(s.NewSimpleQuote(parameterX))
	q2 := pricingMust(s.NewSimpleQuote(0.5))
	parameters := []Quote{
		pricingMust(s.NewCompositeQuote(q1, q1, CompositeMultiply)),
		pricingMust(s.NewCompositeQuote(q1, q2, CompositeSubtract)),
		pricingMust(s.NewCompositeQuote(q1, q2, CompositeMultiply)),
	}
	rows := pricingMust(s.ParameterSensitivityVector([]*SimpleQuote{q1, q2}, parameters, parameterH, Centered))
	want := [][][2]float64{{{1.5, 2}, {1, 0}, {0.5, 0}}, {{0, 0}, {-1, 0}, {parameterX, 0}}}
	if len(rows) != len(want) {
		t.Fatalf("got %d rows want %d", len(rows), len(want))
	}
	for r, q := range []*SimpleQuote{q1, q2} {
		cells := make([][2]*float64, len(want[r]))
		for c, w := range want[r] {
			cells[c] = [2]*float64{optional(w[0]), optional(w[1])}
		}
		parameterSlots(t, rows[r], cells)
		single := pricingMust(s.ParameterSensitivity(q, parameters, nil, parameterH, Centered))
		for c := range single {
			cells[c] = [2]*float64{single[c].First, single[c].Second}
		}
		parameterSlots(t, rows[r], cells)
	}
	parameterRestored(t, q1, parameterX)
	parameterRestored(t, q2, 0.5)
}

// A supplied reference of 0.0 enters the formulas: for x * x at x = 0.75,
// h = 0.0625, the centered second is ((x + h)^2 + (x - h)^2) / h^2 = 290 and
// the one-sided first is (x + h)^2 / h = 10.5625.
func TestParameterSensitivityZeroReferenceIsAValue(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	q, parameters := parameterFixture(t, s)
	references := []*float64{optional(0), nil}
	centered := pricingMust(s.ParameterSensitivity(q, parameters, references, parameterH, Centered))
	parameterSlots(t, centered, [][2]*float64{{optional(1.5), optional(290)}, {nil, nil}})
	oneSide := pricingMust(s.ParameterSensitivity(q, parameters, references, parameterH, OneSide))
	parameterSlots(t, oneSide, [][2]*float64{{optional(10.5625), nil}, {nil, nil}})
	parameterRestored(t, q, parameterX)
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
	for _, bad := range []float64{math.NaN(), math.Inf(1), math.Inf(-1)} {
		cases = append(cases, []struct {
			name string
			run  func() error
			want string
		}{
			{"non-finite shift", func() error {
				_, e := s.ParameterSensitivity(q, parameters, nil, bad, Centered)
				return e
			}, "non-finite rate or amount"},
			{"non-finite reference", func() error {
				_, e := s.ParameterSensitivity(q, parameters, []*float64{optional(bad), nil}, parameterH, Centered)
				return e
			}, "non-finite rate or amount"},
			{"non-finite vector shift", func() error {
				_, e := s.ParameterSensitivityVector([]*SimpleQuote{q}, parameters, bad, Centered)
				return e
			}, "non-finite rate or amount"},
		}...)
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
