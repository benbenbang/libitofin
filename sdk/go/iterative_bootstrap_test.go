package itofin

import (
	"math"
	"os"
	"regexp"
	"strconv"
	"strings"
	"testing"
)

func iterativeInputs(t *testing.T, s *Session, rate float64) (PiecewiseCurveConfig, *SimpleQuote, *Settings, *IborIndex) {
	t.Helper()
	settings := sessionMust(s.NewSettings())
	reference := curveDate(t, 15, 6, 2026)
	if err := settings.SetEvaluationDate(reference); err != nil {
		t.Fatal(err)
	}
	dc := sessionMust(s.Actual365Fixed())
	index := sessionMust(s.NewIborIndex(IborIndexConfig{FamilyName: "oracle", Tenor: Period{1, Years}, Currency: sessionMust(s.EUR()), FixingCalendar: sessionMust(s.NullCalendar()), Convention: Unadjusted, DayCounter: dc, Settings: settings}))
	quote := sessionMust(s.NewSimpleQuote(rate))
	helper := sessionMust(s.NewDepositRateHelper(quote, index))
	return PiecewiseCurveConfig{ReferenceDate: reference, Helpers: []*RateHelper{helper}, DayCounter: dc}, quote, settings, index
}

func iterativeOracle(t *testing.T) map[string][3]float64 {
	t.Helper()
	raw, err := os.ReadFile("testdata/iterative_bootstrap_oracle.txt")
	if err != nil {
		t.Fatal(err)
	}
	result := map[string][3]float64{}
	pattern := regexp.MustCompile(`(?m)^(\w+) \(([^)]+)\) (\S+)$`)
	for _, row := range pattern.FindAllStringSubmatch(string(raw), -1) {
		fields := append(strings.Split(row[2], ","), row[3])
		var values [3]float64
		if len(fields) != 3 {
			t.Fatal(fields)
		}
		for i, s := range fields {
			values[i], err = strconv.ParseFloat(strings.TrimSpace(s), 64)
			if err != nil {
				t.Fatal(err)
			}
		}
		result[row[1]] = values
	}
	if len(result) != 7 {
		t.Fatal(result)
	}
	return result
}

func iterativeNear(t *testing.T, got, want float64) {
	t.Helper()
	if !(math.Abs(got-want) < 1e-12) {
		t.Fatalf("got %.17g want %.17g", got, want)
	}
}

func TestIterativeBootstrapQuantLibOracle(t *testing.T) {
	oracle := iterativeOracle(t)
	cases := []struct {
		name            string
		rate, lo, hi    float64
		attempts, evals uint
		fallback        bool
	}{
		{"positive_min", .015, .04, .1, 3, 100, false}, {"negative_max", -.015, -.1, -.04, 3, 100, false},
		{"positive", .25, .01, .1, 3, 100, false}, {"negative", -.25, -.1, -.01, 3, 100, false},
		{"fallback_upper", .25, .01, .1, 1, 100, true}, {"fallback_lower", -.25, -.1, -.01, 1, 100, true},
		{"eval_fallback", .25, .01, .4, 1, 1, true},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			s := sessionMust(NewSession())
			defer s.Close()
			cfg, _, _, _ := iterativeInputs(t, s, tc.rate)
			opts := DefaultIterativeBootstrapOptions()
			opts.MinValue, opts.MaxValue, opts.MaxAttempts, opts.MaxEvaluations, opts.DontThrow = &tc.lo, &tc.hi, tc.attempts, tc.evals, tc.fallback
			cfg.IterativeOptions = &opts
			curve := sessionMust(s.NewPiecewiseLinearZero(cfg))
			nodes := sessionMust(curve.Data())
			want := oracle[tc.name]
			if len(nodes) != 2 {
				t.Fatal(nodes)
			}
			iterativeNear(t, nodes[0], want[0])
			iterativeNear(t, nodes[1], want[1])
			iterativeNear(t, sessionMust(curve.Discount(1, false)), want[2])
		})
	}
}

