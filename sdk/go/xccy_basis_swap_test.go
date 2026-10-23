package itofin

import (
	"encoding/csv"
	"errors"
	"math"
	"os"
	"strconv"
	"sync"
	"testing"
)

func xccyRows(t *testing.T) [][]string {
	t.Helper()
	f, e := os.Open("testdata/xccy_basis_native.csv")
	bondCheck(t, e)
	defer f.Close()
	rows, e := csv.NewReader(f).ReadAll()
	bondCheck(t, e)
	return rows[1:]
}
func xccyFixture(t *testing.T, s *Session, row []string) (XccyBasisSwapConfig, []*SimpleQuote) {
	t.Helper()
	n := func(i int) float64 { x, e := strconv.ParseFloat(row[i], 64); bondCheck(t, e); return x }
	d := func(serial float64) Date { return pricingMust(DateFromSerial(int32(serial))) }
	settings := pricingMust(s.NewSettings())
	bondCheck(t, settings.SetEvaluationDate(d(n(2))))
	if n(13) >= 0 {
		bondCheck(t, settings.SetIncludeTodaysCashFlows(pricingPtr(n(13) != 0)))
	}
	dc := pricingMust(s.Actual360())
	cal := pricingMust(s.NullCalendar())
	base := pricingMust(s.EUR())
	quote := pricingMust(s.USD())
	ccy := []*Currency{base, quote}
	quotes := []*SimpleQuote{}
	curves := []*YieldTermStructure{}
	for i := 9; i <= 12; i++ {
		q := pricingMust(s.NewSimpleQuote(n(i)))
		quotes = append(quotes, q)
		curves = append(curves, pricingMust(s.NewFlatForwardFromQuote(d(46027), q, dc)))
	}
	indices := []*IborIndex{}
	for i := 0; i < 2; i++ {
		indices = append(indices, pricingMust(s.NewIborIndex(IborIndexConfig{FamilyName: "xccy-" + strconv.Itoa(i), Tenor: Period{3, Months}, Currency: ccy[i], FixingCalendar: cal, Convention: Unadjusted, DayCounter: dc, Forwarding: curves[i], Settings: settings})))
	}
	start, end := testDate(t, 8, 1, 2026), testDate(t, 8, 1, 2027)
	qs, qe := start, end
	quoteCal := cal
	if n(1) == 1 {
		qs, qe = testDate(t, 8, 2, 2026), testDate(t, 8, 5, 2027)
	} else if n(1) == 2 {
		qs, qe = testDate(t, 17, 1, 2026), testDate(t, 17, 1, 2027)
		quoteCal = pricingMust(s.WeekendsOnly())
	}
	bs := pricingMust(s.NewSchedule(ScheduleConfig{Start: start, End: end, Frequency: Quarterly, Calendar: cal, Convention: Unadjusted}))
	qsch := pricingMust(s.NewSchedule(ScheduleConfig{Start: qs, End: qe, Frequency: Quarterly, Calendar: quoteCal, Convention: Unadjusted}))
	for i, schedule := range []*Schedule{bs, qsch} {
		dates := pricingMust(schedule.Dates())
		for _, date := range dates[:len(dates)-1] {
			if date.Serial() < int32(n(2)) {
				fixing := .022
				if i == 1 {
					fixing = .034
				}
				bondCheck(t, indices[i].AddFixing(date, fixing))
			}
		}
	}
	return XccyBasisSwapConfig{BaseSchedule: bs, QuoteSchedule: qsch, BaseIndex: indices[0], QuoteIndex: indices[1], BaseCurve: curves[2], QuoteCurve: curves[3], BaseDayCounter: dc, QuoteDayCounter: dc, BaseCurrency: base, QuoteCurrency: quote, BaseNotional: 1e6, QuoteNotional: n(7), QuoteSpread: n(8), Spot: 1.27, Settings: settings, Valuation: &XccyBasisValuation{d(n(3)), d(n(4)), n(6) != 0}, SpotSettlementDate: pricingPtr(d(n(5)))}, quotes
}
func xccyNear(got, want, tolerance float64) bool {
	return !math.IsNaN(got) && !math.IsInf(got, 0) && !math.IsNaN(want) && !math.IsInf(want, 0) && math.Abs(got-want) <= tolerance
}
func TestXccyBasisFiniteComparisons(t *testing.T) {
	for _, bad := range []float64{math.NaN(), math.Inf(1), math.Inf(-1)} {
		if xccyNear(bad, 0, 2e-9) || xccyNear(0, bad, 2e-9) {
			t.Fatal("nonfinite oracle comparison accepted")
		}
	}
	if !xccyNear(1, 1, 2e-13) || xccyNear(1, 2, 2e-9) {
		t.Fatal("finite oracle comparison lost its numerical assertion")
	}
}
func TestXccyBasisNativeOracle(t *testing.T) {
	for _, row := range xccyRows(t) {
		t.Run(row[0], func(t *testing.T) {
			s := pricingMust(NewSession())
			defer s.Close()
			cfg, _ := xccyFixture(t, s, row)
			swap := pricingMust(s.NewXccyBasisSwap(cfg))
			defer swap.Close()
			methods := []func() (float64, error){swap.BaseLegPV, swap.QuoteLegPV, swap.QuoteLegBPS, swap.NPV, swap.NPVInQuote, swap.FairQuoteBasisSpread}
			for i, method := range methods {
				got, e := method()
				if row[0] == "expired" && i == 5 {
					if e == nil {
						t.Fatal("expired fair spread")
					}
					continue
				}
				bondCheck(t, e)
				want, e := strconv.ParseFloat(row[14+i], 64)
				bondCheck(t, e)
				tol := 2e-9
				if i == 5 {
					tol = 2e-13
				}
				if !xccyNear(got, want, tol) {
					t.Fatalf("query%d %.17g != %.17g", i, got, want)
				}
			}
			if row[0] != "expired" {
				cfg.QuoteSpread = pricingMust(swap.FairQuoteBasisSpread())
				fair := pricingMust(s.NewXccyBasisSwap(cfg))
				defer fair.Close()
				if !xccyNear(pricingMust(fair.NPVInQuote()), 0, 2e-9) {
					t.Fatal("fair spread not absolute")
				}
			}
			if pricingMust(swap.Spot()) != 1.27 {
				t.Fatal("spot")
			}
			bc := pricingMust(swap.BaseCurrency())
			qc := pricingMust(swap.QuoteCurrency())
			defer bc.Close()
			defer qc.Close()
			if pricingMust(bc.Code()) != "EUR" || pricingMust(qc.Code()) != "USD" {
				t.Fatal("currency")
			}
		})
	}
}
func TestXccyBasisLiveRetentionAndSessions(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	cfg, quotes := xccyFixture(t, s, xccyRows(t)[0])
	swap := pricingMust(s.NewXccyBasisSwap(cfg))
	before := pricingMust(swap.NPVInQuote())
	for _, v := range []interface{ Close() error }{cfg.BaseSchedule, cfg.QuoteSchedule, cfg.BaseIndex, cfg.QuoteIndex, cfg.BaseCurve, cfg.QuoteCurve, cfg.BaseDayCounter, cfg.BaseCurrency, cfg.QuoteCurrency, cfg.Settings} {
		bondCheck(t, v.Close())
	}
	for i, q := range quotes {
		value := []float64{.052, .075, .05, .06}[i]
		bondCheck(t, q.SetValue(value))
	}
	want, parseErr := strconv.ParseFloat(xccyRows(t)[7][18], 64)
	bondCheck(t, parseErr)
	original, parseErr := strconv.ParseFloat(xccyRows(t)[0][18], 64)
	bondCheck(t, parseErr)
	if !xccyNear(before, original, 2e-9) || !xccyNear(pricingMust(swap.NPVInQuote()), want, 2e-9) || before == want {
		t.Fatal("retained live inputs")
	}
	var wg sync.WaitGroup
	for i := 0; i < 4; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			got, e := swap.NPVInQuote()
			if e != nil || !xccyNear(got, want, 2e-9) {
				t.Errorf("concurrent value %.17g, expected %.17g, error %v", got, want, e)
			}
		}()
	}
	wg.Wait()
	copy := pricingMust(swap.BaseCurrency())
	bondCheck(t, swap.Close())
	if pricingMust(copy.Code()) != "EUR" {
		t.Fatal("copied currency lifetime")
	}
	bondCheck(t, copy.Close())
	if _, e := swap.NPV(); e == nil {
		t.Fatal("closed swap")
	}
	var nilSwap *XccyBasisSwap
	if _, e := nilSwap.NPV(); e == nil {
		t.Fatal("nil swap")
	}
	fresh, _ := xccyFixture(t, s, xccyRows(t)[0])
	other := pricingMust(NewSession())
	defer other.Close()
	fresh.BaseCurrency = pricingMust(other.EUR())
	if _, e := s.NewXccyBasisSwap(fresh); e == nil {
		t.Fatal("foreign currency")
	}
	bondCheck(t, s.Close())
	if _, e := s.NewXccyBasisSwap(fresh); !errors.Is(e, ErrClosed) {
		t.Fatal(e)
	}
}
func TestXccyBasisDomainsAndLegSelectiveFixings(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	a, _ := xccyFixture(t, s, xccyRows(t)[0])
	a.Valuation = nil
	swap := pricingMust(s.NewXccyBasisSwap(a))
	bondCheck(t, a.BaseIndex.AddFixing(testDate(t, 8, 1, 2026), .022))
	bondCheck(t, a.Settings.SetEvaluationDate(testDate(t, 9, 1, 2026)))
	if _, e := swap.BaseLegPV(); e != nil {
		t.Fatal(e)
	}
	if _, e := swap.QuoteLegPV(); e == nil {
		t.Fatal("missing quote fixing")
	}
	if _, e := swap.QuoteLegBPS(); e == nil {
		t.Fatal("core npvbps needs fixing")
	}
	if pricingMust(swap.Spot()) != 1.27 {
		t.Fatal("spot queried fixings")
	}
	mutations := []func(*XccyBasisSwapConfig){func(c *XccyBasisSwapConfig) { c.Spot = math.NaN() }, func(c *XccyBasisSwapConfig) { c.BaseNotional = 0 }, func(c *XccyBasisSwapConfig) { c.QuoteNotional = math.Inf(1) }, func(c *XccyBasisSwapConfig) { c.QuoteSpread = math.NaN() }, func(c *XccyBasisSwapConfig) { c.QuoteCurrency = c.BaseCurrency }, func(c *XccyBasisSwapConfig) { c.Settings = pricingMust(s.NewSettings()) }, func(c *XccyBasisSwapConfig) { c.BaseIndex = nil }}
	for _, mutate := range mutations {
		c := a
		mutate(&c)
		if _, e := s.NewXccyBasisSwap(c); e == nil {
			t.Fatal("invalid constructor accepted")
		}
	}
	wrong := a
	wrong.QuoteCurve = pricingMust(s.NewFlatForward(testDate(t, 6, 1, 2026), .04, a.QuoteDayCounter))
	bad := pricingMust(s.NewXccyBasisSwap(wrong))
	if _, e := bad.NPV(); e == nil {
		t.Fatal("mismatched references")
	}
}
