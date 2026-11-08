package itofin

import (
	"math"
	"testing"
)

type yieldJumpCase struct {
	name   string
	tol    float64
	moving bool
	setup  func(*PiecewiseCurveConfig, *int)
}

func yieldJumpCases() []yieldJumpCase {
	accuracy := 1e-14
	modes := []yieldJumpCase{
		{"iterative", 1e-10, false, func(*PiecewiseCurveConfig, *int) {}},
		{"iterative-options", 1e-10, false, func(cfg *PiecewiseCurveConfig, _ *int) {
			opts := DefaultIterativeBootstrapOptions()
			opts.Accuracy = &accuracy
			cfg.Interpolation, cfg.IterativeOptions = "Linear", &opts
		}},
		{"global", 1e-8, false, func(cfg *PiecewiseCurveConfig, _ *int) { cfg.Bootstrap = "global" }},
		{"global-callbacks", 1e-8, false, func(cfg *PiecewiseCurveConfig, calls *int) {
			cfg.Bootstrap = "global"
			cfg.AdditionalPenalties = func(BootstrapState) ([]float64, error) { *calls++; return nil, nil }
		}},
	}
	for _, m := range modes[:4] {
		m.name, m.moving = m.name+"-moving", true
		modes = append(modes, m)
	}
	return modes
}

func yieldJumpStrip(t *testing.T, s *Session, settings *Settings, c yieldJumpCase, calls *int) PiecewiseCurveConfig {
	t.Helper()
	cfg := PiecewiseCurveConfig{ReferenceDate: curveDate(t, 17, 6, 2026), DayCounter: sessionMust(s.Actual365Fixed())}
	if c.moving {
		cfg.Settings, cfg.Calendar, cfg.SettlementDays = settings, sessionMust(s.Target()), 2
	}
	for i, months := range []int32{1, 3, 6, 12} {
		index := sessionMust(s.NewEuribor(Period{months, Months}, nil, settings))
		cfg.Helpers = append(cfg.Helpers, sessionMust(s.NewDepositRateHelperFromRate(.03+.002*float64(i), index)))
	}
	c.setup(&cfg, calls)
	return cfg
}

func yieldJumpSession(t *testing.T) (*Session, *Settings) {
	t.Helper()
	s := sessionMust(NewSession())
	t.Cleanup(func() { _ = s.Close() })
	settings := sessionMust(s.NewSettings())
	if err := settings.SetEvaluationDate(curveDate(t, 15, 6, 2026)); err != nil {
		t.Fatal(err)
	}
	return s, settings
}

// Deposits pin the total discount at each pillar, so with jump dates strictly
// between pillars every jumped node times the jumps before it equals the
// jump-free node: exact, and blind to nothing the bootstrap does with jumps.
func TestPiecewiseYieldJumpsNodeOracle(t *testing.T) {
	for _, c := range yieldJumpCases() {
		t.Run(c.name, func(t *testing.T) {
			s, settings := yieldJumpSession(t)
			calls := 0
			plain := sessionMust(s.NewPiecewiseYieldCurve(yieldJumpStrip(t, s, settings, c, &calls)))
			cfg := yieldJumpStrip(t, s, settings, c, &calls)
			values := []float64{.995, .99}
			jumpDates := []Date{curveDate(t, 3, 8, 2026), curveDate(t, 1, 3, 2027)}
			quotes := []*SimpleQuote{sessionMust(s.NewSimpleQuote(values[0])), sessionMust(s.NewSimpleQuote(values[1]))}
			cfg.JumpQuotes, cfg.JumpDates = quotes, jumpDates
			jumped := sessionMust(s.NewPiecewiseYieldCurve(cfg))
			if got := sessionMust(jumped.JumpDates()); len(got) != 2 || got[0] != jumpDates[0] || got[1] != jumpDates[1] {
				t.Fatalf("jump dates %v", got)
			}
			ds, jv := yieldNodes(t, jumped)
			pds, pv := yieldNodes(t, plain)
			if len(ds) != len(pds) || len(ds) != 5 {
				t.Fatalf("node counts %d vs %d", len(ds), len(pds))
			}
			applied := 0
			for i, d := range ds {
				if d != pds[i] {
					t.Fatalf("node %d date %v vs %v", i, d, pds[i])
				}
				product := 1.0
				for k, jd := range jumpDates {
					if jd.Serial() < d.Serial() {
						product *= values[k]
					}
				}
				if product != 1 {
					applied++
				}
				curveNear(t, jv[i]*product, pv[i], c.tol)
			}
			if applied < 2 {
				t.Fatalf("jumps applied to %d nodes", applied)
			}
			if err := quotes[0].SetValue(.98); err != nil {
				t.Fatal(err)
			}
			_, moved := yieldNodes(t, jumped)
			if math.Abs(moved[2]-jv[2]) < 1e-6 {
				t.Fatalf("jump quote change did not re-bootstrap: %v vs %v", moved[2], jv[2])
			}
			curveNear(t, moved[2]*.98, pv[2], c.tol)
			if cfg.AdditionalPenalties != nil && calls == 0 {
				t.Fatal("penalty callback never ran")
			}
		})
	}
}

