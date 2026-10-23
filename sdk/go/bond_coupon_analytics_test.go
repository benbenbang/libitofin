package itofin

import (
	"encoding/csv"
	"errors"
	"math"
	"os"
	"strconv"
	"strings"
	"testing"
)

func TestBondCouponAnalyticsValuesAndErrors(t *testing.T) {
	s, e := NewSession()
	bondCheck(t, e)
	defer s.Close()
	a := analyticsOracles(t)[0]
	a.Compounding, a.Frequency = Continuous, Annual
	b, _, dc, curve := analyticsFixture(t, s, a)
	settlement, e := DateFromSerial(a.Settlement)
	bondCheck(t, e)
	config := BondYieldConfig{DayCounter: dc, Compounding: Continuous, Frequency: Annual, SettlementDate: &settlement}
	dates := []Date{testDate(t, 24, 3, 2026), testDate(t, 24, 9, 2026), testDate(t, 24, 3, 2027), testDate(t, 24, 9, 2027), testDate(t, 24, 3, 2028)}
	curveBPS, yieldBPS, couponsPV := 0.0, 0.0, 0.0
	for i, date := range dates[1:] {
		accrual := float64(date.Serial()-dates[i].Serial()) / 360
		discount := math.Exp(-.032 * float64(date.Serial()-settlement.Serial()) / 365)
		curveBPS += .01 * accrual * discount
		yieldBPS += .01 * accrual * math.Exp(-.052*float64(date.Serial()-settlement.Serial())/360)
		coupon := .058
		if i == 0 {
			coupon = .047
		}
		couponsPV += coupon * accrual * discount
	}
	for _, c := range []struct {
		read func() (float64, error)
		want float64
	}{
		{func() (float64, error) { return b.BPSFromCurve(curve, &settlement) }, curveBPS},
		{func() (float64, error) { return b.BPSFromYield(.052, config) }, yieldBPS},
		{func() (float64, error) { return b.ATMRateFromCurve(curve, &settlement) }, couponsPV / (curveBPS / .01)},
	} {
		got, e := c.read()
		bondCheck(t, e)
		analyticsNear(t, got, c.want)
	}
	if _, e := b.BPSFromCurve(curve, nil); e == nil {
		t.Fatal("missing evaluation date accepted")
	}
	if _, e := b.BPSFromYield(math.NaN(), config); e == nil {
		t.Fatal("nonfinite yield accepted")
	}
	if _, e := b.BPSFromCurve(nil, &settlement); e == nil {
		t.Fatal("nil curve accepted")
	}
	var missing *Bond
	if _, e := missing.ATMRateFromCurve(curve, &settlement); e == nil {
		t.Fatal("nil bond accepted")
	}
	s2, e := NewSession()
	bondCheck(t, e)
	defer s2.Close()
	foreignDC, e := s2.Actual365Fixed()
	bondCheck(t, e)
	foreign, e := s2.NewFlatForward(settlement, .03, foreignDC)
	bondCheck(t, e)
	if _, e := b.BPSFromCurve(foreign, &settlement); !errors.Is(e, ErrSessionMismatch) {
		t.Fatalf("foreign curve: %v", e)
	}
	bondCheck(t, curve.Close())
	if _, e := b.ATMRateFromCurve(curve, &settlement); e == nil {
		t.Fatal("closed curve accepted")
	}
	bondCheck(t, b.Close())
	if _, e := b.BPSFromYield(.052, config); e == nil {
		t.Fatal("closed bond accepted")
	}
}

func TestBondCouponAnalyticsQuantLibOracles(t *testing.T) {
	data, err := os.ReadFile("testdata/bond_coupon_analytics.tsv")
	bondCheck(t, err)
	reader := csv.NewReader(strings.NewReader(string(data)))
	reader.Comma = '\t'
	records, err := reader.ReadAll()
	bondCheck(t, err)
	if len(records) != 56 {
		t.Fatal("coupon oracle row count")
	}
	for _, record := range records[1:] {
		fields := map[string]string{}
		for i, key := range records[0] {
			fields[key] = record[i]
		}
		integer := func(key string) int32 {
			v, e := strconv.ParseInt(fields[key], 10, 32)
			bondCheck(t, e)
			return int32(v)
		}
		real := func(key string) float64 { v, e := strconv.ParseFloat(fields[key], 64); bondCheck(t, e); return v }
		t.Run(fields["name"], func(t *testing.T) {
			s, e := NewSession()
			bondCheck(t, e)
			defer s.Close()
			a := analyticsCase{Kind: fields["kind"], Face: real("face"), Start: integer("start"), End: integer("end"), Reference: integer("reference"), Settlement: integer("settlement")}
			bond, settings, dc, curve := analyticsFixture(t, s, a)
			settlement, e := DateFromSerial(a.Settlement)
			bondCheck(t, e)
			config := BondYieldConfig{DayCounter: dc, Compounding: Compounding(integer("compounding")), Frequency: Frequency(integer("frequency")), SettlementDate: &settlement}
			for _, c := range []struct {
				read func() (float64, error)
				key  string
			}{
				{func() (float64, error) { return bond.BPSFromCurve(curve, &settlement) }, "bps_curve"},
				{func() (float64, error) { return bond.BPSFromYield(real("yield"), config) }, "bps_yield"},
				{func() (float64, error) { return bond.ATMRateFromCurve(curve, &settlement) }, "atm_curve"},
			} {
				got, e := c.read()
				bondCheck(t, e)
				analyticsNear(t, got, real(c.key))
			}
			bondCheck(t, settings.SetEvaluationDate(settlement))
		})
	}
}
