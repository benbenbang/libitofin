package itofin

import (
	"encoding/json"
	"errors"
	"math"
	"os"
	"strings"
	"sync"
	"testing"
	"time"
)

type cpiBondSnapshot struct {
	NPV        float64 `json:"npv"`
	Clean      float64 `json:"clean_price"`
	Dirty      float64 `json:"dirty_price"`
	Accrued    float64 `json:"accrued_per100"`
	Notional   float64 `json:"notional"`
	Settlement string  `json:"settlement"`
	Maturity   string  `json:"maturity"`
	Flows      []struct {
		Date   string  `json:"date"`
		Amount float64 `json:"amount"`
	} `json:"cashflows"`
}

func cpiBondRows(t *testing.T) map[string]cpiBondSnapshot {
	t.Helper()
	data, err := os.ReadFile("testdata/cpi-oracle-bonds.json")
	bondCheck(t, err)
	var fixture struct {
		Bonds []struct {
			Name   string          `json:"name"`
			Values cpiBondSnapshot `json:"values"`
		} `json:"bonds"`
	}
	bondCheck(t, json.Unmarshal(data, &fixture))
	result := map[string]cpiBondSnapshot{}
	for _, row := range fixture.Bonds {
		result[row.Name] = row.Values
	}
	return result
}
func cpiISO(t *testing.T, text string) Date {
	t.Helper()
	d, err := time.Parse("2006-01-02", text)
	bondCheck(t, err)
	return testDate(t, d.Day(), int(d.Month()), d.Year())
}
func cpiBondConfig(t *testing.T, s *Session, linear, weekend, omitApril bool, april float64) (CPIBondConfig, *SimpleQuote) {
	t.Helper()
	settings := pricingMust(s.NewSettings())
	today := testDate(t, 15, 6, 2025)
	bondCheck(t, settings.SetEvaluationDate(today))
	index := pricingMust(s.NewUKRPI(settings))
	dc := pricingMust(s.Actual365Fixed())
	curve := pricingMust(s.NewInterpolatedZeroInflationCurve(InflationCurveConfig{ReferenceDate: today, Frequency: Monthly, DayCounter: dc}, []Date{testDate(t, 1, 1, 2025), testDate(t, 1, 1, 2035)}, []float64{.025, .025}))
	bondCheck(t, index.LinkTo(curve))
	for i, value := range []float64{99.5, 99.8, 100, 100.2, 100.4, 100.6, april, 101} {
		month, year := 10+i, 2024
		if month > 12 {
			month -= 12
			year = 2025
		}
		if month == 4 && omitApril {
			continue
		}
		bondCheck(t, index.AddFixing(testDate(t, 1, month, year), value))
	}
	calendar := pricingMust(s.NullCalendar())
	end := testDate(t, 15, 1, 2028)
	payment := Unadjusted
	if weekend {
		end = testDate(t, 15, 7, 2028)
		payment = Following
	}
	schedule := pricingMust(s.NewSchedule(ScheduleConfig{Start: testDate(t, 15, 1, 2025), End: end, Frequency: Semiannual, Calendar: calendar, Convention: Unadjusted}))
	config := CPIBondConfig{SettlementDays: 2, FaceAmount: 100000, BaseCPI: 100, ObservationLag: Period{3, Months}, Index: index, Schedule: schedule, Coupons: []float64{.02}, AccrualDayCounter: dc, Settings: settings, PaymentConvention: &payment, IssueDate: pricingPtr(testDate(t, 15, 1, 2025)), PaymentCalendar: calendar}
	if linear {
		config.ObservationInterpolation = CpiLinear
	}
	if weekend {
		config.PaymentCalendar = pricingMust(s.WeekendsOnly())
		config.ExCouponCalendar = config.PaymentCalendar
		config.ExCouponPeriod = &Period{5, Days}
	}
	quote := pricingMust(s.NewSimpleQuote(.04))
	return config, quote
}
func cpiBondAssert(t *testing.T, bond *CPIBond, row cpiBondSnapshot) {
	t.Helper()
	reads := []func() (float64, error){bond.NPV, bond.CleanPrice, bond.DirtyPrice, func() (float64, error) { return bond.AccruedAmount(nil) }, func() (float64, error) { return bond.Notional(nil) }}
	for i, want := range []float64{row.NPV, row.Clean, row.Dirty, row.Accrued, row.Notional} {
		got, err := reads[i]()
		bondCheck(t, err)
		if math.IsNaN(got) || math.IsInf(got, 0) || math.IsNaN(want) || math.IsInf(want, 0) || math.Abs(got-want) > 2e-8 {
			t.Fatalf("value %d got %.15g want %.15g", i, got, want)
		}
	}
	if pricingMust(bond.SettlementDate(nil)).Serial() != cpiISO(t, row.Settlement).Serial() || pricingMust(bond.MaturityDate()).Serial() != cpiISO(t, row.Maturity).Serial() {
		t.Fatal("dates")
	}
	leg := pricingMust(bond.Cashflows())
	defer leg.Close()
	if pricingMust(leg.Len()) != len(row.Flows) {
		t.Fatal("cashflow count")
	}
	for i, want := range row.Flows {
		flow := pricingMust(leg.At(i))
		if pricingMust(flow.Date()).Serial() != cpiISO(t, want.Date).Serial() {
			t.Fatal("cashflow date")
		}
		got := pricingMust(flow.Amount())
		if math.IsNaN(got) || math.IsInf(got, 0) || math.Abs(got-want.Amount) > 2e-8 {
			t.Fatalf("flow%d %.15g != %.15g", i, got, want.Amount)
		}
		bondCheck(t, flow.Close())
	}
	redemptions := pricingMust(bond.Redemptions())
	defer redemptions.Close()
	flow := pricingMust(redemptions.At(0))
	defer flow.Close()
	if got := pricingMust(flow.Amount()); math.IsNaN(got) || math.IsInf(got, 0) || math.Abs(got-row.Flows[len(row.Flows)-1].Amount) > 2e-8 {
		t.Fatal("indexed principal")
	}
}
func TestCPIBondNativeAndLive(t *testing.T) {
	rows := cpiBondRows(t)
	for _, linear := range []bool{false, true} {
		t.Run(map[bool]string{false: "flat", true: "linear"}[linear], func(t *testing.T) {
			s := pricingMust(NewSession())
			defer s.Close()
			config, quote := cpiBondConfig(t, s, linear, false, false, 100.8)
			b := pricingMust(s.NewCPIBond(config))
			curve := pricingMust(s.NewFlatForwardFromQuote(testDate(t, 15, 6, 2025), quote, config.AccrualDayCounter))
			engine := pricingMust(s.NewDiscountingBondEngine(curve, config.Settings, nil))
			bondCheck(t, b.SetEngine(engine))
			label := map[bool]string{false: "flat", true: "linear"}[linear]
			cpiBondAssert(t, b, rows[label])
			bondCheck(t, quote.SetValue(.05))
			cpiBondAssert(t, b, rows[label+"_discount_quote_5pct"])
			config2, quote2 := cpiBondConfig(t, s, linear, false, false, 101.8)
			b2 := pricingMust(s.NewCPIBond(config2))
			curve2 := pricingMust(s.NewFlatForwardFromQuote(testDate(t, 15, 6, 2025), quote2, config2.AccrualDayCounter))
			bondCheck(t, b2.SetEngine(pricingMust(s.NewDiscountingBondEngine(curve2, config2.Settings, nil))))
			bondCheck(t, quote2.SetValue(.05))
			cpiBondAssert(t, b2, rows[label+"_fixing_updated"])
			updated := pricingMust(s.NewInterpolatedZeroInflationCurve(InflationCurveConfig{ReferenceDate: testDate(t, 15, 6, 2025), Frequency: Monthly, DayCounter: config2.AccrualDayCounter}, []Date{testDate(t, 1, 1, 2025), testDate(t, 1, 1, 2035)}, []float64{.03, .03}))
			bondCheck(t, config2.Index.LinkTo(updated))
			cpiBondAssert(t, b2, rows[label+"_inflation_relinked_3pct"])
		})
	}
}
func TestCPIBondWeekendExCouponAndRetainedFixings(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	rows := cpiBondRows(t)
	config, quote := cpiBondConfig(t, s, true, true, false, 100.8)
	b := pricingMust(s.NewCPIBond(config))
	curve := pricingMust(s.NewFlatForwardFromQuote(testDate(t, 15, 6, 2025), quote, config.AccrualDayCounter))
	engine := pricingMust(s.NewDiscountingBondEngine(curve, config.Settings, nil))
	bondCheck(t, b.SetEngine(engine))
	cpiBondAssert(t, b, rows["linear_adjusted_terminal_weekend"])
	bondCheck(t, config.Settings.SetEvaluationDate(testDate(t, 10, 7, 2025)))
	cpiBondAssert(t, b, rows["linear_negative_excoupon"])
	missing, q := cpiBondConfig(t, s, false, false, true, 100.8)
	lazy := pricingMust(s.NewCPIBond(missing))
	discount := pricingMust(s.NewFlatForwardFromQuote(testDate(t, 15, 6, 2025), q, missing.AccrualDayCounter))
	bondCheck(t, lazy.SetEngine(pricingMust(s.NewDiscountingBondEngine(discount, missing.Settings, nil))))
	if _, err := lazy.NPV(); err == nil {
		t.Fatal("missing fixing accepted")
	}
	bondCheck(t, missing.Index.AddFixing(testDate(t, 1, 4, 2025), 100.8))
	cpiBondAssert(t, lazy, rows["flat"])
	for _, close := range []func() error{missing.Index.Close, discount.Close, q.Close, missing.Schedule.Close, missing.AccrualDayCounter.Close} {
		bondCheck(t, close())
	}
	cpiBondAssert(t, lazy, rows["flat"])
	var wg sync.WaitGroup
	for range 4 {
		wg.Add(1)
		go func() {
			defer wg.Done()
			_, err := lazy.NPV()
			if err != nil {
				t.Error(err)
			}
		}()
	}
	wg.Wait()
	redemptions := pricingMust(lazy.Redemptions())
	retained := pricingMust(redemptions.At(0))
	wantPrincipal := pricingMust(retained.Amount())
	bondCheck(t, redemptions.Close())
	bondCheck(t, lazy.Close())
	bondCheck(t, lazy.Close())
	gotPrincipal := pricingMust(retained.Amount())
	if math.IsNaN(gotPrincipal) || gotPrincipal != wantPrincipal {
		t.Fatal("retained principal after bond close")
	}
	bondCheck(t, missing.Settings.SetEvaluationDate(testDate(t, 16, 1, 2028)))
	if _, err := retained.Amount(); err == nil {
		t.Fatal("retained CPI principal lost fixing dependency")
	}
	bondCheck(t, retained.Close())
	_, err := lazy.NPV()
	var native *Error
	if !errors.As(err, &native) || native.Code != 2 {
		t.Fatalf("released bond handle %v", err)
	}
	bondCheck(t, s.Close())
	if _, err := lazy.NPV(); !errors.Is(err, ErrClosed) {
		t.Fatalf("closed session %v", err)
	}
}
func TestCPIBondErrorsAndSessions(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	a, _ := cpiBondConfig(t, s, false, false, true, 100.8)
	for _, bad := range []float64{0, -1, math.NaN(), math.Inf(1)} {
		copy := a
		copy.BaseCPI = bad
		if _, err := s.NewCPIBond(copy); err == nil {
			t.Fatal("invalid base CPI")
		}
	}
	other := pricingMust(NewSession())
	defer other.Close()
	foreign, _ := cpiBondConfig(t, other, false, false, false, 100.8)
	copy := a
	copy.Index = foreign.Index
	if _, err := s.NewCPIBond(copy); !errors.Is(err, ErrSessionMismatch) {
		t.Fatal("mixed session")
	}
	copy = a
	copy.Settings = pricingMust(s.NewSettings())
	if _, err := s.NewCPIBond(copy); err == nil {
		t.Fatal("mixed settings")
	}
	b := pricingMust(s.NewCPIBond(a))
	curve := pricingMust(s.NewFlatForward(testDate(t, 15, 6, 2025), .04, a.AccrualDayCounter))
	_, err := s.NewBondForward(BondForwardConfig{Bond: &b.Bond, ValueDate: testDate(t, 15, 6, 2025), DeliveryDate: testDate(t, 15, 8, 2025), Position: PositionLong, Strike: 100, DayCounter: a.AccrualDayCounter, Calendar: a.PaymentCalendar, Discount: curve})
	if err == nil {
		t.Fatal("CPI bondforward unexpectedly supported")
	}
}

