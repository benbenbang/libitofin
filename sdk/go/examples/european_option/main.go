package main

import (
	"errors"
	"fmt"
	"log"

	itofin "github.com/benbenbang/libitofin/sdk/go"
)

func main() {
	if err := run(); err != nil {
		log.Fatal(err)
	}
}

func run() (err error) {
	session, err := itofin.NewSession()
	if err != nil {
		return err
	}
	defer func() { err = errors.Join(err, session.Close()) }()

	today, err := itofin.NewDate(15, 6, 2026)
	if err != nil {
		return err
	}
	expiry, err := today.AddDays(90)
	if err != nil {
		return err
	}
	settings, err := session.NewSettings()
	if err != nil {
		return err
	}
	if err = settings.SetEvaluationDate(today); err != nil {
		return err
	}
	dayCounter, err := session.Actual360()
	if err != nil {
		return err
	}
	process, err := session.NewBlackScholesProcess(itofin.BlackScholesConfig{
		Spot:          60,
		RiskFreeRate:  0.08,
		DividendYield: 0,
		Volatility:    0.30,
		ReferenceDate: today,
		DayCounter:    dayCounter,
	})
	if err != nil {
		return err
	}
	option, err := session.NewVanillaOption(itofin.Call, 65, expiry, settings)
	if err != nil {
		return err
	}
	if err = option.SetEngine(process); err != nil {
		return err
	}

	fmt.Println("European call, K=65, 90d, spot=60, vol=30%, r=8%")
	for _, result := range []struct {
		name  string
		value func() (float64, error)
	}{
		{"NPV", option.NPV},
		{"delta", option.Delta},
		{"gamma", option.Gamma},
		{"theta", option.Theta},
		{"vega", option.Vega},
		{"rho", option.Rho},
		{"dividend_rho", option.DividendRho},
	} {
		value, err := result.value()
		if err != nil {
			return fmt.Errorf("%s: %w", result.name, err)
		}
		fmt.Printf("  %-12s = %.10f\n", result.name, value)
	}
	return nil
}
