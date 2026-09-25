package itofin

import (
	"encoding/json"
	"errors"
	"math"
	"os"
	"os/exec"
	"strings"
	"testing"
)

type fdOracleValues struct {
	NPV   float64 `json:"npv"`
	Delta float64 `json:"delta"`
	Gamma float64 `json:"gamma"`
	Theta float64 `json:"theta"`
}

func fdCoreOracle(t *testing.T) map[string]fdOracleValues {
	t.Helper()
	var data []byte
	var err error
	if path := os.Getenv("ITOFIN_FD_ORACLE_JSON"); path != "" {
		data, err = os.ReadFile(path)
	} else {
		cmd := exec.Command("cargo", "run", "--quiet", "--release", "-p", "libitofin", "--example", "fd_binding_oracle")
		cmd.Dir = "../.."
		data, err = cmd.Output()
	}
	if err != nil {
		t.Fatalf("load Rust FD oracle: %v", err)
	}
	var oracle map[string]fdOracleValues
	if err := json.Unmarshal(data, &oracle); err != nil {
		t.Fatalf("decode Rust FD oracle: %v", err)
	}
	for _, exercise := range []string{"european", "american", "bermudan"} {
		if _, ok := oracle[exercise]; !ok {
			t.Fatalf("Rust FD oracle missing %s", exercise)
		}
	}
	return oracle
}

func TestFdBlackScholesVanillaCoreOracles(t *testing.T) {
	oracle := fdCoreOracle(t)
	s := pricingMust(NewSession())
	defer s.Close()
	today := pricingMust(NewDate(15, 1, 2025))
	expiry := pricingMust(NewDate(15, 1, 2026))
	settings := pricingMust(s.NewSettings())
	pricingOK(t, settings.SetEvaluationDate(today))
	dc := pricingMust(s.Actual365Fixed())
	p := pricingMust(s.NewBlackScholesProcess(BlackScholesConfig{
		Spot: 80, RiskFreeRate: .05, DividendYield: 0, Volatility: .25,
		ReferenceDate: today, DayCounter: dc,
	}))
	engine := pricingMust(s.NewFdBlackScholesVanillaEngine(p, FdConfig{
		TGrid: pricingPtr(uint(200)), XGrid: pricingPtr(uint(200)),
	}))
	european := pricingMust(s.NewVanillaOption(Put, 100, expiry, settings))
	american := pricingMust(s.NewAmericanOption(Put, 100, today, expiry, settings))
	bermudanExercise := pricingMust(s.NewBermudanExercise([]Date{
		pricingMust(NewDate(15, 4, 2025)), pricingMust(NewDate(15, 7, 2025)),
		pricingMust(NewDate(15, 10, 2025)), expiry,
	}))
	bermudan := pricingMust(s.NewBermudanOption(Put, 100, bermudanExercise, settings))
	for _, row := range []struct {
		name   string
		option *VanillaOption
		want   [3]float64
	}{
		{"European", european, [3]float64{18.266147644485358, -0.71491824907787493, 0.016981361087847299}},
		{"American", american, [3]float64{20.357667204554883, -0.85902979493468978, 0.026600330114210077}},
		{"Bermudan", bermudan, [3]float64{19.954434523211695, -0.81750545328763524, 0.019414167279763642}},
	} {
		t.Run(row.name, func(t *testing.T) {
			pricingOK(t, row.option.SetFdEngine(engine))
			value := pricingMust(row.option.NPV())
			delta := pricingMust(row.option.Delta())
			gamma := pricingMust(row.option.Gamma())
			theta := pricingMust(row.option.Theta())
			got := [4]float64{value, delta, gamma, theta}
			core := oracle[strings.ToLower(row.name)]
			for i, want := range [4]float64{core.NPV, core.Delta, core.Gamma, core.Theta} {
				pricingNear(t, got[i], want, 1e-12)
			}
			for i, want := range row.want {
				pricingNear(t, got[i], want, 1e-12)
			}
			pricingNear(t, pricingMust(row.option.PriceFd(engine)), value, 1e-12)
		})
	}
	if pricingMust(bermudan.NPV()) <= pricingMust(european.NPV())+0.01 ||
		pricingMust(bermudan.NPV()) > pricingMust(american.NPV()) {
		t.Fatal("Bermudan exercise condition is missing")
	}
	pricingOK(t, p.Close())
	pricingOK(t, engine.Close())
	pricingOK(t, settings.SetEvaluationDate(pricingMust(today.AddDays(1))))
	if _, err := american.NPV(); err != nil {
		t.Fatalf("option failed to retain its engine and process: %v", err)
	}
}

