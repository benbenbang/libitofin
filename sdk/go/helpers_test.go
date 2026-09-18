package itofin

import (
	"math"
	"testing"
)

// Independent forward-price reconstruction from test_futures_rate_helper.py.
func TestFuturesHelpersRepriceAndRetainConvexity(t *testing.T) {
	s, e := NewSession()
	s = curveMust(t, s, e)
	defer s.Close()
	dc, e := s.Actual360()
	dc = curveMust(t, dc, e)
	dc365, e := s.Actual365Fixed()
	dc365 = curveMust(t, dc365, e)
	cal, e := s.Target()
	cal = curveMust(t, cal, e)
	ref, start, end := curveDate(t, 15, 6, 2026), curveDate(t, 17, 6, 2026), curveDate(t, 16, 9, 2026)
	settings, e := s.NewSettings()
	settings = curveMust(t, settings, e)
	if e = settings.SetEvaluationDate(ref); e != nil {
		t.Fatal(e)
	}
	idx, e := s.NewEuribor(Period{3, Months}, nil, settings)
	idx = curveMust(t, idx, e)
	for mode, factory := range []func(*Session, FuturesRateHelperConfig) (*RateHelper, error){(*Session).NewFuturesRateHelper, (*Session).NewFuturesRateHelperFromEndDate, (*Session).NewFuturesRateHelperFromIndex} {
		q, e := s.NewSimpleQuote(96)
		q = curveMust(t, q, e)
		adj, e := s.NewSimpleQuote(0.001)
		adj = curveMust(t, adj, e)
		cfg := FuturesRateHelperConfig{Price: q, IborStartDate: start, IborEndDate: &end, LengthInMonths: 3, Calendar: cal, Convention: ModifiedFollowing, DayCounter: dc, ConvexityAdjustment: adj, FuturesType: Imm, Index: idx}
		h, e := factory(s, cfg)
		h = curveMust(t, h, e)
		maturity, e := h.MaturityDate()
		maturity = curveMust(t, maturity, e)
		for _, query := range []func() (Date, error){h.PillarDate, h.LatestDate, h.LatestRelevantDate} {
			d, e := query()
			if curveMust(t, d, e) != maturity {
				t.Fatal("helper date disagrees with maturity")
			}
		}
		earliest, e := h.EarliestDate()
		if curveMust(t, earliest, e) != start {
			t.Fatal("wrong start date")
		}
		v, e := h.ConvexityAdjustment()
		curveNear(t, curveMust(t, v, e), 0.001, 0)
		curve, e := s.NewPiecewiseYieldCurve(PiecewiseCurveConfig{ReferenceDate: ref, Helpers: []*RateHelper{h}, DayCounter: dc365})
		curve = curveMust(t, curve, e)
		d1, e := curve.DiscountDate(start, false)
		d1 = curveMust(t, d1, e)
		d2, e := curve.DiscountDate(maturity, false)
		d2 = curveMust(t, d2, e)
		tau := float64(maturity.Serial()-start.Serial()) / 360
		curveNear(t, 100*(1-(d1/d2-1)/tau-0.001), 96, 1e-9)
		v, e = h.ImpliedQuote()
		curveNear(t, curveMust(t, v, e), 96, 1e-9)
		if e = adj.SetValue(0.002); e != nil {
			t.Fatal(e)
		}
		if e = adj.Close(); e != nil {
			t.Fatal(e)
		}
		v, e = h.ConvexityAdjustment()
		curveNear(t, curveMust(t, v, e), 0.002, 0)
		if mode == 1 {
			cfg.IborEndDate = nil
			_, e = s.NewFuturesRateHelperFromEndDate(cfg)
			if e == nil {
				t.Fatal("released convexity handle accepted")
			}
		}
	}
	q, e := s.NewSimpleQuote(96)
	q = curveMust(t, q, e)
	if _, e = s.NewFuturesRateHelperFromEndDate(FuturesRateHelperConfig{Price: q, IborStartDate: ref, DayCounter: dc, FuturesType: Custom}); e == nil {
		t.Fatal("custom future without end date accepted")
	}
	h, e := s.NewFuturesRateHelperFromEndDate(FuturesRateHelperConfig{Price: q, IborStartDate: start, DayCounter: dc, FuturesType: Imm})
	h = curveMust(t, h, e)
	m, e := h.MaturityDate()
	if curveMust(t, m, e) != end {
		t.Fatal("default IMM maturity changed")
	}
}