func TestIterativeBootstrapAllYieldFactories(t *testing.T) {
	factories := []struct {
		name     string
		discount bool
		build    func(*Session, PiecewiseCurveConfig) (*YieldTermStructure, error)
	}{
		{"loglinear", true, (*Session).NewPiecewiseLogLinearDiscount},
		{"linear-zero", false, (*Session).NewPiecewiseLinearZero}, {"cubic-zero", false, (*Session).NewPiecewiseCubicZero},
		{"linear-forward", false, (*Session).NewPiecewiseLinearForward}, {"convex-forward", false, (*Session).NewPiecewiseConvexMonotoneForward},
		{"flat-forward", false, (*Session).NewPiecewiseFlatForward},
	}
	for _, interp := range []string{"LogLinear", "Linear", "Cubic"} {
		factories = append(factories, struct {
			name     string
			discount bool
			build    func(*Session, PiecewiseCurveConfig) (*YieldTermStructure, error)
		}{interp, true, func(s *Session, cfg PiecewiseCurveConfig) (*YieldTermStructure, error) {
			cfg.Interpolation = interp
			return s.NewPiecewiseYieldCurve(cfg)
		}})
	}
	for _, factory := range factories {
		t.Run(factory.name, func(t *testing.T) {
			s := sessionMust(NewSession())
			defer s.Close()
			for _, attempts := range []uint{1, 3} {
				cfg, _, _, _ := iterativeInputs(t, s, .25)
				lo, hi := .01, .1
				if factory.discount {
					lo, hi = .9, 1
				}
				accuracy := 1e-12
				opts := DefaultIterativeBootstrapOptions()
				opts.MinValue, opts.MaxValue, opts.MaxAttempts, opts.Accuracy = &lo, &hi, attempts, &accuracy
				cfg.IterativeOptions = &opts
				curve := sessionMust(factory.build(s, cfg))
				got, err := curve.Discount(1, false)
				if attempts == 1 {
					if err == nil {
						t.Fatal("narrow bounds ignored")
					}
				} else {
					if err != nil {
						t.Fatal(err)
					}
					iterativeNear(t, got, .8)
				}
			}
			for _, explicit := range []bool{false, true} {
				cfg, _, _, _ := iterativeInputs(t, s, .25)
				if explicit {
					opts := DefaultIterativeBootstrapOptions()
					cfg.IterativeOptions = &opts
				}
				iterativeNear(t, sessionMust(sessionMust(factory.build(s, cfg)).Discount(1, false)), .8)
			}
		})
	}
}

func TestIterativeBootstrapRetryRetentionAndErrors(t *testing.T) {
	s := sessionMust(NewSession())
	defer s.Close()
	cfg, quote, settings, index := iterativeInputs(t, s, .25)
	lo, hi := .01, .1
	opts := DefaultIterativeBootstrapOptions()
	opts.MinValue, opts.MaxValue, opts.MaxAttempts = &lo, &hi, 2
	cfg.IterativeOptions = &opts
	curve := sessionMust(s.NewPiecewiseLinearZero(cfg))
	if _, err := curve.Discount(1, false); err == nil {
		t.Fatal("strict failure lost")
	}
	if err := quote.SetValue(.1); err != nil {
		t.Fatal(err)
	}
	iterativeNear(t, sessionMust(curve.Discount(1, false)), 1/1.1)
	if err := quote.SetValue(.15); err != nil {
		t.Fatal(err)
	}
	opts.MaxAttempts = 0
	for _, close := range []func() error{quote.Close, cfg.Helpers[0].Close, settings.Close, index.Close, cfg.DayCounter.Close} {
		if err := close(); err != nil {
			t.Fatal(err)
		}
	}
	iterativeNear(t, sessionMust(curve.Discount(1, false)), 1/1.15)
	if _, err := s.NewPiecewiseLinearZero(cfg); err == nil {
		t.Fatal("closed inputs accepted")
	}
	cfg, quote, settings, _ = iterativeInputs(t, s, .01)
	curve = sessionMust(s.NewPiecewiseLinearZero(cfg))
	iterativeNear(t, sessionMust(curve.Data())[1], math.Log1p(.01))
	if err := quote.SetValue(.4); err != nil {
		t.Fatal(err)
	}
	iterativeNear(t, sessionMust(curve.Data())[1], math.Log1p(.4))
	if err := quote.SetValue(10); err != nil {
		t.Fatal(err)
	}
	if _, err := curve.Data(); err == nil {
		t.Fatal("unrecoverable quote accepted")
	}
	if err := quote.SetValue(.01); err != nil {
		t.Fatal(err)
	}
	iterativeNear(t, sessionMust(curve.Data())[1], math.Log1p(.01))
	tomorrow := curveDate(t, 16, 6, 2026)
	if err := settings.SetEvaluationDate(tomorrow); err != nil {
		t.Fatal(err)
	}
	maturity := sessionMust(cfg.Helpers[0].MaturityDate())
	iterativeNear(t, sessionMust(curve.DiscountDate(maturity, false))/sessionMust(curve.DiscountDate(tomorrow, false)), 1/1.01)

	other := sessionMust(NewSession())
	defer other.Close()
	foreign, _, _, _ := iterativeInputs(t, other, .1)
	cfg.Helpers = foreign.Helpers
	if _, err := s.NewPiecewiseLinearZero(cfg); err == nil {
		t.Fatal("foreign helper accepted")
	}
	iterativeNear(t, sessionMust(curve.DiscountDate(maturity, false))/sessionMust(curve.DiscountDate(tomorrow, false)), 1/1.01)
}

