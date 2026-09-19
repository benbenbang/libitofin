package itofin

import "testing"

func TestCreditProtectionEndDate(t *testing.T) {
	s := pricingMust(NewSession())
	defer s.Close()
	settings := pricingMust(s.NewSettings())
	if err := settings.SetEvaluationDate(pricingMust(NewDate(19, 6, 2025))); err != nil {
		t.Fatal(err)
	}
	cal := pricingMust(s.WeekendsOnly())
	dc := pricingMust(s.Actual360())
	end := pricingMust(NewDate(20, 6, 2026))
	schedule := pricingMust(s.NewSchedule(ScheduleConfig{
		Start: pricingMust(NewDate(20, 6, 2025)), End: end, Frequency: Quarterly,
		Calendar: cal, Convention: Following, TerminationConvention: pricingPtr(Unadjusted),
	}))
	cfg := DefaultCdsConfig()
	cfg.Side, cfg.Notional, cfg.Spread = ProtectionBuyer, 10000000, .01
	cfg.Schedule, cfg.DayCounter, cfg.Settings = schedule, dc, settings
	cfg.PaymentConvention = Following
	cds := pricingMust(s.NewCreditDefaultSwap(cfg))
	got, err := cds.ProtectionEndDate()
	if err != nil || got != end {
		t.Fatalf("QuantLib 1.43 protection end: %v, %v", got, err)
	}
	for _, close := range []func() error{schedule.Close, dc.Close, cal.Close, settings.Close} {
		if err := close(); err != nil {
			t.Fatal(err)
		}
	}
	if got, err = cds.ProtectionEndDate(); err != nil || got != end {
		t.Fatalf("retained protection end: %v, %v", got, err)
	}
	if err := cds.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err := cds.ProtectionEndDate(); err == nil {
		t.Fatal("closed CDS accepted")
	}
	var absent *CreditDefaultSwap
	if _, err := absent.ProtectionEndDate(); err == nil {
		t.Fatal("nil CDS accepted")
	}
}
