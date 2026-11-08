package itofin

import (
	"math"
	"testing"
)

func (p sensitivityPortfolio) checkKind(t *testing.T, s *Session) {
	t.Helper()
	p.checkManual(t, s, []float64{1})
	got, e := s.ParallelAnalysis(p.quotes, p.instruments, nil, DefaultShift, Centered, nil)
	bondCheck(t, e)
	if got.First == 0 {
		t.Fatal("reprice did not react to the bumped quote")
	}
}

type optionMarket struct {
	option          *VanillaOption
	spot, rate, vol *SimpleQuote
	settings        *Settings
	today           Date
	dc              *DayCounter
	riskFree        *YieldTermStructure
	dividend        *YieldTermStructure
	volatility      *BlackVolTermStructure
}

func optionFixture(t *testing.T, s *Session) optionMarket {
	t.Helper()
	m := optionMarket{today: testDate(t, 15, 6, 2026)}
	var e error
	m.dc, e = s.Actual365Fixed()
	bondCheck(t, e)
	m.settings, e = s.NewSettings()
	bondCheck(t, e)
	bondCheck(t, m.settings.SetEvaluationDate(m.today))
	m.spot, e = s.NewSimpleQuote(100)
	bondCheck(t, e)
	m.rate, e = s.NewSimpleQuote(.05)
	bondCheck(t, e)
	m.vol, e = s.NewSimpleQuote(.2)
	bondCheck(t, e)
	m.riskFree, e = s.NewFlatForwardFromQuote(m.today, m.rate, m.dc)
	bondCheck(t, e)
	m.dividend, e = s.NewFlatForward(m.today, .02, m.dc)
	bondCheck(t, e)
	calendar, e := s.Target()
	bondCheck(t, e)
	m.volatility, e = s.NewBlackConstantVolFromQuote(m.today, m.vol, m.dc, calendar)
	bondCheck(t, e)
	process, e := s.NewBlackScholesProcessFromQuote(m.spot, m.riskFree, m.dividend, m.volatility)
	bondCheck(t, e)
	expiry, e := m.today.AddDays(360)
	bondCheck(t, e)
	m.option, e = s.NewVanillaOption(Call, 105, expiry, m.settings)
	bondCheck(t, e)
	bondCheck(t, m.option.SetEngine(process))
	return m
}

func TestSensitivityOptionMatchesManualBump(t *testing.T) {
	s, e := NewSession()
	bondCheck(t, e)
	defer s.Close()
	m := optionFixture(t, s)
	for _, q := range []*SimpleQuote{m.spot, m.vol, m.rate} {
		sensitivityPortfolio{[]*SimpleQuote{q}, []Instrument{m.option}, []func() (float64, error){m.option.NPV}}.checkKind(t, s)
	}
}

func TestBlackScholesProcessFromQuoteLinksTheSpot(t *testing.T) {
	s, e := NewSession()
	bondCheck(t, e)
	defer s.Close()
	m := optionFixture(t, s)
	bondCheck(t, m.spot.SetValue(110))
	linked, e := m.option.NPV()
	bondCheck(t, e)
	fixed, e := s.NewBlackScholesProcessFromCurves(110, m.riskFree, m.dividend, m.volatility)
	bondCheck(t, e)
	want, e := m.option.Price(fixed)
	bondCheck(t, e)
	if linked != want {
		t.Fatalf("linked spot %.15g vs fixed 110 %.15g", linked, want)
	}
	if _, e := s.NewBlackScholesProcessFromQuote(nil, m.riskFree, m.dividend, m.volatility); e == nil {
		t.Fatal("nil spot accepted")
	}
}

func TestSensitivityOptionSpotBumpMatchesDeltaAtSecondOrder(t *testing.T) {
	s, e := NewSession()
	bondCheck(t, e)
	defer s.Close()
	m := optionFixture(t, s)
	delta, e := m.option.Delta()
	bondCheck(t, e)
	errorAt := func(h float64) float64 {
		got, e := s.ParallelAnalysis([]*SimpleQuote{m.spot}, []Instrument{m.option}, nil, h, Centered, nil)
		bondCheck(t, e)
		return math.Abs(got.First - delta)
	}
	coarse, fine := errorAt(1), errorAt(.5)
	if coarse > 1e-3 || math.Abs(coarse/fine-4) > .1 {
		t.Fatalf("delta %g: error %g at h=1, %g at h=0.5", delta, coarse, fine)
	}
}

func rateCurveFixture(t *testing.T, s *Session) (*Settings, *SimpleQuote, *YieldTermStructure, *IborIndex, *DayCounter, Date) {
	t.Helper()
	today := testDate(t, 13, 3, 2002)
	settings, e := s.NewSettings()
	bondCheck(t, e)
	bondCheck(t, settings.SetEvaluationDate(today))
	dc, e := s.Actual365Fixed()
	bondCheck(t, e)
	quote, e := s.NewSimpleQuote(.05)
	bondCheck(t, e)
	curve, e := s.NewFlatForwardFromQuote(today, quote, dc)
	bondCheck(t, e)
	index, e := s.NewEuriborSixMonths(curve, settings)
	bondCheck(t, e)
	return settings, quote, curve, index, dc, today
}

