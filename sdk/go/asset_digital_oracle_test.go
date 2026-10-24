package itofin

import (
	"encoding/csv"
	"math"
	"os"
	"strconv"
	"testing"
)

type assetDigitalReference struct {
	id                                        string
	kind                                      OptionType
	spot, rate, dividend, vol, strike, closed float64
	days                                      int
	grid, damping                             uint
	values                                    [4]float64
}

func assetDigitalReferences(t *testing.T) []assetDigitalReference {
	t.Helper()
	f, err := os.Open("testdata/asset_fd_reference.csv")
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	records, err := csv.NewReader(f).ReadAll()
	if err != nil || len(records) != 65 {
		t.Fatalf("reference rows: %d, error %v", len(records), err)
	}
	var rows []assetDigitalReference
	for _, fields := range records[1:] {
		if len(fields) != 18 || fields[1] != "asset" {
			t.Fatal("invalid asset reference columns")
		}
		number := func(index int) float64 {
			value, err := strconv.ParseFloat(fields[index], 64)
			if err != nil || math.IsNaN(value) || math.IsInf(value, 0) {
				t.Fatal("nonfinite or invalid reference")
			}
			return value
		}
		kind := Call
		if fields[2] == "put" {
			kind = Put
		} else if fields[2] != "call" {
			t.Fatal("invalid reference option type")
		}
		row := assetDigitalReference{id: fields[0], kind: kind, spot: number(3), rate: number(4), dividend: number(5), vol: number(6), days: int(number(7)), strike: number(8), grid: uint(number(10)), damping: uint(number(12)), closed: number(17)}
		if number(10) != number(11) {
			t.Fatal("reference requires equal time and space grids")
		}
		for i := range row.values {
			row.values[i] = number(13 + i)
		}
		rows = append(rows, row)
	}
	return rows
}

func assetDigitalPrice(t *testing.T, s *Session, row assetDigitalReference) [4]float64 {
	t.Helper()
	today := pricingMust(NewDate(15, 6, 2026))
	settings := pricingMust(s.NewSettings())
	pricingOK(t, settings.SetEvaluationDate(today))
	dc := pricingMust(s.Actual360())
	process := pricingMust(s.NewBlackScholesProcess(BlackScholesConfig{Spot: row.spot, RiskFreeRate: row.rate, DividendYield: row.dividend, Volatility: row.vol, ReferenceDate: today, DayCounter: dc}))
	engine := pricingMust(s.NewFdBlackScholesVanillaEngine(process, FdConfig{TGrid: &row.grid, XGrid: &row.grid, DampingSteps: &row.damping}))
	option := pricingMust(s.NewAssetOrNothingOption(row.kind, row.strike, pricingMust(today.AddDays(int64(row.days))), settings))
	value := pricingMust(option.PriceFd(engine))
	result := [4]float64{value, pricingMust(option.Delta()), pricingMust(option.Gamma()), pricingMust(option.Theta())}
	if math.Float64bits(value) != math.Float64bits(pricingMust(option.NPV())) {
		t.Fatal("FD cache changed without notification")
	}
	for _, close := range []func() error{option.Close, engine.Close, process.Close, dc.Close, settings.Close} {
		pricingOK(t, close())
	}
	return result
}

func TestAssetDigitalQuantLibFdOrdinary(t *testing.T) {
	s := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()) })
	envelope := map[uint]float64{100: 0, 200: 0, 400: 0, 800: 0}
	count := 0
	for _, row := range assetDigitalReferences(t) {
		if row.days <= 0 || row.vol <= 0 || row.strike <= 0 {
			continue
		}
		got := assetDigitalPrice(t, s, row)
		for field, want := range row.values {
			tolerance := 1e-8
			if field == 3 {
				tolerance = 1e-6
			}
			if math.IsNaN(got[field]) || math.IsInf(got[field], 0) || math.Abs(got[field]-want) > tolerance {
				t.Fatalf("%s grid%d field%d: got %.17g, want %.17g", row.id, row.grid, field, got[field], want)
			}
		}
		if row.days == 360 {
			envelope[row.grid] = math.Max(envelope[row.grid], math.Abs(got[0]-row.closed))
		}
		count++
	}
	if count != 40 {
		t.Fatalf("ordinary rows: got %d, want40", count)
	}
	if !(envelope[100] > envelope[200] && envelope[200] > envelope[400] && envelope[400] > envelope[800] && envelope[800] < .001) {
		t.Fatalf("FD aggregate convergence envelope: %v", envelope)
	}
}

func TestAssetDigitalFiniteBoundaryPolicy(t *testing.T) {
	s := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()) })
	count := 0
	for _, row := range assetDigitalReferences(t) {
		if row.days > 0 && row.vol > 0 && row.strike > 0 {
			continue
		}
		got := assetDigitalPrice(t, s, row)
		for field, want := range row.values {
			tolerance := 1e-8
			if field == 3 {
				tolerance = 1e-6
			}
			if math.IsNaN(got[field]) || math.IsInf(got[field], 0) || math.Abs(got[field]-want) > tolerance {
				t.Fatalf("separate edge %s grid%d field%d: got%.17g want%.17g", row.id, row.grid, field, got[field], want)
			}
		}
		count++
	}
	if count != 24 {
		t.Fatalf("finite diagnostic rows: %d", count)
	}
}
