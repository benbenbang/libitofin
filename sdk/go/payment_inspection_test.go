package itofin

import (
	"encoding/csv"
	"os"
	"strconv"
	"strings"
	"testing"
)

func TestPaymentInspectionBondActualUnitsAndRetainedLeg(t *testing.T) {
	s, e := NewSession()
	bondCheck(t, e)
	defer s.Close()
	b, _, settings, _, schedule, _, _, _ := bondFixture(t, s, "fixed", 375)
	leg, e := b.Cashflows()
	bondCheck(t, e)
	dates, e := schedule.Dates()
	bondCheck(t, e)
	start, e := b.StartDate()
	bondCheck(t, e)
	legStart, e := leg.StartDate()
	bondCheck(t, e)
	end, e := leg.MaturityDate()
	bondCheck(t, e)
	if start != dates[0] || legStart != start || end != dates[len(dates)-1] {
		t.Fatal("incorrect accrual span")
	}
	before := dates[0]
	if v, e := b.PreviousCashFlowDate(&before); e != nil || v != nil {
		t.Fatalf("previous date %v: %v", v, e)
	}
	if v, e := b.PreviousCashFlowAmount(&before); e != nil || v != nil {
		t.Fatalf("previous amount %v: %v", v, e)
	}
	last := dates[len(dates)-1]
	penultimate := dates[len(dates)-2]
	want := 375 * (1 + .05*float64(last.Serial()-penultimate.Serial())/365)
	value, e := b.NextCashFlowAmount(&penultimate)
	bondCheck(t, e)
	if value == nil {
		t.Fatal("missing final amount")
	}
	bondNear(t, *value, want)
	previous, e := b.PreviousCashFlowAmount(&last)
	bondCheck(t, e)
	if previous == nil {
		t.Fatal("missing previous amount")
	}
	bondNear(t, *previous, want)
	if v, e := b.NextCashFlowDate(&last); e != nil || v != nil {
		t.Fatalf("next date %v: %v", v, e)
	}
	if v, e := b.NextCashFlowAmount(&last); e != nil || v != nil {
		t.Fatalf("next amount %v: %v", v, e)
	}
	if tradable, e := b.IsTradable(&last); e != nil || tradable {
		t.Fatalf("tradability %v: %v", tradable, e)
	}
	if tradable, e := b.IsTradable(nil); e != nil || !tradable {
		t.Fatalf("default tradability %v: %v", tradable, e)
	}
	if calculated, e := b.IsCalculated(); e != nil || calculated {
		t.Fatalf("inspection calculated bond: %v", e)
	}
	bondCheck(t, b.Close())
	config := CashFlowsPaymentConfig{Settings: settings, SettlementDate: &last}
	amount, e := leg.PreviousCashFlowAmount(config)
	bondCheck(t, e)
	if amount == nil {
		t.Fatal("lost retained leg")
	}
	bondNear(t, *amount, want)
}

func TestPaymentInspectionTodayPolicyAndErrors(t *testing.T) {
	s, e := NewSession()
	bondCheck(t, e)
	defer s.Close()
	b, _, settings, _, schedule, _, _, _ := bondFixture(t, s, "fixed", 375)
	leg, e := b.Cashflows()
	bondCheck(t, e)
	dates, e := schedule.Dates()
	bondCheck(t, e)
	today := dates[1]
	bondCheck(t, settings.SetEvaluationDate(today))
	for _, todayFlag := range []*bool{nil, ptrBool(false), ptrBool(true)} {
		bondCheck(t, settings.SetIncludeTodaysCashFlows(todayFlag))
		for _, include := range []*bool{nil, ptrBool(false), ptrBool(true)} {
			config := CashFlowsPaymentConfig{Settings: settings, IncludeSettlementDateFlows: include, SettlementDate: &today}
			includes := include != nil && *include
			if todayFlag != nil {
				includes = *todayFlag
			}
			index := 2
			if includes {
				index = 1
			}
			next, e := leg.NextCashFlowDate(config)
			bondCheck(t, e)
			if next == nil || *next != dates[index] {
				t.Fatalf("next date %v", next)
			}
			previous, e := leg.PreviousCashFlowDate(config)
			bondCheck(t, e)
			if includes && previous != nil || !includes && (previous == nil || *previous != today) {
				t.Fatalf("previous date %v", previous)
			}
			amount, e := leg.NextCashFlowAmount(config)
			bondCheck(t, e)
			if amount == nil {
				t.Fatal("missing amount")
			}
		}
	}
	if _, e := leg.NextCashFlowDate(CashFlowsPaymentConfig{}); e == nil {
		t.Fatal("nil settings accepted")
	}
	invalid := Date{}
	if _, e := b.NextCashFlowDate(&invalid); e == nil {
		t.Fatal("null date accepted")
	}
	if _, e := leg.NextCashFlowAmount(CashFlowsPaymentConfig{Settings: settings, SettlementDate: &invalid}); e == nil {
		t.Fatal("null leg date accepted")
	}
	foreign, e := NewSession()
	bondCheck(t, e)
	defer foreign.Close()
	other, e := foreign.NewSettings()
	bondCheck(t, e)
	if _, e := leg.NextCashFlowDate(CashFlowsPaymentConfig{Settings: other}); e == nil {
		t.Fatal("mixed sessions accepted")
	}
	bondCheck(t, leg.Close())
	if _, e := leg.NextCashFlowDate(CashFlowsPaymentConfig{Settings: settings}); e == nil {
		t.Fatal("released leg accepted")
	}
}