func TestSensitivitySwaptionMatchesManualBump(t *testing.T) {
	s, e := NewSession()
	bondCheck(t, e)
	defer s.Close()
	settings, quote, curve, index, dc, today := rateCurveFixture(t, s)
	calendar, e := s.Target()
	bondCheck(t, e)
	exerciseDate, e := calendar.Advance(today, 5, Years, Following, false)
	bondCheck(t, e)
	start, e := calendar.Advance(exerciseDate, 2, Days, Following, false)
	bondCheck(t, e)
	rate := .06
	swap, e := s.MakeVanillaSwap(MakeVanillaSwapConfig{Tenor: Period{10, Years}, Index: index, Settings: settings, FixedRate: &rate, EffectiveDate: &start})
	bondCheck(t, e)
	exercise, e := s.NewEuropeanExercise(exerciseDate)
	bondCheck(t, e)
	swaption, e := s.NewSwaption(SwaptionConfig{Swap: swap, Exercise: exercise, Settings: settings})
	bondCheck(t, e)
	vol, e := s.NewSimpleQuote(.2)
	bondCheck(t, e)
	engine, e := s.NewBlackSwaptionEngineFlat(RateEngineFlatVolConfig{Discount: curve, Volatility: vol, DayCounter: dc, Settings: settings})
	bondCheck(t, e)
	bondCheck(t, swaption.SetBlackEngine(engine))
	sensitivityPortfolio{[]*SimpleQuote{quote}, []Instrument{swaption}, []func() (float64, error){swaption.NPV}}.checkKind(t, s)
}

func TestSensitivityCapFloorMatchesManualBump(t *testing.T) {
	s, e := NewSession()
	bondCheck(t, e)
	defer s.Close()
	settings, quote, curve, index, dc, _ := rateCurveFixture(t, s)
	cap, e := s.NewCapFloor(CapFloorConfig{Type: CapType, Tenor: Period{3, Years}, ForwardStart: Period{1, Years}, Index: index, Strike: .04, Settings: settings})
	bondCheck(t, e)
	vol, e := s.NewSimpleQuote(.2)
	bondCheck(t, e)
	engine, e := s.NewBlackCapFloorEngineFlat(RateEngineFlatVolConfig{Discount: curve, Volatility: vol, DayCounter: dc, Settings: settings})
	bondCheck(t, e)
	bondCheck(t, cap.SetBlackEngine(engine))
	sensitivityPortfolio{[]*SimpleQuote{quote}, []Instrument{cap}, []func() (float64, error){cap.NPV}}.checkKind(t, s)
}

func TestSensitivityCdsMatchesManualBump(t *testing.T) {
	s, e := NewSession()
	bondCheck(t, e)
	defer s.Close()
	today := testDate(t, 9, 6, 2006)
	dc, e := s.Actual360()
	bondCheck(t, e)
	calendar, e := s.Target()
	bondCheck(t, e)
	settings, e := s.NewSettings()
	bondCheck(t, e)
	bondCheck(t, settings.SetEvaluationDate(today))
	hazard, e := s.NewSimpleQuote(.01234)
	bondCheck(t, e)
	probability, e := s.NewFlatHazardRate(FlatHazardConfig{Quote: hazard, DayCounter: dc, Calendar: calendar, Settings: settings})
	bondCheck(t, e)
	rate, e := s.NewSimpleQuote(.06)
	bondCheck(t, e)
	discount, e := s.NewFlatForwardFromQuote(today, rate, dc)
	bondCheck(t, e)
	engine, e := s.NewMidPointCdsEngine(CdsEngineConfig{Probability: probability, Discount: discount, Settings: settings, Recovery: .4})
	bondCheck(t, e)
	schedule, e := s.NewSchedule(ScheduleConfig{Start: testDate(t, 9, 6, 2005), End: testDate(t, 9, 6, 2015), Frequency: Semiannual, Calendar: calendar, Convention: ModifiedFollowing})
	bondCheck(t, e)
	cfg := DefaultCdsConfig()
	cfg.Side = ProtectionSeller
	cfg.Notional = 10000
	cfg.Spread = .012
	cfg.Schedule = schedule
	cfg.PaymentConvention = ModifiedFollowing
	cfg.DayCounter = dc
	cfg.Settings = settings
	cds, e := s.NewCreditDefaultSwap(cfg)
	bondCheck(t, e)
	bondCheck(t, cds.SetEngine(engine))
	for _, q := range []*SimpleQuote{rate, hazard} {
		sensitivityPortfolio{[]*SimpleQuote{q}, []Instrument{cds}, []func() (float64, error){cds.NPV}}.checkKind(t, s)
	}
}
