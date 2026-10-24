package itofin

import (
	"errors"
	"math"
	"testing"
)

func TestAssetDigitalDomains(t *testing.T) {
	s := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, s.Close()) })
	other := pricingMust(NewSession())
	t.Cleanup(func() { pricingOK(t, other.Close()) })
	settings := pricingMust(s.NewSettings())
	expiry := pricingMust(NewDate(15, 6, 2027))
	for _, bad := range []float64{-1, math.NaN(), math.Inf(1), math.Inf(-1)} {
		if _, err := s.NewAssetOrNothingOption(Call, bad, expiry, settings); err == nil {
			t.Fatal("invalid strike accepted")
		}
	}
	if _, err := s.NewAssetOrNothingOption(OptionType(2), 100, expiry, settings); err == nil {
		t.Fatal("invalid option kind accepted")
	}
	if _, err := s.NewAssetOrNothingOption(Call, 100, Date{}, settings); err == nil {
		t.Fatal("invalid expiry accepted")
	}
	if _, err := s.NewAssetOrNothingOption(Call, 100, expiry, nil); err == nil {
		t.Fatal("nil settings accepted")
	}
	if _, err := other.NewAssetOrNothingOption(Call, 100, expiry, settings); !errors.Is(err, ErrSessionMismatch) {
		t.Fatalf("foreign settings: %v", err)
	}
	pricingOK(t, settings.Close())
	if _, err := s.NewAssetOrNothingOption(Call, 100, expiry, settings); err == nil {
		t.Fatal("closed settings accepted")
	}
	pricingOK(t, s.Close())
	if _, err := s.NewAssetOrNothingOption(Call, 100, expiry, settings); err == nil {
		t.Fatal("closed session accepted")
	}
}
