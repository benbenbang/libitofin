package main

import (
	"errors"
	"fmt"
	"log"

	itofin "github.com/benbenbang/libitofin/sdk/go"
)

func must[T any](value T, err error) T {
	if err != nil {
		panic(err)
	}
	return value
}

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
	today := must(itofin.NewDate(2, 10, 2026))
	expiry := must(today.AddDays(365))
	settings := must(s.NewSettings())
	if err = settings.SetEvaluationDate(today); err != nil {
		return err
	}
	dc := must(s.Actual365Fixed())
	spot := must(s.NewSimpleQuote(100))
	rate := must(s.NewSimpleQuote(.03))
	dividend := must(s.NewSimpleQuote(.01))
	process := must(s.NewBatesProcess(itofin.BatesProcessConfig{
		Spot:     spot,
		RiskFree: must(s.NewFlatForwardFromQuote(today, rate, dc)),
		Dividend: must(s.NewFlatForwardFromQuote(today, dividend, dc)),
		V0:       .04, Kappa: 1.5, Theta: .04, Sigma: .3, Rho: -.5,
		Lambda: .7, Nu: -.12, Delta: .18,
	}))
	model := must(s.NewBatesModel(process))
	engine := must(s.NewBatesEngine(model, 144))
	option := must(s.NewVanillaOption(itofin.Call, 100, expiry, settings))
	if err = option.SetBatesEngine(engine); err != nil {
		return err
	}
	fmt.Println("Bates NPV:", must(option.NPV()))
	if err = spot.SetValue(105); err != nil {
		return err
	}
	fmt.Println("After spot update:", must(option.NPV()))
	fmt.Println("Calibration parameter order:", must(model.Params()))
	return nil
}