func TestFdBlackScholesVanillaConfigAndSessions(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	other := pricingMust(NewSession())
	defer other.Close()
	today := pricingMust(NewDate(15, 1, 2025))
	settings := pricingMust(s.NewSettings())
	pricingOK(t, settings.SetEvaluationDate(today))
	p := pricingMust(s.NewBlackScholesProcess(BlackScholesConfig{
		Spot: 80, RiskFreeRate: .05, Volatility: .25,
		ReferenceDate: today, DayCounter: pricingMust(s.Actual365Fixed()),
	}))
	if _, err := s.NewFdBlackScholesVanillaEngine(nil, FdConfig{}); err == nil {
		t.Fatal("nil process accepted")
	}
	if _, err := other.NewFdBlackScholesVanillaEngine(p, FdConfig{}); !errors.Is(err, ErrSessionMismatch) {
		t.Fatalf("cross-session process: %v", err)
	}
	for _, cfg := range []FdConfig{
		{TGrid: pricingPtr(uint(0))},
		{XGrid: pricingPtr(uint(2))},
		{Scheme: pricingPtr(FdScheme(-1))},
		{Scheme: pricingPtr(FdScheme(2))},
	} {
		if _, err := s.NewFdBlackScholesVanillaEngine(p, cfg); err == nil {
			t.Fatalf("invalid configuration accepted: %+v", cfg)
		}
	}
	option := pricingMust(s.NewVanillaOption(Put, 100, pricingMust(NewDate(15, 1, 2026)), settings))
	if err := option.SetFdEngine(nil); err == nil {
		t.Fatal("nil engine accepted")
	}
	defaultEngine := pricingMust(s.NewFdBlackScholesVanillaEngine(p, FdConfig{}))
	if _, err := option.PriceFd(defaultEngine); err != nil {
		t.Fatalf("failed after invalid config: %v", err)
	}
	implicit := pricingMust(s.NewFdBlackScholesVanillaEngine(p, FdConfig{
		TGrid: pricingPtr(uint(200)), XGrid: pricingPtr(uint(200)),
		DampingSteps: pricingPtr(uint(2)), Scheme: pricingPtr(FdImplicitEuler),
	}))
	if got := pricingMust(option.PriceFd(implicit)); math.IsNaN(got) || math.IsInf(got, 0) {
		t.Fatalf("implicit Euler returned %g", got)
	}
	foreignOption := pricingMust(other.NewVanillaOption(Put, 100, pricingMust(NewDate(15, 1, 2026)), pricingMust(other.NewSettings())))
	if err := foreignOption.SetFdEngine(defaultEngine); !errors.Is(err, ErrSessionMismatch) {
		t.Fatalf("cross-session attachment: %v", err)
	}
}

func TestFdBlackScholesVanillaQuantLibAmericanCall(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	today := pricingMust(NewDate(15, 6, 2026))
	expiry := pricingMust(today.AddDays(1080))
	settings := pricingMust(s.NewSettings())
	pricingOK(t, settings.SetEvaluationDate(today))
	p := pricingMust(s.NewBlackScholesProcess(BlackScholesConfig{
		Spot: 100, RiskFreeRate: .03, DividendYield: .07, Volatility: .2,
		ReferenceDate: today, DayCounter: pricingMust(s.Actual360()),
	}))
	engine := pricingMust(s.NewFdBlackScholesVanillaEngine(p, FdConfig{
		TGrid: pricingPtr(uint(100)), XGrid: pricingPtr(uint(400)),
	}))
	option := pricingMust(s.NewAmericanOption(Call, 100, today, expiry, settings))
	pricingNear(t, pricingMust(option.PriceFd(engine)), 9.065, .08)
}
