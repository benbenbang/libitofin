package itofin

import "testing"

func TestSettingsOptionalFlagAndIsolation(t *testing.T) {
	s, err := NewSession()
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	a, err := s.NewSettings()
	if err != nil {
		t.Fatal(err)
	}
	b, _ := s.NewSettings()
	if v, err := a.IncludeTodaysCashFlows(); err != nil || v != nil {
		t.Fatal(v, err)
	}
	for _, flag := range []bool{true, false} {
		if err := a.SetIncludeTodaysCashFlows(&flag); err != nil {
			t.Fatal(err)
		}
		v, err := a.IncludeTodaysCashFlows()
		if err != nil || v == nil || *v != flag {
			t.Fatal(v, err)
		}
	}
	if err := a.SetIncludeTodaysCashFlows(nil); err != nil {
		t.Fatal(err)
	}
	if v, err := a.IncludeTodaysCashFlows(); err != nil || v != nil {
		t.Fatal(v, err)
	}
	if v, err := b.IncludeTodaysCashFlows(); err != nil || v != nil {
		t.Fatal(v, err)
	}
	if err := a.SetEvaluationDate(Date{}); err == nil {
		t.Fatal("accepted invalid date")
	}
	if err := a.SetEvaluationDate(testDate(t, 1, 1, 2025)); err != nil {
		t.Fatal(err)
	}
	if err := a.Close(); err != nil {
		t.Fatal(err)
	}
	if err := a.SetIncludeTodaysCashFlows(nil); err == nil {
		t.Fatal("released settings usable")
	}
}
