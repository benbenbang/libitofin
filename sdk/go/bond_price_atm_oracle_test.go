package itofin

import (
	"bufio"
	"encoding/csv"
	"encoding/json"
	"math"
	"os"
	"strconv"
	"testing"
	"time"
)

type priceATMCase struct {
	ID, Kind, Reference, Settlement string
	SettlementDays                  uint32 `json:"settlement_days"`
	Dates                           []string
	Balances                        []float64
	CouponRates                     []float64 `json:"coupon_rates"`
	Redemption                      float64
	DiscountRate                    float64 `json:"discount_rate"`
	ExDays                          int32   `json:"ex_days"`
}

func priceATMCases(t *testing.T) map[string]priceATMCase {
	t.Helper()
	f, e := os.Open("testdata/bond_price_atm/cases.jsonl")
	bondCheck(t, e)
	defer f.Close()
	cases := map[string]priceATMCase{}
	scanner := bufio.NewScanner(f)
	for scanner.Scan() {
		var row struct {
			Type  string `json:"record_type"`
			Value json.RawMessage
		}
		bondCheck(t, json.Unmarshal(scanner.Bytes(), &row))
		if row.Type != "case" {
			continue
		}
		var c priceATMCase
		bondCheck(t, json.Unmarshal(row.Value, &c))
		if _, exists := cases[c.ID]; exists {
			t.Fatal("duplicate oracle case")
		}
		cases[c.ID] = c
	}
	bondCheck(t, scanner.Err())
	if len(cases) != 49 {
		t.Fatal("oracle input count")
	}
	return cases
}

func priceATMDate(t *testing.T, value string) Date {
	t.Helper()
	d, e := time.Parse("2006-01-02", value)
	bondCheck(t, e)
	return testDate(t, d.Day(), int(d.Month()), d.Year())
}

func priceATMFixture(t *testing.T, s *Session, a priceATMCase) (*Bond, *YieldTermStructure, Date) {
	t.Helper()
	settings, e := s.NewSettings()
	bondCheck(t, e)
	cal, e := s.NullCalendar()
	bondCheck(t, e)
	dc, e := s.Actual360()
	bondCheck(t, e)
	curveDC, e := s.Actual365Fixed()
	bondCheck(t, e)
	start, end := priceATMDate(t, a.Dates[0]), priceATMDate(t, a.Dates[len(a.Dates)-1])
	convention, backward := Unadjusted, Backward
	base := BondConfig{SettlementDays: a.SettlementDays, FaceAmount: a.Balances[0], Calendar: cal, Settings: settings, IssueDate: &start, PaymentConvention: &convention, Redemption: &a.Redemption}
	var bond *Bond
	if a.Kind == "zero" {
		zero, err := s.NewZeroCouponBond(ZeroCouponBondConfig{BondConfig: base, MaturityDate: end})
		bondCheck(t, err)
		bond = &zero.Bond
	} else {
		if a.Kind != "fixed" {
			t.Fatal("amortizing cases require integrated factory QA")
		}
		schedule, err := s.NewSchedule(ScheduleConfig{Start: start, End: end, Frequency: Semiannual, Calendar: cal, Convention: Unadjusted, Rule: &backward})
		bondCheck(t, err)
		c := FixedRateBondConfig{BondConfig: base, Schedule: schedule, Coupons: a.CouponRates, AccrualDayCounter: dc}
		if a.ExDays > 0 {
			c.ExCouponPeriod = &Period{a.ExDays, Days}
			c.ExCouponCalendar = cal
		}
		fixed, err := s.NewFixedRateBond(c)
		bondCheck(t, err)
		bond = &fixed.Bond
	}
	curve, e := s.NewFlatForward(priceATMDate(t, a.Reference), a.DiscountRate, curveDC)
	bondCheck(t, e)
	return bond, curve, priceATMDate(t, a.Settlement)
}

func priceATMRows(t *testing.T, name string) [][]string {
	t.Helper()
	f, e := os.Open("testdata/bond_price_atm/" + name)
	bondCheck(t, e)
	defer f.Close()
	rows, e := csv.NewReader(f).ReadAll()
	bondCheck(t, e)
	return rows[1:]
}

func priceATMReal(t *testing.T, text string) float64 {
	t.Helper()
	v, e := strconv.ParseFloat(text, 64)
	bondCheck(t, e)
	if math.IsNaN(v) || math.IsInf(v, 0) {
		t.Fatal("nonfinite oracle value")
	}
	return v
}

func TestBondPriceATMIndependentFixedAndZeroRows(t *testing.T) {
	cases := priceATMCases(t)
	checked, skipped := 0, 0
	for _, row := range priceATMRows(t, "bond_atm_from_price_reference.csv") {
		if row[1] == "amortizing" {
			skipped++
			continue
		}
		t.Run(row[0]+"_"+row[9]+"_"+row[10], func(t *testing.T) {
			s, e := NewSession()
			bondCheck(t, e)
			defer s.Close()
			b, curve, settlement := priceATMFixture(t, s, cases[row[0]])
			kind := Clean
			if row[9] == "dirty" {
				kind = Dirty
			}
			got, e := b.ATMRateFromPrice(curve, priceATMReal(t, row[10]), kind, &settlement)
			bondCheck(t, e)
			for _, field := range []string{row[13], row[14]} {
				expected := priceATMReal(t, field)
				if math.IsNaN(got) || math.IsInf(got, 0) || math.Abs(got-expected) > 2e-13 {
					t.Fatalf("got %.17g expected %.17g", got, expected)
				}
			}
			n, e := b.Notional(&settlement)
			bondCheck(t, e)
			if n != priceATMReal(t, row[11]) {
				t.Fatal("current notional")
			}
			accrued, e := b.AccruedAmount(&settlement)
			bondCheck(t, e)
			if math.IsNaN(accrued) || math.IsInf(accrued, 0) || math.Abs(accrued-priceATMReal(t, row[12])) > 2e-12 {
				t.Fatal("accrued")
			}
		})
		checked++
	}
	if checked != 182 || skipped != 60 {
		t.Fatalf("rows %d skipped %d", checked, skipped)
	}
}

func TestBondPriceATMIndependentMaturityAndZeroBPSErrors(t *testing.T) {
	cases := priceATMCases(t)
	checked := 0
	for _, row := range priceATMRows(t, "bond_atm_from_price_errors.csv") {
		if row[1] == "amortizing" {
			continue
		}
		func() {
			s, e := NewSession()
			bondCheck(t, e)
			defer s.Close()
			b, curve, settlement := priceATMFixture(t, s, cases[row[0]])
			kind := Clean
			if row[2] == "dirty" {
				kind = Dirty
			}
			if _, e = b.ATMRateFromPrice(curve, priceATMReal(t, row[3]), kind, &settlement); e == nil {
				t.Fatalf("missing domain error: %s", row[0])
			}
		}()
		checked++
	}
	if checked != 40 {
		t.Fatalf("error rows %d", checked)
	}
}
