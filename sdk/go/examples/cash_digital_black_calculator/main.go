package main

import (
	"fmt"
	itofin "github.com/benbenbang/libitofin/sdk/go"
)

func main() {
	session, err := itofin.NewSession()
	if err != nil {
		panic(err)
	}
	defer session.Close()
	calculator, err := session.NewCashDigitalBlackCalculator(itofin.Call, 100, 7, 110, .3, .94)
	if err != nil {
		panic(err)
	}
	defer calculator.Close()
	value, err := calculator.Value()
	if err != nil {
		panic(err)
	}
	delta, err := calculator.DeltaForward()
	if err != nil {
		panic(err)
	}
	vega, err := calculator.Vega(2)
	if err != nil {
		panic(err)
	}
	fmt.Printf("value: %.8f\nforward delta: %.8f\nvega (two-year expiry): %.8f\n", value, delta, vega)
}
