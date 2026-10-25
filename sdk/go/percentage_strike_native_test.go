package itofin

import (
	"bufio"
	"bytes"
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"math"
	"os"
	"testing"
)

func TestPercentageStrikeNativeGridLadder(t *testing.T) {
	data := pricingMust(os.ReadFile("testdata/percentage_fd_reference.jsonl"))
	if fmt.Sprintf("%x", sha256.Sum256(data)) != "d5748a30f7b95fef4c2bc92e9459c56c0b2839060d75c79f632d6d7f4af830b6" {
		t.Fatal("independent native fixture bytes changed")
	}
	scanner := bufio.NewScanner(bytes.NewReader(data))
	count := 0
	for scanner.Scan() {
		var row map[string]json.RawMessage
		pricingOK(t, json.Unmarshal(scanner.Bytes(), &row))
		count++
		t.Run(fmt.Sprint(count), func(t *testing.T) {
			f := func(key string) float64 {
				var value float64
				pricingOK(t, json.Unmarshal(row[key], &value))
				if math.IsNaN(value) || math.IsInf(value, 0) {
					t.Fatal("nonfinite native scalar")
				}
				return value
			}
			s := pricingMust(NewSession())
			defer s.Close()
			today := pricingMust(NewDate(15, 6, 2026))
			settings := pricingMust(s.NewSettings())
			pricingOK(t, settings.SetEvaluationDate(today))
			dc := pricingMust(s.Actual360())
			process := pricingMust(s.NewBlackScholesProcess(BlackScholesConfig{Spot: f("S"), RiskFreeRate: f("r"), DividendYield: f("q"), Volatility: f("sigma"), ReferenceDate: today, DayCounter: dc}))
			var name string
			pricingOK(t, json.Unmarshal(row["kind"], &name))
			kind := Call
			if name == "put" {
				kind = Put
			}
			o := pricingMust(s.NewPercentageStrikeOption(kind, f("m"), pricingMust(today.AddDays(int64(f("days")))), settings))
			engine := pricingMust(s.NewFdBlackScholesVanillaEngine(process, FdConfig{TGrid: pricingPtr(uint(f("grid"))), XGrid: pricingPtr(uint(f("grid")))}))
			pricingOK(t, o.SetFdEngine(engine))
			for i, query := range []func() (float64, error){o.NPV, o.Delta, o.Gamma, o.Theta} {
				tolerance := 1e-8
				if i == 3 {
					tolerance = 1e-7
				}
				percentageNear(t, pricingMust(query()), f([]string{"npv", "delta", "gamma", "theta"}[i]), tolerance)
			}
		})
	}
	pricingOK(t, scanner.Err())
	if count != 48 {
		t.Fatal("native row count")
	}
}
