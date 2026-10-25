package itofin

import (
	"encoding/json"
	"math"
	"os"
	"testing"
)

func cpiMarket(t *testing.T) (*Session, *Settings, *ZeroInflationIndex, *DayCounter, *YieldTermStructure) {
	t.Helper()
	s0, e := NewSession()
	s := creditMust(t, s0, e)
	t.Cleanup(func() { s.Close() })
	settings0, e := s.NewSettings()
	settings := creditMust(t, settings0, e)
	if e = settings.SetEvaluationDate(inflationDate(t, 15, 6, 2025)); e != nil {
		t.Fatal(e)
	}
	i0, e := s.NewUKRPI(settings)
	index := creditMust(t, i0, e)
	for _, x := range []struct {
		m, y int
		v    float64
	}{{10, 2024, 99.5}, {11, 2024, 99.8}, {12, 2024, 100}, {1, 2025, 100.2}, {2, 2025, 100.4}, {3, 2025, 100.6}, {4, 2025, 100.8}, {5, 2025, 101}} {
		if e = index.AddFixing(inflationDate(t, 1, x.m, x.y), x.v); e != nil {
			t.Fatal(e)
		}
	}
	dc0, e := s.Actual365Fixed()
	dc := creditMust(t, dc0, e)
	z0, e := s.NewInterpolatedZeroInflationCurve(InflationCurveConfig{ReferenceDate: inflationDate(t, 15, 6, 2025), BaseDate: inflationDate(t, 1, 1, 2025), Frequency: Monthly, DayCounter: dc}, []Date{inflationDate(t, 1, 1, 2025), inflationDate(t, 1, 1, 2035)}, []float64{.025, .025})
	z := creditMust(t, z0, e)
	if e = index.LinkTo(z); e != nil {
		t.Fatal(e)
	}
	discount0, e := s.NewFlatForward(inflationDate(t, 15, 6, 2025), .04, dc)
	discount := creditMust(t, discount0, e)
	return s, settings, index, dc, discount
}
func cpiNear(got, want, tolerance float64) bool {
	return !math.IsNaN(got) && !math.IsInf(got, 0) && !math.IsNaN(want) && !math.IsInf(want, 0) && math.Abs(got-want) <= tolerance
}

