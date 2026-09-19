package itofin

import (
	"encoding/json"
	"math"
	"os"
	"testing"
)

func TestEoniaOisSwaptionCachedLifecycle(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	today := pricingMust(NewDate(13, 3, 2002))
	settings := pricingMust(s.NewSettings())
	ratesOK(t, settings.SetEvaluationDate(today))
	calendar := pricingMust(s.Target())
	dc := pricingMust(s.Actual365Fixed())
	fixedDC := pricingMust(s.Thirty360BondBasis())
	settlement := pricingMust(calendar.Advance(today, 2, Days, Following, false))
	discount := pricingMust(s.NewFlatForward(settlement, .05, dc))
	forward := pricingMust(s.NewFlatForward(settlement, .04, dc))
	index := pricingMust(s.NewEonia(forward, settings))
	if days := pricingMust(index.FixingDays()); days != 0 {
		t.Fatalf("Eonia fixing days %d", days)
	}
	indexCalendar := pricingMust(index.FixingCalendar())
	indexDC := pricingMust(index.DayCounter())
	currency := pricingMust(index.Currency())
	if code := pricingMust(currency.Code()); code != "EUR" {
		t.Fatalf("Eonia currency %s", code)
	}
	fixing := pricingMust(index.Fixing(settlement, true))
	if math.Abs(fixing-math.Expm1(.04*3/365)*360/3) > 1e-12 {
		t.Fatalf("Eonia Actual360 zero-lag forecast: %.16g", fixing)
	}
	exerciseDate := pricingMust(calendar.Advance(settlement, 5, Years, Following, false))
	start := pricingMust(calendar.Advance(exerciseDate, 2, Days, Following, false))
	rate := .06
	swap := pricingMust(s.MakeOis(MakeOisConfig{Tenor: Period{10, Years}, Index: index, Settings: settings, FixedRate: &rate, EffectiveDate: &start, FixedLegDayCounter: fixedDC}))
	exercise := pricingMust(s.NewEuropeanExercise(exerciseDate))
	cfg := OisSwaptionConfig{Swap: swap, Exercise: exercise, Settings: settings}
	option := pricingMust(s.NewOisSwaption(cfg))
	if _, err := option.NPV(); err == nil {
		t.Fatal("missing engine accepted")
	}
	vol := pricingMust(s.NewSimpleQuote(.20))
	engine := pricingMust(s.NewBlackSwaptionEngineFlat(RateEngineFlatVolConfig{Discount: discount, Volatility: vol, DayCounter: dc, Settings: settings}))
	value := pricingMust(option.Price(engine))
	if math.Abs(value-.014101075767) > 1e-12 {
		t.Fatalf("QuantLib Eonia cached NPV: %.16g", value)
	}
	other := pricingMust(NewSession())
	defer other.Close()
	foreign := pricingMust(other.NewSettings())
	cfg.Settings = foreign
	if _, err := s.NewOisSwaption(cfg); err == nil {
		t.Fatal("foreign settings accepted")
	}
	if _, err := s.NewEonia(forward, foreign); err == nil {
		t.Fatal("foreign Eonia settings accepted")
	}
	if _, err := s.NewEonia(nil, nil); err == nil {
		t.Fatal("nil Eonia settings accepted")
	}
	if _, err := s.NewOisSwaption(OisSwaptionConfig{}); err == nil {
		t.Fatal("nil OIS accepted")
	}
	cfg.Settings = settings
	cfg.SettlementType = SettlementType(99)
	if _, err := s.NewOisSwaption(cfg); err == nil {
		t.Fatal("invalid settlement accepted")
	}
	ratesOK(t, swap.Close())
	ratesOK(t, index.Close())
	if name := pricingMust(indexCalendar.Name()); name != "TARGET" {
		t.Fatalf("retained Eonia calendar %s", name)
	}
	if name := pricingMust(indexDC.Name()); name != "Actual/360" {
		t.Fatalf("retained Eonia day counter %s", name)
	}
	ratesOK(t, indexCalendar.Close())
	ratesOK(t, indexDC.Close())
	ratesOK(t, currency.Close())
	if _, err := index.FixingDays(); err == nil {
		t.Fatal("closed index inspector accepted")
	}
	ratesOK(t, forward.Close())
	ratesOK(t, discount.Close())
	ratesOK(t, settings.Close())
	ratesOK(t, exercise.Close())
	ratesOK(t, engine.Close())
	if got := pricingMust(option.NPV()); got != value {
		t.Fatalf("released dependencies changed NPV: %g", got)
	}
	ratesOK(t, vol.SetValue(.30))
	if got := pricingMust(option.NPV()); got <= value {
		t.Fatalf("volatility update not observed: %g", got)
	}
	ratesOK(t, vol.Close())
	ratesOK(t, option.Close())
	if _, err := option.NPV(); err == nil {
		t.Fatal("closed swaption accepted")
	}
}

