package itofin

import (
	"encoding/json"
	"math"
	"os"
	"reflect"
	"testing"
)

type bmaOracleNode struct {
	Years                   int
	Fraction, Discount, NPV float64
	Pillar                  int32
	FairFraction            float64 `json:"fair_fraction"`
	FairSpread              float64 `json:"fair_spread"`
	LiborNPV                float64 `json:"libor_npv"`
	BMANPV                  float64 `json:"bma_npv"`
	LiborBPS                float64 `json:"libor_bps"`
	BMABPS                  float64 `json:"bma_bps"`
}
type bmaOracle struct {
	Quantlib string

	Today, Settlement int32
	Nodes             []bmaOracleNode
	Coupon            struct {
		Rate, Amount float64
		FixingDates  []int32 `json:"fixing_dates"`
	}
	HolidayFixings []int32 `json:"holiday_fixings"`
}

func bmaNear(t *testing.T, got, want float64) {
	t.Helper()
	if math.IsNaN(got) || math.IsInf(got, 0) || math.IsNaN(want) || math.IsInf(want, 0) || math.Abs(got-want) > 1e-9 {
		t.Fatalf("got %.17g want %.17g", got, want)
	}
}

type bmaMarket struct {
	s           *Session
	settings    *Settings
	today, spot Date
	dc, bdc     *DayCounter
	cal         *Calendar
	risk        *YieldTermStructure
	libor       *IborIndex
	bma, index  *BMAIndex
	quotes      []*SimpleQuote
	curve       *YieldTermStructure
	helpers     []*RateHelper
	oracle      bmaOracle
	owned       []object
}

func newBMAMarket(t *testing.T, history bool) *bmaMarket {
	t.Helper()
	m := &bmaMarket{s: sessionMust(NewSession())}
	t.Cleanup(func() { _ = m.s.Close() })
	bytes, err := os.ReadFile("testdata/bma.json")
	pricingOK(t, err)
	pricingOK(t, json.Unmarshal(bytes, &m.oracle))
	if m.oracle.Quantlib != "1.43" || len(m.oracle.Nodes) != 10 {
		t.Fatal("BMA oracle must contain all ten QuantLib 1.43 tenors")
	}
	m.today = curveDate(t, 23, 10, 2025)
	m.spot = curveDate(t, 27, 10, 2025)
	m.settings = sessionMust(m.s.NewSettings())
	pricingOK(t, m.settings.SetEvaluationDate(m.today))
	m.dc = sessionMust(m.s.Actual360())
	m.bdc = sessionMust(m.s.ActualActualISDA())
	m.cal = sessionMust(m.s.UnitedStates("GovernmentBond"))
	m.risk = sessionMust(m.s.NewFlatForward(m.spot, .04, m.dc))
	m.libor = sessionMust(m.s.NewUsdLibor(Period{3, Months}, m.risk, m.settings))
	m.bma = sessionMust(m.s.NewBMAIndex(nil, m.settings))
	if history {
		pricingOK(t, m.bma.AddFixing(curveDate(t, 22, 10, 2025), .03))
	}
	m.owned = []object{m.settings.object, m.dc.object, m.bdc.object, m.cal.object, m.risk.object, m.libor.object, m.bma.object}
	for _, n := range m.oracle.Nodes {
		q := sessionMust(m.s.NewSimpleQuote(n.Fraction))
		m.quotes = append(m.quotes, q)
		m.owned = append(m.owned, q.object)
	}
	m.curve, m.helpers = m.build(t)
	m.index = sessionMust(m.s.NewBMAIndex(m.curve, m.settings))
	m.owned = append(m.owned, m.index.object)
	return m
}
func (m *bmaMarket) build(t *testing.T) (*YieldTermStructure, []*RateHelper) {
	var helpers []*RateHelper
	for i, n := range m.oracle.Nodes {
		h := sessionMust(m.s.NewBMASwapRateHelper(BMASwapRateHelperConfig{Quote: m.quotes[i], Tenor: Period{int32(n.Years), Years}, SettlementDays: 2, Calendar: m.cal, BMAPeriod: Period{3, Months}, BMAConvention: Following, BMADayCounter: m.bdc, BMAIndex: m.bma, LiborIndex: m.libor}))
		helpers = append(helpers, h)
		m.owned = append(m.owned, h.object)
	}
	c := sessionMust(m.s.NewPiecewiseLogLinearDiscount(PiecewiseCurveConfig{ReferenceDate: m.today, Helpers: helpers, DayCounter: m.dc}))
	m.owned = append(m.owned, c.object)
	return c, helpers
}
func (m *bmaMarket) swap(t *testing.T, years int, index *BMAIndex) *BMASwap {
	end := curveDate(t, 27, 10, 2025+years)
	lc := sessionMust(m.libor.FixingCalendar())
	liborDates := sessionMust(m.s.NewSchedule(ScheduleConfig{Start: m.spot, End: end, Frequency: Quarterly, Calendar: lc, Convention: ModifiedFollowing}))
	bmaDates := sessionMust(m.s.NewSchedule(ScheduleConfig{Start: m.spot, End: end, Frequency: Quarterly, Calendar: m.cal, Convention: Following}))
	m.owned = append(m.owned, lc.object, liborDates.object, bmaDates.object)
	b := sessionMust(m.s.NewBMASwap(BMASwapConfig{Type: SwapPayer, Nominal: 100, LiborFraction: .75, LiborSchedule: liborDates, BMASchedule: bmaDates, LiborIndex: m.libor, BMAIndex: index, LiborDayCounter: m.dc, BMADayCounter: m.bdc, Settings: m.settings}))
	pricingOK(t, b.SetEngine(m.risk, m.settings))
	return b
}
func (m *bmaMarket) coupon(t *testing.T) *AverageBMACoupon {
	gear, spread := 1.2, .001
	return sessionMust(m.s.NewAverageBMACoupon(AverageBMACouponConfig{PaymentDate: curveDate(t, 27, 1, 2026), StartDate: m.spot, EndDate: curveDate(t, 27, 1, 2026), Nominal: 100, Index: m.index, DayCounter: m.dc, Gearing: &gear, Spread: &spread}))
}
func bmaSerials(dates []Date) []int32 {
	result := make([]int32, len(dates))
	for i, d := range dates {
		result[i] = d.Serial()
	}
	return result
}

