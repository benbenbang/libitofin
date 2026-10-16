package itofin

import (
	"encoding/json"
	"math"
	"os"
	"testing"
)

type forwardRow struct {
	Kind     string    `json:"kind"`
	Face     float64   `json:"face"`
	Delivery int32     `json:"delivery"`
	Separate bool      `json:"separate"`
	Short    bool      `json:"short"`
	Values   []float64 `json:"values"`
}

func forwardFixture(t *testing.T, s *Session, kind string, face float64) (*Bond, *Settings, *SimpleQuote, *YieldTermStructure, *YieldTermStructure, *Calendar, *DayCounter) {
	t.Helper()
	settings, e := s.NewSettings()
	bondCheck(t, e)
	today := testDate(t, 2, 1, 2025)
	bondCheck(t, settings.SetEvaluationDate(today))
	cal, e := s.NullCalendar()
	bondCheck(t, e)
	dc, e := s.Actual365Fixed()
	bondCheck(t, e)
	quote, e := s.NewSimpleQuote(.03)
	bondCheck(t, e)
	spot, e := s.NewFlatForwardFromQuote(today, quote, dc)
	bondCheck(t, e)
	finance, e := s.NewFlatForward(today, .04, dc)
	bondCheck(t, e)
	income, e := s.NewFlatForward(today, .025, dc)
	bondCheck(t, e)
	pricing, e := s.NewDiscountingBondEngine(spot, settings, nil)
	bondCheck(t, e)
	issue := testDate(t, 2, 7, 2024)
	rule := Unadjusted
	config := BondConfig{SettlementDays: 2, FaceAmount: face, Settings: settings, Calendar: cal, IssueDate: &issue, PaymentConvention: &rule}
	var bond *Bond
	if kind == "zero" {
		zero, e := s.NewZeroCouponBond(ZeroCouponBondConfig{config, testDate(t, 2, 7, 2026)})
		bondCheck(t, e)
		bond = &zero.Bond
	} else {
		backward := Backward
		schedule, e := s.NewSchedule(ScheduleConfig{Start: issue, End: testDate(t, 2, 7, 2026), Frequency: Semiannual, Calendar: cal, Convention: Unadjusted, Rule: &backward})
		bondCheck(t, e)
		fixed, e := s.NewFixedRateBond(FixedRateBondConfig{BondConfig: config, Schedule: schedule, Coupons: []float64{.05}, AccrualDayCounter: dc})
		bondCheck(t, e)
		bond = &fixed.Bond
	}
	bondCheck(t, bond.SetEngine(pricing))
	return bond, settings, quote, finance, income, cal, dc
}
func forwardConfig(t *testing.T, bond *Bond, finance *YieldTermStructure, cal *Calendar, dc *DayCounter) BondForwardConfig {
	t.Helper()
	return BondForwardConfig{Bond: bond, ValueDate: testDate(t, 2, 1, 2025), DeliveryDate: testDate(t, 2, 7, 2025), Position: PositionLong, Strike: 105, DayCounter: dc, Calendar: cal, Convention: Unadjusted, Discount: finance}
}
func TestBondForwardCompiledQuantLib(t *testing.T) {
	data, e := os.ReadFile("testdata/bond_forward_native.json")
	bondCheck(t, e)
	var fixture struct {
		Rows []forwardRow `json:"rows"`
	}
	bondCheck(t, json.Unmarshal(data, &fixture))
	for _, row := range fixture.Rows {
		s, e := NewSession()
		bondCheck(t, e)
		bond, _, _, finance, income, cal, dc := forwardFixture(t, s, row.Kind, row.Face)
		a := forwardConfig(t, bond, finance, cal, dc)
		a.DeliveryDate, e = DateFromSerial(row.Delivery)
		bondCheck(t, e)
		if row.Separate {
			a.IncomeDiscount = income
		}
		if row.Short {
			a.Position = PositionShort
		}
		forward, e := s.NewBondForward(a)
		bondCheck(t, e)
		for i, value := range []func() (float64, error){forward.NPV, forward.FairForwardPrice, forward.CleanForwardPrice} {
			got, e := value()
			bondCheck(t, e)
			if math.Abs(got-row.Values[i]) > 2e-11 {
				t.Fatalf("%+v field%d got%.17g want%.17g", row, i, got, row.Values[i])
			}
		}
		d, e := forward.DeliveryDate()
		bondCheck(t, e)
		if d.Serial() != row.Delivery {
			t.Fatal(d)
		}
		bondCheck(t, s.Close())
	}
}
func TestBondForwardRetentionFreezeExpiryAndSession(t *testing.T) {
	s, e := NewSession()
	bondCheck(t, e)
	defer s.Close()
	bond, settings, quote, finance, income, cal, dc := forwardFixture(t, s, "fixed", 100)
	a := forwardConfig(t, bond, finance, cal, dc)
	a.IncomeDiscount = income
	forward, e := s.NewBondForward(a)
	bondCheck(t, e)
	initial, e := forward.NPV()
	bondCheck(t, e)
	bondCheck(t, forward.Freeze())
	bondCheck(t, quote.SetValue(.08))
	got, e := forward.NPV()
	bondCheck(t, e)
	if got != initial {
		t.Fatal("freeze changed value")
	}
	bondCheck(t, forward.Recalculate())
	got, e = forward.NPV()
	bondCheck(t, e)
	if got >= initial {
		t.Fatal("forced update missing")
	}
	bondCheck(t, forward.Unfreeze())
	bondCheck(t, bond.Close())
	bondCheck(t, finance.Close())
	bondCheck(t, income.Close())
	bondCheck(t, cal.Close())
	bondCheck(t, dc.Close())
	_, e = forward.NPV()
	bondCheck(t, e)
	bondCheck(t, settings.SetEvaluationDate(testDate(t, 3, 7, 2025)))
	expired, e := forward.IsExpired()
	bondCheck(t, e)
	if !expired {
		t.Fatal("expected expired")
	}
	got, e = forward.NPV()
	bondCheck(t, e)
	if got != 0 {
		t.Fatal(got)
	}
	if _, e = forward.FairForwardPrice(); e == nil {
		t.Fatal("expired fair quote")
	}
	other, e := NewSession()
	bondCheck(t, e)
	defer other.Close()
	if _, e = other.NewBondForward(a); e == nil {
		t.Fatal("cross-session input accepted")
	}
	var absent *BondForward
	if _, e = absent.NPV(); e == nil {
		t.Fatal("nil receiver accepted")
	}
}
func TestBondForwardInvalidConstruction(t *testing.T) {
	s, e := NewSession()
	bondCheck(t, e)
	defer s.Close()
	bond, _, _, finance, _, cal, dc := forwardFixture(t, s, "zero", 100)
	a := forwardConfig(t, bond, finance, cal, dc)
	for _, strike := range []float64{-1, math.NaN(), math.Inf(1)} {
		bad := a
		bad.Strike = strike
		if _, e = s.NewBondForward(bad); e == nil {
			t.Fatal("invalid strike")
		}
	}
	bad := a
	bad.Position = Position(7)
	if _, e = s.NewBondForward(bad); e == nil {
		t.Fatal("invalid position")
	}
	bad = a
	bad.Convention = BusinessDayConvention(99)
	if _, e = s.NewBondForward(bad); e == nil {
		t.Fatal("invalid convention")
	}
	bad = a
	bad.DeliveryDate = Date{}
	if _, e = s.NewBondForward(bad); e == nil {
		t.Fatal("null delivery")
	}
	bad = a
	bad.Discount = nil
	if _, e = s.NewBondForward(bad); e == nil {
		t.Fatal("nil curve")
	}
}

