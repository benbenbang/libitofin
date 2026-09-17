package itofin

import (
	"reflect"
	"testing"
)

func TestCalendarHolidayOracles(t *testing.T) {
	s, err := NewSession()
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	target, _ := s.Target()
	us, _ := s.UnitedStates("NYSE")
	japan, _ := s.Japan()
	july := testDate(t, 4, 7, 2025)
	for _, tc := range []struct {
		cal     *Calendar
		date    Date
		holiday bool
	}{{us, july, true}, {target, july, false}, {japan, testDate(t, 25, 12, 2025), false}, {japan, testDate(t, 31, 12, 2025), true}, {us, testDate(t, 19, 6, 2025), true}} {
		got, err := tc.cal.IsHoliday(tc.date)
		if err != nil || got != tc.holiday {
			t.Fatal(got, err)
		}
		business, err := tc.cal.IsBusinessDay(tc.date)
		if err != nil || business == got {
			t.Fatal(business, err)
		}
	}
	if weekend, err := target.IsWeekend(testDate(t, 5, 7, 2025)); err != nil || !weekend {
		t.Fatal(weekend, err)
	}
	start, end := testDate(t, 1, 12, 2025), testDate(t, 31, 12, 2025)
	dates, err := target.HolidayList(start, end)
	if err != nil || !reflect.DeepEqual(dates, []Date{testDate(t, 25, 12, 2025), testDate(t, 26, 12, 2025)}) {
		t.Fatal(dates, err)
	}
	dates, err = target.HolidayList(start, end, true)
	if err != nil || len(dates) != 10 {
		t.Fatal(dates, err)
	}
	monday, next := testDate(t, 22, 12, 2025), testDate(t, 29, 12, 2025)
	no := false
	for _, tc := range []struct {
		from, to Date
		options  BusinessDayCountOptions
		want     int32
	}{{monday, next, BusinessDayCountOptions{}, 3}, {monday, next, BusinessDayCountOptions{IncludeLast: true}, 4}, {monday, next, BusinessDayCountOptions{IncludeFirst: &no}, 2}, {next, monday, BusinessDayCountOptions{}, -3}, {monday, monday, BusinessDayCountOptions{}, 0}, {monday, monday, BusinessDayCountOptions{IncludeLast: true}, 1}} {
		got, err := target.BusinessDaysBetween(tc.from, tc.to, tc.options)
		if err != nil || got != tc.want {
			t.Fatal(got, tc.want, err)
		}
	}
	if got, err := target.BusinessDaysBetween(monday, next); err != nil || got != 3 {
		t.Fatal(got, err)
	}
	if _, err := target.HolidayList(end, start); err == nil {
		t.Fatal("accepted reversed range")
	}
	null, _ := s.NullCalendar()
	dates, err = null.HolidayList(start, end, true)
	if err != nil || len(dates) != 0 {
		t.Fatal(dates, err)
	}
	max := testDate(t, 31, 12, 2199)
	if _, err := target.HolidayList(max, max, true); err != nil {
		t.Fatal(err)
	}
	for _, query := range []func(Date) (bool, error){target.IsHoliday, target.IsBusinessDay, target.IsWeekend} {
		if _, err := query(Date{}); err == nil {
			t.Fatal("accepted null date")
		}
	}
	if _, err := target.HolidayList(start, end, true, false); err == nil {
		t.Fatal("accepted extra flags")
	}
	if _, err := target.BusinessDaysBetween(start, end, BusinessDayCountOptions{}, BusinessDayCountOptions{}); err == nil {
		t.Fatal("accepted extra options")
	}
}
