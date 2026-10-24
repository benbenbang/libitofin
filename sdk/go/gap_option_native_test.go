package itofin

import (
	"crypto/sha256"
	"encoding/csv"
	"fmt"
	"math"
	"os"
	"strconv"
	"testing"
)

func TestGapNativeGridLadder(t *testing.T) {
	data := pricingMust(os.ReadFile("testdata/gap_fd_reference.csv"))
	if fmt.Sprintf("%x", sha256.Sum256(data)) != "2d90130bb09080fae340d4fcd2743102a49244aee950b25efce05b7fa4d461b3" {
		t.Fatal("independent native fixture bytes changed")
	}
	file := pricingMust(os.Open("testdata/gap_fd_reference.csv"))
	defer file.Close()
	rows := pricingMust(csv.NewReader(file).ReadAll())
	if len(rows) != 241 {
		t.Fatal("native grid fixture coverage")
	}
	ordinary := 0
	for _, row := range rows[1:] {
		f := func(index int) float64 {
			value := pricingMust(strconv.ParseFloat(row[index], 64))
			if math.IsNaN(value) || math.IsInf(value, 0) {
				t.Fatal("nonfinite native CSV scalar")
			}
			return value
		}
		if f(3) <= 0 || f(6) <= 0 || f(7) <= 0 || f(8) <= 0 {
			continue
		}
		ordinary++
		t.Run(row[0]+"-grid"+row[10]+"-damping"+row[12], func(t *testing.T) {
			s := pricingMust(NewSession())
			defer s.Close()
			today := pricingMust(NewDate(15, 6, 2026))
			settings := pricingMust(s.NewSettings())
			pricingOK(t, settings.SetEvaluationDate(today))
			dc := pricingMust(s.Actual360())
			process := pricingMust(s.NewBlackScholesProcess(BlackScholesConfig{Spot: f(3), RiskFreeRate: f(4), DividendYield: f(5), Volatility: f(6), ReferenceDate: today, DayCounter: dc}))
			kind := Call
			if row[2] == "put" {
				kind = Put
			}
			o := pricingMust(s.NewGapOption(kind, f(8), f(9), pricingMust(today.AddDays(int64(f(7)))), settings))
			engine := pricingMust(s.NewFdBlackScholesVanillaEngine(process, FdConfig{TGrid: pricingPtr(uint(f(10))), XGrid: pricingPtr(uint(f(11))), DampingSteps: pricingPtr(uint(f(12)))}))
			pricingOK(t, o.SetFdEngine(engine))
			for i, query := range []func() (float64, error){o.NPV, o.Delta, o.Gamma, o.Theta} {
				tolerance := 1e-8
				if i == 3 {
					tolerance = 1e-7
				}
				gapNear(t, pricingMust(query()), f(13+i), tolerance)
			}
		})
	}
	if ordinary != 192 {
		t.Fatalf("ordinary native grid rows %d, want 192", ordinary)
	}
}
