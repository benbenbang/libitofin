package main

import (
	"fmt"

	itofin "github.com/benbenbang/libitofin/sdk/go"
)

func main() {
	s, err := itofin.NewSession()
	if err != nil {
		panic(err)
	}
	defer s.Close()
	payoff, err := s.NewGapPayoff(itofin.Call, 100, 110)
	if err != nil {
		panic(err)
	}
	defer payoff.Close()
	value, err := payoff.Value(100)
	if err != nil {
		panic(err)
	}
	terms, err := payoff.Parameters()
	if err != nil {
		panic(err)
	}
	fmt.Printf("kind=%d trigger=%.2f payment=%.2f value=%.2f\n", terms.Kind, terms.Strike, terms.Extra, value)
}
