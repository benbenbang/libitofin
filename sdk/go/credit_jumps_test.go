package itofin

import (
	"encoding/csv"
	"math"
	"os"
	"strconv"
	"strings"
	"testing"
)

func TestCreditJumpsQuantLibOracle(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	today := pricingMust(NewDate(15, 6, 2026))
	dc := pricingMust(s.Actual365Fixed())
	first := pricingMust(s.NewSimpleQuote(.9))
	second := pricingMust(s.NewSimpleQuote(.8))
	cfg := FlatHazardConfig{ReferenceDate: today, Rate: .02, DayCounter: dc, JumpQuotes: []*SimpleQuote{first, second}, JumpDates: []Date{pricingMust(today.AddDays(100)), pricingMust(today.AddDays(200))}}
	explicit := pricingMust(s.NewFlatHazardRate(cfg))
	settings := pricingMust(s.NewSettings())
	pricingOK(t, settings.SetEvaluationDate(today))
	cfg.Settings, cfg.Calendar, cfg.JumpDates = settings, pricingMust(s.NullCalendar()), nil
	moving := pricingMust(s.NewFlatHazardRate(cfg))
	jumpDates := pricingMust(moving.JumpDates())
	if len(jumpDates) != 2 || jumpDates[0] != pricingMust(NewDate(31, 12, 2026)) || jumpDates[1] != pricingMust(NewDate(31, 12, 2027)) {
		t.Fatal("automatic jump dates changed")
	}
	data := pricingMust(os.ReadFile("testdata/credit_jumps_oracle.csv"))
	rows := pricingMust(csv.NewReader(strings.NewReader(string(data))).ReadAll())
	if len(rows) != 13 {
		t.Fatal("incomplete QuantLib fixture")
	}
	for _, row := range rows[1:] {
		curve := explicit
		switch row[0] {
		case "quote_changed":
			pricingOK(t, first.SetValue(.85))
		case "moving_initial":
			pricingOK(t, first.SetValue(.9))
			curve = moving
		case "moving_shifted":
			pricingOK(t, settings.SetEvaluationDate(pricingMust(NewDate(2, 1, 2027))))
			curve = moving
		}
		time := pricingMust(strconv.ParseFloat(row[1], 64))
		values := []float64{pricingMust(curve.SurvivalProbability(time, false)), pricingMust(curve.DefaultProbability(time, false)), pricingMust(curve.DefaultDensity(time, false)), pricingMust(curve.HazardRate(time, false))}
		for i, value := range values {
			pricingNear(t, value, pricingMust(strconv.ParseFloat(row[i+2], 64)), 1e-15)
		}
	}
	times := pricingMust(moving.JumpTimes())
	pricingNear(t, times[0], -2.0/365, 1e-15)
	pricingNear(t, times[1], 363.0/365, 1e-15)
	if pricingMust(moving.JumpDates())[0] != jumpDates[0] {
		t.Fatal("moving reference changed fixed jump dates")
	}
	jumpDates[0] = Date{}
	times[0] = 12
	if pricingMust(moving.JumpDates())[0] == (Date{}) || pricingMust(moving.JumpTimes())[0] == 12 {
		t.Fatal("inspectors alias native state")
	}
	pricingOK(t, first.Close())
	pricingOK(t, second.Close())
	pricingOK(t, dc.Close())
	pricingOK(t, settings.Close())
	pricingNear(t, pricingMust(explicit.SurvivalProbability(2, false)), .72*math.Exp(-.04), 1e-15)
	pricingNear(t, pricingMust(moving.SurvivalProbability(1, false)), .72*math.Exp(-.02), 1e-15)
}

func TestCreditJumpsInvalidArguments(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	other := pricingMust(NewSession())
	defer other.Close()
	today := pricingMust(NewDate(15, 6, 2026))
	dc := pricingMust(s.Actual365Fixed())
	quote := pricingMust(s.NewSimpleQuote(.9))
	cfg := FlatHazardConfig{ReferenceDate: today, Rate: .02, DayCounter: dc, JumpQuotes: []*SimpleQuote{quote}, JumpDates: []Date{pricingMust(today.AddDays(365))}}
	curve := pricingMust(s.NewFlatHazardRate(cfg))
	for _, invalid := range []float64{0, -.1, 1.01, math.Inf(1)} {
		pricingOK(t, quote.SetValue(invalid))
		pricingNear(t, pricingMust(curve.SurvivalProbability(1, false)), math.Exp(-.02), 1e-15)
		if _, err := curve.SurvivalProbability(1.01, false); err == nil {
			t.Fatal("invalid elapsed jump accepted")
		}
	}
	cfg.JumpDates = append(cfg.JumpDates, today)
	if _, err := s.NewFlatHazardRate(cfg); err == nil {
		t.Fatal("jump count mismatch accepted")
	}
	cfg.JumpDates = nil
	cfg.JumpQuotes = []*SimpleQuote{nil}
	if _, err := s.NewFlatHazardRate(cfg); err == nil {
		t.Fatal("nil quote accepted")
	}
	cfg.JumpQuotes = []*SimpleQuote{pricingMust(other.NewSimpleQuote(.9))}
	if _, err := s.NewFlatHazardRate(cfg); err == nil {
		t.Fatal("foreign quote accepted")
	}
	pricingOK(t, curve.Close())
	if _, err := curve.JumpTimes(); err == nil {
		t.Fatal("closed curve accepted")
	}
	plain := pricingMust(s.NewFlatHazardRate(FlatHazardConfig{ReferenceDate: today, Rate: .02, DayCounter: dc}))
	if len(pricingMust(plain.JumpDates())) != 0 || len(pricingMust(plain.JumpTimes())) != 0 {
		t.Fatal("plain curve has jumps")
	}
}

func TestCreditJumpDatesWithoutEvaluationDate(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	settings := pricingMust(s.NewSettings())
	today := pricingMust(NewDate(15, 6, 2026))
	jump := pricingMust(today.AddDays(100))
	curve := pricingMust(s.NewFlatHazardRate(FlatHazardConfig{
		Settings: settings, Calendar: pricingMust(s.NullCalendar()), Rate: .02,
		DayCounter: pricingMust(s.Actual365Fixed()),
		JumpQuotes: []*SimpleQuote{pricingMust(s.NewSimpleQuote(.9))}, JumpDates: []Date{jump},
	}))
	dates := pricingMust(curve.JumpDates())
	if len(dates) != 1 || dates[0] != jump {
		t.Fatal("explicit jump date requires evaluation date")
	}
	if _, err := curve.JumpTimes(); err == nil {
		t.Fatal("jump time accepted missing evaluation date")
	}
	pricingOK(t, settings.SetEvaluationDate(today))
	if pricingMust(curve.JumpDates())[0] != jump {
		t.Fatal("setting evaluation date changed jump date")
	}
	pricingNear(t, pricingMust(curve.JumpTimes())[0], 100.0/365, 1e-15)
}
