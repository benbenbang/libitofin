package itofin

import (
	"encoding/csv"
	"fmt"
	"os"
	"sync"
	"testing"
)

var selectedCurrencyPresets = []func(*Session) (*Currency, error){
	(*Session).TWD,
	(*Session).CHF,
	(*Session).AUD,
	(*Session).CAD,
	(*Session).SEK,
	(*Session).NOK,
	(*Session).DKK,
	(*Session).NZD,
	(*Session).HKD,
	(*Session).SGD,
	(*Session).CNY,
	(*Session).KRW,
	(*Session).INR,
	(*Session).BRL,
	(*Session).MXN,
	(*Session).ZAR,
	(*Session).PLN,
	(*Session).CZK,
	(*Session).HUF,
	(*Session).TRY,
}

func TestCurrencyPresetCodesAndOwnership(t *testing.T) {
	file, err := os.Open("testdata/currency_presets.tsv")
	if err != nil {
		t.Fatal(err)
	}
	defer file.Close()
	reader := csv.NewReader(file)
	reader.Comma = '\t'
	reader.FieldsPerRecord = 10
	rows, err := reader.ReadAll()
	if err != nil {
		t.Fatal(err)
	}
	if len(rows) != 21 || len(selectedCurrencyPresets) != 20 {
		t.Fatal("preset scope changed")
	}
	session, err := NewSession()
	session = curveMust(t, session, err)
	defer session.Close()
	other, err := NewSession()
	other = curveMust(t, other, err)
	defer other.Close()
	for i, factory := range selectedCurrencyPresets {
		currency, err := factory(session)
		currency = curveMust(t, currency, err)
		code, err := currency.Code()
		if curveMust(t, code, err) != rows[i+1][2] {
			t.Fatal("wrong preset")
		}
		repr, err := currency.Repr()
		if curveMust(t, repr, err) != "Currency("+rows[i+1][2]+")" {
			t.Fatal("wrong repr")
		}
		foreign := &Currency{object{other, currency.id}}
		if _, err := foreign.Code(); err == nil {
			t.Fatal("foreign currency accepted")
		}
		if err := currency.Close(); err != nil {
			t.Fatal(err)
		}
		if _, err := currency.Code(); err == nil {
			t.Fatal("released currency accepted")
		}
	}
	if err := session.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err := session.TWD(); err == nil {
		t.Fatal("closed session accepted")
	}
}

func TestCurrencyPresetSessionConcurrency(t *testing.T) {
	session, err := NewSession()
	session = curveMust(t, session, err)
	defer session.Close()
	errors := make(chan error, len(selectedCurrencyPresets))
	var wait sync.WaitGroup
	for _, factory := range selectedCurrencyPresets {
		wait.Add(1)
		go func(factory func(*Session) (*Currency, error)) {
			defer wait.Done()
			currency, err := factory(session)
			if err != nil {
				errors <- err
				return
			}
			defer currency.Close()
			code, err := currency.Code()
			if err != nil {
				errors <- err
				return
			}
			if len(code) != 3 {
				errors <- fmt.Errorf("bad currency code %q", code)
			}
		}(factory)
	}
	wait.Wait()
	close(errors)
	for err := range errors {
		t.Error(err)
	}
}
