package itofin

import (
	"errors"
	"math"
	"strings"
	"sync"
	"testing"
)

func TestBondPriceATMUnitsDefaultsAndLifecycle(t *testing.T) {
	s, err := NewSession()
	bondCheck(t, err)
	defer s.Close()
	a := analyticsOracles(t)[0]
	b, settings, dc, curve := analyticsFixture(t, s, a)
	settlement, err := DateFromSerial(a.Settlement)
	bondCheck(t, err)
	dates := []Date{testDate(t, 24, 3, 2026), testDate(t, 24, 9, 2026), testDate(t, 24, 3, 2027), testDate(t, 24, 9, 2027), testDate(t, 24, 3, 2028)}
	annuity := 0.0
	for i, d := range dates[1:] {
		annuity += 100 * float64(d.Serial()-dates[i].Serial()) / 360 * math.Exp(-.032*float64(d.Serial()-settlement.Serial())/365)
	}
	principal := 102 * math.Exp(-.032*float64(dates[4].Serial()-settlement.Serial())/365)
	accrued := 100 * .047 * float64(settlement.Serial()-dates[0].Serial()) / 360
	for _, price := range []float64{-20, 80, 100, 110} {
		want := (price + accrued - principal) / annuity
		got, e := b.ATMRateFromPrice(curve, price, Clean, &settlement)
		bondCheck(t, e)
		analyticsNear(t, got, want)
		got, e = b.ATMRateFromPrice(curve, price+accrued, Dirty, &settlement)
		bondCheck(t, e)
		analyticsNear(t, got, want)
	}
	for _, price := range []float64{math.NaN(), math.Inf(1), math.Inf(-1), math.MaxFloat64} {
		if _, e := b.ATMRateFromPrice(curve, price, Clean, &settlement); e == nil {
			t.Fatal("invalid price accepted")
		}
	}
	if _, e := b.ATMRateFromPrice(curve, 100, BondPriceType(5), &settlement); e == nil {
		t.Fatal("invalid kind accepted")
	}
	if _, e := b.ATMRateFromPrice(nil, 100, Clean, &settlement); e == nil {
		t.Fatal("nil curve accepted")
	}
	var absent *Bond
	if _, e := absent.ATMRateFromPrice(curve, 100, Clean, &settlement); e == nil {
		t.Fatal("nil bond accepted")
	}
	if _, e := b.ATMRateFromPrice(curve, 100, Clean, nil); e == nil {
		t.Fatal("missing evaluation date accepted")
	}
	bondCheck(t, settings.SetEvaluationDate(testDate(t, 19, 6, 2026)))
	actual, e := b.SettlementDate(nil)
	bondCheck(t, e)
	got, e := b.ATMRateFromPrice(curve, 100, Clean, nil)
	bondCheck(t, e)
	explicit, e := b.ATMRateFromPrice(curve, 100, Clean, &actual)
	bondCheck(t, e)
	analyticsNear(t, got, explicit)
	s2, e := NewSession()
	bondCheck(t, e)
	defer s2.Close()
	foreignDC, e := s2.Actual365Fixed()
	bondCheck(t, e)
	foreign, e := s2.NewFlatForward(settlement, .03, foreignDC)
	bondCheck(t, e)
	if _, e := b.ATMRateFromPrice(foreign, 100, Clean, &settlement); !errors.Is(e, ErrSessionMismatch) {
		t.Fatalf("foreign curve: %v", e)
	}
	callable, _, _, _, callableCurve := callableFoundationFixture(t, s, "zero")
	if _, e := callable.ATMRateFromPrice(callableCurve, 100, Clean, &settlement); e == nil || !strings.Contains(e.Error(), "callable") {
		t.Fatalf("callable backing accepted: %v", e)
	}
	bondCheck(t, settings.Close())
	bondCheck(t, dc.Close())
	var wg sync.WaitGroup
	for range 4 {
		wg.Go(func() {
			for range 8 {
				v, e := b.ATMRateFromPrice(curve, 100, Clean, &settlement)
				if e != nil || math.IsNaN(v) || math.IsInf(v, 0) || math.Abs(v-(100+accrued-principal)/annuity) > 3e-12 {
					t.Errorf("concurrent price ATM %g %v", v, e)
				}
			}
		})
	}
	wg.Wait()
	bondCheck(t, curve.Close())
	if _, e := b.ATMRateFromPrice(curve, 100, Clean, &settlement); e == nil {
		t.Fatal("closed curve accepted")
	}
	bondCheck(t, b.Close())
	if _, e := b.ATMRateFromPrice(foreign, 100, Clean, &settlement); e == nil {
		t.Fatal("closed bond accepted")
	}
	bondCheck(t, s.Close())
	if _, e := b.ATMRateFromPrice(curve, 100, Clean, &settlement); !errors.Is(e, ErrClosed) {
		t.Fatalf("closed session: %v", e)
	}
}

