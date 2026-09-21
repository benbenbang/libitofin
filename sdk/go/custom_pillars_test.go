package itofin

import (
	"encoding/json"
	"math"
	"os"
	"testing"
	"time"
)

type customPillarYieldRow struct {
	Kind                               int
	Earliest, Maturity, Pillar, Latest string
	LatestRelevant                     string `json:"latest_relevant"`
	Discount                           float64
	UpdatedDiscount                    float64 `json:"updated_discount"`
}
type customPillarInflationRow struct {
	Flat           bool
	Pillar, Latest string
	YoyPillar      string  `json:"yoy_pillar"`
	YoyLatest      string  `json:"yoy_latest"`
	ZeroRate       float64 `json:"zero_rate"`
	YoyRate        float64 `json:"yoy_rate"`
}

func customPillarFixture(t *testing.T) ([]customPillarYieldRow, []customPillarInflationRow) {
	t.Helper()
	bytes, err := os.ReadFile("testdata/custom_pillars.json")
	pricingOK(t, err)
	var data struct {
		Yield     []customPillarYieldRow
		Inflation []customPillarInflationRow
	}
	pricingOK(t, json.Unmarshal(bytes, &data))
	return data.Yield, data.Inflation
}
func customPillarDate(text string) Date {
	d := pricingMust(time.Parse("2006-01-02", text))
	return pricingMust(NewDate(d.Day(), int(d.Month()), d.Year()))
}
func customPillarNear(t *testing.T, got, want float64) {
	t.Helper()
	if math.IsNaN(got) || math.IsInf(got, 0) || math.Abs(got-want) > 1e-12 {
		t.Fatalf("got %.17g want %.17g", got, want)
	}
}

func TestCustomYieldPillarsQuantLibAndRecovery(t *testing.T) {
	rows, _ := customPillarFixture(t)
	for _, row := range rows {
		s := pricingMust(NewSession())
		t.Cleanup(func() { s.Close() })
		today := customPillarDate("2026-06-15")
		settings := pricingMust(s.NewSettings())
		pricingOK(t, settings.SetEvaluationDate(today))
		q := pricingMust(s.NewSimpleQuote(.03))
		idx := pricingMust(s.NewEuribor(Period{3, Months}, nil, settings))
		overnight := pricingMust(s.NewEstr(nil, settings))
		cal := pricingMust(s.Target())
		dc := pricingMust(s.Thirty360BondBasis())
		factory := func(custom *Date) (*RateHelper, error) {
			switch row.Kind {
			case 0:
				cfg := DefaultFraRateHelperConfig()
				cfg.Quote, cfg.Index, cfg.PeriodToStart, cfg.Pillar, cfg.CustomPillarDate = q, idx, Period{3, Months}, CustomDate, custom
				return s.NewFraRateHelper(cfg)
			case 1:
				return s.NewSwapRateHelper(SwapRateHelperConfig{Quote: q, Tenor: Period{1, Years}, Calendar: cal, FixedFrequency: Annual, FixedConvention: ModifiedFollowing, FixedDayCount: dc, IborIndex: idx, Pillar: pricingPtr(CustomDate), CustomPillarDate: custom})
			default:
				cfg := DefaultOISRateHelperConfig()
				cfg.Quote, cfg.OvernightIndex, cfg.Settings = q, overnight, settings
				cfg.SettlementDays, cfg.Tenor, cfg.PaymentLag, cfg.PaymentConvention, cfg.PaymentFrequency = 2, Period{1, Years}, 2, Following, Annual
				cfg.Pillar, cfg.CustomPillarDate = CustomDate, custom
				return s.NewOISRateHelper(cfg)
			}
		}
		early := pricingMust(customPillarDate(row.Earliest).AddDays(-1))
		late := pricingMust(customPillarDate(row.LatestRelevant).AddDays(1))
		for _, invalid := range []*Date{nil, &early, &late} {
			if _, err := factory(invalid); err == nil {
				t.Fatalf("kind%d accepted invalid custom date", row.Kind)
			}
		}
		custom := customPillarDate(row.Pillar)
		h := pricingMust(factory(&custom))
		for want, query := range map[string]func() (Date, error){row.Earliest: h.EarliestDate, row.Maturity: h.MaturityDate} {
			if got := pricingMust(query()); got != customPillarDate(want) {
				t.Fatalf("kind%d date %v want%s", row.Kind, got, want)
			}
		}
		if pricingMust(h.PillarDate()) != custom || pricingMust(h.LatestDate()) != customPillarDate(row.Latest) || pricingMust(h.LatestRelevantDate()) != customPillarDate(row.LatestRelevant) {
			t.Fatalf("kind%d pillar/latest mismatch", row.Kind)
		}
		curve := pricingMust(s.NewPiecewiseLogLinearDiscount(PiecewiseCurveConfig{ReferenceDate: today, Helpers: []*RateHelper{h}, DayCounter: pricingMust(s.Actual365Fixed())}))
		for _, source := range []object{idx.object, overnight.object, cal.object, dc.object} {
			pricingOK(t, source.Close())
		}
		customPillarNear(t, pricingMust(curve.DiscountDate(customPillarDate(row.LatestRelevant), true)), row.Discount)
		pricingOK(t, q.SetValue(.031))
		customPillarNear(t, pricingMust(curve.DiscountDate(customPillarDate(row.LatestRelevant), true)), row.UpdatedDiscount)
		pricingOK(t, settings.SetEvaluationDate(customPillarDate("2028-06-15")))
		if _, err := curve.DiscountDate(customPillarDate(row.LatestRelevant), true); err == nil {
			t.Fatal("invalid rolled pillar priced")
		}
		pricingOK(t, settings.SetEvaluationDate(today))
		pricingOK(t, q.SetValue(.03))
		pricingOK(t, q.Close())
		pricingOK(t, h.Close())
		customPillarNear(t, pricingMust(curve.DiscountDate(customPillarDate(row.LatestRelevant), true)), row.Discount)
	}
}

