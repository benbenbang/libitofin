package itofin

/*
#include "itofin.h"
*/
import "C"

import (
	"fmt"
	"unsafe"
)

// DatedIntervalPrice holds one date and its open, high, low, and close prices.
type DatedIntervalPrice struct {
	Date  Date
	Open  float64
	High  float64
	Low   float64
	Close float64
}

// IntervalPrices sorts bars by date and keeps the last bar for duplicate dates.
// Prices may be negative, but all prices and dates must be valid.
func IntervalPrices(dates []Date, open, high, low, close []float64) ([]DatedIntervalPrice, error) {
	n := len(dates)
	if len(open) != n || len(high) != n || len(low) != n || len(close) != n {
		return nil, fmt.Errorf("itofin: interval price lengths differ")
	}
	if n > DefaultMaxOutputValues/32 {
		return nil, fmt.Errorf("itofin: interval price result exceeds output limit")
	}
	input := make([]C.ItofinDatedIntervalPrice, n)
	output := make([]C.ItofinDatedIntervalPrice, n)
	for i := range input {
		input[i] = C.ItofinDatedIntervalPrice{
			date: C.int32_t(dates[i].serial),
			open: C.double(open[i]), high: C.double(high[i]),
			low: C.double(low[i]), close: C.double(close[i]),
		}
	}
	var count C.size_t
	var e C.ItofinError
	status := C.itofin_interval_prices_normalize(
		unsafe.SliceData(input), C.size_t(n),
		unsafe.SliceData(output), C.size_t(n), &count, &e,
	)
	if err := ffiError(status, &e); err != nil {
		return nil, err
	}
	if count > C.size_t(n) {
		return nil, fmt.Errorf("itofin: interval price result exceeds output capacity")
	}
	result := make([]DatedIntervalPrice, int(count))
	for i := range result {
		row := output[i]
		result[i] = DatedIntervalPrice{
			Date: Date{serial: int32(row.date)},
			Open: float64(row.open), High: float64(row.high),
			Low: float64(row.low), Close: float64(row.close),
		}
	}
	return result, nil
}
