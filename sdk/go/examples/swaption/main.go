// Package main demonstrates Black pricing through MakeSwaption and an Eonia OIS.
// MakeSwaption builds payer vanilla swaps only; overnight swaps use MakeOis and
// NewOisSwaption. ATM and OIS pins come from QuantLib 1.43: the repository's
// makeswaption_oracle.json and test-suite/swaption.cpp testCachedValue.
// Run from sdk/go with the native library configured: go run ./examples/swaption.
package main

import (
	"fmt"
	"math"

	itofin "github.com/benbenbang/libitofin/sdk/go"
)

func must[T any](value T, err error) T {
	check(err)
	return value
}

func check(err error) {
	if err != nil {
		panic(err)
	}
}

func checkClose(label string, actual, expected float64) {
	if math.IsNaN(actual) || math.Abs(actual-expected) > 1e-12 {
		panic(fmt.Errorf("%s: got %.16g, expected %.16g", label, actual, expected))
	}
}

func vanillaSwaption(s *itofin.Session) {
	settings := must(s.NewSettings())
	today := must(itofin.NewDate(9, 10, 2015))
	check(settings.SetEvaluationDate(today))
	dc := must(s.Actual360())
	curve := must(s.NewFlatForward(today, .05, dc))
	index := must(s.NewSwapIndex(itofin.SwapIndexConfig{
		Family: "EuriborSwapIsdaFixA", Tenor: itofin.Period{Length: 5, Unit: itofin.Years},
		SettlementDays: 2, Currency: must(s.EUR()), Calendar: must(s.Target()),
		FixedLegTenor:      itofin.Period{Length: 1, Unit: itofin.Years},
		FixedLegConvention: itofin.ModifiedFollowing,
		FixedLegDayCounter: must(s.Thirty360BondBasis()),
		Index:              must(s.NewEuriborSixMonths(curve, settings)), Settings: settings,
	}))
	tenor := itofin.Period{Length: 1, Unit: itofin.Years}
	option := must(s.MakeSwaption(itofin.MakeSwaptionConfig{Index: index, OptionTenor: &tenor}))
	exercise := must(option.ExerciseDate())
	if exercise != must(itofin.NewDate(10, 10, 2016)) {
		panic(fmt.Errorf("unexpected vanilla exercise date: %s", exercise))
	}
	strike := must(option.UnderlyingFixedRate())
	checkClose("vanilla ATM strike", strike, .05202914613654939)
	engine := must(s.NewBlackSwaptionEngineFlat(itofin.RateEngineFlatVolConfig{
		Discount: curve, Volatility: must(s.NewSimpleQuote(.20)), DayCounter: dc, Settings: settings,
	}))
	value := must(option.Price(engine))
	fmt.Printf("Vanilla exercise=2016-10-10 ATM=%.12f Black NPV=%.12f\n", strike, value)
}

func overnightSwaption(s *itofin.Session) {
	settings := must(s.NewSettings())
	today := must(itofin.NewDate(13, 3, 2002))
	check(settings.SetEvaluationDate(today))
	calendar := must(s.Target())
	dc := must(s.Actual365Fixed())
	settlement := must(calendar.Advance(today, 2, itofin.Days, itofin.Following, false))
	discount := must(s.NewFlatForward(settlement, .05, dc))
	forward := must(s.NewFlatForward(settlement, .04, dc))
	index := must(s.NewEonia(forward, settings))
	exercise := must(calendar.Advance(settlement, 5, itofin.Years, itofin.Following, false))
	start := must(calendar.Advance(exercise, 2, itofin.Days, itofin.Following, false))
	rate := .06
	swap := must(s.MakeOis(itofin.MakeOisConfig{
		Tenor: itofin.Period{Length: 10, Unit: itofin.Years}, Index: index, Settings: settings,
		FixedRate: &rate, EffectiveDate: &start, FixedLegDayCounter: must(s.Thirty360BondBasis()),
	}))
	option := must(s.NewOisSwaption(itofin.OisSwaptionConfig{
		Swap: swap, Exercise: must(s.NewEuropeanExercise(exercise)), Settings: settings,
	}))
	vol := must(s.NewSimpleQuote(.20))
	engine := must(s.NewBlackSwaptionEngineFlat(itofin.RateEngineFlatVolConfig{
		Discount: discount, Volatility: vol, DayCounter: dc, Settings: settings,
	}))
	value := must(option.Price(engine))
	checkClose("Eonia OIS Black NPV", value, .014101075767)
	check(swap.Close())
	check(index.Close())
	check(engine.Close())
	check(vol.SetValue(.30))
	repriced := must(option.NPV())
	if !(repriced > value) {
		panic("the volatility increase did not increase the OIS swaption value")
	}
	fmt.Printf("Eonia OIS Black NPV=%.12f; volatility 20%% -> 30%%: %.12f\n", value, repriced)
}

func main() {
	session := must(itofin.NewSession())
	defer func() { check(session.Close()) }()
	vanillaSwaption(session)
	overnightSwaption(session)
}
