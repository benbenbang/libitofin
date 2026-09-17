package itofin

import "testing"

func TestCalendarBoundsSurviveIndexInspection(t *testing.T) {
	s, err := NewSession()
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	settings, _ := s.NewSettings()
	currency, _ := s.USD()
	dc, _ := s.Actual360()
	for _, kind := range []string{"Indonesia", "MOEX", "JoinHolidays", "JoinBusinessDays"} {
		t.Run(kind, func(t *testing.T) {
			var cal *Calendar
			var err error
			switch kind {
			case "Indonesia":
				cal, err = s.Indonesia()
			case "MOEX":
				cal, err = s.Russia("MOEX")
			default:
				indonesia, _ := s.Indonesia()
				moex, _ := s.Russia("MOEX")
				cal, err = s.JointCalendar([]*Calendar{indonesia, moex}, kind)
				indonesia.Close()
				moex.Close()
			}
			if err != nil {
				t.Fatal(err)
			}
			key, _ := cal.Key()
			cfg := IborIndexConfig{FamilyName: "bound-check", Tenor: Period{3, Months}, Currency: currency, FixingCalendar: cal, ValueCalendar: cal, MaturityCalendar: cal, Convention: Following, DayCounter: dc, Settings: settings}
			ordinary, err := s.NewIborIndex(cfg)
			if err != nil {
				t.Fatal(err)
			}
			custom, err := s.NewCustomIborIndex(cfg)
			if err != nil {
				t.Fatal(err)
			}
			cal.Close()
			for _, index := range []*IborIndex{ordinary, custom} {
				inspected, err := index.FixingCalendar()
				if err != nil {
					t.Fatal(err)
				}
				index.Close()
				if got, err := inspected.Key(); err != nil || got != key {
					t.Fatal(got, err)
				}
				invalid := testDate(t, 1, 1, 2015)
				if kind == "MOEX" {
					invalid = testDate(t, 1, 1, 2011)
				}
				if _, err := inspected.IsHoliday(invalid); err == nil {
					t.Fatal("lost calendar bound")
				}
				if _, err := inspected.IsHoliday(testDate(t, 1, 6, 2013)); err != nil {
					t.Fatal("session poisoned", err)
				}
				inspected.Close()
			}
		})
	}
}
