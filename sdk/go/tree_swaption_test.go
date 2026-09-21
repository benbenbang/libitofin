package itofin

import (
	"encoding/json"
	"math"
	"os"
	"testing"
)

func treeSwaptionNear(t *testing.T, value, want float64) {
	t.Helper()
	if math.IsNaN(value) || math.IsInf(value, 0) || math.Abs(value-want) > 1e-4 {
		t.Fatalf("tree NPV %.12g, QuantLib %.12g", value, want)
	}
}

func TestTreeSwaptionQuantLibRetentionAndUpdates(t *testing.T) {
	var oracle struct {
		Quantlib string
		Rows     []struct {
			AtPar                       bool `json:"at_par"`
			Multiplier, Initial, Bumped float64
		}
	}
	ratesOK(t, json.Unmarshal(pricingMust(os.ReadFile("testdata/tree_swaption_oracle.json")), &oracle))
	if oracle.Quantlib != "1.43" || len(oracle.Rows) != 6 {
		t.Fatal("invalid oracle")
	}

	for _, mode := range []struct {
		name   string
		par    bool
		values [3]float64
	}{
		{"indexed", false, [3]float64{42.2402, 12.9032, 2.49758}},
		{"par", true, [3]float64{42.2460, 12.9069, 2.4985}},
	} {
		t.Run(mode.name, func(t *testing.T) {
			s := pricingMust(NewSession())
			defer s.Close()
			settings := pricingMust(s.NewSettings())
			today := testDate(t, 15, 2, 2002)
			ratesOK(t, settings.SetEvaluationDate(today))
			if !pricingMust(settings.UsingAtParCoupons()) {
				t.Fatal("default changed")
			}
			ratesOK(t, settings.SetUsingAtParCoupons(mode.par))
			if pricingMust(settings.UsingAtParCoupons()) != mode.par {
				t.Fatal("mode lost")
			}
			cal := pricingMust(s.Target())
			dc := pricingMust(s.Actual365Fixed())
			fixedDC := pricingMust(s.Thirty360BondBasis())
			floatDC := pricingMust(s.Actual360())
			settlement := pricingMust(cal.Advance(today, 2, Days, Following, false))
			quote := pricingMust(s.NewSimpleQuote(.04875825))
			curve := pricingMust(s.NewFlatForwardFromQuote(settlement, quote, dc))
			index := pricingMust(s.NewEuriborSixMonths(curve, settings))
			start := pricingMust(cal.Advance(settlement, 1, Years, Following, false))
			end := pricingMust(cal.Advance(start, 5, Years, Following, false))
			fixed := pricingMust(s.NewSchedule(ScheduleConfig{Start: start, End: end, Frequency: Annual, Calendar: cal, Convention: Unadjusted}))
			floating := pricingMust(s.NewSchedule(ScheduleConfig{Start: start, End: end, Frequency: Semiannual, Calendar: cal, Convention: ModifiedFollowing}))
			cfg := VanillaSwapConfig{Type: SwapPayer, Nominal: 1000, FixedSchedule: fixed, FloatingSchedule: floating, FixedDayCounter: fixedDC, FloatingDayCounter: floatDC, Index: index, Settings: settings}
			parSwap := pricingMust(s.NewVanillaSwap(cfg))
			pricingMust(parSwap.Price(curve, settings))
			rate := pricingMust(parSwap.FairRate())
			dates := pricingMust(fixed.Dates())
			exercise := pricingMust(s.NewBermudanExercise(dates[:len(dates)-1]))
			copied := pricingMust(exercise.Dates())
			if len(copied) != 5 || copied[0] != start {
				t.Fatal(copied)
			}
			copied[0] = today
			if pricingMust(exercise.Dates())[0] != start {
				t.Fatal("date copy aliased")
			}
			model := pricingMust(s.NewHullWhite(curve, .048696, .0058904))
			if _, err := s.NewTreeSwaptionEngine(model, 0, settings); err == nil {
				t.Fatal("zero steps accepted")
			}
			engine := pricingMust(s.NewTreeSwaptionEngine(model, 50, settings))
			var options []*Swaption
			for i, multiplier := range []float64{.8, 1, 1.2} {
				cfg.FixedRate = rate * multiplier
				swap := pricingMust(s.NewVanillaSwap(cfg))
				before := pricingMust(swap.Price(curve, settings))
				option := pricingMust(s.NewBermudanSwaption(BermudanSwaptionConfig{Swap: swap, Exercise: exercise, Settings: settings}))
				ratesOK(t, option.SetTreeEngine(engine))
				if i != 1 {
					treeSwaptionNear(t, pricingMust(option.NPV()), mode.values[i])
					if pricingMust(swap.NPV()) != before {
						t.Fatal("source swap changed")
					}
				}
				ratesOK(t, swap.Close())
				options = append(options, option)
			}
			for _, source := range []object{settings.object, cal.object, dc.object, fixedDC.object, floatDC.object, curve.object, index.object, fixed.object, floating.object, parSwap.object, exercise.object, model.object, engine.object} {
				ratesOK(t, source.Close())
			}
			for i, option := range options {
				treeSwaptionNear(t, pricingMust(option.NPV()), mode.values[i])
			}
			ratesOK(t, quote.SetValue(.06))
			for i, option := range options {
				rowIndex := i
				if mode.par {
					rowIndex += 3
				}
				row := oracle.Rows[rowIndex]
				if row.AtPar != mode.par || row.Multiplier != []float64{.8, 1, 1.2}[i] {
					t.Fatal("misordered oracle")
				}
				treeSwaptionNear(t, pricingMust(option.NPV()), row.Bumped)
			}
			ratesOK(t, quote.SetValue(.04875825))
			ratesOK(t, quote.Close())
			treeSwaptionNear(t, pricingMust(options[1].NPV()), mode.values[1])
		})
	}
}

