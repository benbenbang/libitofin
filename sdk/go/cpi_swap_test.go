package itofin

import (
	"encoding/json"
	"errors"
	"math"
	"os"
	"testing"
)

type cpiSwapMarket struct {
	swap     *CPISwap
	settings *Settings
	quote    *SimpleQuote
	discount *YieldTermStructure
	index    *ZeroInflationIndex
	floating *IborIndex
	config   CPISwapConfig
}
type cpiSwapValues struct {
	NPV   float64 `json:"npv"`
	CPI   float64 `json:"cpi_leg_npv"`
	Float float64 `json:"floating_leg_npv"`
	BPS   float64 `json:"floating_leg_bps"`
	Fair  float64 `json:"fair_spread"`
}
type cpiSwapRow struct {
	Interpolation string                   `json:"interpolation"`
	Growth        bool                     `json:"growth_only"`
	Nominal       float64                  `json:"inflation_nominal"`
	Values        map[string]cpiSwapValues `json:"values"`
	Side          string
}

func cpiSwapRows(t *testing.T) []cpiSwapRow {
	t.Helper()
	data, e := os.ReadFile("testdata/cpi-oracle-swaps.json")
	bondCheck(t, e)
	var oracle struct {
		Swaps []cpiSwapRow `json:"swaps"`
	}
	bondCheck(t, json.Unmarshal(data, &oracle))
	if len(oracle.Swaps) != 8 {
		t.Fatalf("oracle profiles=%d, want8", len(oracle.Swaps))
	}
	var rows []cpiSwapRow
	for _, profile := range oracle.Swaps {
		for _, side := range []string{"payer", "receiver"} {
			if _, ok := profile.Values[side]; !ok {
				t.Fatalf("missing side %s", side)
			}
			profile.Side = side
			rows = append(rows, profile)
		}
	}
	return rows
}
func cpiSwapFixture(t *testing.T, s *Session, row cpiSwapRow, spread float64) cpiSwapMarket {
	t.Helper()
	settings := pricingMust(s.NewSettings())
	today := testDate(t, 15, 6, 2025)
	bondCheck(t, settings.SetEvaluationDate(today))
	dc := pricingMust(s.Actual365Fixed())
	index := pricingMust(s.NewUKRPI(settings))
	curve := pricingMust(s.NewInterpolatedZeroInflationCurve(InflationCurveConfig{ReferenceDate: today, Frequency: Monthly, DayCounter: dc}, []Date{testDate(t, 1, 1, 2025), testDate(t, 1, 1, 2035)}, []float64{0.025, 0.025}))
	bondCheck(t, index.LinkTo(curve))
	for i, value := range []float64{99.5, 99.8, 100.0, 100.2, 100.4, 100.6, 100.8, 101.0} {
		month, year := 10+i, 2024
		if month > 12 {
			month -= 12
			year++
		}
		bondCheck(t, index.AddFixing(testDate(t, 1, month, year), value))
	}
	quote := pricingMust(s.NewSimpleQuote(0.04))
	discount := pricingMust(s.NewFlatForwardFromQuote(today, quote, dc))
	cal := pricingMust(s.UnitedKingdom("Exchange"))
	floating := pricingMust(s.NewIborIndex(IborIndexConfig{FamilyName: "CpiSwapGBP", Tenor: Period{6, Months}, SettlementDays: 0, Currency: pricingMust(s.GBP()), FixingCalendar: cal, Convention: ModifiedFollowing, EndOfMonth: true, DayCounter: dc, Forwarding: discount, Settings: settings}))
	bondCheck(t, floating.AddFixing(testDate(t, 15, 1, 2025), 0.042))
	schedule := pricingMust(s.NewSchedule(ScheduleConfig{Start: testDate(t, 15, 1, 2025), End: testDate(t, 15, 1, 2028), Frequency: Semiannual, Calendar: pricingMust(s.NullCalendar()), Convention: Unadjusted}))
	nominal := row.Nominal
	a := CPISwapConfig{Type: SwapPayer, Nominal: 100000, InflationNominal: &nominal, SubtractInflationNominal: row.Growth, Spread: spread, FloatSchedule: schedule, FloatIndex: floating, FloatDayCounter: dc, FloatConvention: Unadjusted, FixedRate: 0.02, BaseCPI: 100, FixedSchedule: schedule, FixedIndex: index, FixedDayCounter: dc, FixedConvention: Unadjusted, ObservationLag: Period{3, Months}, Interpolation: CpiFlat, Settings: settings}
	if row.Interpolation == "linear" {
		a.Interpolation = CpiLinear
	}
	if row.Side == "receiver" {
		a.Type = SwapReceiver
	}
	swap := pricingMust(s.NewCPISwap(a))
	bondCheck(t, swap.SetEngine(discount, settings))
	return cpiSwapMarket{swap, settings, quote, discount, index, floating, a}
}
func TestCPISwapIndependentNativeProfiles(t *testing.T) {
	for _, row := range cpiSwapRows(t) {
		t.Run(row.Interpolation+row.Side, func(t *testing.T) {
			s := pricingMust(NewSession())
			defer s.Close()
			m := cpiSwapFixture(t, s, row, 0.001)
			methods := []func() (float64, error){m.swap.NPV, m.swap.FixedLegNPV, m.swap.FloatLegNPV, m.swap.FloatLegBPS, m.swap.FairSpread}
			for i, method := range methods {
				value := pricingMust(method())
				expected := row.Values[row.Side]
				want := []float64{expected.NPV, expected.CPI, expected.Float, expected.BPS, expected.Fair}[i]
				tolerance := 2e-9
				if i == 4 {
					tolerance = 2e-13
				}
				if !isFiniteFloatFloat(value) || !isFiniteFloatFloat(want) || math.Abs(value-want) > tolerance {
					t.Fatalf("query%d %.17g want %.17g", i, value, want)
				}
			}
			config := m.config
			config.Spread = pricingMust(m.swap.FairSpread())
			par := pricingMust(s.NewCPISwap(config))
			bondCheck(t, par.SetEngine(m.discount, m.settings))
			parNPV := pricingMust(par.NPV())
			if !isFiniteFloatFloat(parNPV) || math.Abs(parNPV) > 2e-9 {
				t.Fatal("fair spread does not reprice")
			}
		})
	}
}
func TestCPISwapLifecycleRetainsInputsAndSession(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	m := cpiSwapFixture(t, s, cpiSwapRows(t)[0], 0.001)
	before := pricingMust(m.swap.NPV())
	bondCheck(t, m.swap.Freeze())
	bondCheck(t, m.quote.SetValue(0.05))
	if pricingMust(m.swap.NPV()) != before {
		t.Fatal("frozen result changed")
	}
	bondCheck(t, m.swap.Recalculate())
	recalculated := pricingMust(m.swap.NPV())
	if !isFiniteFloatFloat(recalculated) || math.Abs(recalculated-before) < 1 {
		t.Fatal("forced recalculation unchanged")
	}
	bondCheck(t, m.swap.Unfreeze())
	other := pricingMust(NewSession())
	defer other.Close()
	if _, e := other.NewCPISwap(m.config); !errors.Is(e, ErrSessionMismatch) {
		t.Fatal(e)
	}
	for _, close := range []func() error{m.settings.Close, m.discount.Close, m.index.Close, m.floating.Close, m.config.FloatSchedule.Close} {
		bondCheck(t, close())
	}
	bondCheck(t, m.quote.SetValue(0.04))
	if !isFiniteFloatFloat(pricingMust(m.swap.NPV())) {
		t.Fatal("retention failed")
	}
	failures := make(chan error, 16)
	for range 16 {
		go func() { _, e := m.swap.NPV(); failures <- e }()
	}
	for range 16 {
		bondCheck(t, <-failures)
	}
	bondCheck(t, m.swap.Close())
	if _, e := m.swap.NPV(); e == nil {
		t.Fatal("released swap accepted")
	}
}
func TestCPISwapNilNumericAndEnumGuards(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	m := cpiSwapFixture(t, s, cpiSwapRows(t)[0], 0.001)
	for _, value := range []float64{0, -1, math.NaN(), math.Inf(1)} {
		a := m.config
		a.BaseCPI = value
		if _, e := s.NewCPISwap(a); e == nil {
			t.Fatal("invalid CPI accepted")
		}
	}
	a := m.config
	a.Type = SwapType(99)
	if _, e := s.NewCPISwap(a); e == nil {
		t.Fatal("invalid side")
	}
	a = m.config
	a.Interpolation = CpiInterpolationType(99)
	if _, e := s.NewCPISwap(a); e == nil {
		t.Fatal("invalid interpolation")
	}
	a = m.config
	a.FixedIndex = nil
	if _, e := s.NewCPISwap(a); e == nil {
		t.Fatal("nil index")
	}
	var nilSwap *CPISwap
	if _, e := nilSwap.NPV(); e == nil {
		t.Fatal("nil swap")
	}
	if nilSwap.Freeze() == nil {
		t.Fatal("nil action")
	}
}