func TestCPIBondExtractedLegSettingsAndFiniteSums(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	config, _ := cpiBondConfig(t, s, false, false, false, 100.8)
	bond := pricingMust(s.NewCPIBond(config))
	legs := []*Leg{pricingMust(bond.Cashflows()), pricingMust(bond.Redemptions())}
	bondCheck(t, bond.Close())
	discount := pricingMust(s.NewFlatForward(testDate(t, 15, 6, 2025), .04, config.AccrualDayCounter))
	other := pricingMust(s.NewSettings())
	bondCheck(t, other.SetEvaluationDate(testDate(t, 15, 6, 2025)))
	for _, leg := range legs {
		if _, err := leg.NPV(CashFlowsNPVConfig{Discount: discount, Settings: other}); err == nil {
			t.Fatal("extracted CPI leg accepted mixed settings")
		}
		badSpreadConfig := CashFlowsZSpreadConfig{CashFlowsNPVConfig: CashFlowsNPVConfig{Discount: discount, Settings: other}, Compounding: Continuous, Frequency: Annual}
		if _, err := leg.NPVAtZSpread(0, badSpreadConfig); err == nil || !strings.Contains(err.Error(), "settings") {
			t.Fatal("CPI spreadNPV settings not checked")
		}
		if _, err := leg.ZSpread(100000, CashFlowsZSpreadSolveConfig{CashFlowsZSpreadConfig: badSpreadConfig}); err == nil || !strings.Contains(err.Error(), "settings") {
			t.Fatal("CPI spreadsolve settings not checked")
		}
		if got := pricingMust(leg.NPV(CashFlowsNPVConfig{Discount: discount, Settings: config.Settings})); math.IsNaN(got) || math.IsInf(got, 0) {
			t.Fatal("nonfinite leg")
		}
	}
	config.FaceAmount = 8e307
	config.Coupons = []float64{1}
	huge := pricingMust(s.NewCPIBond(config))
	flows := pricingMust(huge.Cashflows())
	for i := 0; i < pricingMust(flows.Len()); i++ {
		flow := pricingMust(flows.At(i))
		if got := pricingMust(flow.Amount()); math.IsNaN(got) || math.IsInf(got, 0) {
			t.Fatal("individual flow not finite")
		}
		bondCheck(t, flow.Close())
	}
	if _, err := flows.NPV(CashFlowsNPVConfig{Discount: discount, Settings: config.Settings}); err == nil {
		t.Fatal("overflowing CPI sum accepted")
	}
}

