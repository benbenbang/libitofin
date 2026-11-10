package itofin

import "testing"

// testHelperDatesFromNonBusinessEvaluationDate (QuantLib v1.44
// test-suite/piecewiseyieldcurve.cpp:2422) through the C ABI: on Saturday
// 20 June 2026 the Euribor6M fixing days count from the Saturday itself, so the
// deposit starts Tuesday 23 June and a 3x9 FRA Wednesday 23 September.
func TestHelperDatesFromASaturdayEvaluationDate(t *testing.T) {
	s, e := NewSession()
	s = curveMust(t, s, e)
	defer s.Close()
	settings, e := s.NewSettings()
	settings = curveMust(t, settings, e)
	if e = settings.SetEvaluationDate(curveDate(t, 20, 6, 2026)); e != nil {
		t.Fatal(e)
	}
	idx, e := s.NewEuribor(Period{6, Months}, nil, settings)
	idx = curveMust(t, idx, e)
	depo, e := s.NewDepositRateHelperFromRate(0.03, idx)
	depo = curveMust(t, depo, e)
	start, e := depo.EarliestDate()
	if got := curveMust(t, start, e); got != curveDate(t, 23, 6, 2026) {
		t.Fatalf("deposit earliest date %v", got)
	}
	q, e := s.NewSimpleQuote(0.03)
	q = curveMust(t, q, e)
	cfg := DefaultFraRateHelperConfig()
	cfg.Quote, cfg.MonthsToStart, cfg.Index = q, 3, idx
	fra, e := s.NewFraRateHelperFromMonths(cfg)
	fra = curveMust(t, fra, e)
	start, e = fra.EarliestDate()
	if got := curveMust(t, start, e); got != curveDate(t, 23, 9, 2026) {
		t.Fatalf("FRA earliest date %v", got)
	}
}
