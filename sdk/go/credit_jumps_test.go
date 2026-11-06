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

func piecewiseJumpHelpers(s *Session, settings *Settings, discount *YieldTermStructure) []*DefaultProbabilityHelper {
	cal := pricingMust(s.Target())
	dc := pricingMust(s.Actual365Fixed())
	var helpers []*DefaultProbabilityHelper
	for i, spread := range []float64{.3, .25, .22} {
		quote := pricingMust(s.NewSimpleQuote(spread))
		helpers = append(helpers, pricingMust(s.NewSpreadCdsHelper(SpreadCdsHelperConfig{RunningSpread: quote, Tenor: Period{int32(i + 1), Years}, SettlementDays: 1, Calendar: cal, Frequency: Quarterly, PaymentConvention: Following, Rule: TwentiethIMM, DayCounter: dc, RecoveryRate: .4, DiscountCurve: discount, Settings: settings})))
	}
	return helpers
}

func unjumpedSurvival(curve *PiecewiseDefaultCurve, t float64) float64 {
	times, rates := pricingMust(curve.Times()), pricingMust(curve.Data())
	integral := 0.0
	for i := 1; i < len(times) && times[i-1] < t; i++ {
		integral += rates[i] * (math.Min(t, times[i]) - times[i-1])
	}
	return math.Exp(-integral)
}

func requireNodesMoved(t *testing.T, before, after []float64, what string) {
	t.Helper()
	if len(before) != len(after) {
		t.Fatalf("%s: node count changed from %d to %d", what, len(before), len(after))
	}
	moved := 0.0
	for i := range before {
		moved = math.Max(moved, math.Abs(before[i]-after[i]))
	}
	if moved <= 1e-6 {
		t.Fatalf("%s: bootstrapped nodes moved by only %g", what, moved)
	}
}

func TestPiecewiseDefaultJumpsQuantLibOracle(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	today := pricingMust(NewDate(15, 6, 2026))
	dc := pricingMust(s.Actual365Fixed())
	settings := pricingMust(s.NewSettings())
	pricingOK(t, settings.SetEvaluationDate(today))
	pricingOK(t, settings.SetIncludeTodaysCashFlows(pricingPtr(true)))
	discount := pricingMust(s.NewFlatForward(today, .03, dc))
	plain := pricingMust(s.NewPiecewiseDefaultCurveWithConfig(PiecewiseDefaultConfig{Helpers: piecewiseJumpHelpers(s, settings, discount), ReferenceDate: today, DayCounter: dc}))
	for _, time := range []float64{0, .3, 1, 2.5} {
		pricingNear(t, unjumpedSurvival(plain, time), pricingMust(plain.SurvivalProbability(time, false)), 1e-14)
	}
	first := pricingMust(s.NewSimpleQuote(.9))
	second := pricingMust(s.NewSimpleQuote(.8))
	cfg := PiecewiseDefaultConfig{Helpers: piecewiseJumpHelpers(s, settings, discount), ReferenceDate: today, DayCounter: dc, JumpQuotes: []*SimpleQuote{first, second}, JumpDates: []Date{pricingMust(today.AddDays(100)), pricingMust(today.AddDays(200))}}
	explicit := pricingMust(s.NewPiecewiseDefaultCurveWithConfig(cfg))
	requireNodesMoved(t, pricingMust(plain.Data()), pricingMust(explicit.Data()), "jumps entering the bootstrap")
	cfg.Helpers, cfg.Settings, cfg.Calendar, cfg.JumpDates = piecewiseJumpHelpers(s, settings, discount), settings, pricingMust(s.NullCalendar()), nil
	moving := pricingMust(s.NewPiecewiseDefaultCurveWithConfig(cfg))
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
			before := pricingMust(explicit.Data())
			pricingOK(t, first.SetValue(.85))
			requireNodesMoved(t, before, pricingMust(explicit.Data()), "jump quote change")
		case "moving_initial":
			pricingOK(t, first.SetValue(.9))
			curve = moving
		case "moving_shifted":
			pricingOK(t, settings.SetEvaluationDate(pricingMust(NewDate(2, 1, 2027))))
			curve = moving
		}
		time := pricingMust(strconv.ParseFloat(row[1], 64))
		factor := pricingMust(strconv.ParseFloat(row[2], 64)) * pricingMust(strconv.ParseFloat(row[5], 64)) / pricingMust(strconv.ParseFloat(row[4], 64))
		base := unjumpedSurvival(curve, time)
		hazard := pricingMust(curve.HazardRate(time, false))
		survival := pricingMust(curve.SurvivalProbability(time, false))
		pricingNear(t, survival/base, factor, 1e-13)
		pricingNear(t, pricingMust(curve.DefaultProbability(time, false)), 1-survival, 1e-15)
		pricingNear(t, pricingMust(curve.DefaultDensity(time, false)), hazard*base, 1e-15)
	}
	if pricingMust(moving.Dates())[0] != pricingMust(NewDate(2, 1, 2027)) {
		t.Fatal("moving curve reference did not roll")
	}
	times := pricingMust(moving.JumpTimes())
	pricingNear(t, times[0], -2.0/365, 1e-15)
	pricingNear(t, times[1], 363.0/365, 1e-15)
}