func TestCPICouponNativeAndLifetimes(t *testing.T) {
	for _, interpolation := range []CpiInterpolationType{CpiFlat, CpiLinear} {
		t.Run(map[CpiInterpolationType]string{CpiFlat: "flat", CpiLinear: "linear"}[interpolation], func(t *testing.T) {
			s, _, index, dc, discount := cpiMarket(t)
			ex := inflationDate(t, 10, 7, 2025)
			config := CPICouponConfig{Index: index, BaseCPI: 100, Nominal: 100000, FixedRate: .02, PaymentDate: inflationDate(t, 15, 7, 2025), StartDate: inflationDate(t, 15, 1, 2025), EndDate: inflationDate(t, 15, 7, 2025), ExCouponDate: &ex, ObservationLag: Period{3, Months}, Interpolation: interpolation, DayCounter: dc}
			c0, e := s.NewCPICoupon(config)
			c := creditMust(t, c0, e)
			if _, e = c.Rate(); e == nil {
				t.Fatal("missing pricer accepted")
			}
			p0, e := s.NewCPICouponPricer(discount)
			p := creditMust(t, p0, e)
			if e = c.SetPricer(p); e != nil {
				t.Fatal(e)
			}
			if e = discount.Close(); e != nil {
				t.Fatal(e)
			}
			data, e := os.ReadFile("testdata/cpi-oracle-coupons.json")
			if e != nil {
				t.Fatal(e)
			}
			var fixture struct {
				Coupons []struct {
					Name   string
					Values struct {
						Amount, Rate  float64
						IndexFixing   float64 `json:"index_fixing"`
						AccrualPeriod float64 `json:"accrual_period"`
					}
				}
			}
			if e = json.Unmarshal(data, &fixture); e != nil {
				t.Fatal(e)
			}
			if len(fixture.Coupons) != 8 {
				t.Fatal("wrong coupon fixture count")
			}
			found := false
			for _, row := range fixture.Coupons {
				if row.Name != map[CpiInterpolationType]string{CpiFlat: "flat_historical_excoupon", CpiLinear: "linear_historical_excoupon"}[interpolation] {
					continue
				}
				found = true
				rate, e := c.Rate()
				if e != nil || !cpiNear(rate, row.Values.Rate, 1e-13) {
					t.Fatalf("rate %.17g want %.17g: %v", rate, row.Values.Rate, e)
				}
				a, e := c.Amount()
				if e != nil || !cpiNear(a, row.Values.Amount, 1e-9) {
					t.Fatalf("amount %.17g want %.17g: %v", a, row.Values.Amount, e)
				}
				fixing, e := c.IndexFixing()
				if e != nil || !cpiNear(fixing, row.Values.IndexFixing, 1e-12) {
					t.Fatalf("index fixing %g: %v", fixing, e)
				}
				ratio, e := c.IndexRatio(config.EndDate)
				if e != nil || !cpiNear(ratio, row.Values.IndexFixing/100, 1e-13) {
					t.Fatalf("ratio %g: %v", ratio, e)
				}
				accrual, e := c.AccrualPeriod()
				if e != nil || !cpiNear(accrual, row.Values.AccrualPeriod, 1e-14) {
					t.Fatalf("accrual %g: %v", accrual, e)
				}
				rate, e = p.SwapletRate(c)
				if e != nil || !cpiNear(rate, row.Values.Rate, 1e-13) {
					t.Fatalf("swaplet rate %g: %v", rate, e)
				}
				price, e := p.SwapletPrice(c)
				expected := row.Values.Amount / 100000 * math.Exp(-.04*30/365)
				if e != nil || !cpiNear(price, expected, 1e-13) {
					t.Fatalf("swaplet price %g want %g: %v", price, expected, e)
				}
				level := 100.8
				if interpolation == CpiLinear {
					level += .2 * 9 / 31
				}
				accrued, e := p.AccruedRate(c, ex)
				if e != nil || !cpiNear(accrued, .02*level/100, 1e-13) {
					t.Fatalf("accrued rate %g: %v", accrued, e)
				}
			}
			if !found {
				t.Fatal("missing coupon fixture")
			}
			if a, e := c.AccruedAmount(ex); e != nil || math.IsNaN(a) || math.IsInf(a, 0) || a >= 0 {
				t.Fatalf("ex-coupon %g %v", a, e)
			}
			if _, e = p.SwapletPrice(c); e != nil {
				t.Fatal(e)
			}
			zero := Date{}
			config.ReferenceStart = &zero
			if _, e = s.NewCPICoupon(config); e == nil {
				t.Fatal("explicit null reference date accepted")
			}
			config.ReferenceStart = nil
			config.BaseCPI = math.NaN()
			if _, e = s.NewCPICoupon(config); e == nil {
				t.Fatal("invalid CPI accepted")
			}
			config.BaseCPI = 100
			p.Close()
			index.Close()
			if _, e = c.Amount(); e != nil {
				t.Fatal(e)
			}
			foreign0, e := NewSession()
			foreign := creditMust(t, foreign0, e)
			defer foreign.Close()
			other0, e := foreign.NewCPICouponPricer(nil)
			other := creditMust(t, other0, e)
			if e = c.SetPricer(other); e == nil {
				t.Fatal("mixed session accepted")
			}
			c.Close()
			if _, e = c.Amount(); e == nil {
				t.Fatal("released coupon accepted")
			}
		})
	}
}
func TestCPILegNativeNPVAndSettings(t *testing.T) {
	data, e := os.ReadFile("testdata/cpi-oracle-legs.json")
	if e != nil {
		t.Fatal(e)
	}
	var fixture struct {
		Legs []struct {
			Interpolation string
			GrowthOnly    bool `json:"growth_only"`
			NPV           float64
			Cashflows     []struct{ Amount float64 }
		}
	}
	if e = json.Unmarshal(data, &fixture); e != nil {
		t.Fatal(e)
	}
	if len(fixture.Legs) != 4 {
		t.Fatal("wrong leg fixture count")
	}
	for _, row := range fixture.Legs {
		s, settings, index, dc, discount := cpiMarket(t)
		cal0, e := s.NullCalendar()
		cal := creditMust(t, cal0, e)
		schedule0, e := s.NewSchedule(ScheduleConfig{Start: inflationDate(t, 15, 1, 2025), End: inflationDate(t, 15, 1, 2028), Frequency: Semiannual, Calendar: cal, Convention: Unadjusted})
		schedule := creditMust(t, schedule0, e)
		interpolation := CpiFlat
		if row.Interpolation == "linear" {
			interpolation = CpiLinear
		}
		builder0, e := s.NewCPILeg(CPILegConfig{Schedule: schedule, Index: index, BaseCPI: 100, ObservationLag: Period{3, Months}, Interpolation: interpolation, Notionals: []float64{100000}, FixedRates: []float64{.02}, DayCounter: dc, PaymentCalendar: cal, PaymentAdjustment: Unadjusted, GrowthOnly: row.GrowthOnly})
		builder := creditMust(t, builder0, e)
		leg0, e := builder.Build()
		leg := creditMust(t, leg0, e)
		if len(row.Cashflows) != 7 {
			t.Fatal("wrong flow fixture count")
		}
		for i, want := range row.Cashflows {
			f0, e := leg.At(i)
			f := creditMust(t, f0, e)
			got, e := f.Amount()
			if e != nil || !cpiNear(got, want.Amount, 1e-8) {
				t.Fatalf("flow%d got%.17g want%.17g: %v", i, got, want.Amount, e)
			}
		}
		actual, e := leg.NPV(CashFlowsNPVConfig{Discount: discount, Settings: settings})
		if e != nil || !cpiNear(actual, row.NPV, 1e-8) {
			t.Fatalf("NPV %.17g want %.17g: %v", actual, row.NPV, e)
		}
		other0, e := s.NewSettings()
		other := creditMust(t, other0, e)
		other.SetEvaluationDate(inflationDate(t, 15, 6, 2025))
		if _, e = leg.NPV(CashFlowsNPVConfig{Discount: discount, Settings: other}); e == nil {
			t.Fatal("foreign CPI settings accepted")
		}
		principal0, e := leg.At(-1)
		principal := creditMust(t, principal0, e)
		index.Close()
		builder.Close()
		leg.Close()
		actual, e = principal.Amount()
		if e != nil || !cpiNear(actual, row.Cashflows[len(row.Cashflows)-1].Amount, 1e-8) {
			t.Fatalf("retained principal %g %v", actual, e)
		}
	}
}
func TestCPINilReceivers(t *testing.T) {
	var c *CPICoupon
	if _, e := c.Rate(); e == nil {
		t.Fatal("nil coupon accepted")
	}
	var l *CPILeg
	if _, e := l.Build(); e == nil {
		t.Fatal("nil leg accepted")
	}
	var p *CPICouponPricer
	if _, e := p.SwapletRate(c); e == nil {
		t.Fatal("nil pricer accepted")
	}
}
