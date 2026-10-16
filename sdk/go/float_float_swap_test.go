package itofin

import (
	"encoding/csv"
	"encoding/json"
	"errors"
	"math"
	"os"
	"strconv"
	"testing"
)

type floatFloatMarket struct {
	swap     *FloatFloatSwap
	settings *Settings
	quotes   []*SimpleQuote
	curves   []*YieldTermStructure
	indexes  []*IborIndex
	config   FloatFloatSwapConfig
}

func floatFloatFixture(t *testing.T, s *Session, row []string) floatFloatMarket {
	t.Helper()
	n := func(i int) float64 { x, e := strconv.ParseFloat(row[i], 64); bondCheck(t, e); return x }
	settings := pricingMust(s.NewSettings())
	bondCheck(t, settings.SetEvaluationDate(pricingMust(DateFromSerial(int32(n(1))))))
	dc := pricingMust(s.Actual360())
	dc365 := pricingMust(s.Actual365Fixed())
	currency := pricingMust(s.EUR())
	cal := pricingMust(s.NullCalendar())
	weekends := pricingMust(s.WeekendsOnly())
	ref := testDate(t, 2, 1, 2026)
	quotes := []*SimpleQuote{}
	curves := []*YieldTermStructure{}
	for _, rate := range []float64{n(6), n(7), n(8)} {
		q := pricingMust(s.NewSimpleQuote(rate))
		quotes = append(quotes, q)
		curves = append(curves, pricingMust(s.NewFlatForwardFromQuote(ref, q, dc)))
	}
	indexes := []*IborIndex{}
	for i, months := range []int32{3, 6} {
		indexes = append(indexes, pricingMust(s.NewIborIndex(IborIndexConfig{FamilyName: "binding-float-" + strconv.Itoa(i), Tenor: Period{months, Months}, Currency: currency, FixingCalendar: cal, Convention: Unadjusted, DayCounter: dc, Forwarding: curves[i], Settings: settings})))
	}
	schedules := []*Schedule{}
	for _, frequency := range []Frequency{Quarterly, Semiannual} {
		schedules = append(schedules, pricingMust(s.NewSchedule(ScheduleConfig{Start: testDate(t, 17, 1, 2026), End: testDate(t, 17, 1, 2027), Frequency: frequency, Calendar: weekends, Convention: Unadjusted})))
	}
	config := FloatFloatSwapConfig{Nominal: 1_000_000, PaySpread: n(9), ReceiveSpread: n(10), PaySchedule: schedules[0], ReceiveSchedule: schedules[1], PayIndex: indexes[0], ReceiveIndex: indexes[1], PayDayCounter: dc, ReceiveDayCounter: dc365, Settings: settings}
	if n(11) != 0 {
		config.PaymentConvention = pricingPtr(Following)
	}
	swap := pricingMust(s.NewFloatFloatSwap(config))
	bondCheck(t, swap.SetEngine(curves[2], settings))
	return floatFloatMarket{swap, settings, quotes, curves, indexes, config}
}
func floatFloatRows(t *testing.T) [][]string {
	t.Helper()
	f, e := os.Open("testdata/float_float_swap_native.csv")
	bondCheck(t, e)
	defer f.Close()
	rows, e := csv.NewReader(f).ReadAll()
	bondCheck(t, e)
	return rows[1:]
}
func TestFloatFloatNativeSignedOracle(t *testing.T) {
	for _, row := range floatFloatRows(t) {
		if row[0] != "base" && row[0] != "negative" && row[0] != "following" && row[0] != "expired" {
			continue
		}
		t.Run(row[0], func(t *testing.T) {
			s := pricingMust(NewSession())
			defer s.Close()
			m := floatFloatFixture(t, s, row)
			if pricingMust(m.swap.IsCalculated()) {
				t.Fatal("initial cache flag")
			}
			bondCheck(t, m.swap.Calculate())
			if !pricingMust(m.swap.IsCalculated()) {
				t.Fatal("calculate did not populate cache")
			}
			if pricingMust(m.swap.IsExpired()) != (row[0] == "expired") {
				t.Fatal("expiry flag")
			}
			methods := []func() (float64, error){m.swap.NPV, m.swap.PayLegNPV, m.swap.ReceiveLegNPV, m.swap.PayLegBPS, m.swap.ReceiveLegBPS, m.swap.FairPaySpread, m.swap.FairReceiveSpread}
			for i, method := range methods {
				value, e := method()
				if row[0] == "expired" && i >= 5 {
					if e == nil {
						t.Fatal("expired fair sensitivity")
					}
					continue
				}
				bondCheck(t, e)
				want, e := strconv.ParseFloat(row[12+i], 64)
				bondCheck(t, e)
				tol := 2e-9
				if i >= 5 {
					tol = 2e-13
				}
				if math.Abs(value-want) > tol {
					t.Fatalf("query %d: %.17g want %.17g", i, value, want)
				}
			}
		})
	}
}
func TestFloatFloatIdentityLifetimeAndFreeze(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	m := floatFloatFixture(t, s, floatFloatRows(t)[0])
	before := pricingMust(m.swap.NPV())
	other := pricingMust(s.NewSettings())
	bondCheck(t, other.SetEvaluationDate(testDate(t, 2, 1, 2026)))
	config := m.config
	config.Settings = other
	if _, e := s.NewFloatFloatSwap(config); e == nil {
		t.Fatal("distinct constructor Settings accepted")
	}
	if m.swap.SetEngine(m.curves[0], other) == nil {
		t.Fatal("distinct settings accepted")
	}
	if pricingMust(m.swap.NPV()) != before {
		t.Fatal("failed engine replacement mutated value")
	}
	bondCheck(t, m.swap.Freeze())
	bondCheck(t, m.quotes[0].SetValue(.05))
	if pricingMust(m.swap.NPV()) != before {
		t.Fatal("frozen value changed")
	}
	bondCheck(t, m.swap.Recalculate())
	if math.Abs(pricingMust(m.swap.NPV())-before) < 1 {
		t.Fatal("forced recalc did not change value")
	}
	bondCheck(t, m.swap.Unfreeze())
	for _, close := range []func() error{m.settings.Close, m.indexes[0].Close, m.indexes[1].Close, m.config.PaySchedule.Close, m.config.ReceiveSchedule.Close, m.curves[2].Close} {
		bondCheck(t, close())
	}
	bondCheck(t, m.quotes[2].SetValue(.04))
	if !isFiniteFloatFloat(pricingMust(m.swap.NPV())) {
		t.Fatal("retention")
	}
	bondCheck(t, m.swap.Close())
	if _, e := m.swap.NPV(); e == nil {
		t.Fatal("released swap accepted")
	}
}
func isFiniteFloatFloat(x float64) bool { return !math.IsNaN(x) && !math.IsInf(x, 0) }
func TestFloatFloatSessionNilAndNumericGuards(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	m := floatFloatFixture(t, s, floatFloatRows(t)[0])
	other := pricingMust(NewSession())
	defer other.Close()
	if _, e := other.NewFloatFloatSwap(m.config); !errors.Is(e, ErrSessionMismatch) {
		t.Fatal(e)
	}
	config := m.config
	config.Nominal = 0
	if _, e := s.NewFloatFloatSwap(config); e == nil {
		t.Fatal("zero nominal")
	}
	config = m.config
	config.PayIndex = nil
	if _, e := s.NewFloatFloatSwap(config); e == nil {
		t.Fatal("nil index")
	}
	if m.swap.SetEngine(nil, m.settings) == nil {
		t.Fatal("nil engine curve")
	}
	var nilSwap *FloatFloatSwap
	if _, e := nilSwap.NPV(); e == nil {
		t.Fatal("nil swap")
	}
	if nilSwap.Freeze() == nil {
		t.Fatal("nil freeze")
	}
}