func TestBMAQuantLibCurveSwapCouponAndHolidayOracles(t *testing.T) {
	m := newBMAMarket(t, true)
	dates, data, err := m.curve.Nodes()
	pricingOK(t, err)
	if m.today.Serial() != m.oracle.Today || m.spot.Serial() != m.oracle.Settlement {
		t.Fatal("oracle dates")
	}
	for i, n := range m.oracle.Nodes {
		if dates[i+1].Serial() != n.Pillar {
			t.Fatalf("pillar %d", i)
		}
		bmaNear(t, data[i+1], n.Discount)
		bmaNear(t, sessionMust(m.helpers[i].ImpliedQuote()), n.Fraction)
		b := m.swap(t, n.Years, m.index)
		if sessionMust(b.IsCalculated()) {
			t.Fatal("new swap warm")
		}
		bmaNear(t, sessionMust(b.NPV()), n.NPV)
		if !sessionMust(b.IsCalculated()) {
			t.Fatal("calculation not cached")
		}
		bmaNear(t, sessionMust(b.FairLiborFraction()), n.FairFraction)
		bmaNear(t, sessionMust(b.FairLiborSpread()), n.FairSpread)
		bmaNear(t, sessionMust(b.LegNPV(0)), n.LiborNPV)
		bmaNear(t, sessionMust(b.LegNPV(1)), n.BMANPV)
		bmaNear(t, sessionMust(b.LegBPS(0)), n.LiborBPS)
		bmaNear(t, sessionMust(b.LegBPS(1)), n.BMABPS)
		if _, err := b.LegNPV(2); err == nil {
			t.Fatal("invalid leg accepted")
		}
		if _, err := b.LegBPS(-1); err == nil {
			t.Fatal("negative leg accepted")
		}
		bmaNear(t, sessionMust(b.NPV()), n.NPV)
		pricingOK(t, b.Close())
	}
	coupon := m.coupon(t)
	bmaNear(t, sessionMust(coupon.Rate()), m.oracle.Coupon.Rate)
	bmaNear(t, sessionMust(coupon.Amount()), m.oracle.Coupon.Amount)
	bmaNear(t, sessionMust(coupon.AccrualPeriod()), 92.0/360)
	if !reflect.DeepEqual(bmaSerials(sessionMust(coupon.FixingDates())), m.oracle.Coupon.FixingDates) {
		t.Fatal("coupon dates")
	}
	if !reflect.DeepEqual(bmaSerials(sessionMust(m.index.FixingSchedule(curveDate(t, 20, 12, 2024), curveDate(t, 8, 1, 2025)))), m.oracle.HolidayFixings) {
		t.Fatal("holiday schedule")
	}
	if !sessionMust(m.index.IsValidFixingDate(curveDate(t, 26, 12, 2024))) || sessionMust(m.index.IsValidFixingDate(curveDate(t, 25, 12, 2024))) {
		t.Fatal("holiday fixing validity")
	}
	value := sessionMust(m.index.ValueDate(curveDate(t, 22, 10, 2025)))
	if value != m.today || sessionMust(m.index.MaturityDate(value)) != curveDate(t, 30, 10, 2025) {
		t.Fatal("weekly date conventions")
	}
	cal := sessionMust(m.index.FixingCalendar())
	if sessionMust(cal.IsBusinessDay(curveDate(t, 25, 12, 2024))) {
		t.Fatal("wrong calendar")
	}
	bmaNear(t, sessionMust(m.index.Fixing(curveDate(t, 22, 10, 2025), false)), .03)
	forward := sessionMust(m.index.Fixing(curveDate(t, 29, 10, 2025), false))
	if math.IsNaN(forward) || math.IsInf(forward, 0) {
		t.Fatal("nonfinite forecast")
	}
	if !sessionMust(m.index.HasHistoricalFixing(curveDate(t, 22, 10, 2025))) {
		t.Fatal("missing history")
	}
	v, found, err := m.index.PastFixing(curveDate(t, 22, 10, 2025))
	pricingOK(t, err)
	if !found {
		t.Fatal("past fixing absent")
	}
	bmaNear(t, v, .03)
	_, found, err = m.index.PastFixing(curveDate(t, 29, 10, 2025))
	pricingOK(t, err)
	if found {
		t.Fatal("future history invented")
	}
}

