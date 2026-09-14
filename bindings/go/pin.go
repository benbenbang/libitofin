package itofin

import "runtime"

// C configuration records may contain Go array pointers. Pin their pointees
// explicitly for the duration of the synchronous cgo call.
type runtimePinner struct{ p runtime.Pinner }

func (p *runtimePinner) pin(arrays ...[]float64) {
	for _, a := range arrays {
		if len(a) > 0 {
			p.p.Pin(&a[0])
		}
	}
}
func (p *runtimePinner) unpin() { p.p.Unpin() }
