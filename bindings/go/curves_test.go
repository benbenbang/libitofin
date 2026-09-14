package itofin

import (
	"math"
	"sync"
	"testing"
)

func curveMust[T any](t *testing.T, v T, e error) T {
	t.Helper()
	if e != nil {
		t.Fatal(e)
	}
	return v
}
func curveNear(t *testing.T, got, want, tol float64) {
	t.Helper()
	if math.IsNaN(got) || math.Abs(got-want) > tol {
		t.Fatalf("got %.17g want %.17g tolerance %g", got, want, tol)
	}
}
func curveDate(t *testing.T, day, month, year int) Date {
	t.Helper()
	v, e := NewDate(day, month, year)
	return curveMust(t, v, e)
}

// Public Python test_market.py oracle: risk-free/dividend order must remain distinct.
func TestMarketArgumentOrderAndLifetime(t *testing.T) {
	s, e := NewSession()
	s = curveMust(t, s, e)
	defer s.Close()
	dc, e := s.Actual360()
	dc = curveMust(t, dc, e)
	p, e := s.NewBlackScholesProcess(BlackScholesConfig{Spot: 60, RiskFreeRate: 0.08, DividendYield: 0.02, Volatility: 0.30, ReferenceDate: curveDate(t, 15, 6, 2026), DayCounter: dc})
	p = curveMust(t, p, e)
	if e = dc.Close(); e != nil {
		t.Fatal(e)
	}
	r, e := p.RiskFreeRate()
	curveNear(t, curveMust(t, r, e), 0.08, 1e-10)
	q, e := p.DividendYield()
	curveNear(t, curveMust(t, q, e), 0.02, 1e-10)
	quote, e := s.NewSimpleQuote(60)
	quote = curveMust(t, quote, e)
	if e = quote.SetValue(61); e != nil {
		t.Fatal(e)
	}
	v, e := quote.Value()
	curveNear(t, curveMust(t, v, e), 61, 0)
	if e = quote.Close(); e != nil {
		t.Fatal(e)
	}
	if _, e = quote.Value(); e == nil {
		t.Fatal("released quote accepted")
	}
}
func TestCurveNodeQueriesAndConcurrentCallers(t *testing.T) {
	s, e := NewSession()
	s = curveMust(t, s, e)
	defer s.Close()
	dc, e := s.Actual365Fixed()
	dc = curveMust(t, dc, e)
	start, end := curveDate(t, 15, 6, 2026), curveDate(t, 15, 6, 2027)
	for _, kind := range []string{"Linear", "Cubic"} {
		c, e := s.NewZeroCurve(NodeCurveConfig{Dates: []Date{start, end}, Values: []float64{0.04, 0.04}, DayCounter: dc, Interpolation: kind})
		c = curveMust(t, c, e)
		v, e := c.Discount(0.5, false)
		curveNear(t, curveMust(t, v, e), math.Exp(-0.02), 1e-14)
		r, e := c.ZeroRate(0.5, false)
		curveNear(t, curveMust(t, r, e), 0.04, 1e-12)
		f, e := c.ForwardRate(0.25, 0.75, false)
		curveNear(t, curveMust(t, f, e), 0.04, 1e-12)
	}
	for _, kind := range []string{"LogLinear", "Cubic"} {
		c, e := s.NewDiscountCurve(NodeCurveConfig{Dates: []Date{start, end}, Values: []float64{1, 0.95}, DayCounter: dc, Interpolation: kind})
		c = curveMust(t, c, e)
		v, e := c.DiscountDate(end, false)
		curveNear(t, curveMust(t, v, e), 0.95, 1e-14)
	}
	c, e := s.NewForwardCurve(NodeCurveConfig{Dates: []Date{start, end}, Values: []float64{0.04, 0.04}, DayCounter: dc})
	c = curveMust(t, c, e)
	d, e := c.ReferenceDate()
	if curveMust(t, d, e) != start {
		t.Fatal("reference date mismatch")
	}
	d, e = c.MaxDate()
	if curveMust(t, d, e) != end {
		t.Fatal("max date mismatch")
	}
	if _, e = c.Discount(2, false); e == nil {
		t.Fatal("extrapolation unexpectedly permitted")
	}
	if e = c.EnableExtrapolation(); e != nil {
		t.Fatal(e)
	}
	b, e := c.AllowsExtrapolation()
	if !curveMust(t, b, e) {
		t.Fatal("extrapolation flag unchanged")
	}
	var wg sync.WaitGroup
	for k := 0; k < 8; k++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			v, e := c.Discount(2, false)
			if e != nil || math.Abs(v-math.Exp(-0.08)) > 1e-14 {
				t.Errorf("concurrent query: %v %v", v, e)
			}
		}()
	}
	wg.Wait()
	if e = c.DisableExtrapolation(); e != nil {
		t.Fatal(e)
	}
	if _, e = c.Discount(2, false); e == nil {
		t.Fatal("disable extrapolation ignored")
	}
	if _, e = s.NewZeroCurve(NodeCurveConfig{Dates: []Date{start}, Values: []float64{0.04, 0.05}, DayCounter: dc}); e == nil {
		t.Fatal("mismatched arrays accepted")
	}
	other, e := NewSession()
	other = curveMust(t, other, e)
	defer other.Close()
	if _, e = other.NewFlatForward(start, 0.04, dc); e == nil {
		t.Fatal("foreign day counter accepted")
	}
}