func yieldNodes(t *testing.T, c *YieldTermStructure) ([]Date, []float64) {
	t.Helper()
	ds, vs, err := c.Nodes()
	if err != nil {
		t.Fatal(err)
	}
	return ds, vs
}

func TestPiecewiseYieldJumpsMovingRollAndGeneratedDates(t *testing.T) {
	s, settings := yieldJumpSession(t)
	c := yieldJumpCases()[4]
	cfg := yieldJumpStrip(t, s, settings, c, nil)
	cfg.JumpQuotes = []*SimpleQuote{sessionMust(s.NewSimpleQuote(.995)), sessionMust(s.NewSimpleQuote(.99))}
	curve := sessionMust(s.NewPiecewiseYieldCurve(cfg))
	generated := sessionMust(curve.JumpDates())
	if len(generated) != 2 || generated[0] != curveDate(t, 31, 12, 2026) || generated[1] != curveDate(t, 31, 12, 2027) {
		t.Fatalf("generated jump dates %v", generated)
	}
	if err := settings.SetEvaluationDate(curveDate(t, 16, 6, 2026)); err != nil {
		t.Fatal(err)
	}
	c.moving = false
	fixed := yieldJumpStrip(t, s, settings, c, nil)
	fixed.ReferenceDate = curveDate(t, 18, 6, 2026)
	fixed.JumpQuotes, fixed.JumpDates = cfg.JumpQuotes, generated
	fresh := sessionMust(s.NewPiecewiseYieldCurve(fixed))
	if got := sessionMust(curve.ReferenceDate()); got != fixed.ReferenceDate {
		t.Fatalf("rolled reference %v", got)
	}
	rolledTimes, freshTimes := sessionMust(curve.JumpTimes()), sessionMust(fresh.JumpTimes())
	for i := range freshTimes {
		curveNear(t, rolledTimes[i], freshTimes[i], 1e-15)
	}
	for _, x := range []float64{.05, .3, .6, .9, .99} {
		curveNear(t, sessionMust(curve.Discount(x, false)), sessionMust(fresh.Discount(x, false)), 1e-10)
	}
	unset := sessionMust(s.NewSettings())
	cfg.Settings = unset
	if _, err := s.NewPiecewiseYieldCurve(cfg); err == nil {
		t.Fatal("generated jump dates on an unset evaluation date did not error")
	}
	cfg.JumpDates = generated
	lazy := sessionMust(s.NewPiecewiseYieldCurve(cfg))
	if _, err := lazy.Discount(.5, false); err == nil {
		t.Fatal("moving curve read on an unset evaluation date did not error")
	}
}

func TestPiecewiseYieldJumpsInvalidArguments(t *testing.T) {
	s, settings := yieldJumpSession(t)
	other := sessionMust(NewSession())
	defer other.Close()
	base := yieldJumpCases()[0]
	quote := sessionMust(s.NewSimpleQuote(.99))
	for name, mutate := range map[string]func(*PiecewiseCurveConfig){
		"nil quote": func(cfg *PiecewiseCurveConfig) { cfg.JumpQuotes = []*SimpleQuote{nil} },
		"foreign quote": func(cfg *PiecewiseCurveConfig) {
			cfg.JumpQuotes = []*SimpleQuote{sessionMust(other.NewSimpleQuote(.99))}
		},
		"length mismatch": func(cfg *PiecewiseCurveConfig) {
			cfg.JumpQuotes, cfg.JumpDates = []*SimpleQuote{quote, quote}, []Date{curveDate(t, 3, 8, 2026)}
		},
		"dates only": func(cfg *PiecewiseCurveConfig) { cfg.JumpDates = []Date{curveDate(t, 3, 8, 2026)} },
		"global cubic": func(cfg *PiecewiseCurveConfig) {
			cfg.Bootstrap, cfg.Interpolation, cfg.JumpQuotes = "global", "Cubic", []*SimpleQuote{quote}
		},
		"additional helpers iterative": func(cfg *PiecewiseCurveConfig) {
			cfg.AdditionalHelpers, cfg.JumpQuotes = cfg.Helpers[:1], []*SimpleQuote{quote}
		},
	} {
		cfg := yieldJumpStrip(t, s, settings, base, nil)
		mutate(&cfg)
		if _, err := s.NewPiecewiseYieldCurve(cfg); err == nil {
			t.Fatalf("%s: accepted", name)
		}
	}
	flat := sessionMust(s.NewFlatForward(curveDate(t, 17, 6, 2026), .03, sessionMust(s.Actual365Fixed())))
	if got := sessionMust(flat.JumpDates()); len(got) != 0 {
		t.Fatalf("flat curve jumps %v", got)
	}
}
