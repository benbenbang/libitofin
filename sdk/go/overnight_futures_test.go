package itofin

import (
	"math"
	"strconv"
	"testing"
)

func TestSofrFutureBootstrapAndCustomPillars(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	settings := pricingMust(s.NewSettings())
	today := testDate(t, 27, 6, 2024)
	pricingOK(t, settings.SetEvaluationDate(today))
	index := pricingMust(s.NewSofr(nil, settings))
	for _, day := range []int{18, 20, 21, 24, 25, 26, 27} {
		pricingOK(t, index.AddFixing(testDate(t, day, 6, 2024), .02))
	}
	futureNear(t, pricingMust(index.Fixing(testDate(t, 18, 6, 2024), false)), .02)
	var helpers []*RateHelper
	var quotes []*SimpleQuote
	for _, row := range futureRows(t, "curves") {
		if row[0] != "juneteenth" {
			continue
		}
		q := pricingMust(s.NewSimpleQuote(pricingMust(strconv.ParseFloat(row[4], 64))))
		quotes = append(quotes, q)
		helpers = append(helpers, pricingMust(s.NewSofrFutureRateHelper(SofrFutureHelperConfig{Price: q, ReferenceMonth: uint32(pricingMust(strconv.Atoi(row[2]))), ReferenceYear: int32(pricingMust(strconv.Atoi(row[1]))), ReferenceFrequency: Quarterly, Settings: settings})))
	}
	dc := pricingMust(s.Actual365Fixed())
	curve := pricingMust(s.NewPiecewiseLogLinearDiscount(PiecewiseCurveConfig{ReferenceDate: today, Helpers: helpers, DayCounter: dc}))
	forward := pricingMust(s.NewSofr(curve, settings))
	f := pricingMust(s.NewOvernightIndexFuture(OvernightFutureConfig{Index: forward, ValueDate: testDate(t, 19, 6, 2024), MaturityDate: testDate(t, 18, 9, 2024)}))
	futureNear(t, pricingMust(f.NPV()), 97.220)
	pricingOK(t, quotes[0].SetValue(97.1))
	futureNear(t, pricingMust(f.NPV()), 97.1)
	pricingOK(t, settings.SetEvaluationDate(testDate(t, 15, 3, 2024)))
	price := pricingMust(s.NewSimpleQuote(99))
	custom := testDate(t, 20, 4, 2024)
	choice := CustomDate
	cfg := OvernightFutureHelperConfig{OvernightFutureConfig: OvernightFutureConfig{Index: index, ValueDate: testDate(t, 20, 3, 2024), MaturityDate: testDate(t, 20, 6, 2024)}, Price: price, Pillar: &choice, CustomPillarDate: &custom}
	h := pricingMust(s.NewOvernightIndexFutureRateHelper(cfg))
	if pricingMust(h.PillarDate()) != custom {
		t.Fatal("custom pillar")
	}
	for _, invalid := range []*Date{nil, {}, ptrFutureDate(testDate(t, 1, 3, 2024)), ptrFutureDate(testDate(t, 20, 7, 2024))} {
		cfg.CustomPillarDate = invalid
		if _, err := s.NewOvernightIndexFutureRateHelper(cfg); err == nil {
			t.Fatal("invalid pillar accepted")
		}
	}
	for _, month := range []uint32{0, 13} {
		if _, err := s.NewSofrFutureRateHelper(SofrFutureHelperConfig{Price: price, ReferenceMonth: month, ReferenceYear: 2024, ReferenceFrequency: Quarterly, Settings: settings}); err == nil {
			t.Fatal("invalid month")
		}
	}
	if err := index.AddFixing(testDate(t, 18, 6, 2024), math.NaN()); err == nil {
		t.Fatal("NaN fixing accepted")
	}
	if err := index.AddFixing(testDate(t, 18, 6, 2024), .03); err == nil {
		t.Fatal("conflicting fixing accepted")
	}
	other := pricingMust(NewSession())
	defer other.Close()
	foreign := pricingMust(other.NewSimpleQuote(99))
	cfg.CustomPillarDate = &custom
	cfg.Price = foreign
	if _, err := s.NewOvernightIndexFutureRateHelper(cfg); err == nil {
		t.Fatal("foreign price accepted")
	}
	good := pricingMust(s.NewSofrFutureRateHelper(SofrFutureHelperConfig{Price: price, ReferenceMonth: 6, ReferenceYear: 2024, ReferenceFrequency: Quarterly, Settings: settings}))
	if pricingMust(good.PillarDate()) != testDate(t, 18, 9, 2024) {
		t.Fatal("session recovery")
	}
}
func ptrFutureDate(date Date) *Date { return &date }

func TestMonthlyAndGenericOvernightHelperRepricing(t *testing.T) {
	for _, generic := range []bool{false, true} {
		s := pricingMust(NewSession())
		defer s.Close()
		settings := pricingMust(s.NewSettings())
		today := testDate(t, 27, 6, 2024)
		pricingOK(t, settings.SetEvaluationDate(today))
		q := pricingMust(s.NewSimpleQuote(96.5))
		index := pricingMust(s.NewSofr(nil, settings))
		var helper *RateHelper
		simple := SimpleAveraging
		start := testDate(t, 1, 7, 2024)
		end := testDate(t, 1, 8, 2024)
		if generic {
			custom := testDate(t, 15, 7, 2024)
			pillar := CustomDate
			helper = pricingMust(s.NewOvernightIndexFutureRateHelper(OvernightFutureHelperConfig{OvernightFutureConfig: OvernightFutureConfig{Index: index, ValueDate: start, MaturityDate: end, AveragingMethod: &simple}, Price: q, Pillar: &pillar, CustomPillarDate: &custom}))
		} else {
			helper = pricingMust(s.NewSofrFutureRateHelper(SofrFutureHelperConfig{Price: q, ReferenceMonth: 7, ReferenceYear: 2024, ReferenceFrequency: Monthly, Settings: settings}))
		}
		dc := pricingMust(s.Actual365Fixed())
		curve := pricingMust(s.NewPiecewiseLogLinearDiscount(PiecewiseCurveConfig{ReferenceDate: today, Helpers: []*RateHelper{helper}, DayCounter: dc}))
		forward := pricingMust(s.NewSofr(curve, settings))
		f := pricingMust(s.NewOvernightIndexFuture(OvernightFutureConfig{Index: forward, ValueDate: start, MaturityDate: end, AveragingMethod: &simple}))
		futureNear(t, pricingMust(f.NPV()), 96.5)
		futureNear(t, pricingMust(helper.ImpliedQuote()), 96.5)
		for _, o := range []interface{ Close() error }{curve, index, helper, forward} {
			pricingOK(t, o.Close())
		}
		pricingOK(t, q.SetValue(97))
		futureNear(t, pricingMust(f.NPV()), 97)
	}
}
