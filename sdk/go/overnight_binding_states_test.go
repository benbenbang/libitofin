package itofin

import (
	"encoding/json"
	"errors"
	"math"
	"os"
	"strings"
	"testing"
)

type overnightBindingOracle struct {
	Quantlib string `json:"quantlib"`
	Swaps    []struct {
		Name   string `json:"name"`
		Inputs struct {
			EvaluationDate string  `json:"evaluation_date"`
			ReferenceDate  string  `json:"reference_date"`
			EffectiveDate  string  `json:"effective_date"`
			TenorYears     int32   `json:"tenor_years"`
			Nominal        float64 `json:"nominal"`
			FixedRate      float64 `json:"fixed_rate"`
			ForwardRate    float64 `json:"forward_rate"`
			DiscountRate   float64 `json:"discount_rate"`
			PaymentLag     int32   `json:"payment_lag"`
		} `json:"inputs"`
		Results map[string]struct {
			NPV      float64 `json:"npv"`
			FairRate float64 `json:"fair_rate"`
		} `json:"results"`
	} `json:"swap_cases"`
	OIS []overnightOISOracle `json:"ois_cases"`
}

func overnightBindingCases(t *testing.T) overnightBindingOracle {
	t.Helper()
	data := pricingMust(os.ReadFile("testdata/overnight_binding_oracle.json"))
	var oracle overnightBindingOracle
	ratesOK(t, json.Unmarshal(data, &oracle))
	if oracle.Quantlib != "1.43" || len(oracle.Swaps) != 1 || len(oracle.OIS) != 2 {
		t.Fatal("unexpected overnight binding oracle")
	}
	if oracle.Swaps[0].Name != "today_start" || oracle.OIS[0].Name != "spread_initial" || oracle.OIS[1].Name != "spread_updated" {
		t.Fatal("unexpected overnight binding cases")
	}
	return oracle
}

func TestOvernightTodayForecastRecoversAfterMissingPastFixing(t *testing.T) {
	row := overnightBindingCases(t).Swaps[0]
	for mode, averaging := range map[string]RateAveraging{"simple": SimpleAveraging, "compound": CompoundAveraging} {
		t.Run(mode, func(t *testing.T) {
			s := pricingMust(NewSession())
			defer s.Close()
			settings := pricingMust(s.NewSettings())
			today := overnightDate(t, row.Inputs.EvaluationDate)
			ratesOK(t, settings.SetEvaluationDate(today))
			dc := pricingMust(s.Actual365Fixed())
			reference := overnightDate(t, row.Inputs.ReferenceDate)
			forward := pricingMust(s.NewFlatForward(reference, row.Inputs.ForwardRate, dc))
			discount := pricingMust(s.NewFlatForward(reference, row.Inputs.DiscountRate, dc))
			index := pricingMust(s.NewEstr(forward, settings))
			effective := overnightDate(t, row.Inputs.EffectiveDate)
			swap := pricingMust(s.MakeOis(MakeOisConfig{
				Tenor: Period{row.Inputs.TenorYears, Years}, Index: index, Settings: settings,
				FixedRate: &row.Inputs.FixedRate, Nominal: &row.Inputs.Nominal,
				EffectiveDate: &effective, PaymentLag: &row.Inputs.PaymentLag,
				Discount: discount, Averaging: &averaging,
			}))
			want, ok := row.Results[mode]
			if !ok {
				t.Fatal("missing averaging result")
			}
			curveNear(t, pricingMust(swap.NPV()), want.NPV, 1e-7)
			curveNear(t, pricingMust(swap.FairRate()), want.FairRate, 1e-12)
			if !pricingMust(swap.IsCalculated()) {
				t.Fatal("swap should be calculated")
			}
			ratesOK(t, settings.SetEvaluationDate(testDate(t, 8, 7, 2026)))
			if pricingMust(swap.IsCalculated()) {
				t.Fatal("date update must invalidate swap")
			}
			_, err := swap.NPV()
			var native *Error
			if !errors.As(err, &native) || !strings.Contains(strings.ToLower(err.Error()), "fixing") {
				t.Fatalf("missing historical fixing must return native error: %v", err)
			}
			if pricingMust(swap.IsCalculated()) {
				t.Fatal("failed valuation must not cache a result")
			}
			ratesOK(t, settings.SetEvaluationDate(today))
			curveNear(t, pricingMust(swap.NPV()), want.NPV, 1e-7)
			curveNear(t, pricingMust(swap.FairRate()), want.FairRate, 1e-12)
		})
	}
}

func TestOvernightAdditiveSpreadQuoteRebootstrapsExistingCurve(t *testing.T) {
	cases := overnightBindingCases(t).OIS
	for mode, averaging := range map[string]RateAveraging{"simple": SimpleAveraging, "compound": CompoundAveraging} {
		t.Run(mode, func(t *testing.T) {
			s := pricingMust(NewSession())
			defer s.Close()
			cfg := overnightOISConfig(t, s, cases[0], averaging)
			spread := pricingMust(s.NewSimpleQuote(cases[0].Inputs.OvernightSpread))
			cfg.OvernightSpread = spread
			helper := pricingMust(s.NewOISRateHelper(cfg))
			dc := pricingMust(s.Actual360())
			curve := pricingMust(s.NewPiecewiseLogLinearDiscount(PiecewiseCurveConfig{
				ReferenceDate: overnightDate(t, cases[0].Inputs.ReferenceDate),
				Helpers:       []*RateHelper{helper}, DayCounter: dc,
			}))
			discounts := make([]float64, len(cases))
			for n, row := range cases {
				ratesOK(t, spread.SetValue(row.Inputs.OvernightSpread))
				checkOvernightOIS(t, helper, curve, row.Results[mode])
				discounts[n] = pricingMust(curve.DiscountDate(pricingMust(helper.MaturityDate()), false))
			}
			if math.Abs(discounts[0]-discounts[1]) <= 1e-6 {
				t.Fatal("spread update must change fitted discount")
			}
		})
	}
}
