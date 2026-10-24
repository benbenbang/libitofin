package itofin

import (
	"bufio"
	"encoding/json"
	"errors"
	"fmt"
	"math"
	"os"
	"strings"
	"testing"
)

type amortObservation struct {
	OK    bool    `json:"ok"`
	Value float64 `json:"value"`
}
type amortTerms struct {
	Calendar          string
	Evaluation        [3]int   `json:"evaluation_date"`
	DiscountReference [3]int   `json:"discount_curve_reference"`
	ForwardReference  [3]int   `json:"forward_curve_reference"`
	DiscountQuote     float64  `json:"discount_quote"`
	ForwardQuote      float64  `json:"forward_quote"`
	AtPar             bool     `json:"at_par"`
	SettlementDays    uint32   `json:"settlement_days"`
	ExCouponDays      int32    `json:"ex_coupon_days"`
	IssueDate         [3]int   `json:"issue_date"`
	ScheduleDates     [][3]int `json:"schedule_dates"`
	Notionals         []float64
	Spreads           []float64 `json:"floating_spreads"`
	IndexTenor        int32     `json:"index_tenor_months"`
	IndexFixingDays   uint32    `json:"index_fixing_days"`
	Fixings           []struct {
		Date  [3]int
		Value float64
	}
}
type amortNativeRow struct {
	CaseID            string `json:"case_id"`
	Phase             string
	Terms             *amortTerms
	Maturity          [3]int `json:"maturity_date"`
	Settlement        [3]int `json:"settlement_date"`
	Expired           bool   `json:"is_expired"`
	NPV               amortObservation
	SettlementValue   amortObservation `json:"settlement_value"`
	CleanPrice        amortObservation `json:"clean_price"`
	DirtyPrice        amortObservation `json:"dirty_price"`
	Accrued, Notional amortObservation
	Flows             []struct {
		Position int
		Kind     string
		Payment  [3]int `json:"payment_date"`
		Amount   amortObservation
	}
	Redemptions []struct {
		Date   [3]int
		Amount amortObservation
	}
	NotionalsByDate []struct {
		Date  [3]int
		Value amortObservation
	} `json:"notionals_by_date"`
	CleanYield     *amortObservation `json:"clean_yield"`
	DirtyYield     *amortObservation `json:"dirty_yield"`
	CleanInversion *amortObservation `json:"clean_yield_inversion"`
	DirtyInversion *amortObservation `json:"dirty_yield_inversion"`
	Amounts        []amortObservation
}