// Public Python bond fixture: one cash-flow date pins the discount algebraically.
func TestBondHelperRecoversIndependentFlatDiscount(t *testing.T) {
	s, e := NewSession()
	s = curveMust(t, s, e)
	defer s.Close()
	dc, e := s.Actual360()
	dc = curveMust(t, dc, e)
	cal, e := s.Target()
	cal = curveMust(t, cal, e)
	today, end, payment := curveDate(t, 19, 6, 2026), curveDate(t, 19, 6, 2027), curveDate(t, 21, 6, 2027)
	settings, e := s.NewSettings()
	settings = curveMust(t, settings, e)
	if e = settings.SetEvaluationDate(today); e != nil {
		t.Fatal(e)
	}
	rule, term := Backward, Unadjusted
	schedule, e := s.NewSchedule(ScheduleConfig{Start: today, End: end, Frequency: Annual, Calendar: cal, Convention: Unadjusted, Rule: &rule, TerminationConvention: &term})
	schedule = curveMust(t, schedule, e)
	q, e := s.NewSimpleQuote(102.79121165871838)
	q = curveMust(t, q, e)
	h, e := s.NewFixedRateBondHelper(FixedRateBondHelperConfig{Price: q, SettlementDays: 0, FaceAmount: 250, Schedule: schedule, Coupons: []float64{0.05}, DayCounter: dc, PaymentConvention: Following, Redemption: 102, PriceType: Clean, Settings: settings, IssueDate: &today})
	h = curveMust(t, h, e)
	pillar, e := h.PillarDate()
	if curveMust(t, pillar, e) != payment {
		t.Fatal("payment convention lost")
	}
	curve, e := s.NewPiecewiseLogLinearDiscount(PiecewiseCurveConfig{ReferenceDate: today, Helpers: []*RateHelper{h}, DayCounter: dc})
	curve = curveMust(t, curve, e)
	v, e := curve.DiscountDate(payment, false)
	curveNear(t, curveMust(t, v, e), math.Exp(-0.04*float64(payment.Serial()-today.Serial())/360), 1e-12)
}

func TestFraConstructorParityAndOISQuoteRetention(t *testing.T) {
	s, e := NewSession()
	s = curveMust(t, s, e)
	defer s.Close()
	dc, e := s.Actual360()
	dc = curveMust(t, dc, e)
	cal, e := s.Target()
	cal = curveMust(t, cal, e)
	today := curveDate(t, 15, 6, 2026)
	settings, e := s.NewSettings()
	settings = curveMust(t, settings, e)
	if e = settings.SetEvaluationDate(today); e != nil {
		t.Fatal(e)
	}
	index, e := s.NewEuribor(Period{3, Months}, nil, settings)
	index = curveMust(t, index, e)
	q, e := s.NewSimpleQuote(0.04)
	q = curveMust(t, q, e)
	settlement, e := cal.Advance(today, 2, Days, Following, false)
	settlement = curveMust(t, settlement, e)
	start, e := cal.Advance(settlement, 3, Months, ModifiedFollowing, true)
	start = curveMust(t, start, e)
	end, e := cal.Advance(start, 3, Months, ModifiedFollowing, true)
	end = curveMust(t, end, e)
	cfg := DefaultFraRateHelperConfig()
	cfg.Quote = q
	cfg.Rate = 0.04
	cfg.Index = index
	cfg.PeriodToStart = Period{3, Months}
	cfg.MonthsToStart = 3
	cfg.StartDate = start
	cfg.EndDate = end
	for _, factory := range []func(*Session, FraRateHelperConfig) (*RateHelper, error){(*Session).NewFraRateHelper, (*Session).NewFraRateHelperFromRate, (*Session).NewFraRateHelperFromMonths, (*Session).NewFraRateHelperFromDates} {
		h, e := factory(s, cfg)
		h = curveMust(t, h, e)
		v, e := h.QuoteValue()
		curveNear(t, curveMust(t, v, e), 0.04, 0)
		d, e := h.EarliestDate()
		if curveMust(t, d, e) != start {
			t.Fatal("FRA start differs")
		}
		c, e := s.NewPiecewiseYieldCurve(PiecewiseCurveConfig{ReferenceDate: settlement, Helpers: []*RateHelper{h}, DayCounter: dc})
		c = curveMust(t, c, e)
		d1, e := c.DiscountDate(start, false)
		d1 = curveMust(t, d1, e)
		d2, e := c.DiscountDate(end, false)
		d2 = curveMust(t, d2, e)
		curveNear(t, (d1/d2-1)/(float64(end.Serial()-start.Serial())/360), 0.04, 1e-9)
	}
	dep, e := s.NewDepositRateHelperFromRate(0.04, index)
	dep = curveMust(t, dep, e)
	v, e := dep.QuoteValue()
	curveNear(t, curveMust(t, v, e), 0.04, 0)
	overnight, e := s.NewEstr(nil, settings)
	overnight = curveMust(t, overnight, e)
	ois := DefaultOISRateHelperConfig()
	ois.SettlementDays = 2
	ois.Tenor = Period{1, Years}
	ois.Quote = q
	ois.OvernightIndex = overnight
	ois.PaymentLag = 2
	ois.PaymentConvention = Following
	ois.PaymentFrequency = Annual
	ois.Settings = settings
	oh, e := s.NewOISRateHelper(ois)
	oh = curveMust(t, oh, e)
	v, e = oh.QuoteValue()
	curveNear(t, curveMust(t, v, e), 0.04, 0)
	if e = q.SetValue(0.05); e != nil {
		t.Fatal(e)
	}
	v, e = oh.QuoteValue()
	curveNear(t, curveMust(t, v, e), 0.05, 0)
	c, e := s.NewPiecewiseLogLinearDiscount(PiecewiseCurveConfig{ReferenceDate: today, Helpers: []*RateHelper{oh}, DayCounter: dc})
	c = curveMust(t, c, e)
	_, e = c.Discount(0.5, false)
	if e != nil {
		t.Fatal(e)
	}
	v, e = oh.ImpliedQuote()
	curveNear(t, curveMust(t, v, e), 0.05, 1e-9)
}
