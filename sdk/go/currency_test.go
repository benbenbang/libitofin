package itofin

import (
	"errors"
	"sync"
	"testing"
)

func TestCustomCurrencyStoredFieldsAndRetainedIndex(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	cfg := CurrencyConfig{Name: "臺灣\x00 dollar", Code: "TWD", NumericCode: 901, Symbol: "NT$\x00元", FractionSymbol: "分\x00", FractionsPerUnit: 100}
	c := pricingMust(s.NewCurrency(cfg))
	if pricingMust(c.Name()) != cfg.Name || pricingMust(c.Code()) != cfg.Code || pricingMust(c.Symbol()) != cfg.Symbol || pricingMust(c.FractionSymbol()) != cfg.FractionSymbol || pricingMust(c.NumericCode()) != cfg.NumericCode || pricingMust(c.FractionsPerUnit()) != cfg.FractionsPerUnit {
		t.Fatal("stored fields did not round trip")
	}
	if pricingMust(c.Repr()) != "Currency(TWD)" {
		t.Fatal("repr changed")
	}
	settings := pricingMust(s.NewSettings())
	cal, dc := pricingMust(s.NullCalendar()), pricingMust(s.Actual360())
	idx := pricingMust(s.NewIborIndex(IborIndexConfig{FamilyName: "custom-currency", Tenor: Period{3, Months}, Currency: c, FixingCalendar: cal, DayCounter: dc, Settings: settings}))
	if err := c.Close(); err != nil {
		t.Fatal(err)
	}
	retained := pricingMust(idx.Currency())
	if pricingMust(retained.Name()) != cfg.Name || pricingMust(retained.Symbol()) != cfg.Symbol {
		t.Fatal("index lost currency after wrapper release")
	}
	if _, err := c.Name(); err == nil {
		t.Fatal("released handle accepted")
	}
	if err := c.Close(); err != nil {
		t.Fatal("Close is not idempotent", err)
	}
	var wg sync.WaitGroup
	for range 8 {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for range 20 {
				if name, err := retained.Name(); err != nil || name != cfg.Name {
					t.Errorf("concurrent name %q: %v", name, err)
				}
			}
		}()
	}
	wg.Wait()
}

func TestCustomCurrencyValidationAndSessionLifecycle(t *testing.T) {
	s := pricingMust(NewSession())
	other := pricingMust(NewSession())
	defer other.Close()
	defer s.Close()
	for _, code := range []string{"", "TW", "TWDD", "台幣", "TW\x00", "T1D"} {
		if c, err := s.NewCurrency(CurrencyConfig{Code: code}); err == nil || c != nil {
			t.Fatalf("invalid code %q accepted", code)
		}
	}
	c := pricingMust(s.NewCurrency(CurrencyConfig{Code: "abc", NumericCode: -1, FractionsPerUnit: -1}))
	if pricingMust(c.Name()) != "" || pricingMust(c.Symbol()) != "" || pricingMust(c.FractionSymbol()) != "" || pricingMust(c.Code()) != "abc" || pricingMust(c.NumericCode()) != -1 || pricingMust(c.FractionsPerUnit()) != -1 {
		t.Fatal("empty, signed or lowercase fields normalized")
	}
	foreign := &Currency{object{other, c.id}}
	if _, err := foreign.Name(); err == nil {
		t.Fatal("foreign handle accepted")
	}
	settings := pricingMust(s.NewSettings())
	wrong := &Currency{settings.object}
	if _, err := wrong.Name(); err == nil {
		t.Fatal("wrong-type handle accepted")
	}
	foreignCurrency := pricingMust(other.USD())
	_, err := s.NewIborIndex(IborIndexConfig{FamilyName: "foreign", Tenor: Period{3, Months}, Currency: foreignCurrency, FixingCalendar: pricingMust(s.NullCalendar()), DayCounter: pricingMust(s.Actual360()), Settings: settings})
	if !errors.Is(err, ErrSessionMismatch) {
		t.Fatalf("cross-session currency: %v", err)
	}
	var nilCurrency *Currency
	for _, getter := range []func() (string, error){nilCurrency.Name, nilCurrency.Code, nilCurrency.Symbol, nilCurrency.FractionSymbol} {
		if _, err := getter(); err == nil {
			t.Fatal("nil currency accepted")
		}
	}
	if _, err := (&Currency{}).Name(); err == nil {
		t.Fatal("zero currency accepted")
	}
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err := c.Name(); !errors.Is(err, ErrClosed) {
		t.Fatalf("closed session: %v", err)
	}
	if _, err := s.NewCurrency(CurrencyConfig{Code: "TWD"}); !errors.Is(err, ErrClosed) {
		t.Fatalf("closed constructor: %v", err)
	}
	if _, err := c.NumericCode(); !errors.Is(err, ErrClosed) {
		t.Fatalf("closed integer: %v", err)
	}
	if err := c.Close(); err != nil {
		t.Fatal(err)
	}
}