func ptrBool(value bool) *bool { return &value }

func paymentISO(t *testing.T, iso string) Date {
	t.Helper()
	parts := strings.Split(iso, "-")
	year, e := strconv.Atoi(parts[0])
	bondCheck(t, e)
	month, e := strconv.Atoi(parts[1])
	bondCheck(t, e)
	day, e := strconv.Atoi(parts[2])
	bondCheck(t, e)
	return testDate(t, day, month, year)
}
func TestPaymentInspectionIndependentNativeMatrix(t *testing.T) {
	s, e := NewSession()
	bondCheck(t, e)
	defer s.Close()
	settings, e := s.NewSettings()
	bondCheck(t, e)
	bondCheck(t, settings.SetEvaluationDate(paymentISO(t, "2026-09-24")))
	cal, e := s.NullCalendar()
	bondCheck(t, e)
	dc, e := s.Actual360()
	bondCheck(t, e)
	start, end := paymentISO(t, "2026-03-24"), paymentISO(t, "2028-03-24")
	rule := Forward
	schedule, e := s.NewSchedule(ScheduleConfig{Start: start, End: end, Frequency: Semiannual, Calendar: cal, Convention: Unadjusted, Rule: &rule})
	bondCheck(t, e)
	redemption, adjustment := 102.0, Unadjusted
	b, e := s.NewFixedRateBond(FixedRateBondConfig{BondConfig: BondConfig{SettlementDays: 2, FaceAmount: 375, Settings: settings, IssueDate: &start, Redemption: &redemption, PaymentConvention: &adjustment}, Schedule: schedule, Coupons: []float64{.047, .058}, AccrualDayCounter: dc, ExCouponPeriod: &Period{7, Days}, ExCouponCalendar: cal, ExCouponConvention: &adjustment})
	bondCheck(t, e)
	leg, e := b.Cashflows()
	bondCheck(t, e)
	file, e := os.Open("testdata/payment_inspection/bond-native.tsv")
	bondCheck(t, e)
	defer file.Close()
	reader := csv.NewReader(file)
	reader.Comma = '\t'
	rows, e := reader.ReadAll()
	bondCheck(t, e)
	if len(rows) != 61 {
		t.Fatal("wrong native row count")
	}
	for _, row := range rows[1:] {
		var today *bool
		if row[1] != "unset" {
			today = ptrBool(row[1] == "true")
		}
		bondCheck(t, settings.SetIncludeTodaysCashFlows(today))
		settlement := paymentISO(t, row[0])
		include := row[2] == "true"
		config := CashFlowsPaymentConfig{Settings: settings, SettlementDate: &settlement, IncludeSettlementDateFlows: &include}
		tradable, e := b.IsTradable(&settlement)
		bondCheck(t, e)
		if tradable != (row[7] == "true") {
			t.Fatal("native tradability mismatch")
		}
		reads := []struct {
			date       func(CashFlowsPaymentConfig) (*Date, error)
			amount     func(CashFlowsPaymentConfig) (*float64, error)
			bondDate   func(*Date) (*Date, error)
			bondAmount func(*Date) (*float64, error)
			column     int
		}{
			{leg.PreviousCashFlowDate, leg.PreviousCashFlowAmount, b.PreviousCashFlowDate, b.PreviousCashFlowAmount, 3},
			{leg.NextCashFlowDate, leg.NextCashFlowAmount, b.NextCashFlowDate, b.NextCashFlowAmount, 5},
		}
		for _, read := range reads {
			date, e := read.date(config)
			bondCheck(t, e)
			amount, e := read.amount(config)
			bondCheck(t, e)
			if row[read.column] == "-" {
				if date != nil || amount != nil {
					t.Fatal("native absent payment mismatch")
				}
			} else {
				if date == nil || *date != paymentISO(t, row[read.column]) || amount == nil {
					t.Fatal("native payment date mismatch")
				}
				want, e := strconv.ParseFloat(row[read.column+1], 64)
				bondCheck(t, e)
				bondNear(t, *amount, want)
			}
			if !include {
				d, e := read.bondDate(&settlement)
				bondCheck(t, e)
				a, e := read.bondAmount(&settlement)
				bondCheck(t, e)
				if date == nil {
					if d != nil || a != nil {
						t.Fatal("native bond absence mismatch")
					}
				} else {
					if d == nil || *d != *date || a == nil {
						t.Fatal("native bond payment mismatch")
					}
					bondNear(t, *a, *amount)
				}
			}
		}
	}
}

