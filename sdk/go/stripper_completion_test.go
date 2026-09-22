package itofin

import (
	"math"
	"testing"
)

func TestOptionletStripperCompletion(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	settings := pricingMust(s.NewSettings())
	ref := volDate(t, 28, 10, 2013)
	ratesOK(t, settings.SetEvaluationDate(ref))
	dc := pricingMust(s.Actual365Fixed())
	cal := pricingMust(s.Target())
	curve := pricingMust(s.NewFlatForward(ref, .04, dc))
	index := pricingMust(s.NewEuriborSixMonths(curve, settings))
	tenors := []Period{{1, Years}, {2, Years}, {3, Years}, {5, Years}, {7, Years}, {10, Years}}
	strikes := []float64{.01, .02, .04, .06, .1}
	grid := RateVolGridConfig{Calendar: cal, DayCounter: dc, Settings: settings, Convention: Following, OptionTenors: tenors, Strikes: strikes}
	for i := range tenors {
		row := make([]float64, len(strikes))
		for j, k := range strikes {
			row[j] = .008 + .0002*float64(i) + .002*k
		}
		grid.Volatilities = append(grid.Volatilities, row)
	}
	surface := pricingMust(s.CapFloorTermVolSurface(grid))
	cfg := OptionletStripperConfig{TermVolSurface: surface, IborIndex: index, VolatilityType: Normal, Accuracy: 1e-10}
	stripper := pricingMust(s.OptionletStripper1(cfg))
	pricingNear(t, pricingMust(stripper.SwitchStrike()), .0398495071682556, 1e-12)
	adapter := pricingMust(s.StrippedOptionletAdapter(stripper, settings))
	ratesOK(t, adapter.EnableExtrapolation())
	engine := pricingMust(s.NewBachelierCapFloorEngine(BachelierCapFloorEngineConfig{Volatility: adapter, Discount: curve}))
	for _, row := range []struct {
		years         int32
		strike, price float64
	}{{1, .02, .009692958950646112}, {3, .04, .009169836497239313}, {10, .06, .012808348961891588}} {
		cap := pricingMust(s.NewCapFloor(CapFloorConfig{Type: CapType, Tenor: Period{row.years, Years}, ForwardStart: Period{0, Days}, Index: index, Strike: row.strike, Settings: settings}))
		pricingNear(t, pricingMust(cap.PriceBachelier(engine)), row.price, 2.5e-8)
	}
	smile := pricingMust(adapter.SmileSection(4, false))
	pricingNear(t, pricingMust(smile.Volatility(.02)), .008713771862127703, 1e-10)
	pricingNear(t, pricingMust(smile.Variance(.02)), math.Pow(pricingMust(smile.Volatility(.02)), 2)*4, 1e-15)
	for _, invalid := range []float64{math.NaN(), math.Inf(1), math.Inf(-1)} {
		if _, err := smile.Volatility(invalid); err == nil {
			t.Fatal("nonfinite smile strike accepted")
		}
		if _, err := smile.Variance(invalid); err == nil {
			t.Fatal("nonfinite smile variance accepted")
		}
	}
	pricingNear(t, pricingMust(smile.Volatility(.02)), .008713771862127703, 1e-10)
	pricingNear(t, pricingMust(smile.ExerciseTime()), 4, 1e-15)
	dateSmile := pricingMust(adapter.SmileSectionDate(volDate(t, 28, 10, 2017), false))
	if pricingMust(dateSmile.Volatility(.04)) <= 0 {
		t.Fatal("invalid date smile")
	}
	tenorSmile := pricingMust(adapter.SmileSectionTenor(Period{4, Years}, false))
	pricingNear(t, pricingMust(tenorSmile.Volatility(.04)), pricingMust(adapter.Volatility(Period{4, Years}, .04, false)), 1e-12)
	explicit := .03
	cfg.SwitchStrike = &explicit
	fixed := pricingMust(s.OptionletStripper1(cfg))
	pricingNear(t, pricingMust(fixed.SwitchStrike()), explicit, 1e-15)
	pricingNear(t, pricingMust(smile.Volatility(.035)), .0088255669084960128, 1e-10)
	ratesOK(t, stripper.Close())
	ratesOK(t, surface.Close())
	ratesOK(t, adapter.Close())
	pricingNear(t, pricingMust(smile.Volatility(.02)), .008713771862127703, 1e-10)
	for i := range grid.Volatilities {
		for j := range grid.Volatilities[i] {
			grid.Volatilities[i][j] = .18
		}
	}
	logSurface := pricingMust(s.CapFloorTermVolSurface(grid))
	first := pricingMust(s.OptionletStripper1(OptionletStripperConfig{TermVolSurface: logSurface, IborIndex: index, Accuracy: 1e-10}))
	atmTenors := []Period{{1, Years}, {3, Years}, {5, Years}}
	quotes := []*SimpleQuote{pricingMust(s.NewSimpleQuote(.2)), pricingMust(s.NewSimpleQuote(.2)), pricingMust(s.NewSimpleQuote(.2))}
	atmCfg := CapFloorTermVolCurveConfig{Calendar: cal, DayCounter: dc, Settings: settings, Convention: Following, OptionTenors: atmTenors, Quotes: quotes}
	atm := pricingMust(s.CapFloorTermVolCurve(atmCfg))
	atmCfg.Settings = nil
	atmCfg.ReferenceDate = ref
	fixedCurve := pricingMust(s.CapFloorTermVolCurve(atmCfg))
	pricingNear(t, pricingMust(fixedCurve.Volatility(atmTenors[1], false)), .2, 1e-14)
	times := pricingMust(atm.OptionTimes())
	fixedTimes := pricingMust(fixedCurve.OptionTimes())
	for i, x := range times {
		pricingNear(t, x, fixedTimes[i], 1e-15)
	}
	second := pricingMust(s.OptionletStripper2(first, atm))
	for _, spread := range pricingMust(second.SpreadsVol()) {
		if !(math.Abs(spread) >= 1e-4) {
			t.Fatal("ATM correction missing")
		}
	}
	atmStrikes := pricingMust(second.ATMCapFloorStrikes())
	prices := pricingMust(second.ATMCapFloorPrices())
	if len(prices) != 3 || len(atmStrikes) != 3 {
		t.Fatal("invalid ATM correction size")
	}
	wantStrikes := []float64{.039850314101546484, .039850293421813322, .039849613061160541}
	wantPrices := []float64{.0010954230960266622, .0087581286819643916, .019163763351720011}
	wantSpreads := []float64{.020000005236476808, .020000058319575627, .020000166175862723}
	spreads := pricingMust(second.SpreadsVol())
	for i := range prices {
		pricingNear(t, atmStrikes[i], wantStrikes[i], 1e-12)
		pricingNear(t, prices[i], wantPrices[i], 2.5e-8)
		pricingNear(t, spreads[i], wantSpreads[i], 1e-7)
	}
	secondAdapter := pricingMust(s.StrippedOptionletAdapter(second, settings))
	ratesOK(t, secondAdapter.EnableExtrapolation())
	black := pricingMust(s.NewBlackCapFloorEngine(BlackCapFloorEngineConfig{Volatility: secondAdapter, Discount: curve}))
	for i, tenor := range atmTenors {
		cap := pricingMust(s.NewCapFloor(CapFloorConfig{Type: CapType, Tenor: tenor, Index: index, Strike: atmStrikes[i], ForwardStart: Period{0, Days}, Settings: settings}))
		ratesOK(t, cap.SetBlackEngine(black))
		pricingNear(t, pricingMust(cap.NPV()), prices[i], 2.5e-8)
	}
	ratesOK(t, quotes[1].SetValue(.21))
	if pricingMust(second.ATMCapFloorPrices())[1] <= prices[1] {
		t.Fatal("ATM quote update ignored")
	}
	ratesOK(t, quotes[1].SetValue(.2))
	pricingNear(t, pricingMust(second.ATMCapFloorPrices())[1], prices[1], 1e-12)
	if _, err := s.OptionletStripper2(fixed, atm); err == nil {
		t.Fatal("normal Stripper2 accepted")
	}
	ratesOK(t, first.Close())
	ratesOK(t, second.Close())
	ratesOK(t, atm.Close())
	for _, q := range quotes {
		ratesOK(t, q.Close())
	}
	if pricingMust(secondAdapter.Volatility(atmTenors[1], atmStrikes[1], false)) <= 0 {
		t.Fatal("retained adapter failed")
	}
}

