package itofin

import (
	"math"
	"sync"
	"testing"
)

func fxFixture(t *testing.T, s *Session) (FxForwardConfig, *SimpleQuote) {
	t.Helper()
	base, e := s.EUR()
	bondCheck(t, e)
	quote, e := s.USD()
	bondCheck(t, e)
	dc, e := s.Actual365Fixed()
	bondCheck(t, e)
	rate, e := s.NewSimpleQuote(.05)
	bondCheck(t, e)
	bc, e := s.NewFlatForward(testDate(t, 1, 1, 2025), .03, dc)
	bondCheck(t, e)
	qc, e := s.NewFlatForwardFromQuote(testDate(t, 2, 1, 2025), rate, dc)
	bondCheck(t, e)
	return FxForwardConfig{base, quote, 1.1, bc, qc, 1e6, 1.12, testDate(t, 3, 1, 2025), testDate(t, 3, 1, 2026), PositionLong}, rate
}
func TestFxForwardAnalyticalAndFields(t *testing.T) {
	s, e := NewSession()
	bondCheck(t, e)
	defer s.Close()
	a, _ := fxFixture(t, s)
	for _, side := range []Position{PositionLong, PositionShort} {
		a.Position = side
		f, e := s.NewFxForward(a)
		bondCheck(t, e)
		fair, e := f.FairForwardRate()
		bondCheck(t, e)
		if math.Abs(fair-1.1*math.Exp(.02)) > 3e-15 {
			t.Fatal(fair)
		}
		want := 1e6 * (1.1*math.Exp(.02) - 1.12) * math.Exp(-.05*366/365)
		if side == PositionShort {
			want = -want
		}
		got, e := f.NPV(testDate(t, 1, 1, 2025))
		bondCheck(t, e)
		if math.Abs(got-want) > 1e-9 {
			t.Fatalf("%.17g != %.17g", got, want)
		}
		live, e := f.IsExpired(a.DeliveryDate)
		bondCheck(t, e)
		if live {
			t.Fatal("delivery expired")
		}
		got, e = f.NPV(testDate(t, 4, 1, 2026))
		bondCheck(t, e)
		if got != 0 {
			t.Fatal(got)
		}
		expired, e := f.IsExpired(testDate(t, 4, 1, 2026))
		bondCheck(t, e)
		if !expired {
			t.Fatal("expiry")
		}
		spot, e := f.Spot()
		bondCheck(t, e)
		if spot != a.Spot {
			t.Fatal(spot)
		}
		strike, e := f.Strike()
		bondCheck(t, e)
		if strike != a.Strike {
			t.Fatal(strike)
		}
		notional, e := f.BaseNotional()
		bondCheck(t, e)
		if notional != a.BaseNotional {
			t.Fatal(notional)
		}
		settlement, e := f.SpotSettlementDate()
		bondCheck(t, e)
		if settlement != a.SpotSettlementDate {
			t.Fatal(settlement)
		}
		delivery, e := f.DeliveryDate()
		bondCheck(t, e)
		if delivery != a.DeliveryDate {
			t.Fatal(delivery)
		}
		position, e := f.Position()
		bondCheck(t, e)
		if position != side {
			t.Fatal(position)
		}
		for i, get := range []func() (*Currency, error){f.BaseCurrency, f.QuoteCurrency} {
			currency, e := get()
			bondCheck(t, e)
			code, e := currency.Code()
			bondCheck(t, e)
			if code != []string{"EUR", "USD"}[i] {
				t.Fatal(code)
			}
			bondCheck(t, currency.Close())
		}
		bondCheck(t, f.Close())
	}
}
func TestFxForwardRetentionSessionsAndConcurrency(t *testing.T) {
	s, e := NewSession()
	bondCheck(t, e)
	defer s.Close()
	a, q := fxFixture(t, s)
	f, e := s.NewFxForward(a)
	bondCheck(t, e)
	for _, close := range []func() error{a.BaseCurrency.Close, a.QuoteCurrency.Close, a.BaseCurve.Close, a.QuoteCurve.Close} {
		bondCheck(t, close())
	}
	bondCheck(t, q.SetValue(.08))
	got, e := f.FairForwardRate()
	bondCheck(t, e)
	if math.Abs(got-1.1*math.Exp(.05)) > 3e-15 {
		t.Fatal(got)
	}
	var wg sync.WaitGroup
	errors := make(chan error, 8)
	for range 8 {
		wg.Go(func() { _, e := f.NPV(a.SpotSettlementDate); errors <- e })
	}
	wg.Wait()
	close(errors)
	for e := range errors {
		bondCheck(t, e)
	}
	other, e := NewSession()
	bondCheck(t, e)
	defer other.Close()
	b, _ := fxFixture(t, other)
	b.BaseCurve = a.BaseCurve
	if _, e = other.NewFxForward(b); e == nil {
		t.Fatal("mixed session accepted")
	}
	retained, e := f.QuoteCurrency()
	bondCheck(t, e)
	bondCheck(t, f.Close())
	code, e := retained.Code()
	bondCheck(t, e)
	if code != "USD" {
		t.Fatal(code)
	}
	bondCheck(t, retained.Close())
	if _, e = f.FairForwardRate(); e == nil {
		t.Fatal("closed forward accepted")
	}
	var nilForward *FxForward
	if _, e = nilForward.NPV(a.DeliveryDate); e == nil {
		t.Fatal("nil accepted")
	}
	if _, e = nilForward.IsExpired(a.DeliveryDate); e == nil {
		t.Fatal("nil accepted")
	}
	if _, e = nilForward.BaseCurrency(); e == nil {
		t.Fatal("nil accepted")
	}
}
func TestFxForwardInvalidAndExpiredDomains(t *testing.T) {
	s, e := NewSession()
	bondCheck(t, e)
	defer s.Close()
	a, _ := fxFixture(t, s)
	for _, change := range []func(*FxForwardConfig){
		func(a *FxForwardConfig) { a.Spot = math.NaN() }, func(a *FxForwardConfig) { a.Spot = 0 },
		func(a *FxForwardConfig) { a.Strike = math.Inf(1) }, func(a *FxForwardConfig) { a.BaseNotional = -1 },
		func(a *FxForwardConfig) { a.BaseNotional = 1e308; a.Strike = 2 }, func(a *FxForwardConfig) { a.Position = 99 },
		func(a *FxForwardConfig) { a.DeliveryDate = Date{} }, func(a *FxForwardConfig) { a.BaseCurrency = nil },
		func(a *FxForwardConfig) { a.QuoteCurrency = a.BaseCurrency },
		func(a *FxForwardConfig) { a.DeliveryDate = testDate(t, 2, 1, 2025) },
	} {
		bad := a
		change(&bad)
		if _, e = s.NewFxForward(bad); e == nil {
			t.Fatal("invalid accepted")
		}
	}
	dc, e := s.Actual365Fixed()
	bondCheck(t, e)
	a.BaseCurve, e = s.NewFlatForward(testDate(t, 4, 1, 2025), .03, dc)
	bondCheck(t, e)
	f, e := s.NewFxForward(a)
	bondCheck(t, e)
	if _, e = f.FairForwardRate(); e == nil {
		t.Fatal("invalid reference accepted")
	}
	if _, e = f.NPV(a.DeliveryDate); e == nil {
		t.Fatal("active invalid curve accepted")
	}
	got, e := f.NPV(testDate(t, 4, 1, 2026))
	bondCheck(t, e)
	if got != 0 {
		t.Fatal(got)
	}
	if _, e = f.NPV(Date{}); e == nil {
		t.Fatal("null evaluation accepted")
	}
	if _, e = f.IsExpired(Date{}); e == nil {
		t.Fatal("null evaluation accepted")
	}
}
