package itofin

/*
#include "itofin.h"
*/
import "C"

// Calibrate fits eight Bates parameters using Heston helpers and an existing
// optimizer. Mask order is theta,kappa,sigma,rho,v0,nu,delta,lambda.
// Integration order must be 1..192; 144 is conventional.
func (m *BatesModel) Calibrate(helpers []*HestonModelHelper, method OptimizationMethod, criteria *EndCriteria, order uint, options ...*CalibrationOptions) error {
	if m == nil || criteria == nil {
		return errNilArgument("model or criteria")
	}
	methodObject, err := optimizationMethodObject(method)
	if err != nil {
		return err
	}
	objects := []object{m.object, methodObject, criteria.object}
	ids := make([]C.uint64_t, len(helpers))
	for i, h := range helpers {
		if h == nil {
			return errNilArgument("helper")
		}
		objects = append(objects, h.object)
		ids[i] = C.uint64_t(h.id)
	}
	if err := sameSession(m.session, objects...); err != nil {
		return err
	}
	args, err := newCalibrationArgs(m.session, options, false)
	if err != nil {
		return err
	}
	defer args.release()
	var ptr *C.uint64_t
	if len(ids) > 0 {
		ptr = &ids[0]
	}
	return m.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_bates_calibrate_with_options(m.session.ctx, C.uint64_t(m.id), ptr, C.size_t(len(ids)), C.uint64_t(methodObject.id), C.uint64_t(criteria.id), C.size_t(order), &args.cfg, &e), &e)
	})
}
