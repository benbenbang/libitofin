package itofin

import (
	"encoding/json"
	"errors"
	"fmt"
	"math"
	"os"
	"testing"
	"time"
)

type overnightOISOracle struct {
	Name   string `json:"name"`
	Inputs struct {
		EvaluationDate     string   `json:"evaluation_date"`
		ReferenceDate      string   `json:"reference_date"`
		Quote              float64  `json:"quote"`
		OvernightSpread    float64  `json:"overnight_spread"`
		PaymentLag         int32    `json:"payment_lag"`
		TenorYears         int32    `json:"tenor_years"`
		PaymentFrequency   string   `json:"payment_frequency"`
		ForwardStartMonths int32    `json:"forward_start_months"`
		DiscountRate       *float64 `json:"discount_rate"`
	} `json:"inputs"`
	Results map[string]overnightOISResult `json:"results"`
}

type overnightOISResult struct {
	Maturity, Pillar   string
	Discount, Quote    float64
	EndogenousDiscount float64 `json:"endogenous_discount"`
}

func overnightOISCases(t *testing.T) map[string]overnightOISOracle {
	t.Helper()
	data := pricingMust(os.ReadFile("testdata/overnight_averaging_oracle.json"))
	var oracle struct {
		Quantlib string               `json:"quantlib"`
		OIS      []overnightOISOracle `json:"ois_cases"`
	}
	ratesOK(t, json.Unmarshal(data, &oracle))
	if oracle.Quantlib != "1.43" {
		t.Fatalf("unexpected QuantLib version %q", oracle.Quantlib)
	}
	cases := make(map[string]overnightOISOracle, len(oracle.OIS))
	for _, row := range oracle.OIS {
		if len(row.Results) != 2 {
			t.Fatalf("missing averaging oracle for %s", row.Name)
		}
		cases[row.Name] = row
	}
	for _, name := range []string{"baseline", "quote_updated", "date_updated", "forward_start", "external_discount"} {
		if _, ok := cases[name]; !ok {
			t.Fatalf("missing OIS oracle %s", name)
		}
	}
	return cases
}

func overnightDate(t *testing.T, value string) Date {
	t.Helper()
	date := pricingMust(time.Parse("2006-01-02", value))
	return testDate(t, date.Day(), int(date.Month()), date.Year())
}

func overnightOISConfig(t *testing.T, s *Session, row overnightOISOracle, averaging RateAveraging) OISRateHelperConfig {
	t.Helper()
	settings := pricingMust(s.NewSettings())
	ratesOK(t, settings.SetEvaluationDate(overnightDate(t, row.Inputs.EvaluationDate)))
	cfg := DefaultOISRateHelperConfig()
	cfg.SettlementDays, cfg.Tenor = 2, Period{1, Years}
	if row.Inputs.TenorYears != 0 {
		cfg.Tenor.Length = row.Inputs.TenorYears
	}
	cfg.Quote = pricingMust(s.NewSimpleQuote(row.Inputs.Quote))
	cfg.OvernightIndex = pricingMust(s.NewEstr(nil, settings))
	cfg.Settings = settings
	cfg.PaymentLag, cfg.PaymentConvention, cfg.PaymentFrequency = row.Inputs.PaymentLag, Following, Annual
	if row.Inputs.PaymentFrequency == "semiannual" {
		cfg.PaymentFrequency = Semiannual
	}
	cfg.ForwardStart = Period{row.Inputs.ForwardStartMonths, Months}
	cfg.AveragingMethod = averaging
	if row.Inputs.DiscountRate != nil {
		dc := pricingMust(s.Actual365Fixed())
		cfg.DiscountingCurve = pricingMust(s.NewFlatForward(overnightDate(t, row.Inputs.ReferenceDate), *row.Inputs.DiscountRate, dc))
		ratesOK(t, dc.Close())
	}
	return cfg
}

func checkOvernightOIS(t *testing.T, helper *RateHelper, curve *YieldTermStructure, want overnightOISResult) {
	t.Helper()
	maturity, pillar := pricingMust(helper.MaturityDate()), pricingMust(helper.PillarDate())
	if maturity != overnightDate(t, want.Maturity) || pillar != overnightDate(t, want.Pillar) {
		t.Fatalf("unexpected helper dates: maturity %v, pillar %v", maturity, pillar)
	}
	discount := pricingMust(curve.DiscountDate(maturity, false))
	curveNear(t, discount, want.Discount, 1e-12)
	if want.EndogenousDiscount != 0 && math.Abs(discount-want.EndogenousDiscount) <= 1e-12 {
		t.Fatal("external discounting must differ from endogenous discounting")
	}
	curveNear(t, pricingMust(helper.ImpliedQuote()), want.Quote, 1e-9)
}

