package itofin

import (
	"errors"
	"math"
	"testing"
)

func quoteCode(t *testing.T, err error, code int32) {
	t.Helper()
	var e *Error
	if !errors.As(err, &e) || e.Code != code {
		t.Fatalf("got %v want code %d", err, code)
	}
}

// QuantLib test-suite/quotes.cpp testDerived (add10, mul10, sub10 as a*x+b)
// and testComposite (add, multiply, subtract).
func TestDependentQuotesQuantLibOracle(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	me1 := pricingMust(s.NewSimpleQuote(0))
	me2 := pricingMust(s.NewSimpleQuote(0))
	derived := []struct {
		a, b float64
		f    func(float64) float64
	}{
		{1, 10, func(x float64) float64 { return x + 10 }},
		{10, 0, func(x float64) float64 { return x * 10 }},
		{1, -10, func(x float64) float64 { return x - 10 }},
	}
	for _, d := range derived {
		q := pricingMust(s.NewDerivedQuote(me1, d.a, d.b))
		for _, v := range []float64{12, 23, 34} {
			pricingOK(t, me1.SetValue(v))
			curveNear(t, pricingMust(q.Value()), d.f(v), 1e-10)
		}
	}
	composite := []struct {
		op CompositeOp
		f  func(x, y float64) float64
	}{
		{CompositeAdd, func(x, y float64) float64 { return x + y }},
		{CompositeMultiply, func(x, y float64) float64 { return x * y }},
		{CompositeSubtract, func(x, y float64) float64 { return x - y }},
	}
	for _, c := range composite {
		q := pricingMust(s.NewCompositeQuote(me1, me2, c.op))
		for _, v := range []float64{12, 23, 34} {
			pricingOK(t, me1.SetValue(v))
			pricingOK(t, me2.SetValue(v+1))
			curveNear(t, pricingMust(q.Value()), c.f(v, v+1), 1e-10)
		}
	}
}

func TestDependentQuotesChainAndRepriceFlatForward(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	x := pricingMust(s.NewSimpleQuote(0.75))
	square := pricingMust(s.NewCompositeQuote(x, x, CompositeMultiply))
	chained := pricingMust(s.NewDerivedQuote(pricingMust(s.NewDerivedQuote(x, 3, 0)), 2, 1))
	mixed := pricingMust(s.NewCompositeQuote(square, chained, CompositeAdd))
	curveNear(t, pricingMust(mixed.Value()), 6.0625, 0)
	pricingOK(t, x.SetValue(2))
	curveNear(t, pricingMust(square.Value()), 4, 0)
	curveNear(t, pricingMust(chained.Value()), 13, 0)
	curveNear(t, pricingMust(mixed.Value()), 17, 0)

	dc := pricingMust(s.Actual360())
	base := pricingMust(s.NewSimpleQuote(0.01))
	rate := pricingMust(s.NewDerivedQuote(base, 2, 0))
	curve := pricingMust(s.NewFlatForwardFromQuote(curveDate(t, 15, 6, 2026), rate, dc))
	curveNear(t, pricingMust(curve.Discount(2, false)), math.Exp(-0.04), 1e-15)
	pricingOK(t, rate.Close())
	pricingOK(t, dc.Close())
	pricingOK(t, base.SetValue(0.02))
	curveNear(t, pricingMust(curve.Discount(2, false)), math.Exp(-0.08), 1e-15)
	if _, err := rate.Value(); err == nil {
		t.Fatal("released derived quote accepted")
	}
}

func TestDependentQuotesInvalidInputs(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	q := pricingMust(s.NewSimpleQuote(1))
	_, err := s.NewCompositeQuote(q, q, CompositeOp(3))
	quoteCode(t, err, 1)
	dc := pricingMust(s.Actual360())
	notQuote := &SimpleQuote{dc.object}
	_, err = s.NewDerivedQuote(notQuote, 1, 0)
	quoteCode(t, err, 2)
	_, err = s.NewCompositeQuote(q, notQuote, CompositeAdd)
	quoteCode(t, err, 2)
	if _, err = s.NewDerivedQuote(nil, 1, 0); err == nil {
		t.Fatal("nil source accepted")
	}
	var typedNil *DerivedQuote
	if _, err = s.NewDerivedQuote(typedNil, 1, 0); err == nil {
		t.Fatal("typed nil source accepted")
	}
	if _, err = s.NewFlatForwardFromQuote(curveDate(t, 15, 6, 2026), (*SimpleQuote)(nil), dc); err == nil {
		t.Fatal("typed nil flat-forward quote accepted")
	}
	other := pricingMust(NewSession())
	defer other.Close()
	if _, err = other.NewDerivedQuote(q, 1, 0); !errors.Is(err, ErrSessionMismatch) {
		t.Fatalf("got %v want session mismatch", err)
	}
}