// QuantLib piecewiseyieldcurve.cpp deposit strip, also committed in Python test_piecewise_yield_curve.py.
func TestPiecewiseCurveRepricesIndependentDepositFixings(t *testing.T) {
	cases := []struct {
		name  string
		build func(*Session, PiecewiseCurveConfig) (*YieldTermStructure, error)
	}{
		{"discount-loglinear", (*Session).NewPiecewiseLogLinearDiscount}, {"zero-linear", (*Session).NewPiecewiseLinearZero}, {"zero-cubic", (*Session).NewPiecewiseCubicZero}, {"forward-linear", (*Session).NewPiecewiseLinearForward}, {"forward-convex", (*Session).NewPiecewiseConvexMonotoneForward}, {"forward-flat", (*Session).NewPiecewiseFlatForward},
		{"global", func(s *Session, c PiecewiseCurveConfig) (*YieldTermStructure, error) {
			c.Bootstrap = "global"
			return s.NewPiecewiseYieldCurve(c)
		}},
		{"discount-linear", func(s *Session, c PiecewiseCurveConfig) (*YieldTermStructure, error) {
			c.Interpolation = "Linear"
			return s.NewPiecewiseYieldCurve(c)
		}},
		{"discount-cubic", func(s *Session, c PiecewiseCurveConfig) (*YieldTermStructure, error) {
			c.Interpolation = "Cubic"
			return s.NewPiecewiseYieldCurve(c)
		}},
	}
	tenors := []Period{{1, Weeks}, {1, Months}, {2, Months}, {3, Months}, {6, Months}, {9, Months}}
	rates := []float64{0.04559, 0.04581, 0.04573, 0.04557, 0.04496, 0.04490}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			s, e := NewSession()
			s = curveMust(t, s, e)
			defer s.Close()
			settings, e := s.NewSettings()
			settings = curveMust(t, settings, e)
			today := curveDate(t, 15, 6, 2026)
			if e = settings.SetEvaluationDate(today); e != nil {
				t.Fatal(e)
			}
			dc, e := s.Actual360()
			dc = curveMust(t, dc, e)
			cal, e := s.Target()
			cal = curveMust(t, cal, e)
			settlement, e := cal.Advance(today, 2, Days, Following, false)
			settlement = curveMust(t, settlement, e)
			var hs []*RateHelper
			var quotes []*SimpleQuote
			for k, tenor := range tenors {
				idx, e := s.NewEuribor(tenor, nil, settings)
				idx = curveMust(t, idx, e)
				q, e := s.NewSimpleQuote(rates[k])
				q = curveMust(t, q, e)
				quotes = append(quotes, q)
				h, e := s.NewDepositRateHelper(q, idx)
				hs = append(hs, curveMust(t, h, e))
			}
			c, e := tc.build(s, PiecewiseCurveConfig{ReferenceDate: settlement, Helpers: hs, DayCounter: dc})
			c = curveMust(t, c, e)
			_, e = c.Discount(0.5, false)
			if e != nil {
				t.Fatal(e)
			}
			for k, tenor := range tenors {
				idx, e := s.NewEuribor(tenor, c, settings)
				idx = curveMust(t, idx, e)
				v, e := idx.Fixing(today, false)
				curveNear(t, curveMust(t, v, e), rates[k], 1e-9)
				v, e = hs[k].QuoteValue()
				curveNear(t, curveMust(t, v, e), rates[k], 0)
				v, e = hs[k].QuoteError()
				curveNear(t, curveMust(t, v, e), 0, 1e-9)
			}
			dates, values, e := c.Nodes()
			if e != nil {
				t.Fatal(e)
			}
			if len(dates) != 7 || len(values) != 7 || dates[0] != settlement {
				t.Fatalf("wrong nodes: %v %v", dates, values)
			}
			before, e := c.Discount(0.01, false)
			before = curveMust(t, before, e)
			if e = quotes[0].SetValue(0.05559); e != nil {
				t.Fatal(e)
			}
			after, e := c.Discount(0.01, false)
			after = curveMust(t, after, e)
			if before == after {
				t.Fatal("quote mutation had no effect")
			}
			fresh, e := s.NewEuribor(tenors[0], c, settings)
			fresh = curveMust(t, fresh, e)
			v, e := fresh.Fixing(today, false)
			curveNear(t, curveMust(t, v, e), 0.05559, 1e-9)
		})
	}
}