func TestFloatFloatConcurrentSessionCallers(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	m := floatFloatFixture(t, s, floatFloatRows(t)[0])
	want := pricingMust(m.swap.NPV())
	failures := make(chan error, 24)
	for range 24 {
		go func() {
			value, e := m.swap.NPV()
			if e == nil && value != want {
				e = errors.New("concurrent value mismatch")
			}
			failures <- e
		}()
	}
	for range 24 {
		bondCheck(t, <-failures)
	}
}

func TestFloatFloatDirectDateSilentPolicyOracle(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	m := floatFloatFixture(t, s, floatFloatRows(t)[0])
	today := testDate(t, 17, 7, 2026)
	bondCheck(t, m.settings.SetEvaluationDate(today))
	for _, month := range []int{1, 4} {
		bondCheck(t, m.indexes[0].AddFixing(testDate(t, 17, month, 2026), .021))
	}
	bondCheck(t, m.indexes[1].AddFixing(testDate(t, 17, 1, 2026), .031))
	discount := pricingMust(s.NewFlatForward(today, .02, m.config.PayDayCounter))
	bondCheck(t, m.swap.SetEngine(discount, m.settings))
	data, e := os.ReadFile("testdata/float_float_swap_direct.json")
	bondCheck(t, e)
	var oracle struct {
		Version string `json:"version"`
		Rows    []struct {
			Reference bool      `json:"include_reference"`
			Today     *bool     `json:"include_today"`
			Values    []float64 `json:"values"`
		} `json:"rows"`
	}
	bondCheck(t, json.Unmarshal(data, &oracle))
	if oracle.Version != "1.43" {
		t.Fatal("oracle version")
	}
	methods := []func() (float64, error){m.swap.NPV, m.swap.PayLegNPV, m.swap.ReceiveLegNPV, m.swap.PayLegBPS, m.swap.ReceiveLegBPS, m.swap.FairPaySpread, m.swap.FairReceiveSpread}
	for _, row := range oracle.Rows {
		bondCheck(t, m.settings.SetIncludeReferenceDateEvents(row.Reference))
		bondCheck(t, m.settings.SetIncludeTodaysCashFlows(row.Today))
		for i, method := range methods {
			value, e := method()
			bondCheck(t, e)
			tol := 2e-9
			if i >= 5 {
				tol = 2e-13
			}
			if math.Abs(value-row.Values[i]) > tol {
				t.Fatalf("query%d %.17g want %.17g", i, value, row.Values[i])
			}
		}
	}
}