func TestCPIBondNativePrecedingPayment(t *testing.T) {
	data, err := os.ReadFile("testdata/cpi-bond-preceding.json")
	bondCheck(t, err)
	var fixture struct {
		Rows []struct {
			Tenor    int             `json:"tenor_months"`
			Rates    []float64       `json:"rates"`
			Initial  cpiBondSnapshot `json:"initial"`
			Terminal []struct {
				Include     bool    `json:"include_reference_date_events"`
				Expired     bool    `json:"expired"`
				IncludedNPV float64 `json:"cashflow_npv_include_settlement"`
				ExcludedNPV float64 `json:"cashflow_npv_exclude_settlement"`
			} `json:"terminal"`
		} `json:"rows"`
	}
	bondCheck(t, json.Unmarshal(data, &fixture))
	for _, row := range fixture.Rows {
		s := pricingMust(NewSession())
		config, _ := cpiBondConfig(t, s, true, false, false, 100.8)
		config.SettlementDays = 0
		config.FaceAmount = 1000
		config.Coupons = row.Rates
		config.PaymentConvention = pricingPtr(Preceding)
		config.PaymentCalendar = pricingMust(s.WeekendsOnly())
		freq := Semiannual
		if row.Tenor == 12 {
			freq = Annual
		}
		config.Schedule = pricingMust(s.NewSchedule(ScheduleConfig{Start: testDate(t, 15, 1, 2025), End: testDate(t, 19, 7, 2025), Frequency: freq, Calendar: pricingMust(s.NullCalendar()), Convention: Unadjusted}))
		b := pricingMust(s.NewCPIBond(config))
		discount := pricingMust(s.NewFlatForward(testDate(t, 15, 6, 2025), .04, config.AccrualDayCounter))
		bondCheck(t, b.SetEngine(pricingMust(s.NewDiscountingBondEngine(discount, config.Settings, nil))))
		cpiBondAssert(t, b, row.Initial)
		leg := pricingMust(b.Cashflows())
		for _, terminal := range row.Terminal {
			bondCheck(t, config.Settings.SetIncludeReferenceDateEvents(terminal.Include))
			bondCheck(t, config.Settings.SetEvaluationDate(testDate(t, 18, 7, 2025)))
			if pricingMust(b.Notional(pricingPtr(testDate(t, 17, 7, 2025)))) != 1000 || pricingMust(b.Notional(pricingPtr(testDate(t, 18, 7, 2025)))) != 0 || pricingMust(b.Notional(pricingPtr(testDate(t, 19, 7, 2025)))) != 0 {
				t.Fatal("terminal notional convention")
			}
			if pricingMust(b.IsExpired()) != terminal.Expired {
				t.Fatal("event inclusion")
			}
			for _, include := range []bool{false, true} {
				want := terminal.ExcludedNPV
				if include {
					want = terminal.IncludedNPV
				}
				got := pricingMust(leg.NPV(CashFlowsNPVConfig{Discount: discount, Settings: config.Settings, IncludeSettlementDateFlows: &include, SettlementDate: pricingPtr(testDate(t, 18, 7, 2025)), NPVDate: pricingPtr(testDate(t, 18, 7, 2025))}))
				if math.IsNaN(got) || math.IsInf(got, 0) || math.Abs(got-want) > 2e-10 {
					t.Fatal("terminal leg npv")
				}
			}
		}
		bondCheck(t, s.Close())
	}
}

func TestCPIBondRawCouponBPSAndATMAreUnsupported(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	config, _ := cpiBondConfig(t, s, false, false, true, 100.8)
	bond := pricingMust(s.NewCPIBond(config))
	curve := pricingMust(s.NewFlatForward(testDate(t, 15, 6, 2025), .04, config.AccrualDayCounter))
	yieldConfig := BondYieldConfig{DayCounter: config.AccrualDayCounter, Compounding: Continuous, Frequency: Annual}
	queries := []func() (float64, error){func() (float64, error) { return bond.BPSFromCurve(curve, nil) }, func() (float64, error) { return bond.BPSFromYield(.04, yieldConfig) }, func() (float64, error) { return bond.ATMRateFromCurve(curve, nil) }, func() (float64, error) { return bond.ATMRateFromPrice(curve, 100, Clean, nil) }}
	for _, query := range queries {
		if _, err := query(); err == nil || !strings.Contains(err.Error(), "CPI raw-coupon") {
			t.Fatalf("expectedunsupportederrorbeforefixing: %v", err)
		}
	}
}