func TestPiecewiseDefaultMovingRollMatchesFreshFixed(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	today := pricingMust(NewDate(15, 6, 2026))
	dc := pricingMust(s.Actual365Fixed())
	cal := pricingMust(s.Target())
	settings := pricingMust(s.NewSettings())
	pricingOK(t, settings.SetEvaluationDate(today))
	pricingOK(t, settings.SetIncludeTodaysCashFlows(pricingPtr(true)))
	discount := pricingMust(s.NewFlatForward(today, .03, dc))
	jumps := []*SimpleQuote{pricingMust(s.NewSimpleQuote(.9)), pricingMust(s.NewSimpleQuote(.8))}
	for _, quotes := range [][]*SimpleQuote{nil, jumps} {
		pricingOK(t, settings.SetEvaluationDate(today))
		moving := pricingMust(s.NewPiecewiseDefaultCurveWithConfig(PiecewiseDefaultConfig{Helpers: piecewiseJumpHelpers(s, settings, discount), SettlementDays: 2, Calendar: cal, DayCounter: dc, Settings: settings, JumpQuotes: quotes}))
		oldReference := pricingMust(moving.Dates())[0]
		if oldReference != pricingMust(cal.Advance(today, 2, Days, Following, false)) {
			t.Fatal("moving reference ignores settlement days")
		}
		oldTimes := pricingMust(moving.JumpTimes())
		rolled := pricingMust(NewDate(15, 7, 2026))
		pricingOK(t, settings.SetEvaluationDate(rolled))
		reference := pricingMust(cal.Advance(rolled, 2, Days, Following, false))
		if pricingMust(moving.Dates())[0] != reference {
			t.Fatal("moving reference did not roll")
		}
		var fresh *PiecewiseDefaultCurve
		probes := []float64{.1, 1, 1.5, 2, 2.9}
		if quotes == nil {
			fresh = pricingMust(s.NewPiecewiseDefaultCurve(reference, piecewiseJumpHelpers(s, settings, discount), dc))
		} else {
			fresh = pricingMust(s.NewPiecewiseDefaultCurveWithConfig(PiecewiseDefaultConfig{Helpers: piecewiseJumpHelpers(s, settings, discount), ReferenceDate: reference, DayCounter: dc, JumpQuotes: quotes, JumpDates: pricingMust(moving.JumpDates())}))
			newTimes := pricingMust(moving.JumpTimes())
			if newTimes[0] >= oldTimes[0] {
				t.Fatal("jump times did not follow the rolled reference")
			}
			probes = append(probes, (oldTimes[0]+newTimes[0])/2)
		}
		rolledNodes, freshNodes := pricingMust(moving.Nodes()), pricingMust(fresh.Nodes())
		if len(rolledNodes) != len(freshNodes) {
			t.Fatal("node count differs")
		}
		for i := range rolledNodes {
			if rolledNodes[i].Date != freshNodes[i].Date {
				t.Fatal("node dates differ")
			}
			pricingNear(t, rolledNodes[i].Rate, freshNodes[i].Rate, 1e-10)
		}
		for _, time := range probes {
			pricingNear(t, pricingMust(moving.SurvivalProbability(time, false)), pricingMust(fresh.SurvivalProbability(time, false)), 1e-10)
		}
	}
}