func TestBMAUpdatesHistoryRecoveryAndColdRetention(t *testing.T) {
	m := newBMAMarket(t, false)
	b, coupon := m.swap(t, 5, m.index), m.coupon(t)
	if _, err := coupon.Rate(); err == nil {
		t.Fatal("missing history accepted")
	}
	pricingOK(t, m.bma.AddFixing(curveDate(t, 22, 10, 2025), .03))
	bmaNear(t, sessionMust(coupon.Rate()), m.oracle.Coupon.Rate)
	before := sessionMust(b.NPV())
	pricingOK(t, m.quotes[4].SetValue(.69))
	if sessionMust(b.IsCalculated()) {
		t.Fatal("quote did not invalidate swap")
	}
	if math.Abs(sessionMust(b.NPV())-before) < 1e-6 {
		t.Fatal("quote did not change price")
	}
	fresh, _ := m.build(t)
	index := sessionMust(m.s.NewBMAIndex(fresh, m.settings))
	m.owned = append(m.owned, index.object)
	twin := m.swap(t, 5, index)
	bmaNear(t, sessionMust(b.NPV()), sessionMust(twin.NPV()))
	pricingOK(t, m.index.ClearFixings())
	if sessionMust(m.bma.HasHistoricalFixing(curveDate(t, 22, 10, 2025))) {
		t.Fatal("history retained after clearing")
	}
	if _, err := coupon.Amount(); err == nil {
		t.Fatal("clear history did not invalidate coupon")
	}
	pricingOK(t, m.bma.AddFixing(curveDate(t, 22, 10, 2025), .031))
	recovered := sessionMust(coupon.Amount())
	if math.IsNaN(recovered) || math.IsInf(recovered, 0) {
		t.Fatal("failed to recover")
	}
	for _, bad := range []float64{math.NaN(), math.Inf(1)} {
		if err := m.index.AddFixing(curveDate(t, 29, 10, 2025), bad); err == nil {
			t.Fatal("nonfinite fixing accepted")
		}
	}
	if err := m.index.AddFixing(curveDate(t, 24, 10, 2025), .03); err == nil {
		t.Fatal("invalid fixing day")
	}
	if _, err := m.index.FixingSchedule(curveDate(t, 8, 1, 2025), curveDate(t, 20, 12, 2024)); err == nil {
		t.Fatal("inverted range")
	}
	cold := m.swap(t, 10, m.index)
	expected := sessionMust(m.swap(t, 10, m.index).NPV())
	for _, o := range m.owned {
		pricingOK(t, o.Close())
	}
	if sessionMust(cold.IsCalculated()) {
		t.Fatal("cold retained swap already calculated")
	}
	bmaNear(t, sessionMust(cold.NPV()), expected)
	if _, err := m.index.Fixing(m.today, false); err == nil {
		t.Fatal("closed index accepted")
	}
	pricingOK(t, cold.Close())
	if _, err := cold.NPV(); err == nil {
		t.Fatal("closed swap accepted")
	}
}

func TestBMANilForeignSessionAndInvalidConstructors(t *testing.T) {
	m := newBMAMarket(t, true)
	other := sessionMust(NewSession())
	defer other.Close()
	foreign := sessionMust(other.NewSettings())
	if _, err := m.s.NewBMAIndex(nil, foreign); err == nil {
		t.Fatal("foreign settings accepted")
	}
	if _, err := m.s.NewBMAIndex(nil, nil); err == nil {
		t.Fatal("nil settings accepted")
	}
	if _, err := m.s.NewBMASwap(BMASwapConfig{}); err == nil {
		t.Fatal("nil swap inputs accepted")
	}
	if _, err := m.s.NewBMASwapRateHelper(BMASwapRateHelperConfig{}); err == nil {
		t.Fatal("nil helper inputs accepted")
	}
	if _, err := m.s.NewAverageBMACoupon(AverageBMACouponConfig{}); err == nil {
		t.Fatal("nil coupon inputs accepted")
	}
	if _, err := m.s.NewAverageBMACoupon(AverageBMACouponConfig{PaymentDate: m.spot, StartDate: m.spot, EndDate: m.spot, Nominal: 100, Index: m.index, DayCounter: m.dc}); err == nil {
		t.Fatal("empty coupon accepted")
	}
	var index *BMAIndex
	var coupon *AverageBMACoupon
	var swap *BMASwap
	if _, err := index.Fixing(m.today, false); err == nil {
		t.Fatal("nil index")
	}
	if _, err := coupon.Rate(); err == nil {
		t.Fatal("nil coupon")
	}
	if _, err := swap.NPV(); err == nil {
		t.Fatal("nil swap")
	}
	bmaNear(t, sessionMust(m.coupon(t).Rate()), m.oracle.Coupon.Rate)
}
