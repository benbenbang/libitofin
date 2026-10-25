package itofin

import (
	"encoding/csv"
	"math"
	"os"
	"strconv"
	"testing"
)

func baseDateRows(t *testing.T, name string) []map[string]string {
	t.Helper()
	file, err := os.Open("testdata/" + name)
	if err != nil {
		t.Fatal(err)
	}
	defer file.Close()
	reader := csv.NewReader(file)
	reader.Comma = '\t'
	rows, err := reader.ReadAll()
	if err != nil || len(rows) < 2 {
		t.Fatalf("fixture %s: %v", name, err)
	}
	result := make([]map[string]string, 0, len(rows)-1)
	for _, values := range rows[1:] {
		row := make(map[string]string, len(rows[0]))
		for i, key := range rows[0] {
			row[key] = values[i]
		}
		result = append(result, row)
	}
	return result
}
func baseDateFloat(t *testing.T, value string) float64 {
	t.Helper()
	result, err := strconv.ParseFloat(value, 64)
	if err != nil {
		t.Fatal(err)
	}
	return result
}
func baseDateMarket(t *testing.T, row map[string]string) (*Session, *Settings, *ZeroInflationIndex, *DayCounter) {
	t.Helper()
	session, err := NewSession()
	s := creditMust(t, session, err)
	t.Cleanup(func() { s.Close() })
	settings0, err := s.NewSettings()
	settings := creditMust(t, settings0, err)
	if err = settings.SetEvaluationDate(inflationDate(t, 18, 8, 2025)); err != nil {
		t.Fatal(err)
	}
	index0, err := s.NewUKRPI(settings)
	index := creditMust(t, index0, err)
	for _, fixing := range baseDateRows(t, "cpi-base-date-fixings.tsv") {
		value := baseDateFloat(t, fixing["value"])
		if row["stage"] != "original" && fixing["date"] == row["base_overwrite_date"] {
			value = baseDateFloat(t, row["base_overwrite_value"])
		}
		if err = index.AddFixing(cpiISO(t, fixing["date"]), value); err != nil {
			t.Fatal(err)
		}
	}
	dc0, err := s.Actual365Fixed()
	dc := creditMust(t, dc0, err)
	rate := -.0075
	if row["stage"] == "relinked" {
		rate = .065
	}
	baseDateLink(t, s, index, dc, rate)
	return s, settings, index, dc
}
func baseDateLink(t *testing.T, s *Session, index *ZeroInflationIndex, dc *DayCounter, rate float64) {
	t.Helper()
	curve0, err := s.NewInterpolatedZeroInflationCurve(
		InflationCurveConfig{ReferenceDate: inflationDate(t, 18, 8, 2025), BaseDate: inflationDate(t, 1, 1, 2025), Frequency: Monthly, DayCounter: dc},
		[]Date{inflationDate(t, 1, 1, 2025), inflationDate(t, 1, 1, 2035)}, []float64{rate, rate},
	)
	curve := creditMust(t, curve0, err)
	if err = index.LinkTo(curve); err != nil {
		t.Fatal(err)
	}
	if err = curve.Close(); err != nil {
		t.Fatal(err)
	}
}
func baseDateCoupon(t *testing.T, s *Session, index *ZeroInflationIndex, dc *DayCounter, row map[string]string) *CPICoupon {
	t.Helper()
	lag, err := strconv.Atoi(row["lag_months"])
	if err != nil {
		t.Fatal(err)
	}
	interpolation := CpiFlat
	if row["interpolation"] == "linear" {
		interpolation = CpiLinear
	}
	ex := cpiISO(t, row["ex_coupon_date"])
	cfg := CPICouponConfig{Index: index, BaseCPI: math.NaN(), Nominal: baseDateFloat(t, row["nominal"]), FixedRate: baseDateFloat(t, row["fixed_rate"]), PaymentDate: cpiISO(t, row["payment_date"]), StartDate: cpiISO(t, row["start_date"]), EndDate: cpiISO(t, row["end_date"]), ExCouponDate: &ex, ObservationLag: Period{int32(lag), Months}, Interpolation: interpolation, DayCounter: dc}
	result0, err := s.NewCPICouponFromBaseDate(cfg, cpiISO(t, row["base_date"]))
	result := creditMust(t, result0, err)
	p0, err := s.NewCPICouponPricer(nil)
	p := creditMust(t, p0, err)
	if err = result.SetPricer(p); err != nil {
		t.Fatal(err)
	}
	if err = p.Close(); err != nil {
		t.Fatal(err)
	}
	return result
}
func checkBaseDateCoupon(t *testing.T, coupon *CPICoupon, row map[string]string) {
	t.Helper()
	checks := []struct {
		key       string
		value     func() (float64, error)
		tolerance float64
	}{
		{"amount", coupon.Amount, 1e-9}, {"rate", coupon.Rate, 1e-13},
		{"base_fixing", coupon.BaseFixing, 1e-11}, {"index_fixing", coupon.IndexFixing, 1e-11},
		{"index_ratio", func() (float64, error) { return coupon.IndexRatio(cpiISO(t, row["end_date"])) }, 1e-13},
		{"accrual_period", coupon.AccrualPeriod, 1e-14},
	}
	for _, check := range checks {
		actual, err := check.value()
		expected := baseDateFloat(t, row[check.key])
		if err != nil || !cpiNear(actual, expected, check.tolerance) {
			t.Fatalf("%s %g want %g: %v", check.key, actual, expected, err)
		}
	}
	for i := 0; i < 8; i++ {
		prefix := "accrued_" + strconv.Itoa(i)
		actual, err := coupon.AccruedAmount(cpiISO(t, row[prefix+"_date"]))
		expected := baseDateFloat(t, row[prefix+"_value"])
		if err != nil || !cpiNear(actual, expected, 1e-9) {
			t.Fatalf("%s %g want %g: %v", prefix, actual, expected, err)
		}
	}
}
func TestCPICouponBaseDateNative(t *testing.T) {
	rows := baseDateRows(t, "cpi-base-date-coupons.tsv")
	if len(rows) != 108 {
		t.Fatal("incomplete independent native matrix")
	}
	for _, row := range rows {
		t.Run(row["id"]+"-"+row["stage"], func(t *testing.T) {
			s, _, index, dc := baseDateMarket(t, row)
			coupon := baseDateCoupon(t, s, index, dc, row)
			checkBaseDateCoupon(t, coupon, row)
			index.Close()
			dc.Close()
			checkBaseDateCoupon(t, coupon, row)
			coupon.Close()
			if _, err := coupon.BaseFixing(); err == nil {
				t.Fatal("released coupon accepted")
			}
		})
	}
}
func TestCPICouponBaseDateRebindingAndErrors(t *testing.T) {
	rows := baseDateRows(t, "cpi-base-date-coupons.tsv")
	row := rows[len(rows)-2]
	s, _, index, dc := baseDateMarket(t, row)
	coupon := baseDateCoupon(t, s, index, dc, row)
	checkBaseDateCoupon(t, coupon, row)
	baseDateLink(t, s, index, dc, .065)
	checkBaseDateCoupon(t, coupon, rows[len(rows)-1])
	if _, err := s.NewCPICouponFromBaseDate(CPICouponConfig{}, Date{}); err == nil {
		t.Fatal("null base accepted")
	}
	foreign0, err := NewSession()
	foreign := creditMust(t, foreign0, err)
	defer foreign.Close()
	cfg := CPICouponConfig{Index: index, DayCounter: dc}
	if _, err = foreign.NewCPICouponFromBaseDate(cfg, cpiISO(t, row["base_date"])); err == nil {
		t.Fatal("mixed session accepted")
	}
	var missing *CPICoupon
	if _, err = missing.BaseFixing(); err == nil {
		t.Fatal("nil coupon accepted")
	}
}
func TestCPILegBaseDateDefaultNPVAndRetention(t *testing.T) {
	s, settings, index, dc, discount := cpiMarket(t)
	cal0, err := s.NullCalendar()
	cal := creditMust(t, cal0, err)
	schedule0, err := s.NewSchedule(ScheduleConfig{Start: inflationDate(t, 15, 1, 2025), End: inflationDate(t, 15, 7, 2025), Frequency: Semiannual, Calendar: cal, Convention: Unadjusted})
	schedule := creditMust(t, schedule0, err)
	cfg := CPILegConfig{Schedule: schedule, Index: index, BaseCPI: math.NaN(), ObservationLag: Period{3, Months}, Notionals: []float64{100000}, FixedRates: []float64{.02}, DayCounter: dc, PaymentCalendar: cal, PaymentAdjustment: Unadjusted, GrowthOnly: true}
	if _, err := s.NewCPILeg(cfg); err == nil {
		t.Fatal("old explicit constructor accepted NaN base")
	}
	builder0, err := s.NewCPILegFromBaseDate(cfg, nil)
	builder := creditMust(t, builder0, err)
	leg0, err := builder.Build()
	leg := creditMust(t, leg0, err)
	payment0, err := leg.At(-1)
	payment := creditMust(t, payment0, err)
	value, err := payment.Amount()
	expected := 100000 * (100.8/99.5 - 1)
	if err != nil || !cpiNear(value, expected, 1e-8) {
		t.Fatalf("principal %g want %g: %v", value, expected, err)
	}
	if _, err = leg.NPV(CashFlowsNPVConfig{Discount: discount, Settings: settings}); err != nil {
		t.Fatal(err)
	}
	other0, err := s.NewSettings()
	other := creditMust(t, other0, err)
	if _, err = leg.NPV(CashFlowsNPVConfig{Discount: discount, Settings: other}); err == nil {
		t.Fatal("different Settings accepted")
	}
	zero := Date{}
	if _, err = s.NewCPILegFromBaseDate(cfg, &zero); err == nil {
		t.Fatal("explicit null base accepted")
	}
	index.Close()
	builder.Close()
	leg.Close()
	if value, err = payment.Amount(); err != nil || !cpiNear(value, expected, 1e-8) {
		t.Fatalf("retained principal %g %v", value, err)
	}
}