func TestMakeSwaptionCalendarAndForecastOracles(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	today := pricingMust(NewDate(9, 10, 2015))
	settings := pricingMust(s.NewSettings())
	ratesOK(t, settings.SetEvaluationDate(today))
	cal := pricingMust(s.Target())
	us := pricingMust(s.UnitedStates("Settlement"))
	dc := pricingMust(s.Actual360())
	fixedDC := pricingMust(s.Thirty360BondBasis())
	curve := pricingMust(s.NewFlatForward(today, .05, dc))
	ibor := pricingMust(s.NewEuriborSixMonths(curve, settings))
	eur := pricingMust(s.EUR())
	index := pricingMust(s.NewSwapIndex(SwapIndexConfig{Family: "EuriborSwapIsdaFixA", Tenor: Period{5, Years}, FixedLegTenor: Period{1, Years}, SettlementDays: 2, Currency: eur, Calendar: cal, FixedLegDayCounter: fixedDC, Index: ibor, Settings: settings}))
	tenor, strike, nominal := Period{1, Years}, .05, 7.0
	cfg := MakeSwaptionConfig{Index: index, OptionTenor: &tenor, Strike: &strike, Nominal: &nominal}
	option := pricingMust(s.MakeSwaption(cfg))
	targetDate := pricingMust(NewDate(10, 10, 2016))
	if got := pricingMust(option.ExerciseDate()); got != targetDate {
		t.Fatalf("TARGET exercise %v", got)
	}
	if got := pricingMust(option.UnderlyingNominal()); got != nominal {
		t.Fatalf("nominal %g", got)
	}
	cfg.OptionConvention = Preceding
	preceding := pricingMust(s.MakeSwaption(cfg))
	if got := pricingMust(preceding.ExerciseDate()); got != pricingMust(NewDate(7, 10, 2016)) {
		t.Fatalf("preceding exercise %v", got)
	}
	cfg.OptionConvention = ModifiedFollowing
	cfg.ExerciseCalendar = us
	custom := pricingMust(s.MakeSwaption(cfg))
	if got := pricingMust(custom.ExerciseDate()); got != pricingMust(NewDate(11, 10, 2016)) {
		t.Fatalf("US exercise %v", got)
	}
	explicit := pricingMust(NewDate(11, 4, 2016))
	cfg.ExerciseDate = &explicit
	if got := pricingMust(pricingMust(s.MakeSwaption(cfg)).ExerciseDate()); got != explicit {
		t.Fatalf("explicit exercise %v", got)
	}
	cfg.OptionTenor, cfg.FixingDate, cfg.ExerciseDate, cfg.Strike = nil, &targetDate, nil, nil
	atm := pricingMust(s.MakeSwaption(cfg))
	rate := pricingMust(atm.UnderlyingFixedRate())
	var oracle struct {
		Quantlib string `json:"quantlib"`
		Fixings  []struct {
			Date   string  `json:"date"`
			Fixing float64 `json:"fixing"`
		} `json:"fixings"`
	}
	data, err := os.ReadFile("testdata/makeswaption_oracle.json")
	ratesOK(t, err)
	ratesOK(t, json.Unmarshal(data, &oracle))
	if oracle.Quantlib != "1.43" || len(oracle.Fixings) != 3 {
		t.Fatal("invalid QuantLib oracle provenance")
	}
	for _, row := range oracle.Fixings {
		fixing := pricingMust(index.Fixing(overnightDate(t, row.Date), true))
		if math.Abs(fixing-row.Fixing) > 1e-12 {
			t.Fatalf("QuantLib forecast on %s: %.16g vs %.16g", row.Date, fixing, row.Fixing)
		}
	}
	forecast := pricingMust(index.Fixing(targetDate, true))
	if math.Abs(forecast-0.05202914613654939) > 1e-12 {
		t.Fatalf("QuantLib forecast %g", forecast)
	}
	if math.Abs(rate-forecast) > 1e-12 {
		t.Fatalf("ATM %g vs forecast %g", rate, forecast)
	}
	if _, err := s.MakeSwaption(MakeSwaptionConfig{Index: index}); err == nil {
		t.Fatal("absent tenor/date accepted")
	}
	cfg.OptionTenor = &tenor
	if _, err := s.MakeSwaption(cfg); err == nil {
		t.Fatal("ambiguous tenor/date accepted")
	}
	cfg.OptionTenor = nil
	late := pricingMust(NewDate(12, 10, 2016))
	cfg.ExerciseDate = &late
	if _, err := s.MakeSwaption(cfg); err == nil {
		t.Fatal("late exercise accepted")
	}
	cfg.ExerciseDate = nil
	nan := math.NaN()
	cfg.Strike = &nan
	if _, err := s.MakeSwaption(cfg); err == nil {
		t.Fatal("nonfinite strike accepted")
	}
	cfg.Strike = &strike
	other := pricingMust(NewSession())
	defer other.Close()
	cfg.ExerciseCalendar = pricingMust(other.Target())
	if _, err := s.MakeSwaption(cfg); err == nil {
		t.Fatal("foreign exercise calendar accepted")
	}
	cfg.ExerciseCalendar = nil
	vol := pricingMust(s.NewSimpleQuote(.20))
	engine := pricingMust(s.NewBlackSwaptionEngineFlat(RateEngineFlatVolConfig{Discount: curve, Volatility: vol, DayCounter: dc, Settings: settings}))
	initialNPV := pricingMust(atm.Price(engine))
	ratesOK(t, engine.Close())
	ratesOK(t, cal.Close())
	ratesOK(t, us.Close())
	ratesOK(t, dc.Close())
	ratesOK(t, fixedDC.Close())
	ratesOK(t, eur.Close())
	ratesOK(t, index.Close())
	ratesOK(t, settings.Close())
	ratesOK(t, curve.Close())
	ratesOK(t, ibor.Close())
	if _, err := s.MakeSwaption(cfg); err == nil {
		t.Fatal("closed swap index accepted")
	}
	if got := pricingMust(atm.UnderlyingFixedRate()); got != rate {
		t.Fatal("builder lost retained dependencies")
	}
	if got := pricingMust(atm.NPV()); got != initialNPV {
		t.Fatalf("released builder dependencies changed NPV: %.16g vs %.16g", got, initialNPV)
	}
	ratesOK(t, vol.SetValue(.30))
	if pricingMust(atm.IsCalculated()) {
		t.Fatal("live volatility did not invalidate the built swaption")
	}
	if got := pricingMust(atm.NPV()); got <= initialNPV {
		t.Fatalf("built swaption did not reprice after dependency release: %.16g vs %.16g", got, initialNPV)
	}
	ratesOK(t, vol.Close())
}
