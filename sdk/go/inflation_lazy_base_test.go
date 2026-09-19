package itofin

import (
	"encoding/csv"
	"errors"
	"os"
	"strconv"
	"testing"
)

func TestLazyInflationBaseMatchesQuantLibAndRetainsDependencies(t *testing.T) {
	for _, corrected := range []bool{false, true} {
		s := pricingMust(NewSession())
		today := testDate(t, 13, 8, 2007)
		settings := pricingMust(s.NewSettings())
		pricingOK(t, settings.SetEvaluationDate(today))
		index := pricingMust(s.NewUKRPI(settings))
		dc := pricingMust(s.Thirty360BondBasis())
		cal := pricingMust(s.UnitedKingdom())
		rates := []float64{2.93, 2.95, 2.965, 2.98, 3, 3.06, 3.175, 3.243, 3.293, 3.338, 3.348, 3.348, 3.308, 3.228}
		years := []int{2008, 2009, 2010, 2011, 2012, 2014, 2017, 2019, 2022, 2027, 2032, 2037, 2047, 2057}
		days := []int{13, 13, 13, 15, 13, 13, 13, 13, 15, 14, 13, 15, 13, 13}
		quotes := make([]*SimpleQuote, len(rates))
		helpers := make([]*ZeroInflationHelper, len(rates))
		for i := range rates {
			quotes[i] = pricingMust(s.NewSimpleQuote(0))
			helpers[i] = pricingMust(s.NewZeroCouponInflationSwapHelper(InflationHelperConfig{
				Quote: quotes[i], SwapObservationLag: Period{3, Months}, Maturity: testDate(t, days[i], 8, years[i]),
				Calendar: cal, PaymentConvention: ModifiedFollowing, DayCounter: dc, ObservationInterpolation: CpiFlat, Settings: settings,
			}, index))
		}
		config := InflationCurveConfig{ReferenceDate: today, Frequency: Monthly, DayCounter: dc}
		curve := pricingMust(s.NewPiecewiseZeroInflationCurveWithLastFixingDate(config, index, helpers, nil))
		if _, err := curve.BaseDate(); err == nil {
			t.Fatal("missing history accepted")
		}
		for i, quote := range quotes {
			pricingOK(t, quote.SetValue(rates[i]/100))
		}
		fixings := []float64{189.9, 189.9, 189.6, 190.5, 191.6, 192, 192.2, 192.2, 192.6, 193.1, 193.3, 193.6, 194.1, 193.4, 194.2, 195, 196.5, 197.7, 198.5, 198.5, 199.2, 200.1, 200.4, 201.1, 202.7, 201.6, 203.1, 204.4, 205.4, 206.2, 207.3}
		if corrected {
			fixings[30] = 208.1
		}
		for i, value := range fixings {
			pricingOK(t, index.AddFixing(testDate(t, 1, i%12+1, 2005+i/12), value))
		}
		pricingOK(t, index.LinkTo(curve))
		stream, err := os.Open("../../crates/libitofin/tests/fixtures/lazy_inflation_base/oracle.csv")
		if err != nil {
			t.Fatal(err)
		}
		rows, err := csv.NewReader(stream).ReadAll()
		stream.Close()
		if err != nil {
			t.Fatal(err)
		}
		phases := []int{0}
		if corrected {
			phases = []int{1, 2, 3, 4}
		}
		for _, phase := range phases {
			switch phase {
			case 2:
				pricingOK(t, settings.SetEvaluationDate(testDate(t, 13, 9, 2007)))
				pricingOK(t, index.AddFixing(testDate(t, 1, 8, 2007), 208.4))
			case 3:
				pricingOK(t, settings.SetEvaluationDate(testDate(t, 13, 10, 2007)))
			case 4:
				pricingOK(t, quotes[0].SetValue(rates[0]/100+.0005))
			}
			nodes := pricingMust(curve.Nodes())
			if len(nodes) != 15 || pricingMust(curve.BaseDate()) != nodes[0].Date {
				t.Fatal("lazy node base")
			}
			checked := 0
			for _, row := range rows[1:] {
				if row[0] != strconv.Itoa(phase) {
					continue
				}
				n, err := strconv.Atoi(row[1])
				if err != nil {
					t.Fatal(err)
				}
				serial, err := strconv.Atoi(row[2])
				if err != nil {
					t.Fatal(err)
				}
				rate, err := strconv.ParseFloat(row[3], 64)
				if err != nil {
					t.Fatal(err)
				}
				forecast, err := strconv.ParseFloat(row[4], 64)
				if err != nil {
					t.Fatal(err)
				}
				if nodes[n].Date.Serial() != int32(serial) {
					t.Fatal("oracle node date")
				}
				pricingNear(t, nodes[n].Rate, rate, 1e-12)
				pricingNear(t, pricingMust(index.Fixing(testDate(t, 1, 8, 2012), false)), forecast, 1e-7)
				checked++
			}
			if checked != 15 {
				t.Fatal("oracle phase incomplete")
			}
		}
		other := pricingMust(NewSession())
		otherSettings := pricingMust(other.NewSettings())
		foreign := pricingMust(other.NewUKRPI(otherSettings))
		if _, err := s.NewPiecewiseZeroInflationCurveWithLastFixingDate(config, foreign, helpers, nil); !errors.Is(err, ErrSessionMismatch) {
			t.Fatalf("foreign index: %v", err)
		}
		pricingOK(t, other.Close())
		before := pricingMust(curve.Nodes())
		pricingOK(t, quotes[0].SetValue(rates[0]/100+.001))
		for _, helper := range helpers {
			pricingOK(t, helper.Close())
		}
		for _, quote := range quotes {
			pricingOK(t, quote.Close())
		}
		pricingOK(t, index.Close())
		pricingOK(t, settings.Close())
		after := pricingMust(curve.Nodes())
		if after[1].Rate == before[1].Rate {
			t.Fatal("closed dependencies lost pending recalibration")
		}
		if _, err := s.NewPiecewiseZeroInflationCurveWithLastFixingDate(config, nil, nil, nil); err == nil {
			t.Fatal("nil index accepted")
		}
		pricingOK(t, s.Close())
	}
}