func amortOracleDate(t *testing.T, value [3]int) Date {
	t.Helper()
	return testDate(t, value[2], value[1], value[0])
}
func amortOracleObserve(t *testing.T, expected amortObservation, read func() (float64, error)) float64 {
	t.Helper()
	actual, err := read()
	if !expected.OK {
		if err == nil {
			t.Fatal("native query error expected")
		}
		return actual
	}
	bondCheck(t, err)
	if !amortFinite(actual) || !amortFinite(expected.Value) || math.Abs(actual-expected.Value) > 3e-11+3e-13*math.Abs(expected.Value) {
		t.Fatalf("native value: %g != %g", actual, expected.Value)
	}
	return actual
}
func amortNontradable(t *testing.T, read func() (float64, error)) {
	t.Helper()
	_, err := read()
	var native *Error
	if !errors.As(err, &native) || native.Code != 3 || !strings.Contains(native.Message, "non tradable at ") || !strings.Contains(native.Message, " settlement date (maturity being ") {
		t.Fatalf("specific inherited nontradable error required: %v", err)
	}
}
func amortOracleRows(t *testing.T) []amortNativeRow {
	t.Helper()
	file, err := os.Open("testdata/amortizing_floating_oracle.jsonl")
	bondCheck(t, err)
	defer file.Close()
	scanner := bufio.NewScanner(file)
	scanner.Buffer(make([]byte, 64*1024), 1024*1024)
	var rows []amortNativeRow
	for scanner.Scan() {
		var row amortNativeRow
		bondCheck(t, json.Unmarshal(scanner.Bytes(), &row))
		rows = append(rows, row)
	}
	bondCheck(t, scanner.Err())
	if len(rows) != 53 {
		t.Fatal("expected 53 qualified floating rows")
	}
	return rows
}
func amortOracleGraph(t *testing.T, s *Session, row amortNativeRow) (*FloatingRateBond, *DayCounter) {
	t.Helper()
	a := row.Terms
	settings, err := s.NewSettings()
	bondCheck(t, err)
	bondCheck(t, settings.SetEvaluationDate(amortOracleDate(t, a.Evaluation)))
	bondCheck(t, settings.SetUsingAtParCoupons(a.AtPar))
	bondCheck(t, settings.SetIncludeReferenceDateEvents(false))
	includeToday := false
	bondCheck(t, settings.SetIncludeTodaysCashFlows(&includeToday))
	calendar, err := s.NullCalendar()
	bondCheck(t, err)
	if a.Calendar == "WeekendsOnly" {
		calendar, err = s.WeekendsOnly()
	}
	bondCheck(t, err)
	counter, err := s.Actual360()
	bondCheck(t, err)
	currency, err := s.USD()
	bondCheck(t, err)
	discount, err := s.NewFlatForward(amortOracleDate(t, a.DiscountReference), a.DiscountQuote, counter)
	bondCheck(t, err)
	forward, err := s.NewFlatForward(amortOracleDate(t, a.ForwardReference), a.ForwardQuote, counter)
	bondCheck(t, err)
	index, err := s.NewIborIndex(IborIndexConfig{FamilyName: "AmortizingNativeFixture", Tenor: Period{a.IndexTenor, Months}, SettlementDays: a.IndexFixingDays, Currency: currency, FixingCalendar: calendar, Convention: Preceding, DayCounter: counter, Forwarding: forward, Settings: settings})
	bondCheck(t, err)
	for _, fixing := range a.Fixings {
		bondCheck(t, index.AddFixing(amortOracleDate(t, fixing.Date), fixing.Value))
	}
	schedule, err := s.NewSchedule(ScheduleConfig{Start: amortOracleDate(t, a.ScheduleDates[0]), End: amortOracleDate(t, a.ScheduleDates[len(a.ScheduleDates)-1]), Frequency: Quarterly, Calendar: calendar, Convention: Unadjusted})
	bondCheck(t, err)
	issue := amortOracleDate(t, a.IssueDate)
	config := AmortizingFloatingRateBondConfig{SettlementDays: a.SettlementDays, Notionals: a.Notionals, Schedule: schedule, Index: index, AccrualDayCounter: counter, Settings: settings, FixingDays: &a.IndexFixingDays, Spreads: a.Spreads, IssueDate: &issue}
	if a.ExCouponDays != 0 {
		config.ExCouponPeriod = &Period{a.ExCouponDays, Days}
		config.ExCouponCalendar = calendar
	}
	bond, err := s.NewAmortizingFloatingRateBond(config)
	bondCheck(t, err)
	engine, err := s.NewDiscountingBondEngine(discount, settings, nil)
	bondCheck(t, err)
	bondCheck(t, bond.SetEngine(engine))
	return bond, counter
}

