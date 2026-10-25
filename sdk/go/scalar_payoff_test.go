package itofin

import (
	"encoding/csv"
	"errors"
	"math"
	"os"
	"strconv"
	"sync"
	"testing"
)

func scalarPayoffForTest(s *Session, kind PayoffKind, option OptionType, strike, extra float64) (*ScalarPayoff, error) {
	switch kind {
	case PayoffPlainVanilla:
		return s.NewPlainVanillaPayoff(option, strike)
	case PayoffCashOrNothing:
		return s.NewCashOrNothingPayoff(option, strike, extra)
	case PayoffAssetOrNothing:
		return s.NewAssetOrNothingPayoff(option, strike)
	case PayoffGap:
		return s.NewGapPayoff(option, strike, extra)
	case PayoffPercentageStrike:
		return s.NewPercentageStrikePayoff(option, strike)
	default:
		return s.newScalarPayoff(kind, option, strike, extra)
	}
}

func TestScalarPayoffTermsTriggersAndSignedValues(t *testing.T) {
	s := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()) })
	for _, option := range []OptionType{Call, Put} {
		for _, row := range []struct {
			kind            PayoffKind
			strike, extra   float64
			price, expected float64
		}{
			{PayoffPlainVanilla, 100, 0, 100, 0},
			{PayoffCashOrNothing, 100, -7, 100, 0},
			{PayoffAssetOrNothing, 100, 0, 100, 0},
			{PayoffGap, 100, 110, 100, -10},
			{PayoffPercentageStrike, .8, 0, -100, -20},
		} {
			if option == Put {
				if row.kind == PayoffGap {
					row.expected = 10
				}
				if row.kind == PayoffPercentageStrike {
					row.expected = 0
				}
			}
			p := pricingMust(scalarPayoffForTest(s, row.kind, option, row.strike, row.extra))
			terms := pricingMust(p.Parameters())
			if terms.Kind != row.kind || terms.OptionType != option || terms.Strike != row.strike || terms.Extra != row.extra {
				t.Fatalf("stored terms: %+v", terms)
			}
			got := pricingMust(p.Value(row.price))
			if math.IsNaN(got) || math.Abs(got-row.expected) > 1e-12 {
				t.Fatalf("payoff %v/%v: %.17g != %.17g", row.kind, option, got, row.expected)
			}
			pricingOK(t, p.Close())
		}
	}
	cash := pricingMust(s.NewCashOrNothingPayoff(Call, 100, -7))
	if pricingMust(cash.Value(math.Nextafter(100, math.Inf(1)))) != -7 {
		t.Fatal("cash strict upper trigger")
	}
	if pricingMust(cash.Value(math.Nextafter(100, math.Inf(-1)))) != 0 {
		t.Fatal("cash strict lower trigger")
	}
	asset := pricingMust(s.NewAssetOrNothingPayoff(Call, -2))
	if pricingMust(asset.Value(-1)) != -1 {
		t.Fatal("signed asset payout")
	}
}

func TestScalarPayoffDomainsAndLifecycle(t *testing.T) {
	s := pricingMust(NewSession())
	for kind := PayoffPlainVanilla; kind <= PayoffPercentageStrike; kind++ {
		for _, bad := range []float64{math.NaN(), math.Inf(1), math.Inf(-1)} {
			if _, err := scalarPayoffForTest(s, kind, Call, bad, 1); err == nil {
				t.Fatal("nonfinite strike", kind)
			}
			if kind == PayoffCashOrNothing || kind == PayoffGap {
				if _, err := scalarPayoffForTest(s, kind, Call, 1, bad); err == nil {
					t.Fatal("nonfinite extra", kind)
				}
			}
		}
		for _, bad := range []OptionType{-1, 2} {
			if _, err := scalarPayoffForTest(s, kind, bad, 1, 0); err == nil {
				t.Fatal("invalid option", kind)
			}
		}
	}
	for _, kind := range []PayoffKind{-1, 5} {
		if _, err := s.newScalarPayoff(kind, Call, 1, 0); err == nil {
			t.Fatal("invalid kind")
		}
	}
	p := pricingMust(s.NewPlainVanillaPayoff(Call, -math.MaxFloat64))
	for _, bad := range []float64{math.NaN(), math.Inf(1), math.Inf(-1), math.MaxFloat64} {
		if _, err := p.Value(bad); err == nil {
			t.Fatal("nonfinite input/result")
		}
	}
	terms := pricingMust(p.Parameters())
	terms.Strike = 42
	if pricingMust(p.Parameters()).Strike != -math.MaxFloat64 {
		t.Fatal("terms snapshot mutated payoff")
	}
	pricingOK(t, p.Close())
	if _, err := p.Value(1); err == nil {
		t.Fatal("released payoff")
	}
	if _, err := p.Parameters(); err == nil {
		t.Fatal("released terms")
	}
	var nilPayoff *ScalarPayoff
	if _, err := nilPayoff.Value(1); err == nil {
		t.Fatal("nil payoff")
	}
	if _, err := nilPayoff.Parameters(); err == nil {
		t.Fatal("nil terms")
	}
	pricingOK(t, s.Close())
	if _, err := s.NewPlainVanillaPayoff(Call, 1); !errors.Is(err, ErrClosed) {
		t.Fatal("closed session", err)
	}
	var nilSession *Session
	if _, err := nilSession.NewPlainVanillaPayoff(Call, 1); err == nil {
		t.Fatal("nil session")
	}
}

func TestScalarPayoffConcurrentSessionCalls(t *testing.T) {
	s := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()) })
	p := pricingMust(s.NewGapPayoff(Call, 100, 110))
	var group sync.WaitGroup
	for range 12 {
		group.Go(func() {
			for range 10 {
				if value, err := p.Value(100); err != nil || value != -10 {
					t.Errorf("concurrent payoff %v %v", value, err)
				}
			}
		})
	}
	group.Wait()
}

func TestScalarPayoffNativeFixtures(t *testing.T) {
	f := pricingMust(os.Open("testdata/scalar_payoffs.tsv"))
	defer f.Close()
	reader := csv.NewReader(f)
	reader.Comma = '\t'
	data := pricingMust(reader.ReadAll())
	if len(data) != 385 {
		t.Fatal("native fixture row count", len(data)-1)
	}
	s := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()) })
	for index, row := range data[1:] {
		if len(row) != 6 {
			t.Fatal("native fixture columns", index)
		}
		kind := pricingMust(strconv.ParseInt(row[0], 10, 32))
		option := pricingMust(strconv.ParseInt(row[1], 10, 32))
		strike := pricingMust(strconv.ParseFloat(row[2], 64))
		extra := pricingMust(strconv.ParseFloat(row[3], 64))
		price := pricingMust(strconv.ParseFloat(row[4], 64))
		expected := pricingMust(strconv.ParseFloat(row[5], 64))
		p := pricingMust(scalarPayoffForTest(s, PayoffKind(kind), OptionType(option), strike, extra))
		actual := pricingMust(p.Value(price))
		if actual != expected {
			t.Fatalf("native row %d: %.17g != %.17g", index, actual, expected)
		}
		pricingOK(t, p.Close())
	}
}
