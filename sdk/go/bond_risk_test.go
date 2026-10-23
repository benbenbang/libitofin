package itofin

import (
	"encoding/csv"
	"errors"
	"math"
	"os"
	"strconv"
	"strings"
	"sync"
	"testing"
)

func TestBondRiskAnalyticalAndLifecycle(t *testing.T) {
	s, err := NewSession()
	bondCheck(t, err)
	defer s.Close()
	b, quote, settings, calendar, schedule, dc, curve, engine := bondFixture(t, s, "zero", 375)
	settlement := testDate(t, 2, 1, 2025)
	a := BondYieldConfig{DayCounter: dc, Compounding: Compounded, Frequency: Semiannual, SettlementDate: &settlement}
	end := testDate(t, 15, 1, 2027)
	years := float64(end.Serial()-settlement.Serial()) / 365
	rate := .052
	modified, convexity := years/(1+rate/2), years*(years+.5)/math.Pow(1+rate/2, 2)
	price := 375 * math.Pow(1+rate/2, -2*years)
	calls := []struct {
		call func() (float64, error)
		want float64
	}{
		{func() (float64, error) { return b.Duration(rate, SimpleDuration, a) }, years},
		{func() (float64, error) { return b.Duration(rate, MacaulayDuration, a) }, years},
		{func() (float64, error) { return b.Duration(rate, ModifiedDuration, a) }, modified},
		{func() (float64, error) { return b.Convexity(rate, a) }, convexity},
		{func() (float64, error) { return b.BasisPointValue(rate, a) }, -modified*price*.0001 + convexity/100*price*.0001*.0001/2},
		{func() (float64, error) { return b.YieldValueBasisPoint(rate, a) }, .01 / (-price * modified)},
	}
	for _, c := range calls {
		got, e := c.call()
		bondCheck(t, e)
		analyticsNear(t, got, c.want)
	}
	cached, e := b.NPV()
	bondCheck(t, e)
	bondCheck(t, b.Freeze())
	bondCheck(t, quote.SetValue(.08))
	for _, c := range calls {
		got, e := c.call()
		bondCheck(t, e)
		analyticsNear(t, got, c.want)
	}
	got, e := b.NPV()
	bondCheck(t, e)
	bondNear(t, got, cached)
	for _, close := range []func() error{quote.Close, settings.Close, calendar.Close, schedule.Close, curve.Close, engine.Close} {
		bondCheck(t, close())
	}
	for _, c := range calls {
		got, e := c.call()
		bondCheck(t, e)
		analyticsNear(t, got, c.want)
	}
	var wg sync.WaitGroup
	for range 8 {
		wg.Go(func() {
			for range 10 {
				got, e := b.Convexity(rate, a)
				if e != nil || math.Abs(got-convexity) > 3e-12 {
					t.Errorf("concurrent risk: %g %v", got, e)
				}
			}
		})
	}
	wg.Wait()
	bondCheck(t, b.Close())
	if _, e := b.Convexity(rate, a); e == nil {
		t.Fatalf("closed bond: %v", e)
	}
}

func TestBondRiskDomainsSessionsAndRecovery(t *testing.T) {
	s, err := NewSession()
	bondCheck(t, err)
	defer s.Close()
	b, _, _, _, _, dc, _, _ := bondFixture(t, s, "zero", 375)
	settlement := testDate(t, 2, 1, 2025)
	a := BondYieldConfig{DayCounter: dc, Compounding: Continuous, Frequency: Annual, SettlementDate: &settlement}
	if _, e := b.Duration(.03, MacaulayDuration, a); e == nil {
		t.Fatal("continuous Macaulay accepted")
	}
	if _, e := b.Duration(.03, DurationType(3), a); e == nil {
		t.Fatal("invalid duration accepted")
	}
	for _, rate := range []float64{math.NaN(), math.Inf(1), math.Inf(-1)} {
		if _, e := b.Convexity(rate, a); e == nil {
			t.Fatal("nonfinite yield accepted")
		}
	}
	for _, mode := range []Compounding{-1, 5} {
		bad := a
		bad.Compounding = mode
		if _, e := b.Convexity(.03, bad); e == nil {
			t.Fatal("invalid mode accepted")
		}
	}
	maturity := testDate(t, 15, 1, 2027)
	bad := a
	bad.SettlementDate = &maturity
	if _, e := b.Convexity(.03, bad); e == nil {
		t.Fatal("nontradable bond accepted")
	}
	bad = a
	bad.Frequency = Frequency(99)
	if _, e := b.Convexity(.03, bad); e == nil {
		t.Fatal("invalid frequency accepted")
	}
	bad = a
	bad.DayCounter = nil
	if _, e := b.Convexity(.03, bad); e == nil {
		t.Fatal("nil counter accepted")
	}
	s2, e := NewSession()
	bondCheck(t, e)
	defer s2.Close()
	foreign, e := s2.Actual365Fixed()
	bondCheck(t, e)
	bad = a
	bad.DayCounter = foreign
	if _, e := b.Convexity(.03, bad); !errors.Is(e, ErrSessionMismatch) {
		t.Fatalf("foreign counter: %v", e)
	}
	var nilBond *Bond
	if _, e := nilBond.Convexity(.03, a); e == nil {
		t.Fatal("nil bond accepted")
	}
	if _, e := b.Convexity(.03, a); e != nil {
		t.Fatal(e)
	}
	bondCheck(t, dc.Close())
	if _, e := b.Convexity(.03, a); e == nil {
		t.Fatalf("closed counter: %v", e)
	}
	bondCheck(t, s.Close())
	if _, e := b.Convexity(.03, a); !errors.Is(e, ErrClosed) {
		t.Fatalf("closed session: %v", e)
	}
}