func TestOptionletStripperOvernightAndFallback(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	settings := pricingMust(s.NewSettings())
	ref := volDate(t, 28, 10, 2013)
	ratesOK(t, settings.SetEvaluationDate(ref))
	dc := pricingMust(s.Actual365Fixed())
	cal := pricingMust(s.Target())
	curve := pricingMust(s.NewFlatForward(ref, .04, dc))
	index := pricingMust(s.NewEuriborSixMonths(curve, settings))
	grid := RateVolGridConfig{Calendar: cal, DayCounter: dc, Settings: settings, Convention: Following, OptionTenors: []Period{{1, Years}, {3, Years}, {5, Years}}, Strikes: []float64{.01, .04, .1}, Volatilities: [][]float64{{.18, .18, .18}, {.18, .18, .18}, {.18, .18, .18}}}
	surface := pricingMust(s.CapFloorTermVolSurface(grid))
	cfg := OptionletStripperConfig{TermVolSurface: surface, IborIndex: index, Accuracy: 1e-16, MaxIterations: 1}
	strict := pricingMust(s.OptionletStripper1(cfg))
	if _, err := strict.ATMOptionletRates(); err == nil {
		t.Fatal("strict inversion did not fail")
	}
	cfg.DontThrow = true
	relaxed := pricingMust(s.OptionletStripper1(cfg))
	if pricingMust(relaxed.SwitchStrike()) <= 0 {
		t.Fatal("fallback failed")
	}
	grid.OptionTenors = []Period{{1, Years}, {2, Years}, {3, Years}, {5, Years}, {7, Years}, {10, Years}}
	grid.Strikes = []float64{.01, .02, .04, .06, .1}
	grid.Volatilities = nil
	for i := range grid.OptionTenors {
		row := make([]float64, len(grid.Strikes))
		for j, strike := range grid.Strikes {
			row[j] = .008 + .0002*float64(i) + .002*strike
		}
		grid.Volatilities = append(grid.Volatilities, row)
	}
	normalSurface := pricingMust(s.CapFloorTermVolSurface(grid))
	cfg.TermVolSurface = normalSurface
	cfg.VolatilityType = Normal
	overnight := pricingMust(s.NewEonia(curve, settings))
	cfg.IborIndex = nil
	cfg.OvernightIndex = overnight
	cfg.Accuracy = 1e-10
	cfg.MaxIterations = 100
	cfg.DontThrow = false
	if _, err := s.OptionletStripper1(cfg); err == nil {
		t.Fatal("missing overnight frequency accepted")
	}
	step := Period{6, Months}
	cfg.OptionletFrequency = &step
	on := pricingMust(s.OptionletStripper1(cfg))
	adapter := pricingMust(s.StrippedOptionletAdapter(on, settings))
	before := pricingMust(adapter.Volatility(Period{3, Years}, .04, false))
	if !(before > 0) {
		t.Fatal("overnight strip failed")
	}
	ratesOK(t, on.Close())
	ratesOK(t, overnight.Close())
	ratesOK(t, surface.Close())
	ratesOK(t, normalSurface.Close())
	pricingNear(t, pricingMust(adapter.Volatility(Period{3, Years}, .04, false)), before, 1e-12)
	foreign := pricingMust(NewSession())
	defer foreign.Close()
	foreignSettings := pricingMust(foreign.NewSettings())
	if _, err := s.StrippedOptionletAdapter(relaxed, foreignSettings); err == nil {
		t.Fatal("foreign settings accepted")
	}
	var nilStripper *OptionletStripper2
	if _, err := s.StrippedOptionletAdapter(nilStripper, settings); err == nil {
		t.Fatal("nil stripper accepted")
	}
}

