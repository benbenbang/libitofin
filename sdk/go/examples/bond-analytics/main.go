package main

import (
	"fmt"
	"log"

	itofin "github.com/benbenbang/libitofin/sdk/go"
)

func main() {
	s, err := itofin.NewSession()
	if err != nil {
		log.Fatal(err)
	}
	defer s.Close()
	settings, err := s.NewSettings()
	if err != nil {
		log.Fatal(err)
	}
	calendar, err := s.NullCalendar()
	if err != nil {
		log.Fatal(err)
	}
	counter, err := s.Actual365Fixed()
	if err != nil {
		log.Fatal(err)
	}
	reference, err := itofin.NewDate(1, 1, 2026)
	if err != nil {
		log.Fatal(err)
	}
	settlement, err := itofin.NewDate(15, 1, 2026)
	if err != nil {
		log.Fatal(err)
	}
	maturity, err := itofin.NewDate(1, 1, 2028)
	if err != nil {
		log.Fatal(err)
	}
	bond, err := s.NewZeroCouponBond(itofin.ZeroCouponBondConfig{BondConfig: itofin.BondConfig{SettlementDays: 2, FaceAmount: 1000, Calendar: calendar, Settings: settings}, MaturityDate: maturity})
	if err != nil {
		log.Fatal(err)
	}
	curve, err := s.NewFlatForward(reference, .035, counter)
	if err != nil {
		log.Fatal(err)
	}
	config := itofin.BondZSpreadConfig{Discount: curve, Compounding: itofin.Compounded, Frequency: itofin.Semiannual, SettlementDate: &settlement}
	price, err := bond.CleanPriceAtZSpread(.007, config)
	if err != nil {
		log.Fatal(err)
	}
	spread, err := bond.ZSpread(price, itofin.BondZSpreadSolveConfig{BondZSpreadConfig: config})
	if err != nil {
		log.Fatal(err)
	}
	fmt.Printf("Clean price per 100: %.12f\nRecovered Z-spread: %.12f\n", price, spread)
}