func TestCustomFraOverloadsAndSwapDiscount(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	today := customPillarDate("2026-06-15")
	settings := pricingMust(s.NewSettings())
	pricingOK(t, settings.SetEvaluationDate(today))
	idx := pricingMust(s.NewEuribor(Period{3, Months}, nil, settings))
	q := pricingMust(s.NewSimpleQuote(.03))
	custom := customPillarDate("2026-10-15")
	cfg := DefaultFraRateHelperConfig()
	cfg.Index, cfg.Quote, cfg.Rate, cfg.PeriodToStart, cfg.MonthsToStart = idx, q, .03, Period{3, Months}, 3
	cfg.StartDate, cfg.EndDate = customPillarDate("2026-09-17"), customPillarDate("2026-12-17")
	cfg.Pillar, cfg.CustomPillarDate = CustomDate, &custom
	for _, factory := range []func(FraRateHelperConfig) (*RateHelper, error){s.NewFraRateHelperFromRate, s.NewFraRateHelperFromMonths, s.NewFraRateHelperFromDates} {
		h := pricingMust(factory(cfg))
		if pricingMust(h.PillarDate()) != custom {
			t.Fatal("FRA overload ignored custom pillar")
		}
		bad := cfg
		bad.CustomPillarDate = nil
		if _, err := factory(bad); err == nil {
			t.Fatal("missing custom date accepted")
		}
	}
	swapCustom := customPillarDate("2027-03-15")
	swap := SwapRateHelperConfig{Quote: q, Tenor: Period{1, Years}, Calendar: pricingMust(s.Target()), FixedFrequency: Annual, FixedConvention: ModifiedFollowing, FixedDayCount: pricingMust(s.Thirty360BondBasis()), IborIndex: idx, Pillar: pricingPtr(CustomDate), CustomPillarDate: &swapCustom}
	discount := pricingMust(s.NewFlatForward(today, .025, pricingMust(s.Actual365Fixed())))
	h := pricingMust(s.NewSwapRateHelperWithDiscount(swap, discount))
	if pricingMust(h.PillarDate()) != swapCustom {
		t.Fatal("discounted helper ignored custom pillar")
	}
	swap.Pillar = nil
	if _, err := s.NewSwapRateHelper(swap); err == nil {
		t.Fatal("date with default convention silently ignored")
	}
}

func TestCustomInflationPillarsQuantLib(t *testing.T) {
	_, rows := customPillarFixture(t)
	for _, row := range rows {
		m := newInflationCompletionMarket(t)
		interpolation := CpiLinear
		if row.Flat {
			interpolation = CpiFlat
		}
		cfg := m.helperConfig(t, 13, 2008, interpolation, pricingPtr(CustomDate))
		if _, err := m.s.NewZeroCouponInflationSwapHelper(cfg, m.zero); err == nil {
			t.Fatal("missing zero custom date accepted")
		}
		if _, err := m.s.NewYearOnYearInflationSwapHelper(cfg, m.yoy, m.nominal); err == nil {
			t.Fatal("missing yoy custom date accepted")
		}
		if !row.Flat {
			for _, text := range []string{"2008-05-31", "2008-07-02"} {
				invalid := customPillarDate(text)
				cfg.CustomPillarDate = &invalid
				if _, err := m.s.NewZeroCouponInflationSwapHelper(cfg, m.zero); err == nil {
					t.Fatal("out of range zero pillar accepted")
				}
				if _, err := m.s.NewYearOnYearInflationSwapHelper(cfg, m.yoy, m.nominal); err == nil {
					t.Fatal("out of range yoy pillar accepted")
				}
			}
		}
		custom := customPillarDate("2008-06-15")
		cfg.CustomPillarDate = &custom
		zh := pricingMust(m.s.NewZeroCouponInflationSwapHelper(cfg, m.zero))
		yh := pricingMust(m.s.NewYearOnYearInflationSwapHelper(cfg, m.yoy, m.nominal))
		if pricingMust(zh.PillarDate()) != customPillarDate(row.Pillar) || pricingMust(zh.LatestDate()) != customPillarDate(row.Latest) || pricingMust(yh.PillarDate()) != customPillarDate(row.YoyPillar) || pricingMust(yh.LatestDate()) != customPillarDate(row.YoyLatest) {
			t.Fatal("inflation custom dates disagree with QuantLib")
		}
		curveCfg := InflationCurveConfig{ReferenceDate: customPillarDate("2007-08-13"), BaseDate: customPillarDate("2007-07-01"), BaseYoYRate: .0295, Frequency: Monthly, DayCounter: m.dc}
		zc := pricingMust(m.s.NewPiecewiseZeroInflationCurve(curveCfg, []*ZeroInflationHelper{zh}))
		yc := pricingMust(m.s.NewPiecewiseYoYInflationCurve(curveCfg, []*YoYInflationHelper{yh}))
		for _, source := range []object{zh.object, yh.object, m.zero.object, m.yoy.object, m.nominal.object, cfg.Quote.object} {
			pricingOK(t, source.Close())
		}
		zn := pricingMust(zc.Nodes())
		yn := pricingMust(yc.Nodes())
		customPillarNear(t, zn[len(zn)-1].Rate, row.ZeroRate)
		customPillarNear(t, yn[len(yn)-1].Rate, row.YoyRate)
	}
}
