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
	p, err := s.NewMfStateProcess(0, []float64{0.5, 1.5}, []float64{0.2, 0.4, 0.1})
	if err != nil {
		log.Fatal(err)
	}
	defer p.Close()
	variance, err := p.Variance(0, 0, 1)
	if err != nil {
		log.Fatal(err)
	}
	fmt.Printf("MF state variance: %.10f\n", variance)
}
