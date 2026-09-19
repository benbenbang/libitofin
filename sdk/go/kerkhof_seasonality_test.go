package itofin

import (
	"encoding/csv"
	"math"
	"os"
	"strconv"
	"strings"
	"testing"
)

func kerkhofFactors() []float64 {
	return []float64{1.20, 1.004, .997, 1.006, .995, 1.003, .991, 1.008, .998, 1.005, .996, 1.002}
}

func TestKerkhofSeasonalityQuantLibOracle(t *testing.T) {
	file, err := os.Open("../../crates/libitofin/tests/fixtures/kerkhof_seasonality.csv")
	if err != nil {
		t.Fatal(err)
	}
	defer file.Close()
	rows, err := csv.NewReader(file).ReadAll()
	if err != nil {
		t.Fatal(err)
	}
	if len(rows) != 73 {
		t.Fatalf("fixture rows: %d", len(rows))
	}
	s := pricingMust(NewSession())
	defer s.Close()
	for _, row := range rows[1:] {
		date := func(i int) Date {
			serial, err := strconv.ParseInt(row[i], 10, 32)
			if err != nil {
				t.Fatal(err)
			}
			return pricingMust(DateFromSerial(int32(serial)))
		}
		number := func(i int) float64 {
			value, err := strconv.ParseFloat(row[i], 64)
			if err != nil {
				t.Fatal(err)
			}
			return value
		}
		check := func(actual, expected float64) {
			t.Helper()
			if math.IsInf(expected, 0) {
				if actual != expected {
					t.Fatalf("%v: %g != %g", row, actual, expected)
				}
			} else if math.IsNaN(actual) || math.Abs(actual-expected) >= 1e-13 {
				t.Fatalf("%v: %.17g != %.17g", row, actual, expected)
			}
		}
		frequency := Monthly
		if row[3] == "4" {
			frequency = Quarterly
		}
		dc := pricingMust(s.Actual365Fixed())
		if row[4] == "30/360" {
			dc = pricingMust(s.Thirty360BondBasis())
		}
		curve := pricingMust(s.NewInterpolatedZeroInflationCurve(
			InflationCurveConfig{ReferenceDate: inflationDate(t, 13, 8, 2007), Frequency: frequency, DayCounter: dc},
			[]Date{date(1), inflationDate(t, 1, 1, 2015)}, []float64{.02, .035}))
		seasonality := pricingMust(s.NewKerkhofSeasonality(date(0), kerkhofFactors()))
		check(pricingMust(seasonality.SeasonalityFactor(date(2))), number(5))
		if pricingMust(seasonality.SeasonalityBaseDate()) != date(0) || pricingMust(seasonality.Frequency()) != Monthly {
			t.Fatal("Kerkhof metadata changed")
		}
		if err := curve.SetSeasonality(seasonality); err != nil {
			t.Fatal(err)
		}
		if err := seasonality.Close(); err != nil {
			t.Fatal(err)
		}
		if date(2).Serial() < date(1).Serial() {
			if _, err := curve.ZeroRateDate(date(2), true); err == nil {
				t.Fatal("before-base query accepted")
			}
		} else {
			check(pricingMust(curve.ZeroRateDate(date(2), true)), number(9))
		}
		if err := curve.SetSeasonality(nil); err != nil {
			t.Fatal(err)
		}
		if pricingMust(curve.HasSeasonality()) {
			t.Fatal("cleared correction retained")
		}
		if err := curve.Close(); err != nil {
			t.Fatal(err)
		}
		if err := dc.Close(); err != nil {
			t.Fatal(err)
		}
	}
}

func TestKerkhofSeasonalityOwnershipAndErrors(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	other := pricingMust(NewSession())
	defer other.Close()
	base := inflationDate(t, 1, 7, 2007)
	dc := pricingMust(s.Actual365Fixed())
	config := InflationCurveConfig{ReferenceDate: inflationDate(t, 13, 8, 2007), Frequency: Monthly, DayCounter: dc}
	dates := []Date{base, inflationDate(t, 1, 1, 2015)}
	zero := pricingMust(s.NewInterpolatedZeroInflationCurve(config, dates, []float64{.02, .035}))
	yoy := pricingMust(s.NewInterpolatedYoYInflationCurve(config, dates, []float64{.02, .035}))
	query := inflationDate(t, 15, 8, 2008)
	before := pricingMust(zero.ZeroRateDate(query, false))
	for _, count := range []int{0, 11, 13, 24} {
		if _, err := s.NewKerkhofSeasonality(base, make([]float64, count)); err == nil {
			t.Fatalf("accepted %d factors", count)
		}
	}
	invalid := kerkhofFactors()
	invalid[1] = math.NaN()
	if _, err := s.NewKerkhofSeasonality(base, invalid); err == nil {
		t.Fatal("accepted nonfinite factors")
	}
	foreign := pricingMust(other.NewKerkhofSeasonality(base, kerkhofFactors()))
	if err := zero.SetSeasonality(foreign); err == nil {
		t.Fatal("accepted foreign seasonality")
	}
	input := kerkhofFactors()
	seasonality := pricingMust(s.NewKerkhofSeasonality(base, input))
	input[7] = 9
	copied := pricingMust(seasonality.SeasonalityFactors())
	if copied[7] != 1.008 {
		t.Fatal("constructor retained caller factors")
	}
	copied[7] = 8
	if pricingMust(seasonality.SeasonalityFactors())[7] != 1.008 {
		t.Fatal("factor inspector leaked native data")
	}
	if err := zero.SetSeasonality(seasonality); err != nil {
		t.Fatal(err)
	}
	if err := yoy.SetSeasonality(seasonality); err != nil {
		t.Fatal(err)
	}
	if err := seasonality.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err := seasonality.SeasonalityFactor(query); err == nil {
		t.Fatal("released handle accepted")
	}
	if math.Abs(pricingMust(zero.ZeroRateDate(query, false))-before) < 1e-6 {
		t.Fatal("zero curve correction did not move rate")
	}
	if _, err := yoy.YoYRateDate(query, false); err == nil || !strings.Contains(err.Error(), "not defined on YoY rates") {
		t.Fatalf("YoY: %v", err)
	}
	if err := zero.SetSeasonality(nil); err != nil {
		t.Fatal(err)
	}
	if math.Abs(pricingMust(zero.ZeroRateDate(query, false))-before) >= 1e-13 {
		t.Fatal("clearing did not restore raw rate")
	}
	if err := yoy.SetSeasonality(nil); err != nil {
		t.Fatal(err)
	}
	if _, err := yoy.YoYRateDate(query, false); err != nil {
		t.Fatal(err)
	}
}