func TestAmortizingFloatingQuantLibOracles(t *testing.T) {
	rows := amortOracleRows(t)
	financial := 0
	for i, row := range rows {
		if row.Terms == nil {
			continue
		}
		financial++
		t.Run(fmt.Sprintf("%d-%s-%s", i, row.CaseID, row.Phase), func(t *testing.T) {
			s, err := NewSession()
			bondCheck(t, err)
			defer s.Close()
			bond, counter := amortOracleGraph(t, s, row)
			maturity, err := bond.MaturityDate()
			bondCheck(t, err)
			settlement, err := bond.SettlementDate(nil)
			bondCheck(t, err)
			expired, err := bond.IsExpired()
			bondCheck(t, err)
			if maturity != amortOracleDate(t, row.Maturity) || settlement != amortOracleDate(t, row.Settlement) || expired != row.Expired {
				t.Fatal("native dates/expiry")
			}
			for _, check := range []struct {
				expected amortObservation
				read     func() (float64, error)
			}{{row.NPV, bond.NPV}, {row.SettlementValue, bond.SettlementValue}, {row.CleanPrice, bond.CleanPrice}, {row.DirtyPrice, bond.DirtyPrice}, {row.Accrued, func() (float64, error) { return bond.AccruedAmount(nil) }}, {row.Notional, func() (float64, error) { return bond.Notional(nil) }}} {
				amortOracleObserve(t, check.expected, check.read)
			}
			leg, err := bond.Cashflows()
			bondCheck(t, err)
			count, err := leg.Len()
			bondCheck(t, err)
			if count != len(row.Flows) {
				t.Fatal("native cashflow count")
			}
			for j, expected := range row.Flows {
				flow, err := leg.At(j)
				bondCheck(t, err)
				payment, err := flow.Date()
				bondCheck(t, err)
				if payment != amortOracleDate(t, expected.Payment) {
					t.Fatal("native cashflow date")
				}
				amortOracleObserve(t, expected.Amount, flow.Amount)
				bondCheck(t, flow.Close())
			}
			principal, err := bond.Redemptions()
			bondCheck(t, err)
			count, err = principal.Len()
			bondCheck(t, err)
			if count != len(row.Redemptions) {
				t.Fatal("native principal count")
			}
			for j, expected := range row.Redemptions {
				flow, err := principal.At(j)
				bondCheck(t, err)
				payment, err := flow.Date()
				bondCheck(t, err)
				if payment != amortOracleDate(t, expected.Date) {
					t.Fatal("native principal date")
				}
				amortOracleObserve(t, expected.Amount, flow.Amount)
				bondCheck(t, flow.Close())
			}
			for _, probe := range row.NotionalsByDate {
				on := amortOracleDate(t, probe.Date)
				amortOracleObserve(t, probe.Value, func() (float64, error) { return bond.Notional(&on) })
			}
			accuracy, evaluations, guess := 1e-12, uint(200), 0.03
			analytics := BondYieldConfig{DayCounter: counter, Compounding: Compounded, Frequency: Semiannual, SettlementDate: &settlement}
			for _, check := range []struct {
				kind            BondPriceType
				price           amortObservation
				rate, inversion *amortObservation
			}{{Clean, row.CleanPrice, row.CleanYield, row.CleanInversion}, {Dirty, row.DirtyPrice, row.DirtyYield, row.DirtyInversion}} {
				if check.rate == nil {
					continue
				}
				solve := func() (float64, error) {
					return bond.YieldRate(check.price.Value, BondYieldSolveConfig{BondYieldConfig: analytics, PriceType: check.kind, AnalyticsSolverConfig: AnalyticsSolverConfig{Accuracy: &accuracy, MaxEvaluations: &evaluations, Guess: &guess}})
				}
				if row.Notional.OK && row.Notional.Value == 0 {
					if !check.rate.OK || check.rate.Value != 0 || check.inversion == nil || check.inversion.OK != (check.kind == Dirty) || (check.inversion.OK && check.inversion.Value != 0) {
						t.Fatal("preserved native Bond zero-notional shortcut qualification")
					}
					amortNontradable(t, solve)
					if check.kind == Clean {
						amortNontradable(t, func() (float64, error) { return bond.CleanPriceFromYield(check.rate.Value, analytics) })
					} else {
						amortNontradable(t, func() (float64, error) { return bond.DirtyPriceFromYield(check.rate.Value, analytics) })
					}
					continue
				}
				rate := amortOracleObserve(t, *check.rate, solve)
				if check.rate.OK && check.inversion != nil {
					if check.kind == Clean {
						amortOracleObserve(t, *check.inversion, func() (float64, error) { return bond.CleanPriceFromYield(rate, analytics) })
					} else {
						amortOracleObserve(t, *check.inversion, func() (float64, error) { return bond.DirtyPriceFromYield(rate, analytics) })
					}
				}
			}
		})
	}
	if financial != 52 {
		t.Fatal("expected 52 native financial states")
	}
}

func TestAmortizingFloatingExistingBulletZeroNotionalAnalytics(t *testing.T) {
	s, err := NewSession()
	bondCheck(t, err)
	defer s.Close()
	config, _, _ := amortMarket(t, s, false)
	settlement := testDate(t, 6, 10, 2027)
	bondCheck(t, config.Settings.SetEvaluationDate(settlement))
	bullet, err := s.NewFloatingRateBond(FloatingRateBondConfig{FaceAmount: 1000, Schedule: config.Schedule, Index: config.Index, AccrualDayCounter: config.AccrualDayCounter, Settings: config.Settings})
	bondCheck(t, err)
	notional, err := bullet.Notional(&settlement)
	bondCheck(t, err)
	amortNear(t, notional, 0)
	analytics := BondYieldConfig{DayCounter: config.AccrualDayCounter, Compounding: Compounded, Frequency: Semiannual, SettlementDate: &settlement}
	for _, kind := range []BondPriceType{Clean, Dirty} {
		amortNontradable(t, func() (float64, error) {
			return bullet.YieldRate(0, BondYieldSolveConfig{BondYieldConfig: analytics, PriceType: kind})
		})
	}
	amortNontradable(t, func() (float64, error) { return bullet.CleanPriceFromYield(0, analytics) })
	amortNontradable(t, func() (float64, error) { return bullet.DirtyPriceFromYield(0, analytics) })
}
