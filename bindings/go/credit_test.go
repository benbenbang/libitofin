package itofin

import (
	"math"
	"sync"
	"testing"
)

func creditMust[T any](t *testing.T, v T, e error) T {
	t.Helper()
	if e != nil {
		t.Fatal(e)
	}
	return v
}
func TestCreditCachedMidpointAndLifecycle(t *testing.T) {
	s0, e := NewSession()
	s := creditMust(t, s0, e)
	defer s.Close()
	today0, e := NewDate(9, 6, 2006)
	today := creditMust(t, today0, e)
	issue0, e := NewDate(9, 6, 2005)
	issue := creditMust(t, issue0, e)
	maturity0, e := NewDate(9, 6, 2015)
	maturity := creditMust(t, maturity0, e)
	dc0, e := s.Actual360()
	dc := creditMust(t, dc0, e)
	cal0, e := s.Target()
	cal := creditMust(t, cal0, e)
	settings0, e := s.NewSettings()
	settings := creditMust(t, settings0, e)
	if e = settings.SetEvaluationDate(today); e != nil {
		t.Fatal(e)
	}
	q0, e := s.NewSimpleQuote(.01234)
	q := creditMust(t, q0, e)
	h0, e := s.NewFlatHazardRate(FlatHazardConfig{Quote: q, DayCounter: dc, Calendar: cal, Settings: settings})
	h := creditMust(t, h0, e)
	d0, e := s.NewFlatForward(today, .06, dc)
	d := creditMust(t, d0, e)
	engine0, e := s.NewMidPointCdsEngine(CdsEngineConfig{Probability: h, Discount: d, Settings: settings, Recovery: .4})
	engine := creditMust(t, engine0, e)
	schedule0, e := s.NewSchedule(ScheduleConfig{Start: issue, End: maturity, Frequency: Semiannual, Calendar: cal, Convention: ModifiedFollowing})
	schedule := creditMust(t, schedule0, e)
	cfg := DefaultCdsConfig()
	cfg.Side = ProtectionSeller
	cfg.Notional = 10000
	cfg.Spread = .012
	cfg.Schedule = schedule
	cfg.PaymentConvention = ModifiedFollowing
	cfg.DayCounter = dc
	cfg.Settings = settings
	cds0, e := s.NewCreditDefaultSwap(cfg)
	cds := creditMust(t, cds0, e)
	if _, e = cds.NPV(); e == nil {
		t.Fatal("missing engine accepted")
	}
	if e = cds.SetEngine(engine); e != nil {
		t.Fatal(e)
	}
	v, e := cds.NPV()
	if e != nil || math.Abs(v-295.0153398) > 1e-7 {
		t.Fatalf("cached NPV %.12f: %v", v, e)
	}
	spread, e := cds.FairSpread()
	if e != nil || math.Abs(spread-.007517539081) > 1e-7 {
		t.Fatalf("fair spread %g: %v", spread, e)
	}

	noRebate := false
	noRebateCfg := cfg
	noRebateCfg.RebatesAccrual = &noRebate
	noRebate0, e := s.NewCreditDefaultSwap(noRebateCfg)
	noRebateCds := creditMust(t, noRebate0, e)
	rebate, e := noRebateCds.AccrualRebateAmount()
	if e != nil || rebate != nil {
		t.Fatalf("explicit false rebate flag ignored: %v %v", rebate, e)
	}
	if e = q.SetValue(.02); e != nil {
		t.Fatal(e)
	}
	calc, e := cds.IsCalculated()
	if e != nil || calc {
		t.Fatalf("quote update did not invalidate: %v", e)
	}
	changed, e := cds.NPV()
	if e != nil || changed == v {
		t.Fatalf("quote update did not reprice: %v", e)
	}
	for _, close := range []func() error{engine.Close, h.Close, d.Close, q.Close, dc.Close, schedule.Close} {
		if e = close(); e != nil {
			t.Fatal(e)
		}
	}
	retained, e := cds.NPV()
	if e != nil || retained != changed {
		t.Fatalf("dependency lifetime: %g %v", retained, e)
	}
	var wg sync.WaitGroup
	for range 8 {
		wg.Add(1)
		go func() {
			defer wg.Done()
			x, e := cds.NPV()
			if e != nil || x != retained {
				t.Errorf("concurrent query: %g %v", x, e)
			}
		}()
	}
	wg.Wait()
	if e = cds.Close(); e != nil {
		t.Fatal(e)
	}
	if _, e = cds.NPV(); e == nil {
		t.Fatal("released instrument accepted")
	}
}
func TestCreditCurveNodesAndSessionIsolation(t *testing.T) {
	s0, e := NewSession()
	s := creditMust(t, s0, e)
	defer s.Close()
	other0, e := NewSession()
	other := creditMust(t, other0, e)
	defer other.Close()
	a0, e := NewDate(1, 1, 2025)
	a := creditMust(t, a0, e)
	b0, e := NewDate(1, 1, 2026)
	b := creditMust(t, b0, e)
	dc0, e := s.Actual365Fixed()
	dc := creditMust(t, dc0, e)
	if _, e = other.NewFlatHazardRate(FlatHazardConfig{ReferenceDate: a, Rate: .02, DayCounter: dc}); e == nil {
		t.Fatal("foreign day counter accepted")
	}
	c0, e := s.NewInterpolatedHazardRateCurve([]Date{a, b}, []float64{.01, .02}, dc)
	c := creditMust(t, c0, e)
	n, e := c.Nodes()
	if e != nil || len(n) != 2 || n[1].Rate != .02 || n[1].Date.Serial() != b.Serial() {
		t.Fatalf("nodes: %v %v", n, e)
	}
	survival, e := c.SurvivalProbabilityDate(b, false)
	if e != nil || math.Abs(survival-math.Exp(-.02)) > 1e-12 {
		t.Fatalf("survival %g: %v", survival, e)
	}
	if _, e = c.HazardRate(math.NaN(), false); e == nil {
		t.Fatal("NaN accepted")
	}
	if _, e = s.NewInterpolatedHazardRateCurve([]Date{b, a}, []float64{.01, .02}, dc); e == nil {
		t.Fatal("unsorted dates accepted")
	}
}

