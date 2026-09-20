package itofin

import (
	"math"
	"testing"
)

func jointFinite(t *testing.T, values ...float64) {
	t.Helper()
	for _, value := range values {
		if math.IsNaN(value) || math.IsInf(value, 0) {
			t.Fatalf("nonfinite joint result %g", value)
		}
	}
}
func jointNear(t *testing.T, got, want, tolerance float64) {
	t.Helper()
	jointFinite(t, got, want)
	if math.Abs(got-want) > tolerance {
		t.Fatalf("joint result %.17g != %.17g", got, want)
	}
}

type jointMarket struct {
	s                          *Session
	settings                   *Settings
	today                      Date
	cal                        *Calendar
	dc, fixedDC                *DayCounter
	rate, basis, discountQuote *SimpleQuote
	discount                   *YieldTermStructure
	base, other                *IborIndex
	basisHelpers               []*RateHelper
	basisSettlementDays        uint32
	omitSixMonth               bool
}

func newJointMarket(t *testing.T) *jointMarket {
	t.Helper()
	m := &jointMarket{s: sessionMust(NewSession()), today: curveDate(t, 23, 10, 2025), basisSettlementDays: 2}
	t.Cleanup(func() { _ = m.s.Close() })
	m.settings = sessionMust(m.s.NewSettings())
	pricingOK(t, m.settings.SetEvaluationDate(m.today))
	m.cal = sessionMust(m.s.Target())
	m.dc = sessionMust(m.s.Actual360())
	m.fixedDC = sessionMust(m.s.Thirty360BondBasis())
	m.rate = sessionMust(m.s.NewSimpleQuote(.03))
	m.basis = sessionMust(m.s.NewSimpleQuote(.002))
	m.discountQuote = sessionMust(m.s.NewSimpleQuote(.02))
	spot := sessionMust(m.cal.Advance(m.today, 2, Days, Following, false))
	m.discount = sessionMust(m.s.NewFlatForwardFromQuote(spot, m.discountQuote, m.dc))
	m.base = sessionMust(m.s.NewEuribor(Period{3, Months}, nil, m.settings))
	m.other = sessionMust(m.s.NewEuribor(Period{6, Months}, nil, m.settings))
	for i := int32(2); i <= 10; i++ {
		m.basisHelpers = append(m.basisHelpers, sessionMust(m.s.NewIborIborBasisSwapRateHelper(m.basisConfig(Period{i, Years}, true))))
	}
	for i := int32(1); i <= 3; i++ {
		m.basisHelpers = append(m.basisHelpers, sessionMust(m.s.NewIborIborBasisSwapRateHelper(m.basisConfig(Period{i * 6, Months}, false))))
	}
	return m
}

func (m *jointMarket) basisConfig(tenor Period, side bool) BasisHelperConfig {
	return BasisHelperConfig{Quote: m.basis, Tenor: tenor, SettlementDays: m.basisSettlementDays, Calendar: m.cal, Convention: ModifiedFollowing, BaseIndex: m.base, OtherIndex: m.other, DiscountCurve: m.discount, BootstrapBaseCurve: side}
}

func (m *jointMarket) config() JointYieldCurvesConfig {
	cfg := JointYieldCurvesConfig{ReferenceDate: sessionMust(NewDate(23, 10, 2025)), BasisHelpers: m.basisHelpers, DayCounter: m.dc, Accuracy: 1e-10}
	for i := uint32(1); i <= 9; i++ {
		cfg.FirstHelpers = append(cfg.FirstHelpers, sessionMust(m.s.NewFraRateHelperFromMonths(FraRateHelperConfig{Quote: m.rate, MonthsToStart: i, Index: m.base, UseIndexedCoupon: true, Pillar: LastRelevantDate})))
	}
	for i := int32(2); i <= 10; i++ {
		cfg.SecondHelpers = append(cfg.SecondHelpers, sessionMust(m.s.NewSwapRateHelperWithDiscount(SwapRateHelperConfig{Quote: m.rate, Tenor: Period{i, Years}, Calendar: m.cal, FixedFrequency: Annual, FixedConvention: Following, FixedDayCount: m.fixedDC, IborIndex: m.other}, m.discount)))
	}
	return cfg
}

func (m *jointMarket) curves(t *testing.T) (*JointYieldCurves, [2]*YieldTermStructure) {
	t.Helper()
	cfg := m.config()
	joint := sessionMust(m.s.NewJointYieldCurves(cfg))
	for _, helper := range append(cfg.FirstHelpers, cfg.SecondHelpers...) {
		pricingOK(t, helper.Close())
	}
	return joint, [2]*YieldTermStructure{sessionMust(joint.Curve(0)), sessionMust(joint.Curve(1))}
}