func TestBondForwardMetadataWithoutEvaluationOrEngine(t *testing.T) {
	s, e := NewSession()
	bondCheck(t, e)
	defer s.Close()
	settings, e := s.NewSettings()
	bondCheck(t, e)
	cal, e := s.NullCalendar()
	bondCheck(t, e)
	dc, e := s.Actual365Fixed()
	bondCheck(t, e)
	today := testDate(t, 2, 1, 2025)
	curve, e := s.NewFlatForward(today, .03, dc)
	bondCheck(t, e)
	bond, e := s.NewZeroCouponBond(ZeroCouponBondConfig{BondConfig: BondConfig{SettlementDays: 2, FaceAmount: 100, Settings: settings, Calendar: cal}, MaturityDate: testDate(t, 2, 7, 2026)})
	bondCheck(t, e)
	forward, e := s.NewBondForward(forwardConfig(t, &bond.Bond, curve, cal, dc))
	bondCheck(t, e)
	strike, e := forward.Strike()
	bondCheck(t, e)
	if strike != 105 {
		t.Fatal(strike)
	}
	position, e := forward.Position()
	bondCheck(t, e)
	if position != PositionLong {
		t.Fatal(position)
	}
	date, e := forward.DeliveryDate()
	bondCheck(t, e)
	if date != testDate(t, 2, 7, 2025) {
		t.Fatal(date)
	}
	if _, e = forward.NPV(); e == nil {
		t.Fatal("valuation without evaluation")
	}
}