func TestPaymentInspectionFrozenFloatingSelectedGuards(t *testing.T) {
	s, e := NewSession()
	bondCheck(t, e)
	defer s.Close()
	m := floatingFixture(t, s, floatingOracles(t)[0], true)
	leg, e := m.bond.Cashflows()
	bondCheck(t, e)
	principal, e := m.bond.Redemptions()
	bondCheck(t, e)
	dates, e := m.schedule.Dates()
	bondCheck(t, e)
	bondCheck(t, m.bond.Freeze())
	bondCheck(t, m.settings.SetEvaluationDate(dates[1]))
	settlement := dates[0]
	if value, e := m.bond.NextCashFlowDate(&settlement); e != nil || value == nil {
		t.Fatalf("date forced fixing: %v", e)
	}
	if value, e := leg.NextCashFlowDate(CashFlowsPaymentConfig{Settings: m.settings, SettlementDate: &settlement}); e != nil || value == nil {
		t.Fatalf("leg date forced fixing: %v", e)
	}
	if _, e := m.bond.NextCashFlowAmount(&settlement); e == nil {
		t.Fatal("frozen bond ignored missing selected fixing")
	}
	if _, e := leg.NextCashFlowAmount(CashFlowsPaymentConfig{Settings: m.settings, SettlementDate: &settlement}); e == nil {
		t.Fatal("leg ignored missing selected fixing")
	}
	later := dates[2]
	if value, e := m.bond.NextCashFlowAmount(&later); e != nil || value == nil {
		t.Fatalf("unrelated missing fixing blocked later coupon: %v", e)
	}
	bondCheck(t, m.bond.Close())
	bondCheck(t, m.index.Close())
	value, e := principal.NextCashFlowAmount(CashFlowsPaymentConfig{Settings: m.settings, SettlementDate: &settlement})
	bondCheck(t, e)
	if value == nil {
		t.Fatal("lost retained principal")
	}
	bondNear(t, *value, 1000)
}

func TestPaymentInspectionCPIIdentityMissingFixingAndRetainedLeg(t *testing.T) {
	s, e := NewSession()
	bondCheck(t, e)
	defer s.Close()
	config, _ := cpiBondConfig(t, s, false, false, true, 100.8)
	bond, e := s.NewCPIBond(config)
	bondCheck(t, e)
	leg, e := bond.Cashflows()
	bondCheck(t, e)
	principal, e := bond.Redemptions()
	bondCheck(t, e)
	bondCheck(t, bond.Freeze())
	settlement := testDate(t, 15, 6, 2025)
	if value, e := bond.NextCashFlowDate(&settlement); e != nil || value == nil {
		t.Fatalf("CPI date forced fixing: %v", e)
	}
	if _, e := bond.NextCashFlowAmount(&settlement); e == nil {
		t.Fatal("frozen CPI amount ignored missing April")
	}
	if _, e := leg.NextCashFlowAmount(CashFlowsPaymentConfig{Settings: config.Settings, SettlementDate: &settlement}); e == nil {
		t.Fatal("CPI leg ignored missing April")
	}
	later := testDate(t, 15, 7, 2025)
	if value, e := bond.NextCashFlowAmount(&later); e != nil || value == nil {
		t.Fatalf("unrelated CPI fixing blocked later amount: %v", e)
	}
	other, e := s.NewSettings()
	bondCheck(t, e)
	bondCheck(t, other.SetEvaluationDate(settlement))
	wrong := CashFlowsPaymentConfig{Settings: other, SettlementDate: &settlement}
	if _, e := leg.NextCashFlowDate(wrong); e == nil {
		t.Fatal("different CPI Settings accepted for date")
	}
	if _, e := leg.NextCashFlowAmount(wrong); e == nil {
		t.Fatal("different CPI Settings accepted for amount")
	}
	bondCheck(t, bond.Close())
	bondCheck(t, config.Index.Close())
	value, e := principal.NextCashFlowAmount(CashFlowsPaymentConfig{Settings: config.Settings, SettlementDate: &settlement})
	bondCheck(t, e)
	if value == nil {
		t.Fatal("lost retained CPI principal")
	}
}
