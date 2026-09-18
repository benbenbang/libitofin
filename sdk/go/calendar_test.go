package itofin

import (
	"strings"
	"testing"
)

func TestCalendarCountriesAndMarkets(t *testing.T) {
	s, err := NewSession()
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	factories := []struct {
		name    string
		build   func(...string) (*Calendar, error)
		markets []string
	}{
		{"Argentina", s.Argentina, []string{"Merval"}},
		{"Australia", s.Australia, []string{"Settlement", "ASX"}},
		{"Austria", s.Austria, []string{"Settlement", "Exchange"}},
		{"Brazil", s.Brazil, []string{"Settlement", "Exchange"}},
		{"Canada", s.Canada, []string{"Settlement", "TSX"}},
		{"Chile", s.Chile, []string{"SSE"}},
		{"China", s.China, []string{"SSE", "IB"}},
		{"Croatia", s.Croatia, []string{"ZSE"}},
		{"CzechRepublic", s.CzechRepublic, []string{"PSE"}},
		{"France", s.France, []string{"Settlement", "Exchange"}},
		{"Germany", s.Germany, []string{"Settlement", "FrankfurtStockExchange", "Xetra", "Eurex", "Euwax"}},
		{"HongKong", s.HongKong, []string{"HKEx"}},
		{"Iceland", s.Iceland, []string{"ICEX"}},
		{"India", s.India, []string{"NSE"}},
		{"Indonesia", s.Indonesia, []string{"BEJ", "JSX", "IDX"}},
		{"Israel", s.Israel, []string{"Settlement", "TASE", "SHIR", "Telbor"}},
		{"Italy", s.Italy, []string{"Settlement", "Exchange"}},
		{"Malta", s.Malta, []string{"MSE"}},
		{"Mexico", s.Mexico, []string{"BMV"}},
		{"Montenegro", s.Montenegro, []string{"MNSE"}},
		{"NewZealand", s.NewZealand, []string{"Wellington", "Auckland"}},
		{"NorthMacedonia", s.NorthMacedonia, []string{"MSE"}},
		{"Poland", s.Poland, []string{"Settlement", "WSE"}},
		{"Romania", s.Romania, []string{"Public", "BVB"}},
		{"Russia", s.Russia, []string{"Settlement", "MOEX"}},
		{"SaudiArabia", s.SaudiArabia, []string{"Tadawul"}},
		{"Serbia", s.Serbia, []string{"BSE"}},
		{"Singapore", s.Singapore, []string{"SGX"}},
		{"Slovakia", s.Slovakia, []string{"BSSE"}},
		{"Slovenia", s.Slovenia, []string{"LSE"}},
		{"SouthKorea", s.SouthKorea, []string{"Settlement", "KRX"}},
		{"Taiwan", s.Taiwan, []string{"TSEC"}},
		{"Ukraine", s.Ukraine, []string{"USE"}},
		{"UnitedKingdom", s.UnitedKingdom, []string{"Settlement", "Exchange", "Metals"}},
		{"UnitedStates", s.UnitedStates, []string{"Settlement", "NYSE", "GovernmentBond", "NERC", "LiborImpact", "FederalReserve", "SOFR"}},
		{"Uzbekistan", s.Uzbekistan, []string{"UZSE"}},
	}
	for _, factory := range factories {
		t.Run(factory.name, func(t *testing.T) {
			base, err := factory.build()
			if err != nil {
				t.Fatal(err)
			}
			defer base.Close()
			for i, market := range factory.markets {
				cal, err := factory.build(strings.ToLower(market))
				if err != nil {
					t.Fatal(err)
				}
				name, err := cal.Name()
				if err != nil || name == "" {
					t.Fatal(name, err)
				}
				if i == 0 {
					baseName, _ := base.Name()
					if name != baseName {
						t.Fatal("wrong default market", name, baseName)
					}
				}
				if _, err := cal.Adjust(testDate(t, 1, 1, 2013), Following); err != nil {
					t.Fatal(err)
				}
				cal.Close()
			}
			for _, args := range [][]string{{""}, {"invalid"}, {"HKEx"}, {"Settlement", "Settlement"}} {
				if _, err := factory.build(args...); err == nil {
					t.Fatal("accepted invalid markets", args)
				}
			}
		})
	}
	for _, build := range []func() (*Calendar, error){s.Botswana, s.Denmark, s.Finland, s.Hungary, s.Japan, s.Norway, s.SouthAfrica, s.Sweden, s.Switzerland, s.Thailand, s.Turkey} {
		cal, err := build()
		if err != nil {
			t.Fatal(err)
		}
		if name, err := cal.Name(); err != nil || name == "" {
			t.Fatal(name, err)
		}
		cal.Close()
	}
}
