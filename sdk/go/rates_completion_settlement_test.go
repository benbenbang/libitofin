package itofin

import (
	"encoding/json"
	"os"
	"strings"
	"testing"
)

type ratesCompletionOracle struct {
	Quantlib string `json:"quantlib"`
	Payer    struct {
		NPV      float64 `json:"npv"`
		FairRate float64 `json:"fair_rate"`
	} `json:"payer"`
	Receiver struct {
		NPV      float64 `json:"npv"`
		FairRate float64 `json:"fair_rate"`
	} `json:"receiver"`
	Swaptions []struct {
		Side, Settlement, Annuity string
		NPV                       float64
	} `json:"swaptions"`
	OIS []struct {
		Averaging, Maturity, Pillar string
		Discount, Quote             float64
	} `json:"ois"`
}

func loadRatesCompletionOracle(t *testing.T) ratesCompletionOracle {
	t.Helper()
	data, err := os.ReadFile("testdata/rates_completion_oracle.json")
	ratesOK(t, err)
	var oracle ratesCompletionOracle
	ratesOK(t, json.Unmarshal(data, &oracle))
	if oracle.Quantlib != "1.43" || len(oracle.Swaptions) != 12 || len(oracle.OIS) != 2 {
		t.Fatal("invalid independent oracle")
	}
	return oracle
}

func TestRatesCompletionSwaptionSettlementAndReceiver(t *testing.T) {
	oracle := loadRatesCompletionOracle(t)
	s := pricingMust(NewSession())
	defer s.Close()
	today := testDate(t, 7, 7, 2026)
	settings := pricingMust(s.NewSettings())
	ratesOK(t, settings.SetEvaluationDate(today))
	cal := pricingMust(s.Target())
	dc := pricingMust(s.Actual365Fixed())
	floatDC := pricingMust(s.Actual360())
	curve := pricingMust(s.NewFlatForward(today, .04, dc))
	index := pricingMust(s.NewEuriborSixMonths(curve, settings))
	exerciseDate := pricingMust(cal.Advance(today, 1, Years, Following, false))
	start := pricingMust(cal.Advance(exerciseDate, 2, Days, Following, false))
	end := pricingMust(cal.Advance(start, 3, Years, Following, false))
	rule := Backward
	fixed := pricingMust(s.NewSchedule(ScheduleConfig{Start: start, End: end, Frequency: Annual, Calendar: cal, Convention: ModifiedFollowing, Rule: &rule}))
	floating := pricingMust(s.NewSchedule(ScheduleConfig{Start: start, End: end, Frequency: Semiannual, Calendar: cal, Convention: ModifiedFollowing, Rule: &rule}))
	cfg := VanillaSwapConfig{Nominal: 1, FixedRate: .04, FixedSchedule: fixed, FloatingSchedule: floating, FixedDayCounter: dc, FloatingDayCounter: floatDC, Index: index, Settings: settings}
	swaps := map[string]*VanillaSwap{}
	for _, side := range []struct {
		name      string
		kind      SwapType
		npv, fair float64
	}{
		{"payer", SwapPayer, oracle.Payer.NPV, oracle.Payer.FairRate},
		{"receiver", SwapReceiver, oracle.Receiver.NPV, oracle.Receiver.FairRate},
	} {
		cfg.Type = side.kind
		swap := pricingMust(s.NewVanillaSwap(cfg))
		swaps[side.name] = swap
		curveNear(t, pricingMust(swap.Price(curve, settings)), side.npv, 1e-12)
		curveNear(t, pricingMust(swap.FairRate()), side.fair, 1e-12)
		parCfg := cfg
		parCfg.FixedRate = side.fair
		par := pricingMust(s.NewVanillaSwap(parCfg))
		curveNear(t, pricingMust(par.Price(curve, settings)), 0, 1e-12)
	}
	curveNear(t, pricingMust(swaps["payer"].NPV()), -pricingMust(swaps["receiver"].NPV()), 1e-12)
	exercise := pricingMust(s.NewEuropeanExercise(exerciseDate))
	vol := pricingMust(s.NewSimpleQuote(.20))
	for _, row := range oracle.Swaptions {
		t.Run(row.Side+"/"+row.Settlement+"/"+row.Annuity, func(t *testing.T) {
			settlementType, method := SettlementCash, ParYieldCurve
			switch row.Settlement {
			case "physical_cleared":
				settlementType, method = SettlementPhysical, PhysicalCleared
			case "collateralized_cash":
				method = CollateralizedCashPrice
			case "par_yield":
			default:
				t.Fatal("unknown settlement oracle")
			}
			model := AnnuitySwapRate
			if row.Annuity == "discount_curve" {
				model = AnnuityDiscountCurve
			}
			option := pricingMust(s.NewSwaption(SwaptionConfig{Swap: swaps[row.Side], Exercise: exercise, SettlementType: settlementType, SettlementMethod: method, Settings: settings}))
			engine := pricingMust(s.NewBlackSwaptionEngineFlat(RateEngineFlatVolConfig{Discount: curve, Volatility: vol, DayCounter: dc, Settings: settings, AnnuityModel: model}))
			curveNear(t, pricingMust(option.Price(engine)), row.NPV, 1e-12)
		})
	}
	for _, pair := range []struct {
		kind   SettlementType
		method SettlementMethod
	}{
		{SettlementPhysical, CollateralizedCashPrice}, {SettlementPhysical, ParYieldCurve},
		{SettlementCash, PhysicalOTC}, {SettlementCash, PhysicalCleared},
	} {
		invalid := pricingMust(s.NewSwaption(SwaptionConfig{Swap: swaps["payer"], Exercise: exercise, SettlementType: pair.kind, SettlementMethod: pair.method, Settings: settings}))
		engine := pricingMust(s.NewBlackSwaptionEngineFlat(RateEngineFlatVolConfig{Discount: curve, Volatility: vol, DayCounter: dc, Settings: settings}))
		_, err := invalid.Price(engine)
		if err == nil || !strings.Contains(err.Error(), "invalid settlement method") {
			t.Fatalf("inconsistent settlement %+v: %v", pair, err)
		}
	}
	option := pricingMust(s.NewSwaption(SwaptionConfig{Swap: swaps["payer"], Exercise: exercise, SettlementType: SettlementCash, SettlementMethod: ParYieldCurve, Settings: settings}))
	model := pricingMust(s.NewHullWhite(curve, .03, .01))
	ratesOK(t, option.SetJamshidianEngine(model))
	_, err := option.NPV()
	if err == nil || !strings.Contains(err.Error(), "ParYieldCurve") {
		t.Fatalf("unsupported Jamshidian settlement: %v", err)
	}
}
