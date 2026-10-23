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

type assetSwapMarket struct {
	swap     *AssetSwap
	config   AssetSwapConfig
	settings *Settings
	quotes   []*SimpleQuote
	curves   []*YieldTermStructure
}

func assetSwapRows(t *testing.T) [][]string {
	t.Helper()
	f, err := os.Open("testdata/asset_swap_native.tsv")
	bondCheck(t, err)
	defer f.Close()
	reader := csv.NewReader(f)
	reader.Comma = '\t'
	rows, err := reader.ReadAll()
	bondCheck(t, err)
	return rows[1:]
}
func assetSwapFixture(t *testing.T, s *Session, row []string) assetSwapMarket {
	t.Helper()
	n := func(i int) float64 { x, err := strconv.ParseFloat(row[i], 64); bondCheck(t, err); return x }
	settings := pricingMust(s.NewSettings())
	day, err := strconv.Atoi(row[11][8:])
	bondCheck(t, err)
	ref := testDate(t, day, 1, 2026)
	bondCheck(t, settings.SetEvaluationDate(ref))
	bondCheck(t, settings.SetIncludeReferenceDateEvents(n(10) != 0))
	cal := pricingMust(s.NullCalendar())
	dc := pricingMust(s.Actual360())
	dc365 := pricingMust(s.Actual365Fixed())
	currency := pricingMust(s.EUR())
	quotes := []*SimpleQuote{pricingMust(s.NewSimpleQuote(n(8))), pricingMust(s.NewSimpleQuote(n(9)))}
	curves := []*YieldTermStructure{pricingMust(s.NewFlatForwardFromQuote(ref, quotes[0], dc)), pricingMust(s.NewFlatForwardFromQuote(ref, quotes[1], dc365))}
	index := pricingMust(s.NewIborIndex(IborIndexConfig{FamilyName: "AssetBinding", Tenor: Period{6, Months}, Currency: currency, FixingCalendar: cal, Convention: Unadjusted, DayCounter: dc, Forwarding: curves[0], Settings: settings}))
	start, maturity := testDate(t, 5, 1, 2026), testDate(t, 5, 1, 2029)
	bondCheck(t, index.AddFixing(start, n(8)))
	schedule := pricingMust(s.NewSchedule(ScheduleConfig{Start: start, End: maturity, Frequency: Semiannual, Calendar: cal, Convention: Unadjusted}))
	bondConfig := BondConfig{FaceAmount: n(6), Calendar: cal, Settings: settings, PaymentConvention: pricingPtr(Unadjusted), Redemption: pricingPtr(n(7))}
	var bond *Bond
	if row[1] == "fixed" {
		fixed := pricingMust(s.NewSchedule(ScheduleConfig{Start: start, End: maturity, Frequency: Annual, Calendar: cal, Convention: Unadjusted}))
		b := pricingMust(s.NewFixedRateBond(FixedRateBondConfig{BondConfig: bondConfig, Schedule: fixed, Coupons: []float64{n(5)}, AccrualDayCounter: dc}))
		bond = &b.Bond
	} else {
		b := pricingMust(s.NewZeroCouponBond(ZeroCouponBondConfig{BondConfig: bondConfig, MaturityDate: maturity}))
		bond = &b.Bond
	}
	config := AssetSwapConfig{Bond: bond, Index: index, FloatingSchedule: schedule, CleanPrice: n(3), Spread: n(4), PayBondCoupon: n(2) != 0}
	swap := pricingMust(s.NewAssetSwap(config))
	bondCheck(t, swap.SetEngine(curves[1], settings))
	return assetSwapMarket{swap, config, settings, quotes, curves}
}
func TestAssetSwapNativeOracle(t *testing.T) {
	for _, row := range assetSwapRows(t) {
		t.Run(row[0], func(t *testing.T) {
			s := pricingMust(NewSession())
			defer s.Close()
			m := assetSwapFixture(t, s, row)
			if pricingMust(m.swap.IsCalculated()) || !pricingMust(m.swap.ParSwap()) || pricingMust(m.swap.IsExpired()) {
				t.Fatal("initial state")
			}
			if pricingMust(m.swap.CleanPrice()) != m.config.CleanPrice || pricingMust(m.swap.Spread()) != m.config.Spread || pricingMust(m.swap.PayBondCoupon()) != m.config.PayBondCoupon || pricingMust(m.swap.UpfrontDate()) != testDate(t, 5, 1, 2026) {
				t.Fatal("terms")
			}
			queries := []func() (float64, error){m.swap.NPV, m.swap.BondLegNPV, m.swap.FloatingLegNPV, m.swap.FloatingLegBPS, m.swap.FairSpread, m.swap.FairCleanPrice}
			for i, query := range queries {
				got := pricingMust(query())
				want, err := strconv.ParseFloat(row[12+i], 64)
				bondCheck(t, err)
				if math.Abs(got-want) > math.Max(2e-10, math.Abs(want)*2e-13) {
					t.Fatalf("query%d %.17g want %.17g", i, got, want)
				}
			}
			config := m.config
			config.Spread = pricingMust(m.swap.FairSpread())
			fair := pricingMust(s.NewAssetSwap(config))
			bondCheck(t, fair.SetEngine(m.curves[1], m.settings))
			if math.Abs(pricingMust(fair.NPV())) > 3e-9 {
				t.Fatal("fair spread")
			}
			config = m.config
			config.Spread += 1e-4
			bumped := pricingMust(s.NewAssetSwap(config))
			bondCheck(t, bumped.SetEngine(m.curves[1], m.settings))
			if math.Abs(pricingMust(bumped.NPV())-pricingMust(m.swap.NPV())-pricingMust(m.swap.FloatingLegBPS())) > 3e-9 {
				t.Fatal("signed BPS")
			}
		})
	}
}
func TestAssetSwapRetentionPolicyAndFreeze(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	rows := assetSwapRows(t)
	m := assetSwapFixture(t, s, rows[len(rows)-1])
	before := pricingMust(m.swap.NPV())
	bondCheck(t, m.settings.SetIncludeReferenceDateEvents(true))
	if math.Abs(pricingMust(m.swap.NPV())-before+2.75) > 2e-12 {
		t.Fatal("silent settings policy")
	}
	before = pricingMust(m.swap.NPV())
	bondCheck(t, m.swap.Freeze())
	bondCheck(t, m.quotes[0].SetValue(.05))
	if pricingMust(m.swap.NPV()) != before {
		t.Fatal("frozen cache")
	}
	bondCheck(t, m.swap.Recalculate())
	if math.Abs(pricingMust(m.swap.NPV())-before) < 1 {
		t.Fatal("recalculate")
	}
	bondCheck(t, m.swap.Unfreeze())
	for _, close := range []func() error{m.config.Bond.Close, m.config.Index.Close, m.config.FloatingSchedule.Close, m.curves[0].Close, m.curves[1].Close} {
		bondCheck(t, close())
	}
	bondCheck(t, m.quotes[1].SetValue(.04))
	if !isFiniteFloatFloat(pricingMust(m.swap.NPV())) {
		t.Fatal("retention")
	}
	bondCheck(t, m.swap.Close())
	if _, err := m.swap.NPV(); err == nil {
		t.Fatal("released swap")
	}
}
func TestAssetSwapSessionDomainsAndConcurrency(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	m := assetSwapFixture(t, s, assetSwapRows(t)[0])
	other := pricingMust(NewSession())
	defer other.Close()
	if _, err := other.NewAssetSwap(m.config); !errors.Is(err, ErrSessionMismatch) {
		t.Fatal(err)
	}
	otherSettings := pricingMust(s.NewSettings())
	bondCheck(t, otherSettings.SetEvaluationDate(testDate(t, 2, 1, 2026)))
	if m.swap.SetEngine(m.curves[1], otherSettings) == nil {
		t.Fatal("distinct Settings")
	}
	config := m.config
	config.CleanPrice = math.NaN()
	if _, err := s.NewAssetSwap(config); err == nil {
		t.Fatal("nonfinite price")
	}
	config = m.config
	config.Bond = nil
	if _, err := s.NewAssetSwap(config); err == nil {
		t.Fatal("nil bond")
	}
	var nilSwap *AssetSwap
	if _, err := nilSwap.NPV(); err == nil {
		t.Fatal("nil receiver")
	}
	want := pricingMust(m.swap.NPV())
	var wg sync.WaitGroup
	for range 8 {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for range 8 {
				got, err := m.swap.NPV()
				if err != nil || got != want {
					t.Errorf("serialized query: %g %v", got, err)
				}
			}
		}()
	}
	wg.Wait()
	bondCheck(t, m.swap.Calculate())
	if !pricingMust(m.swap.IsCalculated()) {
		t.Fatal("calculate")
	}
}

func TestAssetSwapGeneratedSchedule(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	rows := assetSwapRows(t)
	m := assetSwapFixture(t, s, rows[len(rows)-1])
	config := m.config
	config.FloatingSchedule = nil
	generated := pricingMust(s.NewAssetSwap(config))
	bondCheck(t, generated.SetEngine(m.curves[1], m.settings))
	if pricingMust(generated.NPV()) != pricingMust(m.swap.NPV()) || pricingMust(generated.UpfrontDate()) != pricingMust(m.swap.UpfrontDate()) {
		t.Fatal("native generated schedule")
	}
}