func TestTreeSwaptionErrorsRecover(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	if _, err := s.NewBermudanExercise(nil); err == nil {
		t.Fatal("empty exercise accepted")
	}
	early, late := testDate(t, 15, 2, 2003), testDate(t, 15, 2, 2004)
	exercise := pricingMust(s.NewBermudanExercise([]Date{late, early, early}))
	dates := pricingMust(exercise.Dates())
	if len(dates) != 3 || dates[0] != early || dates[2] != late {
		t.Fatal(dates)
	}
	if _, err := s.NewBermudanSwaption(BermudanSwaptionConfig{}); err == nil {
		t.Fatal("missing inputs accepted")
	}
	settings := pricingMust(s.NewSettings())
	today := testDate(t, 15, 2, 2002)
	ratesOK(t, settings.SetEvaluationDate(today))
	dc := pricingMust(s.Actual365Fixed())
	curve := pricingMust(s.NewFlatForward(today, .05, dc))
	model := pricingMust(s.NewHullWhite(curve, .048696, .0058904))
	other := pricingMust(NewSession())
	defer other.Close()
	foreign := pricingMust(other.NewSettings())
	if _, err := s.NewTreeSwaptionEngine(model, 50, foreign); err == nil {
		t.Fatal("foreign settings accepted")
	}
	engine := pricingMust(s.NewTreeSwaptionEngine(model, 50, settings))
	overnight := pricingMust(s.NewEstr(curve, settings))
	ois := pricingMust(s.MakeOis(MakeOisConfig{Tenor: Period{5, Years}, Index: overnight, Settings: settings, EffectiveDate: &early}))
	european := pricingMust(s.NewEuropeanExercise(early))
	unsupported := pricingMust(s.NewOisSwaption(OisSwaptionConfig{Swap: ois, Exercise: european, Settings: settings}))
	ratesOK(t, unsupported.SetTreeEngine(engine))
	if _, err := unsupported.NPV(); err == nil {
		t.Fatal("tree OIS accepted")
	}
	index := pricingMust(s.NewEuriborSixMonths(curve, settings))
	rate := .05
	swap := pricingMust(s.MakeVanillaSwap(MakeVanillaSwapConfig{Tenor: Period{5, Years}, Index: index, Settings: settings, FixedRate: &rate, EffectiveDate: &early}))
	config := BermudanSwaptionConfig{Swap: swap, Exercise: exercise, Settings: settings, SettlementType: SettlementCash, SettlementMethod: ParYieldCurve}
	cash := pricingMust(s.NewBermudanSwaption(config))
	ratesOK(t, cash.SetTreeEngine(engine))
	if _, err := cash.NPV(); err == nil {
		t.Fatal("par yield cash accepted")
	}
	config.SettlementType, config.SettlementMethod = SettlementPhysical, PhysicalOTC
	valid := pricingMust(s.NewBermudanSwaption(config))
	ratesOK(t, valid.SetTreeEngine(engine))
	value := pricingMust(valid.NPV())
	if math.IsNaN(value) || math.IsInf(value, 0) || value <= 0 {
		t.Fatal(value)
	}
}