func TestBondForwardLiveIncomeFinanceAndDateAdjustments(t *testing.T) {
	data, e := os.ReadFile("testdata/bond_forward_native.json")
	bondCheck(t, e)
	var fixture struct {
		LiveRows []struct {
			Stage  string      `json:"stage"`
			Values [][]float64 `json:"values"`
		} `json:"live_rows"`
		Boundary struct {
			Values []float64 `json:"values"`
		} `json:"boundary"`
	}
	bondCheck(t, json.Unmarshal(data, &fixture))
	s, e := NewSession()
	bondCheck(t, e)
	defer s.Close()
	bond, _, spotQuote, _, _, cal, dc := forwardFixture(t, s, "fixed", 100)
	financeQuote, e := s.NewSimpleQuote(.04)
	bondCheck(t, e)
	incomeQuote, e := s.NewSimpleQuote(.025)
	bondCheck(t, e)
	today := testDate(t, 2, 1, 2025)
	finance, e := s.NewFlatForwardFromQuote(today, financeQuote, dc)
	bondCheck(t, e)
	income, e := s.NewFlatForwardFromQuote(today, incomeQuote, dc)
	bondCheck(t, e)
	a := forwardConfig(t, bond, finance, cal, dc)
	defaultForward, e := s.NewBondForward(a)
	bondCheck(t, e)
	a.IncomeDiscount = income
	separateForward, e := s.NewBondForward(a)
	bondCheck(t, e)
	for _, row := range fixture.LiveRows {
		switch row.Stage {
		case "finance":
			bondCheck(t, financeQuote.SetValue(.07))
		case "income":
			bondCheck(t, incomeQuote.SetValue(.08))
		case "spot":
			bondCheck(t, spotQuote.SetValue(.06))
		}
		for i, forward := range []*BondForward{defaultForward, separateForward} {
			for j, value := range []func() (float64, error){forward.NPV, forward.FairForwardPrice, forward.CleanForwardPrice} {
				got, e := value()
				bondCheck(t, e)
				if math.Abs(got-row.Values[i][j]) > 2e-11 {
					t.Fatalf("stage%s role%d field%d got%.17g want%.17g", row.Stage, i, j, got, row.Values[i][j])
				}
			}
		}
	}
	weekend, e := s.WeekendsOnly()
	bondCheck(t, e)
	a.Calendar = weekend
	a.SettlementDays = 2
	a.Convention = Following
	a.ValueDate = testDate(t, 7, 4, 2025)
	a.DeliveryDate = testDate(t, 5, 7, 2025)
	boundary, e := s.NewBondForward(a)
	bondCheck(t, e)
	settlement, e := boundary.SettlementDate()
	bondCheck(t, e)
	if settlement != a.ValueDate {
		t.Fatal(settlement)
	}
	delivery, e := boundary.DeliveryDate()
	bondCheck(t, e)
	if delivery != testDate(t, 7, 7, 2025) {
		t.Fatal(delivery)
	}
	for i, value := range []func() (float64, error){boundary.NPV, boundary.FairForwardPrice, boundary.CleanForwardPrice} {
		got, e := value()
		bondCheck(t, e)
		if math.Abs(got-fixture.Boundary.Values[i]) > 2e-11 {
			t.Fatalf("boundary field%d got%.17g want%.17g", i, got, fixture.Boundary.Values[i])
		}
	}
}