func TestCPILegBaseDateNativeMatrix(t *testing.T) {
	flows := baseDateRows(t, "cpi-base-date-leg-flows.tsv")
	npvs := baseDateRows(t, "cpi-base-date-leg-npv.tsv")
	if len(flows) != 480 || len(npvs) != 384 {
		t.Fatal("incomplete native leg matrix")
	}
	for _, row := range flows {
		if row["flow"] != "0" {
			continue
		}
		t.Run(row["case"]+"-"+row["stage"], func(t *testing.T) {
			lag0, err := strconv.Atoi(row["lag_months"])
			if err != nil {
				t.Fatal(err)
			}
			start := inflationDate(t, 31, 1, 2024)
			if row["irregular"] == "1" {
				start = inflationDate(t, 17, 2, 2024)
			}
			base := inflationDate(t, 31, 1, 2024)
			if row["default_base"] == "1" {
				month, year := start.Month()-lag0, start.Year()
				if month < 1 {
					month += 12
					year--
				}
				base = inflationDate(t, start.Day(), month, year)
			}
			stage := "original"
			if row["stage"] != "0" {
				stage = "overwritten"
			}
			marketRow := map[string]string{"stage": stage, "base_overwrite_date": strconv.Itoa(base.Year()) + "-" + fmtBaseMonth(base.Month()) + "-01", "base_overwrite_value": "180.25"}
			s, settings, index, dc := baseDateMarket(t, marketRow)
			if row["stage"] == "2" {
				baseDateLink(t, s, index, dc, .065)
			}
			cal0, err := s.NullCalendar()
			cal := creditMust(t, cal0, err)
			payment0, err := s.WeekendsOnly()
			payment := creditMust(t, payment0, err)
			backward := Backward
			schedule0, err := s.NewSchedule(ScheduleConfig{Start: start, End: inflationDate(t, 31, 1, 2026), Frequency: Semiannual, Calendar: cal, Convention: Unadjusted, Rule: &backward})
			schedule := creditMust(t, schedule0, err)
			interpolation := CpiFlat
			if row["interpolation"] == "linear" {
				interpolation = CpiLinear
			}
			cfg := CPILegConfig{Schedule: schedule, Index: index, BaseCPI: math.NaN(), ObservationLag: Period{int32(lag0), Months}, Interpolation: interpolation, Notionals: []float64{8123.75, 7400, 7100, 6900, 6500}, FixedRates: []float64{-.0125, 0, .0375}, DayCounter: dc, PaymentCalendar: payment, PaymentAdjustment: Following, GrowthOnly: row["growth_only"] == "1", ExCouponPeriod: Period{7, Days}, ExCouponCalendar: cal, ExCouponAdjustment: Unadjusted}
			var supplied *Date
			if row["default_base"] != "1" {
				supplied = &base
			}
			builder0, err := s.NewCPILegFromBaseDate(cfg, supplied)
			builder := creditMust(t, builder0, err)
			leg0, err := builder.Build()
			leg := creditMust(t, leg0, err)
			for _, expected := range flows {
				if expected["case"] != row["case"] || expected["stage"] != row["stage"] {
					continue
				}
				position, err := strconv.Atoi(expected["flow"])
				if err != nil {
					t.Fatal(err)
				}
				flow0, err := leg.At(position)
				flow := creditMust(t, flow0, err)
				amount, err := flow.Amount()
				want := baseDateFloat(t, expected["amount"])
				if err != nil || !cpiNear(amount, want, 1e-8) {
					t.Fatalf("flow %d %g want %g: %v", position, amount, want, err)
				}
				actualDate, err := flow.Date()
				if err != nil || strconv.Itoa(int(actualDate.Serial())) != expected["date"] {
					t.Fatalf("flowdate %v: %v", actualDate, err)
				}
			}
			discount0, err := s.NewFlatForward(inflationDate(t, 1, 1, 2024), -.015, dc)
			discount := creditMust(t, discount0, err)
			settings0, err := s.NewSettings()
			other := creditMust(t, settings0, err)
			if _, err = leg.NPV(CashFlowsNPVConfig{Discount: discount, Settings: other}); err == nil {
				t.Fatal("different Settings accepted")
			}
			for _, expected := range npvs {
				if expected["case"] != row["case"] || expected["stage"] != row["stage"] {
					continue
				}
				serial, err := strconv.Atoi(expected["settlement"])
				if err != nil {
					t.Fatal(err)
				}
				settled := Date{int32(serial)}
				today := inflationDate(t, 18, 8, 2025)
				include := expected["include"] == "1"
				amount, err := leg.NPV(CashFlowsNPVConfig{Discount: discount, Settings: settings, IncludeSettlementDateFlows: &include, SettlementDate: &settled, NPVDate: &today})
				want := baseDateFloat(t, expected["npv"])
				if err != nil || !cpiNear(amount, want, 1e-8) {
					t.Fatalf("npv %g want %g: %v", amount, want, err)
				}
			}
		})
	}
}
func fmtBaseMonth(month int) string {
	if month < 10 {
		return "0" + strconv.Itoa(month)
	}
	return strconv.Itoa(month)
}
