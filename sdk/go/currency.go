package itofin

/*
#include "itofin.h"
#include <stdlib.h>
*/
import "C"
import "unsafe"

// CurrencyConfig specifies the six stored core fields, without registry lookup.
// Code must contain exactly three ASCII letters. Other text retains Unicode and
// embedded NULs. Signed integer fields are copied without normalization.
type CurrencyConfig struct {
	Name, Code             string
	NumericCode            int32
	Symbol, FractionSymbol string
	FractionsPerUnit       int32
}

// NewCurrency constructs a reusable currency belonging to this session.
// Core equality remains name-based, not code-based.
func (s *Session) NewCurrency(a CurrencyConfig) (*Currency, error) {
	names := []string{a.Name, a.Code, a.Symbol, a.FractionSymbol}
	ptrs := make([]*C.char, len(names))
	for i, value := range names {
		ptrs[i] = C.CString(value)
		defer C.free(unsafe.Pointer(ptrs[i]))
	}
	cfg := C.ItofinCurrencyConfig{
		name: (*C.uint8_t)(unsafe.Pointer(ptrs[0])), name_length: C.size_t(len(names[0])),
		code: (*C.uint8_t)(unsafe.Pointer(ptrs[1])), code_length: C.size_t(len(names[1])),
		numeric_code: C.int32_t(a.NumericCode),
		symbol:       (*C.uint8_t)(unsafe.Pointer(ptrs[2])), symbol_length: C.size_t(len(names[2])),
		fraction_symbol: (*C.uint8_t)(unsafe.Pointer(ptrs[3])), fraction_symbol_length: C.size_t(len(names[3])),
		fractions_per_unit: C.int32_t(a.FractionsPerUnit),
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_currency_custom_new(s.ctx, &cfg, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &Currency{object{s, uint64(id)}}, nil
}

func (c *Currency) checked() error {
	if c == nil {
		return errNilArgument("currency")
	}
	return sameSession(c.session, c.object)
}

func (c *Currency) text(field int32) (string, error) {
	if err := c.checked(); err != nil {
		return "", err
	}
	var out []byte
	err := c.session.invoke(func() error {
		var e C.ItofinError
		var size C.size_t
		if err := ffiError(C.itofin_currency_text(c.session.ctx, C.uint64_t(c.id), C.int32_t(field), nil, 0, &size, &e), &e); err != nil {
			return err
		}
		if size == 0 {
			return nil
		}
		out = make([]byte, int(size))
		return ffiError(C.itofin_currency_text(c.session.ctx, C.uint64_t(c.id), C.int32_t(field), (*C.uint8_t)(unsafe.Pointer(&out[0])), size, &size, &e), &e)
	})
	if err != nil {
		return "", err
	}
	return string(out), nil
}

func (c *Currency) integer(field int32) (int32, error) {
	if err := c.checked(); err != nil {
		return 0, err
	}
	var out C.int32_t
	err := c.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_currency_integer(c.session.ctx, C.uint64_t(c.id), C.int32_t(field), &out, &e), &e)
	})
	return int32(out), err
}

// Name returns the name used for core equality.
func (c *Currency) Name() (string, error) { return c.text(0) }

// NumericCode returns the stored numeric identifier.
func (c *Currency) NumericCode() (int32, error) { return c.integer(0) }

// Symbol returns the stored symbol, including any embedded NULs.
func (c *Currency) Symbol() (string, error) { return c.text(2) }

// FractionSymbol returns the stored fraction symbol, including any embedded NULs.
func (c *Currency) FractionSymbol() (string, error) { return c.text(3) }

// FractionsPerUnit returns the stored number of fractions in one unit.
func (c *Currency) FractionsPerUnit() (int32, error) { return c.integer(1) }