func TestBootstrapAlgorithmsAndSwapHelpers(t *testing.T) {
	s, e := NewSession()
	s = curveMust(t, s, e)
	defer s.Close()
	settings, e := s.NewSettings()
	settings = curveMust(t, settings, e)
	today := curveDate(t, 15, 6, 2026)
	if e = settings.SetEvaluationDate(today); e != nil {
		t.Fatal(e)
	}
	dc, e := s.Actual360()
	dc = curveMust(t, dc, e)
	bondDC, e := s.Thirty360BondBasis()
	bondDC = curveMust(t, bondDC, e)
	cal, e := s.Target()
	cal = curveMust(t, cal, e)
	settlement, e := cal.Advance(today, 2, Days, Following, false)
	settlement = curveMust(t, settlement, e)
	index, e := s.NewEuribor(Period{6, Months}, nil, settings)
	index = curveMust(t, index, e)
	var hs []*RateHelper
	for _, row := range []struct {
		years int32
		rate  float64
	}{{1, 0.0454}, {2, 0.0463}, {3, 0.0475}} {
		q, e := s.NewSimpleQuote(row.rate)
		q = curveMust(t, q, e)
		h, e := s.NewSwapRateHelper(SwapRateHelperConfig{Quote: q, Tenor: Period{row.years, Years}, Calendar: cal, FixedFrequency: Annual, FixedConvention: Unadjusted, FixedDayCount: bondDC, IborIndex: index})
		hs = append(hs, curveMust(t, h, e))
	}
	for _, interp := range []string{"LogLinear", "Linear"} {
		c, e := s.NewPiecewiseYieldCurve(PiecewiseCurveConfig{ReferenceDate: settlement, Helpers: hs, DayCounter: dc, Interpolation: interp, Bootstrap: "global"})
		c = curveMust(t, c, e)
		_, e = c.Discount(0.5, false)
		if e != nil {
			t.Fatal(e)
		}
		for _, h := range hs {
			v, e := h.QuoteError()
			curveNear(t, curveMust(t, v, e), 0, 1e-9)
		}
	}
	c, e := s.NewPiecewiseConvexMonotoneForward(PiecewiseCurveConfig{ReferenceDate: settlement, Helpers: hs, DayCounter: dc, Bootstrap: "local"})
	c = curveMust(t, c, e)
	_, e = c.Discount(0.5, false)
	if e != nil {
		t.Fatal(e)
	}
	dates, e := c.Dates()
	if len(curveMust(t, dates, e)) != 4 {
		t.Fatal("local node dates")
	}
	data, e := c.Data()
	if len(curveMust(t, data, e)) != 4 {
		t.Fatal("local node data")
	}
	c, e = s.NewPiecewiseYieldCurve(PiecewiseCurveConfig{ReferenceDate: settlement, Helpers: hs[:2], AdditionalHelpers: hs[2:], DayCounter: dc, Bootstrap: "global"})
	c = curveMust(t, c, e)
	max, e := c.MaxDate()
	max = curveMust(t, max, e)
	pillar, e := hs[2].LatestRelevantDate()
	if max != curveMust(t, pillar, e) {
		t.Fatal("global additional helper did not extend max date")
	}
	if _, e = s.NewPiecewiseYieldCurve(PiecewiseCurveConfig{ReferenceDate: settlement, Helpers: hs[:2], AdditionalHelpers: hs[2:], DayCounter: dc}); e == nil {
		t.Fatal("iterative additional helper accepted")
	}
	if _, e = s.NewPiecewiseYieldCurve(PiecewiseCurveConfig{ReferenceDate: settlement, Helpers: hs, DayCounter: dc, Bootstrap: "global", Interpolation: "Cubic"}); e == nil {
		t.Fatal("global cubic accepted")
	}
}

func TestProcessFromCurvesPreservesOrderAndDependencies(t *testing.T) {
	s, e := NewSession()
	s = curveMust(t, s, e)
	defer s.Close()
	dc, e := s.Actual365Fixed()
	dc = curveMust(t, dc, e)
	ref := curveDate(t, 15, 6, 2026)
	rf, e := s.NewFlatForward(ref, 0.08, dc)
	rf = curveMust(t, rf, e)
	div, e := s.NewFlatForward(ref, 0.02, dc)
	div = curveMust(t, div, e)
	vol, e := s.BlackConstantVol(BlackConstantVolConfig{ReferenceDate: ref, Volatility: 0.30, DayCounter: dc})
	vol = curveMust(t, vol, e)
	p, e := s.NewBlackScholesProcessFromCurves(60, rf, div, vol)
	p = curveMust(t, p, e)
	for _, o := range []object{rf.object, div.object, vol.object, dc.object} {
		if e = o.Close(); e != nil {
			t.Fatal(e)
		}
	}
	r, e := p.RiskFreeRate()
	curveNear(t, curveMust(t, r, e), 0.08, 1e-10)
	d, e := p.DividendYield()
	curveNear(t, curveMust(t, d, e), 0.02, 1e-10)
}
