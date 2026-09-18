package itofin

import (
	"strings"
	"testing"
)

func TestIndexFamiliesAndConventionInspectors(t *testing.T) {
	s, e := NewSession()
	s = curveMust(t, s, e)
	defer s.Close()
	settings, e := s.NewSettings()
	settings = curveMust(t, settings, e)
	today := curveDate(t, 15, 6, 2026)
	if e = settings.SetEvaluationDate(today); e != nil {
		t.Fatal(e)
	}
	dc, e := s.Actual365Fixed()
	dc = curveMust(t, dc, e)
	cal, e := s.WeekendsOnly()
	cal = curveMust(t, cal, e)
	curve, e := s.NewFlatForward(today, 0.04, dc)
	curve = curveMust(t, curve, e)
	for _, tc := range []struct {
		code string
		new  func(*Session) (*Currency, error)
	}{{"EUR", (*Session).EUR}, {"USD", (*Session).USD}, {"GBP", (*Session).GBP}, {"JPY", (*Session).JPY}} {
		c, e := tc.new(s)
		c = curveMust(t, c, e)
		code, e := c.Code()
		if curveMust(t, code, e) != tc.code {
			t.Fatal("wrong currency")
		}
		repr, e := c.Repr()
		if curveMust(t, repr, e) != "Currency("+tc.code+")" {
			t.Fatal("wrong repr")
		}
	}
	usd, e := s.USD()
	usd = curveMust(t, usd, e)
	cfg := IborIndexConfig{FamilyName: "Prøbe\x00Suffix", Tenor: Period{3, Months}, SettlementDays: 3, Currency: usd, FixingCalendar: cal, Convention: Preceding, DayCounter: dc, Forwarding: curve, Settings: settings}
	idx, e := s.NewIborIndex(cfg)
	idx = curveMust(t, idx, e)
	tenor, e := idx.Tenor()
	if curveMust(t, tenor, e) != cfg.Tenor {
		t.Fatal("tenor changed")
	}
	convention, e := idx.BusinessDayConvention()
	if curveMust(t, convention, e) != Preceding {
		t.Fatal("convention changed")
	}
	eom, e := idx.EndOfMonth()
	if curveMust(t, eom, e) {
		t.Fatal("end of month changed")
	}
	idc, e := idx.DayCounter()
	idc = curveMust(t, idc, e)
	n, e := idc.Name()
	if curveMust(t, n, e) != "Actual/365 (Fixed)" {
		t.Fatalf("wrong day counter %q", n)
	}
	ical, e := idx.FixingCalendar()
	ical = curveMust(t, ical, e)
	n, e = ical.Name()
	if curveMust(t, n, e) != "weekends only" {
		t.Fatalf("wrong calendar %q", n)
	}
	cur, e := idx.Currency()
	cur = curveMust(t, cur, e)
	n, e = cur.Code()
	if curveMust(t, n, e) != "USD" {
		t.Fatal("wrong index currency")
	}
	n, e = idx.Name()
	if !strings.HasPrefix(curveMust(t, n, e), cfg.FamilyName) {
		t.Fatal("index family name was truncated")
	}
	value, e := idx.ValueDate(today)
	value = curveMust(t, value, e)
	if value != curveDate(t, 18, 6, 2026) {
		t.Fatal("settlement lag changed")
	}
	fixing, e := idx.FixingDate(value)
	if curveMust(t, fixing, e) != today {
		t.Fatal("fixing date inverse failed")
	}
	maturity, e := idx.MaturityDate(value)
	maturity = curveMust(t, maturity, e)
	if maturity != curveDate(t, 18, 9, 2026) {
		t.Fatal("maturity changed")
	}
	v, e := idx.Fixing(today, true)
	v = curveMust(t, v, e)
	d1, e := curve.DiscountDate(value, false)
	d1 = curveMust(t, d1, e)
	d2, e := curve.DiscountDate(maturity, false)
	d2 = curveMust(t, d2, e)
	tau := float64(maturity.Serial()-value.Serial()) / 365
	curveNear(t, v, (d1/d2-1)/tau, 1e-12)
	if _, e = idx.Fixing(curveDate(t, 12, 6, 2026), false); e == nil {
		t.Fatal("missing historical fixing accepted")
	}
	for _, factory := range []func(*Session, Period, *YieldTermStructure, *Settings) (*IborIndex, error){(*Session).NewEuribor, (*Session).NewUsdLibor, (*Session).NewJpyLibor, (*Session).NewGbpLibor, (*Session).NewEurLibor} {
		i, e := factory(s, Period{6, Months}, curve, settings)
		i = curveMust(t, i, e)
		v, e := i.Fixing(today, true)
		curveMust(t, v, e)
		if _, e = factory(s, Period{1, Days}, curve, settings); e == nil {
			t.Fatal("daily tenor accepted")
		}
	}
	for _, factory := range []func(*Session, *YieldTermStructure, *Settings) (*IborIndex, error){(*Session).NewEuriborThreeMonths, (*Session).NewEuriborSixMonths} {
		i, e := factory(s, curve, settings)
		i = curveMust(t, i, e)
		v, e := i.Fixing(today, true)
		curveMust(t, v, e)
	}
	cfg.ValueCalendar = cal
	cfg.MaturityCalendar = cal
	custom, e := s.NewCustomIborIndex(cfg)
	custom = curveMust(t, custom, e)
	v, e = custom.Fixing(today, true)
	curveNear(t, curveMust(t, v, e), (d1/d2-1)/tau, 1e-12)
	overnight, e := s.NewEstr(curve, settings)
	overnight = curveMust(t, overnight, e)
	v, e = overnight.Fixing(today, true)
	curveMust(t, v, e)
	if e = dc.Close(); e != nil {
		t.Fatal(e)
	}
	if e = cal.Close(); e != nil {
		t.Fatal(e)
	}
	if e = usd.Close(); e != nil {
		t.Fatal(e)
	}
	if e = curve.Close(); e != nil {
		t.Fatal(e)
	}
	if e = settings.Close(); e != nil {
		t.Fatal(e)
	}
	v, e = idx.Fixing(today, true)
	curveNear(t, curveMust(t, v, e), (d1/d2-1)/tau, 1e-12)
}