func (m *jointMarket) legNPV(t *testing.T, index *IborIndex, start, end Date, frequency Frequency, spread float64) float64 {
	t.Helper()
	schedule := sessionMust(m.s.NewSchedule(ScheduleConfig{Start: start, End: end, Frequency: frequency, Calendar: m.cal, Convention: ModifiedFollowing}))
	defer schedule.Close()
	builder := sessionMust(m.s.NewIborLeg(schedule, index))
	defer builder.Close()
	notional := sessionMust(builder.WithNotional(1))
	defer notional.Close()
	leg := sessionMust(notional.Build())
	defer leg.Close()
	reference := sessionMust(m.discount.ReferenceDate())
	npv := sessionMust(leg.NPV(CashFlowsNPVConfig{Discount: m.discount, Settings: m.settings, NPVDate: &reference}))
	dates := sessionMust(schedule.Dates())
	for i := 1; i < len(dates); i++ {
		npv += spread * sessionMust(m.dc.YearFraction(dates[i-1], dates[i])) * sessionMust(m.discount.DiscountDate(dates[i], false))
	}
	return npv
}

func (m *jointMarket) reprice(t *testing.T, curves [2]*YieldTermStructure) {
	t.Helper()
	base := sessionMust(m.s.NewEuribor(Period{3, Months}, curves[0], m.settings))
	other := sessionMust(m.s.NewEuribor(Period{6, Months}, curves[1], m.settings))
	defer base.Close()
	defer other.Close()
	today := m.today
	spot := sessionMust(m.cal.Advance(today, 2, Days, Following, false))
	rate, basis := sessionMust(m.rate.Value()), sessionMust(m.basis.Value())
	for i := int32(1); i <= 9; i++ {
		start := sessionMust(m.cal.Advance(spot, i, Months, ModifiedFollowing, false))
		fra := sessionMust(m.s.NewForwardRateAgreement(FRAConfig{Index: base, ValueDate: start, Position: PositionLong, Strike: rate, Notional: 1, Discount: curves[0]}))
		got := sessionMust(fra.ForwardRate())
		pricingOK(t, fra.Close())
		jointFinite(t, got, rate)
		if math.Abs(got-rate) > math.Abs(rate)*1e-12 {
			t.Fatalf("FRA %d forward %.17g != %.17g", i, got, rate)
		}
	}
	for _, tenor := range []Period{{6, Months}, {12, Months}, {18, Months}, {2, Years}, {3, Years}, {4, Years}, {5, Years}, {6, Years}, {7, Years}, {8, Years}, {9, Years}, {10, Years}} {
		if m.omitSixMonth && tenor == (Period{6, Months}) {
			continue
		}
		basisSpot := sessionMust(m.cal.Advance(today, int32(m.basisSettlementDays), Days, Following, false))
		end := sessionMust(m.cal.Advance(basisSpot, tenor.Length, tenor.Unit, ModifiedFollowing, false))
		npv := m.legNPV(t, base, basisSpot, end, Quarterly, basis) - m.legNPV(t, other, basisSpot, end, Semiannual, 0)
		jointFinite(t, npv)
		if math.Abs(npv) > 1e-10 {
			t.Fatalf("basis %v NPV %.17g", tenor, npv)
		}
	}
	for i := int32(2); i <= 10; i++ {
		end := sessionMust(m.cal.Advance(spot, i, Years, ModifiedFollowing, false))
		fixed := sessionMust(m.s.NewSchedule(ScheduleConfig{Start: spot, End: end, Frequency: Annual, Calendar: m.cal, Convention: Following}))
		floating := sessionMust(m.s.NewSchedule(ScheduleConfig{Start: spot, End: end, Frequency: Semiannual, Calendar: m.cal, Convention: ModifiedFollowing}))
		swap := sessionMust(m.s.NewVanillaSwap(VanillaSwapConfig{Type: SwapPayer, Nominal: 1, FixedRate: rate, FixedSchedule: fixed, FloatingSchedule: floating, FixedDayCounter: m.fixedDC, FloatingDayCounter: m.dc, Index: other, Settings: m.settings}))
		pricingOK(t, swap.SetEngine(m.discount, m.settings))
		npv := sessionMust(swap.NPV())
		pricingOK(t, swap.Close())
		pricingOK(t, fixed.Close())
		pricingOK(t, floating.Close())
		jointFinite(t, npv)
		if math.Abs(npv) > 1e-10 {
			t.Fatalf("vanilla %dy NPV %.17g", i, npv)
		}
	}
}