func TestBondPriceATMFloatingGuardsAndFrozenGraph(t *testing.T) {
	s, e := NewSession()
	bondCheck(t, e)
	defer s.Close()
	missing := false
	for _, a := range floatingOracles(t) {
		if !a.Historical || a.ExCoupon {
			continue
		}
		m := floatingFixture(t, s, a, false)
		settlement, e := DateFromSerial(a.Today)
		bondCheck(t, e)
		for _, kind := range []BondPriceType{Clean, Dirty} {
			if _, e := m.bond.ATMRateFromPrice(m.discountCurve, 100, kind, &settlement); e == nil {
				t.Fatal("missing fixing accepted")
			}
		}
		bondCheck(t, m.index.AddFixing(testDate(t, 13, 7, 2024), .0275))
		bondCheck(t, m.bond.SetCouponPricer(m.pricer))
		for _, kind := range []BondPriceType{Clean, Dirty} {
			value, err := m.bond.ATMRateFromPrice(m.discountCurve, 100, kind, &settlement)
			bondCheck(t, err)
			if math.IsNaN(value) || math.IsInf(value, 0) {
				t.Fatal("fixing recovery returned nonfinite rate")
			}
		}
		missing = true
		break
	}
	if !missing {
		t.Fatal("no fixing fixture")
	}
	a := floatingOracles(t)[0]
	m := floatingFixture(t, s, a, true)
	settlement, e := DateFromSerial(a.Today)
	bondCheck(t, e)
	cached, e := m.bond.NPV()
	bondCheck(t, e)
	bondCheck(t, m.bond.Freeze())
	before, e := m.bond.ATMRateFromPrice(m.discountCurve, 100, Dirty, &settlement)
	bondCheck(t, e)
	bondCheck(t, m.rate.SetValue(.08))
	bondCheck(t, m.volatility.SetValue(.40))
	got, e := m.bond.ATMRateFromPrice(m.discountCurve, 100, Dirty, &settlement)
	bondCheck(t, e)
	analyticsNear(t, got, before)
	bondCheck(t, m.discount.SetValue(.07))
	changed, e := m.bond.ATMRateFromPrice(m.discountCurve, 100, Dirty, &settlement)
	bondCheck(t, e)
	if math.IsNaN(changed) || math.IsInf(changed, 0) || changed == before {
		t.Fatal("live discount ignored")
	}
	got, e = m.bond.NPV()
	bondCheck(t, e)
	analyticsNear(t, got, cached)
	for _, close := range []func() error{m.rate.Close, m.volatility.Close, m.index.Close, m.forward.Close, m.settings.Close, m.pricer.Close} {
		bondCheck(t, close())
	}
	got, e = m.bond.ATMRateFromPrice(m.discountCurve, 100, Dirty, &settlement)
	bondCheck(t, e)
	analyticsNear(t, got, changed)
}
