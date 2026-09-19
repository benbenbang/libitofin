package itofin

import (
	"encoding/csv"
	"os"
	"strconv"
	"testing"
)

func TestIsdaCreditHelpersOracleAndRetention(t *testing.T) {
	file := pricingMust(os.Open("testdata/isda_helpers.csv"))
	defer file.Close()
	rows := pricingMust(csv.NewReader(file).ReadAll())[1:]
	for _, kind := range []string{"spread", "upfront"} {
		t.Run(kind, func(t *testing.T) {
			s := pricingMust(NewSession())
			defer s.Close()
			today := pricingMust(NewDate(15, 6, 2026))
			settings := pricingMust(s.NewSettings())
			pricingOK(t, settings.SetEvaluationDate(today))
			pricingOK(t, settings.SetIncludeTodaysCashFlows(pricingPtr(false)))
			cal := pricingMust(s.Target())
			dc := pricingMust(s.Actual360())
			curveDC := pricingMust(s.Actual365Fixed())
			var quotes []*SimpleQuote
			for _, value := range map[string][]float64{"spread": {.005, .01, .015}, "upfront": {.01, .02, .04}}[kind] {
				quotes = append(quotes, pricingMust(s.NewSimpleQuote(value)))
			}
			var helpers []*DefaultProbabilityHelper
			var curve *DefaultProbabilityTermStructure
			for stage := 0; stage < 3; stage++ {
				if stage != 1 {
					discount := pricingMust(s.NewFlatForward(today, .03+float64(stage)*.005, curveDC))
					helpers = nil
					for i, years := range []int32{1, 3, 5} {
						terms := CdsHelperTerms{Model: Isda}
						base := SpreadCdsHelperConfig{RunningSpread: quotes[i], Tenor: Period{years, Years}, SettlementDays: 1, Calendar: cal, Frequency: Quarterly, PaymentConvention: Following, Rule: CDS, DayCounter: dc, RecoveryRate: .4, DiscountCurve: discount, Settings: settings}
						var helper *DefaultProbabilityHelper
						if kind == "spread" {
							helper = pricingMust(s.NewSpreadCdsHelperWithTerms(base, terms))
						} else {
							helper = pricingMust(s.NewUpfrontCdsHelper(UpfrontCdsHelperConfig{Upfront: quotes[i], RunningSpread: .01, Tenor: base.Tenor, SettlementDays: 1, Calendar: cal, Frequency: Quarterly, PaymentConvention: Following, Rule: CDS, DayCounter: dc, RecoveryRate: .4, DiscountCurve: discount, Settings: settings, Terms: terms}))
						}
						helpers = append(helpers, helper)
					}
					curve = pricingMust(s.NewPiecewiseDefaultCurve(today, helpers, curveDC))
					pricingOK(t, discount.Close())
				} else {
					value := .012
					if kind == "upfront" {
						value = .022
					}
					pricingOK(t, quotes[1].SetValue(value))
				}
				pricingOK(t, curve.Calculate())
				for _, row := range rows {
					if row[0] != kind || row[1] != strconv.Itoa(stage) {
						continue
					}
					i := map[string]int{"1": 0, "3": 1, "5": 2}[row[2]]
					pillar := pricingMust(DateFromSerial(int32(pricingMust(strconv.Atoi(row[3])))))
					if pricingMust(helpers[i].PillarDate()) != pillar {
						t.Fatal("ISDA pillar extension lost")
					}
					pricingNear(t, pricingMust(curve.SurvivalProbabilityDate(pillar, false)), pricingMust(strconv.ParseFloat(row[4], 64)), 1e-10)
					pricingNear(t, pricingMust(helpers[i].ImpliedQuote()), pricingMust(strconv.ParseFloat(row[5], 64)), 1e-10)
				}
				if flag := pricingMust(settings.IncludeTodaysCashFlows()); flag == nil || *flag {
					t.Fatal("helper changed caller settings")
				}
			}
			before := pricingMust(curve.Data())
			pricingOK(t, quotes[1].SetValue(.024))
			restored := .012
			if kind == "upfront" {
				restored = .022
			}
			pricingOK(t, quotes[1].SetValue(restored))
			for _, helper := range helpers {
				pricingOK(t, helper.Close())
			}
			for _, quote := range quotes {
				pricingOK(t, quote.Close())
			}
			pricingOK(t, settings.Close())
			pricingOK(t, dc.Close())
			pricingOK(t, curveDC.Close())
			pricingOK(t, cal.Close())
			pricingOK(t, curve.Calculate())
			for i, value := range pricingMust(curve.Data()) {
				pricingNear(t, value, before[i], 1e-10)
			}
		})
	}
}

func TestCreditHelperExplicitTermsErrors(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	today := pricingMust(NewDate(15, 6, 2026))
	settings := pricingMust(s.NewSettings())
	pricingOK(t, settings.SetEvaluationDate(today))
	dc := pricingMust(s.Actual365Fixed())
	base := SpreadCdsHelperConfig{
		RunningSpread: pricingMust(s.NewSimpleQuote(.01)), Tenor: Period{1, Years},
		SettlementDays: 1, Calendar: pricingMust(s.Target()), Frequency: Quarterly,
		PaymentConvention: Following, Rule: CDS, DayCounter: pricingMust(s.Actual360()),
		RecoveryRate: .4, DiscountCurve: pricingMust(s.NewFlatForward(today, .03, dc)), Settings: settings,
	}
	for _, model := range []PricingModel{-1, 2} {
		if _, err := s.NewSpreadCdsHelperWithTerms(base, CdsHelperTerms{Model: model}); err == nil {
			t.Fatal("invalid model accepted")
		}
	}
	other := pricingMust(NewSession())
	defer other.Close()
	foreign := pricingMust(other.Actual360())
	if _, err := s.NewSpreadCdsHelperWithTerms(base, CdsHelperTerms{LastPeriodDayCounter: foreign}); err == nil {
		t.Fatal("foreign day counter accepted")
	}
	helper := pricingMust(s.NewSpreadCdsHelperWithTerms(base, CdsHelperTerms{Model: Isda}))
	if _, err := helper.ImpliedQuote(); err == nil {
		t.Fatal("missing probability curve accepted")
	}
	pricingOK(t, helper.Close())
	if _, err := helper.ImpliedQuote(); err == nil {
		t.Fatal("closed helper accepted")
	}
	if _, err := s.NewUpfrontCdsHelper(UpfrontCdsHelperConfig{}); err == nil {
		t.Fatal("missing upfront dependencies accepted")
	}
	var absent *DefaultProbabilityHelper
	if _, err := absent.ImpliedQuote(); err == nil {
		t.Fatal("nil helper accepted")
	}
}
