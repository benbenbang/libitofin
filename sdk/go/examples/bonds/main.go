package main

import (
	"errors"
	"fmt"
	itofin "github.com/benbenbang/libitofin/sdk/go"
	"log"
)

func main() {
	if err := run(); err != nil {
		log.Fatal(err)
	}
}
func run() (err error) {
	s, err := itofin.NewSession()
	if err != nil {
		return err
	}
	defer func() { err = errors.Join(err, s.Close()) }()
	today, err := itofin.NewDate(15, 1, 2024)
	if err != nil {
		return err
	}
	maturity, err := itofin.NewDate(15, 1, 2027)
	if err != nil {
		return err
	}
	settings, err := s.NewSettings()
	if err != nil {
		return err
	}
	if err = settings.SetEvaluationDate(today); err != nil {
		return err
	}
	calendar, err := s.NullCalendar()
	if err != nil {
		return err
	}
	counter, err := s.Actual365Fixed()
	if err != nil {
		return err
	}
	curve, err := s.NewFlatForward(today, .04, counter)
	if err != nil {
		return err
	}
	engine, err := s.NewDiscountingBondEngine(curve, settings, nil)
	if err != nil {
		return err
	}
	bond, err := s.NewZeroCouponBond(itofin.ZeroCouponBondConfig{BondConfig: itofin.BondConfig{SettlementDays: 2, FaceAmount: 1000, Calendar: calendar, Settings: settings, IssueDate: &today}, MaturityDate: maturity})
	if err != nil {
		return err
	}
	if err = bond.SetEngine(engine); err != nil {
		return err
	}
	price, err := bond.CleanPrice()
	if err != nil {
		return err
	}
	fmt.Printf("Zero-coupon clean price per 100: %.12f\n", price)
	return nil
}