func TestOvernightCapFloorOracle(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	settings := pricingMust(s.NewSettings())
	ref := volDate(t, 28, 10, 2013)
	ratesOK(t, settings.SetEvaluationDate(ref))
	dc := pricingMust(s.Actual365Fixed())
	cal := pricingMust(s.Target())
	curve := pricingMust(s.NewFlatForward(ref, .04, dc))
	index := pricingMust(s.NewEonia(curve, settings))
	convention := ModifiedFollowing
	schedule := pricingMust(s.NewSchedule(ScheduleConfig{Start: volDate(t, 28, 10, 2014), End: volDate(t, 28, 10, 2016), Frequency: Semiannual, Calendar: cal, Convention: convention, TerminationConvention: &convention}))
	cfg := OvernightCapFloorConfig{Type: CapType, Schedule: schedule, Index: index, Settings: settings, Nominal: 1, CapRates: []float64{.04}, PaymentLag: 2, PaymentAdjustment: ModifiedFollowing}
	cap := pricingMust(s.NewOvernightCapFloor(cfg))
	if pricingMust(cap.CouponCount()) != 4 {
		t.Fatal("overnight coupons missing")
	}
	quote := pricingMust(s.NewSimpleQuote(.008))
	engine := pricingMust(s.NewBachelierCapFloorEngineFlat(RateEngineFlatVolConfig{Discount: curve, Volatility: quote, DayCounter: dc, Settings: settings}))
	pricingNear(t, pricingMust(cap.PriceBachelier(engine)), .008645761084412935, 2.5e-8)
	tenors := []Period{{1, Years}, {2, Years}, {3, Years}, {5, Years}, {7, Years}, {10, Years}}
	strikes := []float64{.01, .02, .04, .06, .1}
	grid := RateVolGridConfig{Calendar: cal, DayCounter: dc, Settings: settings, Convention: Following, OptionTenors: tenors, Strikes: strikes}
	for i := range tenors {
		row := make([]float64, len(strikes))
		for j, k := range strikes {
			row[j] = .008 + .0002*float64(i) + .002*k
		}
		grid.Volatilities = append(grid.Volatilities, row)
	}
	surface := pricingMust(s.CapFloorTermVolSurface(grid))
	step := Period{6, Months}
	stripper := pricingMust(s.OptionletStripper1(OptionletStripperConfig{TermVolSurface: surface, OvernightIndex: index, VolatilityType: Normal, OptionletFrequency: &step, Accuracy: 1e-10}))
	adapter := pricingMust(s.StrippedOptionletAdapter(stripper, settings))
	ratesOK(t, adapter.EnableExtrapolation())
	strippedEngine := pricingMust(s.NewBachelierCapFloorEngine(BachelierCapFloorEngineConfig{Volatility: adapter, Discount: curve}))
	strippedCap := pricingMust(s.NewOvernightCapFloor(cfg))
	pricingNear(t, pricingMust(strippedCap.PriceBachelier(strippedEngine)), 1.542865396979454, 2.5e-8)
	for _, invalid := range []float64{math.NaN(), math.Inf(1), math.Inf(-1)} {
		cfg.Nominal = invalid
		if _, err := s.NewOvernightCapFloor(cfg); err == nil {
			t.Fatal("invalid nominal accepted")
		}
		cfg.Nominal = 1
		cfg.CapRates = []float64{invalid}
		if _, err := s.NewOvernightCapFloor(cfg); err == nil {
			t.Fatal("invalid cap strike accepted")
		}
		cfg.CapRates = []float64{.04}
		cfg.Type = CollarType
		cfg.FloorRates = []float64{invalid}
		if _, err := s.NewOvernightCapFloor(cfg); err == nil {
			t.Fatal("invalid floor strike accepted")
		}
		cfg.Type = CapType
		cfg.FloorRates = nil
	}
	for _, lag := range []int32{math.MaxInt32, math.MinInt32} {
		cfg.PaymentLag = lag
		if _, err := s.NewOvernightCapFloor(cfg); err == nil {
			t.Fatal("payment date overflow accepted")
		}
	}
	cfg.PaymentLag = 2
	recovered := pricingMust(s.NewOvernightCapFloor(cfg))
	pricingNear(t, pricingMust(recovered.PriceBachelier(engine)), .008645761084412935, 2.5e-8)
	ratesOK(t, index.Close())
	ratesOK(t, schedule.Close())
	pricingNear(t, pricingMust(cap.PriceBachelier(engine)), .008645761084412935, 2.5e-8)
}