func TestIterativeBootstrapInvalidOptions(t *testing.T) {
	s := sessionMust(NewSession())
	defer s.Close()
	cfg, _, _, _ := iterativeInputs(t, s, .1)
	badFloat := math.NaN()
	cases := []func(*IterativeBootstrapOptions){
		func(x *IterativeBootstrapOptions) { x.Accuracy = &badFloat }, func(x *IterativeBootstrapOptions) { x.MinValue = &badFloat },
		func(x *IterativeBootstrapOptions) { x.MaxAttempts = 0 }, func(x *IterativeBootstrapOptions) { x.MaxFactor = .5 },
		func(x *IterativeBootstrapOptions) { x.MinFactor = math.Inf(1) }, func(x *IterativeBootstrapOptions) { x.DontThrowSteps = 0 },
		func(x *IterativeBootstrapOptions) { x.MaxEvaluations = 0 },
	}
	for _, mutate := range cases {
		opts := DefaultIterativeBootstrapOptions()
		mutate(&opts)
		cfg.IterativeOptions = &opts
		if _, err := s.NewPiecewiseLinearZero(cfg); err == nil {
			t.Fatal("invalid options accepted")
		}
	}
	opts := DefaultIterativeBootstrapOptions()
	cfg.IterativeOptions = &opts
	for _, algorithm := range []string{"global", "local"} {
		cfg.Bootstrap = algorithm
		if _, err := s.NewPiecewiseYieldCurve(cfg); err == nil {
			t.Fatal("algorithm silently ignored options")
		}
	}
	cfg.Bootstrap = "iterative"
	cfg.AdditionalHelpers = cfg.Helpers
	if _, err := s.NewPiecewiseLinearZero(cfg); err == nil {
		t.Fatal("additional helpers accepted")
	}
	cfg.AdditionalHelpers = nil
	iterativeNear(t, sessionMust(sessionMust(s.NewPiecewiseLinearZero(cfg)).Discount(1, false)), 1/1.1)
}

func TestIterativeBootstrapAsymmetricFactors(t *testing.T) {
	for _, negative := range []bool{false, true} {
		s := sessionMust(NewSession())
		defer s.Close()
		rate, lo, hi := .25, .01, .1
		opts := DefaultIterativeBootstrapOptions()
		opts.MaxAttempts = 2
		opts.MaxFactor = 3
		opts.MinFactor = 1
		if negative {
			rate, lo, hi = -.25, -.1, -.01
			opts.MaxFactor = 1
			opts.MinFactor = 3
		}
		opts.MinValue, opts.MaxValue = &lo, &hi
		cfg, _, _, _ := iterativeInputs(t, s, rate)
		cfg.IterativeOptions = &opts
		curve := sessionMust(s.NewPiecewiseLinearZero(cfg))
		iterativeNear(t, sessionMust(curve.Discount(1, false)), 1/(1+rate))
	}
}

func TestIterativeBootstrapAccuracyOverride(t *testing.T) {
	s := sessionMust(NewSession())
	defer s.Close()
	cfg, _, _, _ := iterativeInputs(t, s, .25)
	accuracy := .01
	opts := DefaultIterativeBootstrapOptions()
	opts.Accuracy = &accuracy
	cfg.IterativeOptions = &opts
	curve := sessionMust(s.NewPiecewiseLinearZero(cfg))
	iterativeNear(t, sessionMust(curve.Data())[1], .2254558624429838)
	iterativeNear(t, sessionMust(curve.Discount(1, false)), .7981522881625791)
}
