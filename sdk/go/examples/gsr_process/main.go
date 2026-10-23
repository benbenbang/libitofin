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
	p, err := s.NewGsrProcess(nil, []float64{.03}, []float64{0}, 10)
	if err != nil {
		log.Fatal(err)
	}
	defer p.Close()
	mean, err := p.Expectation(2, .1, .8)
	if err != nil {
		log.Fatal(err)
	}
	fmt.Printf("GSR mean: %.10f\n", mean)
}