// Same flat Act/365F fixture as the Python ISDA binding oracle.
func TestCreditISDAOracleDefaultsAndImpliedHazard(t *testing.T) {
	s0, e := NewSession()
	s := creditMust(t, s0, e)
	defer s.Close()
	today0, e := NewDate(15, 6, 2026)
	today := creditMust(t, today0, e)
	maturity0, e := NewDate(15, 6, 2029)
	maturity := creditMust(t, maturity0, e)
	settings0, e := s.NewSettings()
	settings := creditMust(t, settings0, e)
	if e = settings.SetEvaluationDate(today); e != nil {
		t.Fatal(e)
	}
	dc0, e := s.Actual360()
	dc := creditMust(t, dc0, e)
	curvedc0, e := s.Actual365Fixed()
	curvedc := creditMust(t, curvedc0, e)
	cal0, e := s.Target()
	cal := creditMust(t, cal0, e)
	probability0, e := s.NewFlatHazardRate(FlatHazardConfig{ReferenceDate: today, Rate: .02, DayCounter: curvedc})
	probability := creditMust(t, probability0, e)
	discount0, e := s.NewFlatForward(today, .03, curvedc)
	discount := creditMust(t, discount0, e)
	engineCfg := CdsEngineConfig{Probability: probability, Discount: discount, Settings: settings, Recovery: .4}
	engine0, e := s.NewIsdaCdsEngine(engineCfg)
	engine := creditMust(t, engine0, e)
	schedule0, e := s.NewSchedule(ScheduleConfig{Start: today, End: maturity, Frequency: Quarterly, Calendar: cal, Convention: Following})
	schedule := creditMust(t, schedule0, e)
	cds0, e := s.NewCreditDefaultSwap(CdsConfig{Side: ProtectionSeller, Notional: 1e7, Spread: .01, Schedule: schedule, PaymentConvention: Following, DayCounter: dc, Settings: settings})
	cds := creditMust(t, cds0, e)
	if e = cds.SetIsdaEngine(engine); e != nil {
		t.Fatal(e)
	}
	npv, e := cds.NPV()
	if e != nil || math.Abs(npv-(-52927.18294373818)) > 1e-8 {
		t.Fatalf("ISDA NPV %.14g: %v", npv, e)
	}
	coupon, e := cds.CouponLegNPV()
	if e != nil || math.Abs(coupon-281656.6267407311) > 1e-8 {
		t.Fatalf("ISDA coupon %.14g: %v", coupon, e)
	}
	protection, e := cds.DefaultLegNPV()
	if e != nil || math.Abs(protection-(-334583.8096844693)) > 1e-8 {
		t.Fatalf("ISDA protection %.14g: %v", protection, e)
	}
	snapshot0, e := cds.Results()
	snapshot := creditMust(t, snapshot0, e)
	if snapshot.NPV == nil || *snapshot.NPV != npv {
		t.Fatalf("snapshot NPV %v", snapshot.NPV)
	}
	implied, e := cds.ImpliedHazardRate(npv, discount, curvedc, .4, 1e-10, Isda)
	if e != nil || math.Abs(implied-.02) > 1e-9 {
		t.Fatalf("implied hazard %g: %v", implied, e)
	}
	numerical := Taylor
	forwards := PiecewiseForwards
	engineCfg.NumericalFix = &numerical
	engineCfg.Forwards = &forwards
	explicit0, e := s.NewIsdaCdsEngine(engineCfg)
	explicit := creditMust(t, explicit0, e)
	if e = cds.SetIsdaEngine(explicit); e != nil {
		t.Fatal(e)
	}
	explicitNPV, e := cds.NPV()
	if e != nil || explicitNPV != npv {
		t.Fatalf("default fidelity differs: %g %v", explicitNPV, e)
	}
	engineCfg.AccrualBias = NoBias
	unbiased0, e := s.NewIsdaCdsEngine(engineCfg)
	unbiased := creditMust(t, unbiased0, e)
	if e = cds.SetIsdaEngine(unbiased); e != nil {
		t.Fatal(e)
	}
	noBiasCoupon, e := cds.CouponLegNPV()
	if e != nil || math.Abs(noBiasCoupon-281648.88829497696) > 1e-8 {
		t.Fatalf("unbiased coupon %.14g: %v", noBiasCoupon, e)
	}
	if snapshot.NPV == nil || *snapshot.NPV != npv {
		t.Fatal("snapshot mutated with instrument")
	}
}
