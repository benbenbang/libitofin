package itofin

import (
	"encoding/csv"
	"math"
	"os"
	"strconv"
	"testing"
)

func futureRows(t *testing.T, name string) [][]string {
	t.Helper()
	f := pricingMust(os.Open("testdata/sofr_futures_" + name + ".csv"))
	defer f.Close()
	return pricingMust(csv.NewReader(f).ReadAll())[1:]
}
func futureSerial(s string) Date {
	return pricingMust(DateFromSerial(int32(pricingMust(strconv.Atoi(s)))))
}
func futureNear(t *testing.T, actual, expected float64) {
	t.Helper()
	if math.IsNaN(actual) || math.IsInf(actual, 0) || math.Abs(actual-expected) > 1e-9 {
		t.Fatalf("%.17g != %.17g", actual, expected)
	}
}
func TestOvernightFutureQuantLibAccrualAndRetention(t *testing.T) {
	for _, row := range futureRows(t, "accrual") {
		t.Run(row[0]+row[3]+row[4], func(t *testing.T) {
			s := pricingMust(NewSession())
			defer s.Close()
			settings := pricingMust(s.NewSettings())
			today := futureSerial(row[0])
			pricingOK(t, settings.SetEvaluationDate(today))
			dc := pricingMust(s.Actual365Fixed())
			curve := pricingMust(s.NewFlatForward(testDate(t, 17, 6, 2024), .035, dc))
			index := pricingMust(s.NewSofr(curve, settings))
			for _, fixing := range []struct {
				day  int
				rate float64
			}{{18, .02}, {20, .025}, {21, .03}, {24, .04}} {
				d := testDate(t, fixing.day, 6, 2024)
				if d.Serial() < today.Serial() || (row[4] == "1" && d == today) {
					pricingOK(t, index.AddFixing(d, fixing.rate))
				}
			}
			mode := CompoundAveraging
			if row[3] == "0" {
				mode = SimpleAveraging
			}
			convexity := pricingMust(s.NewSimpleQuote(0))
			f := pricingMust(s.NewOvernightIndexFuture(OvernightFutureConfig{Index: index, ValueDate: futureSerial(row[1]), MaturityDate: futureSerial(row[2]), ConvexityAdjustment: convexity, AveragingMethod: &mode}))
			want := pricingMust(strconv.ParseFloat(row[5], 64))
			futureNear(t, pricingMust(f.NPV()), want)
			if pricingMust(f.IsExpired()) || pricingMust(f.ValueDate()) != futureSerial(row[1]) || pricingMust(f.MaturityDate()) != futureSerial(row[2]) {
				t.Fatal("future dates")
			}
			pricingOK(t, index.Close())
			pricingOK(t, curve.Close())
			pricingOK(t, dc.Close())
			pricingOK(t, convexity.SetValue(.001))
			futureNear(t, pricingMust(f.NPV()), want-.1)
			futureNear(t, pricingMust(f.ConvexityAdjustment()), .001)
			pricingOK(t, convexity.Close())
			pricingOK(t, settings.SetEvaluationDate(testDate(t, 1, 7, 2024)))
			if !pricingMust(f.IsExpired()) {
				t.Fatal("not expired")
			}
			futureNear(t, pricingMust(f.NPV()), 0)
		})
	}
}
