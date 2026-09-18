package itofin

import (
	"fmt"
	"math"
	"testing"
)

func TestRatesCompletionOISAveragingOracle(t *testing.T) {
	oracle := loadRatesCompletionOracle(t)
	for _, row := range oracle.OIS {
		t.Run(row.Averaging, func(t *testing.T) {
			s := pricingMust(NewSession())
			defer s.Close()
			today := testDate(t, 7, 7, 2026)
			settings := pricingMust(s.NewSettings())
			ratesOK(t, settings.SetEvaluationDate(today))
			dc := pricingMust(s.Actual360())
			overnight := pricingMust(s.NewEstr(nil, settings))
			quote := pricingMust(s.NewSimpleQuote(.05))
			cfg := DefaultOISRateHelperConfig()
			cfg.SettlementDays, cfg.Tenor = 2, Period{1, Years}
			cfg.Quote, cfg.OvernightIndex, cfg.Settings = quote, overnight, settings
			cfg.PaymentLag, cfg.PaymentConvention, cfg.PaymentFrequency = 2, Following, Annual
			cfg.AveragingMethod = CompoundAveraging
			if row.Averaging == "simple" {
				cfg.AveragingMethod = SimpleAveraging
			}
			helper := pricingMust(s.NewOISRateHelper(cfg))
			curve := pricingMust(s.NewPiecewiseLogLinearDiscount(PiecewiseCurveConfig{ReferenceDate: today, Helpers: []*RateHelper{helper}, DayCounter: dc}))
			maturity := pricingMust(helper.MaturityDate())
			pillar := pricingMust(helper.PillarDate())
			if fmt.Sprintf("%04d-%02d-%02d", maturity.Year(), maturity.Month(), maturity.Day()) != row.Maturity || fmt.Sprintf("%04d-%02d-%02d", pillar.Year(), pillar.Month(), pillar.Day()) != row.Pillar {
				t.Fatal("OIS payment and maturity dates changed")
			}
			curveNear(t, pricingMust(curve.DiscountDate(maturity, false)), row.Discount, 1e-12)
			curveNear(t, pricingMust(helper.ImpliedQuote()), row.Quote, 1e-9)
		})
	}
}

func TestRatesCompletionDirtyBondWithAccruedInterest(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	today, issue, end, payment := testDate(t, 7, 7, 2026), testDate(t, 9, 1, 2026), testDate(t, 9, 1, 2027), testDate(t, 11, 1, 2027)
	settings := pricingMust(s.NewSettings())
	ratesOK(t, settings.SetEvaluationDate(today))
	dc := pricingMust(s.Actual360())
	cal := pricingMust(s.Target())
	rule, term := Backward, Unadjusted
	schedule := pricingMust(s.NewSchedule(ScheduleConfig{Start: issue, End: end, Frequency: Annual, Calendar: cal, Convention: Unadjusted, Rule: &rule, TerminationConvention: &term}))
	accrued := 5 * float64(today.Serial()-issue.Serial()) / 360
	discount := math.Exp(-.04 * float64(payment.Serial()-today.Serial()) / 360)
	dirty := (100 + 5*float64(end.Serial()-issue.Serial())/360) * discount
	if accrued <= 0 {
		t.Fatal("fixture must have accrued interest")
	}
	for _, row := range []struct {
		name  string
		kind  BondPriceType
		price float64
	}{
		{"dirty", Dirty, dirty}, {"clean", Clean, dirty - accrued},
	} {
		t.Run(row.name, func(t *testing.T) {
			quote := pricingMust(s.NewSimpleQuote(row.price))
			helper := pricingMust(s.NewFixedRateBondHelper(FixedRateBondHelperConfig{Price: quote, SettlementDays: 0, FaceAmount: 250, Schedule: schedule, Coupons: []float64{.05}, DayCounter: dc, PaymentConvention: Following, Redemption: 100, PriceType: row.kind, Settings: settings, IssueDate: &issue}))
			curve := pricingMust(s.NewPiecewiseLogLinearDiscount(PiecewiseCurveConfig{ReferenceDate: today, Helpers: []*RateHelper{helper}, DayCounter: dc}))
			curveNear(t, pricingMust(curve.DiscountDate(payment, false)), discount, 1e-12)
			curveNear(t, pricingMust(helper.ImpliedQuote()), row.price, 1e-9)
		})
	}
}

func TestRatesCompletionCustomAndASXFutures(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	dc := pricingMust(s.Actual360())
	today := testDate(t, 8, 6, 2026)
	customEnd := testDate(t, 23, 10, 2026)
	asxEnd := testDate(t, 11, 9, 2026)
	for _, row := range []struct {
		name            string
		kind            FuturesType
		start, maturity Date
		end             *Date
	}{
		{"custom", Custom, testDate(t, 23, 6, 2026), customEnd, &customEnd},
		{"asx_default", Asx, testDate(t, 12, 6, 2026), asxEnd, nil},
		{"asx_explicit", Asx, testDate(t, 12, 6, 2026), asxEnd, &asxEnd},
	} {
		t.Run(row.name, func(t *testing.T) {
			quote := pricingMust(s.NewSimpleQuote(96))
			convexity := pricingMust(s.NewSimpleQuote(.001))
			helper := pricingMust(s.NewFuturesRateHelperFromEndDate(FuturesRateHelperConfig{Price: quote, IborStartDate: row.start, IborEndDate: row.end, DayCounter: dc, ConvexityAdjustment: convexity, FuturesType: row.kind}))
			if pricingMust(helper.MaturityDate()) != row.maturity || pricingMust(helper.EarliestDate()) != row.start {
				t.Fatal("futures dates changed")
			}
			curve := pricingMust(s.NewPiecewiseLogLinearDiscount(PiecewiseCurveConfig{ReferenceDate: today, Helpers: []*RateHelper{helper}, DayCounter: dc}))
			tau := float64(row.maturity.Serial()-row.start.Serial()) / 360
			forward := (pricingMust(curve.DiscountDate(row.start, false))/pricingMust(curve.DiscountDate(row.maturity, false)) - 1) / tau
			curveNear(t, forward, .039, 1e-12)
			curveNear(t, pricingMust(helper.ImpliedQuote()), 96, 1e-9)
			flatRate := math.Log1p(.039*tau) / tau
			curveNear(t, pricingMust(curve.DiscountDate(row.maturity, false)), math.Exp(-flatRate*float64(row.maturity.Serial()-today.Serial())/360), 1e-12)
		})
	}
}
