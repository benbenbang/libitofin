package main

import (
	"fmt"
	itofin "github.com/benbenbang/libitofin/sdk/go"
	"log"
)

func main() {
	s, err := itofin.NewSession()
	if err != nil {
		log.Fatal(err)
	}
	defer s.Close()
	p, err := s.NewG2Process(.1, .01, .2, .008, -.75)
	if err != nil {
		log.Fatal(err)
	}
	defer p.Close()
	next, err := p.Evolve(0, [2]float64{}, .25, [2]float64{.5, -.3})
	if err != nil {
		log.Fatal(err)
	}
	fmt.Printf("bare factors: %.8f %.8f\n", next[0], next[1])
}