func TestOvernightAveragingOISUpdatesAndRetention(t *testing.T) {
	cases := overnightOISCases(t)
	for label, averaging := range map[string]RateAveraging{"simple": SimpleAveraging, "compound": CompoundAveraging} {
		t.Run(label, func(t *testing.T) {
			s := pricingMust(NewSession())
			defer s.Close()
			row := cases["baseline"]
			cfg := overnightOISConfig(t, s, row, averaging)
			helper := pricingMust(s.NewOISRateHelper(cfg))
			dc := pricingMust(s.Actual360())
			curve := pricingMust(s.NewPiecewiseLogLinearDiscount(PiecewiseCurveConfig{ReferenceDate: overnightDate(t, row.Inputs.ReferenceDate), Helpers: []*RateHelper{helper}, DayCounter: dc}))
			ratesOK(t, cfg.OvernightIndex.Close())
			ratesOK(t, dc.Close())
			checkOvernightOIS(t, helper, curve, row.Results[label])
			row = cases["quote_updated"]
			ratesOK(t, cfg.Quote.SetValue(row.Inputs.Quote))
			checkOvernightOIS(t, helper, curve, row.Results[label])
			row = cases["date_updated"]
			ratesOK(t, cfg.Settings.SetEvaluationDate(overnightDate(t, row.Inputs.EvaluationDate)))
			checkOvernightOIS(t, helper, curve, row.Results[label])
			ratesOK(t, cfg.Quote.Close())
			ratesOK(t, cfg.Settings.Close())
			ratesOK(t, helper.Close())
			curveNear(t, pricingMust(curve.DiscountDate(overnightDate(t, row.Results[label].Maturity), false)), row.Results[label].Discount, 1e-12)
		})
	}
}

func TestOvernightAveragingOISForwardAndDiscounting(t *testing.T) {
	cases := overnightOISCases(t)
	for _, name := range []string{"forward_start", "external_discount"} {
		for label, averaging := range map[string]RateAveraging{"simple": SimpleAveraging, "compound": CompoundAveraging} {
			t.Run(name+"/"+label, func(t *testing.T) {
				s := pricingMust(NewSession())
				defer s.Close()
				row := cases[name]
				cfg := overnightOISConfig(t, s, row, averaging)
				helper := pricingMust(s.NewOISRateHelper(cfg))
				dc := pricingMust(s.Actual360())
				curve := pricingMust(s.NewPiecewiseLogLinearDiscount(PiecewiseCurveConfig{ReferenceDate: overnightDate(t, row.Inputs.ReferenceDate), Helpers: []*RateHelper{helper}, DayCounter: dc}))
				for _, close := range []func() error{cfg.Quote.Close, cfg.OvernightIndex.Close, cfg.Settings.Close, dc.Close} {
					ratesOK(t, close())
				}
				if cfg.DiscountingCurve != nil {
					ratesOK(t, cfg.DiscountingCurve.Close())
				}
				checkOvernightOIS(t, helper, curve, row.Results[label])
			})
		}
	}
}

func TestOvernightAveragingInvalidEnumDoesNotPoisonSession(t *testing.T) {
	row := overnightOISCases(t)["baseline"]
	s := pricingMust(NewSession())
	defer s.Close()
	cfg := overnightOISConfig(t, s, row, SimpleAveraging)
	for _, invalid := range []RateAveraging{-1, 2, 2147483647} {
		t.Run(fmt.Sprint(invalid), func(t *testing.T) {
			cfg.AveragingMethod = invalid
			helper, err := s.NewOISRateHelper(cfg)
			var native *Error
			if helper != nil || !errors.As(err, &native) || native.Code != 1 {
				t.Fatalf("invalid averaging must return invalid input: helper %v, error %v", helper, err)
			}
			curveNear(t, pricingMust(cfg.Quote.Value()), row.Inputs.Quote, 0)
			cfg.AveragingMethod = SimpleAveraging
			helper = pricingMust(s.NewOISRateHelper(cfg))
			dc := pricingMust(s.Actual360())
			curve := pricingMust(s.NewPiecewiseLogLinearDiscount(PiecewiseCurveConfig{ReferenceDate: overnightDate(t, row.Inputs.ReferenceDate), Helpers: []*RateHelper{helper}, DayCounter: dc}))
			checkOvernightOIS(t, helper, curve, row.Results["simple"])
		})
	}
}