// TestPiecewiseDefaultConfigInvalidArguments reaches the curve's own unset-date
// errors through helpers on a different, dated Settings, the mismatch
// PiecewiseDefaultConfig warns against. The SDK has no other route: a CDS helper
// needs the evaluation date at construction and nothing can unset one later.
// credit_api.rs covers the same errors on one shared Settings at the C ABI.
func TestPiecewiseDefaultConfigInvalidArguments(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	other := pricingMust(NewSession())
	defer other.Close()
	today := pricingMust(NewDate(15, 6, 2026))
	dc := pricingMust(s.Actual365Fixed())
	settings := pricingMust(s.NewSettings())
	helperSettings := pricingMust(s.NewSettings())
	pricingOK(t, helperSettings.SetEvaluationDate(today))
	pricingOK(t, helperSettings.SetIncludeTodaysCashFlows(pricingPtr(true)))
	discount := pricingMust(s.NewFlatForward(today, .03, dc))
	jump := pricingMust(today.AddDays(100))
	if _, err := s.NewSpreadCdsHelper(SpreadCdsHelperConfig{RunningSpread: pricingMust(s.NewSimpleQuote(.3)), Tenor: Period{1, Years}, SettlementDays: 1, Calendar: pricingMust(s.Target()), Frequency: Quarterly, PaymentConvention: Following, Rule: TwentiethIMM, DayCounter: dc, RecoveryRate: .4, DiscountCurve: discount, Settings: settings}); err == nil {
		t.Fatal("CDS helper accepted missing evaluation date")
	}
	cfg := PiecewiseDefaultConfig{Helpers: piecewiseJumpHelpers(s, helperSettings, discount), Settings: settings, Calendar: pricingMust(s.NullCalendar()), DayCounter: dc, JumpQuotes: []*SimpleQuote{pricingMust(s.NewSimpleQuote(.9))}}
	if _, err := s.NewPiecewiseDefaultCurveWithConfig(cfg); err == nil {
		t.Fatal("automatic jump dates accepted missing evaluation date")
	}
	cfg.JumpDates = []Date{jump}
	curve := pricingMust(s.NewPiecewiseDefaultCurveWithConfig(cfg))
	if dates := pricingMust(curve.JumpDates()); len(dates) != 1 || dates[0] != jump {
		t.Fatal("explicit jump date requires evaluation date")
	}
	if _, err := curve.JumpTimes(); err == nil {
		t.Fatal("jump time accepted missing evaluation date")
	}
	plain := pricingMust(s.NewPiecewiseDefaultCurveWithConfig(PiecewiseDefaultConfig{Helpers: piecewiseJumpHelpers(s, helperSettings, discount), Settings: settings, Calendar: cfg.Calendar, DayCounter: dc}))
	for _, moving := range []*PiecewiseDefaultCurve{curve, plain} {
		if _, err := moving.SurvivalProbability(1, false); err == nil {
			t.Fatal("moving curve read accepted missing evaluation date")
		}
	}
	pricingOK(t, settings.SetEvaluationDate(today))
	pricingNear(t, pricingMust(curve.JumpTimes())[0], 100.0/365, 1e-15)
	for _, moving := range []*PiecewiseDefaultCurve{curve, plain} {
		if survival := pricingMust(moving.SurvivalProbability(1, false)); survival <= 0 || survival >= 1 {
			t.Fatalf("moving curve survival %g after setting the evaluation date", survival)
		}
	}
	for name, bad := range map[string]PiecewiseDefaultConfig{
		"no helpers":         {ReferenceDate: today, DayCounter: dc},
		"nil helper":         {Helpers: []*DefaultProbabilityHelper{nil}, ReferenceDate: today, DayCounter: dc},
		"nil day counter":    {Helpers: cfg.Helpers, ReferenceDate: today},
		"settings, calendar": {Helpers: cfg.Helpers, DayCounter: dc, Settings: settings},
		"nil jump quote":     {Helpers: cfg.Helpers, ReferenceDate: today, DayCounter: dc, JumpQuotes: []*SimpleQuote{nil}},
		"foreign jump quote": {Helpers: cfg.Helpers, ReferenceDate: today, DayCounter: dc, JumpQuotes: []*SimpleQuote{pricingMust(other.NewSimpleQuote(.9))}},
		"jump count":         {Helpers: cfg.Helpers, ReferenceDate: today, DayCounter: dc, JumpQuotes: cfg.JumpQuotes, JumpDates: []Date{jump, today}},
	} {
		if _, err := s.NewPiecewiseDefaultCurveWithConfig(bad); err == nil {
			t.Fatalf("%s accepted", name)
		}
	}
}