func TestBondRiskQuantLibOracles(t *testing.T) {
	data, e := os.ReadFile("testdata/bond_risk_oracle.tsv")
	bondCheck(t, e)
	reader := csv.NewReader(strings.NewReader(string(data)))
	reader.Comma = '\t'
	records, e := reader.ReadAll()
	bondCheck(t, e)
	if len(records) != 56 {
		t.Fatal("oracle contract")
	}
	var cases []struct {
		analyticsCase
		Expected map[string]*float64
	}
	for _, record := range records[1:] {
		row := map[string]string{}
		for column, name := range records[0] {
			row[name] = record[column]
		}
		integer := func(key string) int32 { v, e := strconv.ParseInt(row[key], 10, 32); bondCheck(t, e); return int32(v) }
		real := func(key string) float64 { v, e := strconv.ParseFloat(row[key], 64); bondCheck(t, e); return v }
		a := analyticsCase{Name: row["name"], Kind: row["kind"], Compounding: Compounding(integer("compounding")), Frequency: Frequency(integer("frequency")), Face: real("face"), Start: integer("start"), End: integer("end"), Reference: integer("reference"), Settlement: integer("settlement"), Yield: real("yield")}
		expected := map[string]*float64{}
		for _, key := range []string{"simple", "macaulay", "modified", "convexity", "basis_point_value", "yield_value_basis_point"} {
			if row[key] != "NA" {
				value := real(key)
				expected[key] = &value
			}
		}
		cases = append(cases, struct {
			analyticsCase
			Expected map[string]*float64
		}{a, expected})
	}
	for _, row := range cases {
		t.Run(row.Name, func(t *testing.T) {
			s, e := NewSession()
			bondCheck(t, e)
			defer s.Close()
			b, _, dc, _ := analyticsFixture(t, s, row.analyticsCase)
			settlement, e := DateFromSerial(row.Settlement)
			bondCheck(t, e)
			a := BondYieldConfig{DayCounter: dc, Compounding: row.Compounding, Frequency: row.Frequency, SettlementDate: &settlement}
			for field, key := range []string{"simple", "macaulay", "modified", "convexity", "basis_point_value", "yield_value_basis_point"} {
				got, e := b.risk(int32(field), row.Yield, a)
				if row.Expected[key] == nil {
					if e == nil {
						t.Fatal("unsupported Macaulay accepted")
					}
					continue
				}
				bondCheck(t, e)
				analyticsNear(t, got, *row.Expected[key])
			}
		})
	}
}

func TestBondRiskFloatingCurrentInputs(t *testing.T) {
	s, e := NewSession()
	bondCheck(t, e)
	defer s.Close()
	m := floatingFixture(t, s, floatingOracles(t)[5], false)
	a := BondYieldConfig{DayCounter: m.counter, Compounding: Compounded, Frequency: Semiannual}
	if _, e := m.bond.BasisPointValue(.052, a); e == nil {
		t.Fatal("missing coupon pricer accepted")
	}
	bondCheck(t, m.bond.SetCouponPricer(m.pricer))
	cached, e := m.bond.NPV()
	bondCheck(t, e)
	bondCheck(t, m.bond.Freeze())
	initial, e := m.bond.BasisPointValue(.052, a)
	bondCheck(t, e)
	bondCheck(t, m.rate.SetValue(.08))
	bondCheck(t, m.volatility.SetValue(.4))
	before, e := m.bond.IsCalculated()
	bondCheck(t, e)
	changed, e := m.bond.BasisPointValue(.052, a)
	bondCheck(t, e)
	if changed == initial || math.IsNaN(changed) || math.IsInf(changed, 0) || changed >= 0 {
		t.Fatal("risk failed to consume live coupons")
	}
	after, e := m.bond.IsCalculated()
	bondCheck(t, e)
	if after != before {
		t.Fatal("analytical risk changed cached state")
	}
	got, e := m.bond.NPV()
	bondCheck(t, e)
	bondNear(t, got, cached)
	for _, close := range []func() error{m.index.Close, m.schedule.Close, m.pricer.Close, m.surface.Close, m.forward.Close, m.rate.Close, m.volatility.Close, m.settings.Close, m.engine.Close, m.calendar.Close} {
		bondCheck(t, close())
	}
	got, e = m.bond.BasisPointValue(.052, a)
	bondCheck(t, e)
	analyticsNear(t, got, changed)
}