func TestOptionletStripperRetainedCapRecovery(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	settings := pricingMust(s.NewSettings())
	ref := volDate(t, 28, 10, 2013)
	ratesOK(t, settings.SetEvaluationDate(ref))
	dc := pricingMust(s.Actual365Fixed())
	cal := pricingMust(s.Target())
	curve := pricingMust(s.NewFlatForward(ref, .04, dc))
	index := pricingMust(s.NewEuriborSixMonths(curve, settings))
	quote := pricingMust(s.NewSimpleQuote(.04))
	discount := pricingMust(s.NewFlatForwardFromQuote(ref, quote, dc))
	tenors := []Period{{1, Years}, {2, Years}, {3, Years}, {5, Years}, {7, Years}, {10, Years}}
	strikes := []float64{.01, .02, .04, .06, .1}
	grid := RateVolGridConfig{Calendar: cal, DayCounter: dc, Settings: settings, Convention: Following, OptionTenors: tenors, Strikes: strikes}
	for i := range tenors {
		row := make([]*SimpleQuote, len(strikes))
		for j, k := range strikes {
			row[j] = pricingMust(s.NewSimpleQuote(.008 + .0002*float64(i) + .002*k))
		}
		grid.Quotes = append(grid.Quotes, row)
	}
	surface := pricingMust(s.CapFloorTermVolSurface(grid))
	stripper := pricingMust(s.OptionletStripper1(OptionletStripperConfig{TermVolSurface: surface, IborIndex: index, VolatilityType: Normal, Accuracy: 1e-10, Discount: discount, DontThrow: true}))
	adapter := pricingMust(s.StrippedOptionletAdapter(stripper, settings))
	engine := pricingMust(s.NewBachelierCapFloorEngine(BachelierCapFloorEngineConfig{Volatility: adapter, Discount: discount}))
	cap := pricingMust(s.NewCapFloor(CapFloorConfig{Type: CapType, Tenor: Period{3, Years}, Index: index, Strike: .04, ForwardStart: Period{0, Days}, Settings: settings}))
	ratesOK(t, cap.SetBachelierEngine(engine))
	before := pricingMust(cap.NPV())
	for i, row := range grid.Quotes {
		for j, q := range row {
			ratesOK(t, q.SetValue(.008+.0002*float64(i)+.002*strikes[j]+.001))
		}
	}
	if !(pricingMust(cap.NPV()) > before) {
		t.Fatal("retained cap missed volatility update")
	}
	for i, row := range grid.Quotes {
		for j, q := range row {
			ratesOK(t, q.SetValue(.008+.0002*float64(i)+.002*strikes[j]))
		}
	}
	pricingNear(t, pricingMust(cap.NPV()), before, 2.5e-8)
	late := volDate(t, 15, 5, 2023)
	if _, err := adapter.VolatilityDate(late, .04, false); err == nil {
		t.Fatal("old maximum date ignored")
	}
	ratesOK(t, settings.SetEvaluationDate(volDate(t, 28, 12, 2013)))
	if !(pricingMust(adapter.VolatilityDate(late, .04, false)) > 0) {
		t.Fatal("moving maximum date not updated")
	}
	ratesOK(t, settings.SetEvaluationDate(ref))
	if _, err := adapter.VolatilityDate(late, .04, false); err == nil {
		t.Fatal("maximum date did not restore")
	}
	pricingNear(t, pricingMust(cap.NPV()), before, 2.5e-8)
	ratesOK(t, quote.SetValue(math.NaN()))
	if _, err := stripper.ATMOptionletRates(); err == nil {
		t.Fatal("dontThrow hid invalid discount")
	}
	if _, err := cap.NPV(); err == nil {
		t.Fatal("cap hid invalid discount")
	}
	ratesOK(t, quote.SetValue(.04))
	pricingNear(t, pricingMust(cap.NPV()), before, 2.5e-8)
}
