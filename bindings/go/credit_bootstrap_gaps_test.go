package itofin

import "testing"

func TestCreditBootstrapRepricesIndependentContracts(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	today := pricingMust(NewDate(9, 6, 2006))
	settings := pricingMust(s.NewSettings())
	pricingOK(t, settings.SetEvaluationDate(today))
	pricingOK(t, settings.SetIncludeTodaysCashFlows(pricingPtr(true)))
	cal := pricingMust(s.Target())
	dc := pricingMust(s.Thirty360BondBasis())
	discountDC := pricingMust(s.Actual360())
	discount := pricingMust(s.NewFlatForward(today, .06, discountDC))
	spreads := []float64{.005, .006, .007, .009}
	tenors := []int32{1, 2, 3, 5}
	finalDays := []int{20, 20, 22, 20}
	var quotes []*SimpleQuote
	var helpers []*DefaultProbabilityHelper
	for i, tenor := range tenors {
		quote := pricingMust(s.NewSimpleQuote(spreads[i]))
		quotes = append(quotes, quote)
		helper := pricingMust(s.NewSpreadCdsHelper(SpreadCdsHelperConfig{RunningSpread: quote, Tenor: Period{tenor, Years}, SettlementDays: 1, Calendar: cal, Frequency: Quarterly, PaymentConvention: Following, Rule: TwentiethIMM, DayCounter: dc, RecoveryRate: .4, DiscountCurve: discount, Settings: settings}))
		want := pricingMust(NewDate(finalDays[i], 6, 2006+int(tenor)))
		if pricingMust(helper.PillarDate()) != want || pricingMust(helper.LatestDate()) != want {
			t.Fatalf("%dY helper final payment date changed", tenor)
		}
		helpers = append(helpers, helper)
	}
	curve := pricingMust(s.NewPiecewiseDefaultCurve(today, helpers, dc))
	pricingOK(t, curve.Calculate())
	nodes := pricingMust(curve.Nodes())
	dates := pricingMust(curve.Dates())
	times := pricingMust(curve.Times())
	data := pricingMust(curve.Data())
	if len(nodes) != 5 || len(dates) != 5 || len(times) != 5 || len(data) != 5 || dates[0] != today || times[0] != 0 {
		t.Fatal("bootstrap node shape/reference changed")
	}
	for i, tenor := range tenors {
		want := pricingMust(NewDate(finalDays[i], 6, 2006+int(tenor)))
		if dates[i+1] != want || nodes[i+1].Date != want {
			t.Fatalf("%dY node date changed", tenor)
		}
		pricingNear(t, times[i+1], float64(tenor)+float64(finalDays[i]-9)/360, 1e-12)
		pricingNear(t, nodes[i+1].Time, times[i+1], 0)
		pricingNear(t, nodes[i+1].Rate, data[i+1], 0)
	}
	protection := pricingMust(today.AddDays(1))
	start := pricingMust(cal.Adjust(protection, Following))
	var contracts []*CreditDefaultSwap
	for i, tenor := range tenors {
		end := pricingMust(cal.Advance(today, tenor, Years, Unadjusted, false))
		schedule := pricingMust(s.NewSchedule(ScheduleConfig{Start: start, End: end, Frequency: Quarterly, Calendar: cal, Convention: Following, Rule: pricingPtr(TwentiethIMM), TerminationConvention: pricingPtr(Unadjusted)}))
		cds := pricingMust(s.NewCreditDefaultSwap(CdsConfig{Side: ProtectionBuyer, Notional: 1, Spread: spreads[i], Schedule: schedule, PaymentConvention: Following, DayCounter: dc, Settings: settings, ProtectionStart: &protection}))
		engine := pricingMust(s.NewMidPointCdsEngine(CdsEngineConfig{Probability: curve, Recovery: .4, Discount: discount, Settings: settings}))
		pricingMust(cds.Price(engine))
		pricingOK(t, cds.Calculate())
		pricingNear(t, pricingMust(cds.Notional()), 1, 0)
		pricingNear(t, pricingMust(cds.FairSpread()), spreads[i], 1e-6)
		contracts = append(contracts, cds)
	}
	spreads[3] = .012
	pricingOK(t, quotes[3].SetValue(spreads[3]))
	for _, helper := range helpers {
		pricingOK(t, helper.Close())
	}
	for _, quote := range quotes {
		pricingOK(t, quote.Close())
	}
	for i, cds := range contracts {
		pricingNear(t, pricingMust(cds.FairSpread()), spreads[i], 1e-6)
	}
	if data[4] == pricingMust(curve.Data())[4] {
		t.Fatal("quote update did not change terminal hazard")
	}
	dates[0] = Date{}
	times[0], data[0], nodes[0].Rate = -1, -1, -1
	if pricingMust(curve.Dates())[0] != today || pricingMust(curve.Times())[0] != 0 || pricingMust(curve.Data())[0] <= 0 || pricingMust(curve.Nodes())[0].Rate <= 0 {
		t.Fatal("returned arrays mutated native curve")
	}
}
