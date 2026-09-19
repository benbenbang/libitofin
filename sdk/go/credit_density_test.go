package itofin

import (
	"math"
	"testing"
)

func TestDefaultDensityInterpolationAndBoundary(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	other := pricingMust(NewSession())
	defer other.Close()
	dc := pricingMust(s.Actual365Fixed())
	today := pricingMust(NewDate(1, 1, 2025))
	dates := []Date{today, pricingMust(today.AddDays(365)), pricingMust(today.AddDays(730))}
	for _, row := range []struct {
		interpolation     CreditDensityInterpolation
		survival, density float64
	}{{DensityBackwardFlat, .96, .04}, {DensityLinear, .9725, .03}} {
		cfg := DefaultDensityCurveConfig{Dates: dates, Densities: []float64{.01, .02, .04}, DayCounter: dc, Interpolation: row.interpolation}
		curve := pricingMust(s.NewInterpolatedDefaultDensityCurve(cfg))
		pricingNear(t, pricingMust(curve.SurvivalProbability(1.5, false)), row.survival, 1e-14)
		pricingNear(t, pricingMust(curve.DefaultDensity(1.5, false)), row.density, 1e-14)
		pricingNear(t, pricingMust(curve.HazardRate(1.5, false)), row.density/row.survival, 1e-14)
		if len(pricingMust(curve.Nodes())) != 3 || pricingMust(curve.DefaultDensities())[2] != .04 {
			t.Fatal("density nodes changed")
		}
		values := pricingMust(curve.Data())
		values[2] = 5
		pricingNear(t, pricingMust(curve.Data())[2], .04, 0)
		if _, err := other.NewInterpolatedDefaultDensityCurve(cfg); err == nil {
			t.Fatal("foreign day counter accepted")
		}
		for _, densities := range [][]float64{{.01}, {.01, -.02, .04}, {.01, math.NaN(), .04}} {
			cfg.Densities = densities
			if _, err := s.NewInterpolatedDefaultDensityCurve(cfg); err == nil {
				t.Fatal("invalid densities accepted")
			}
		}
		cfg.Densities = []float64{.01, .02, .04}
		cfg.Interpolation = 99
		if _, err := s.NewInterpolatedDefaultDensityCurve(cfg); err == nil {
			t.Fatal("unknown interpolation accepted")
		}
		pricingOK(t, curve.Close())
		if _, err := curve.DefaultDensity(1, false); err == nil {
			t.Fatal("closed curve accepted")
		}
	}
}

func TestDefaultDensityBootstrapLiveHelpers(t *testing.T) {
	for _, interpolation := range []CreditDensityInterpolation{DensityBackwardFlat, DensityLinear} {
		s := pricingMust(NewSession())
		today := pricingMust(NewDate(9, 6, 2006))
		settings := pricingMust(s.NewSettings())
		pricingOK(t, settings.SetEvaluationDate(today))
		cal := pricingMust(s.Target())
		dc := pricingMust(s.Actual365Fixed())
		discount := pricingMust(s.NewFlatForward(today, .06, dc))
		var helpers []*DefaultProbabilityHelper
		var quotes []*SimpleQuote
		spreads := []float64{.005, .006, .007}
		for i, spread := range spreads {
			quote := pricingMust(s.NewSimpleQuote(spread))
			quotes = append(quotes, quote)
			helper := pricingMust(s.NewSpreadCdsHelper(SpreadCdsHelperConfig{RunningSpread: quote, Tenor: Period{int32(i + 1), Years}, SettlementDays: 1, Calendar: cal, Frequency: Quarterly, PaymentConvention: Following, Rule: TwentiethIMM, DayCounter: dc, RecoveryRate: .4, DiscountCurve: discount, Settings: settings}))
			helpers = append(helpers, helper)
		}
		curve := pricingMust(s.NewPiecewiseDefaultDensityCurve(today, helpers, dc, interpolation))
		pricingOK(t, curve.Calculate())
		before := pricingMust(curve.DefaultDensities())
		if len(before) != 4 || pricingMust(curve.Dates())[0] != today {
			t.Fatal("bootstrap nodes changed")
		}
		protection := pricingMust(today.AddDays(1))
		start := pricingMust(cal.Adjust(protection, Following))
		engine := pricingMust(s.NewMidPointCdsEngine(CdsEngineConfig{Probability: curve, Recovery: .4, Discount: discount, Settings: settings}))
		var contracts []*CreditDefaultSwap
		for i, spread := range spreads {
			end := pricingMust(cal.Advance(today, int32(i+1), Years, Unadjusted, false))
			schedule := pricingMust(s.NewSchedule(ScheduleConfig{Start: start, End: end, Frequency: Quarterly, Calendar: cal, Convention: Following, Rule: pricingPtr(TwentiethIMM), TerminationConvention: pricingPtr(Unadjusted)}))
			cds := pricingMust(s.NewCreditDefaultSwap(CdsConfig{Side: ProtectionBuyer, Notional: 1, Spread: spread, Schedule: schedule, PaymentConvention: Following, DayCounter: dc, Settings: settings, ProtectionStart: &protection}))
			pricingMust(cds.Price(engine))
			pricingNear(t, pricingMust(cds.FairSpread()), spread, 1e-10)
			contracts = append(contracts, cds)
		}
		pricingOK(t, quotes[2].SetValue(.008))
		pricingOK(t, curve.Calculate())
		pricingNear(t, pricingMust(contracts[2].FairSpread()), .008, 1e-10)
		for _, helper := range helpers {
			pricingOK(t, helper.Close())
		}
		for _, quote := range quotes {
			pricingOK(t, quote.Close())
		}
		pricingOK(t, discount.Close())
		pricingOK(t, dc.Close())
		if pricingMust(curve.DefaultDensities())[3] == before[3] {
			t.Fatal("quote update did not rebuild density")
		}
		if pricingMust(curve.SurvivalProbability(2, false)) <= 0 {
			t.Fatal("retained curve lost dependencies")
		}
		pricingOK(t, s.Close())
	}
}
