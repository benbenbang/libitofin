package itofin
/*
#include "itofin.h"
*/
import "C"
import "unsafe"

type HestonProcess struct{object}
type HestonModel struct{object}
type HullWhite struct{object}
type HestonModelHelper struct{object}
type SwaptionHelper struct{object}
type LevenbergMarquardt struct{object}
type EndCriteria struct{object}
type CalibrationErrorType int32
const(RelativePriceError CalibrationErrorType=iota;PriceError;ImpliedVolError)
type HestonProcessConfig struct{
 RiskFreeRate,DividendYield,Spot,V0,Kappa,Theta,Sigma,Rho float64
 ReferenceDate Date
 DayCounter *DayCounter
}
func(s *Session)NewHestonProcess(cfg HestonProcessConfig)(*HestonProcess,error){
 if cfg.DayCounter==nil{return nil,errNilArgument("day counter")};if err:=sameSession(s,cfg.DayCounter.object);err!=nil{return nil,err}
 c:=C.HestonProcessConfig{risk_free_rate:C.double(cfg.RiskFreeRate),dividend_yield:C.double(cfg.DividendYield),spot:C.double(cfg.Spot),v0:C.double(cfg.V0),kappa:C.double(cfg.Kappa),theta:C.double(cfg.Theta),sigma:C.double(cfg.Sigma),rho:C.double(cfg.Rho),reference_date:C.int32_t(cfg.ReferenceDate.Serial()),day_counter:C.uint64_t(cfg.DayCounter.id)}
 var id C.uint64_t;err:=s.invoke(func()error{var e C.ItofinError;return ffiError(C.itofin_heston_process_new(s.ctx,c,&id,&e),&e)})
 if err!=nil{return nil,err};return &HestonProcess{object{s,uint64(id)}},nil
}
func(s *Session)NewHestonModel(p *HestonProcess)(*HestonModel,error){
 if p==nil{return nil,errNilArgument("process")};if err:=sameSession(s,p.object);err!=nil{return nil,err}
 var id C.uint64_t;err:=s.invoke(func()error{var e C.ItofinError;return ffiError(C.itofin_heston_model_new(s.ctx,C.uint64_t(p.id),&id,&e),&e)})
 if err!=nil{return nil,err};return &HestonModel{object{s,uint64(id)}},nil
}
func(s *Session)NewHullWhite(curve *YieldTermStructure,a,sigma float64)(*HullWhite,error){
 if curve==nil{return nil,errNilArgument("curve")};if err:=sameSession(s,curve.object);err!=nil{return nil,err}
 var id C.uint64_t;err:=s.invoke(func()error{var e C.ItofinError;return ffiError(C.itofin_hullwhite_new(s.ctx,C.uint64_t(curve.id),C.double(a),C.double(sigma),&id,&e),&e)})
 if err!=nil{return nil,err};return &HullWhite{object{s,uint64(id)}},nil
}
func modelParameter(o object,kind int32,field uint)(float64,error){
 var value C.double;err:=o.session.invoke(func()error{var e C.ItofinError;return ffiError(C.itofin_model_parameter(o.session.ctx,C.uint64_t(o.id),C.int32_t(kind),C.size_t(field),&value,&e),&e)});return float64(value),err
}
func(p *HestonProcess)V0()(float64,error){if p==nil{return 0,errNilArgument("process")};return modelParameter(p.object,0,0)}
func(p *HestonProcess)Kappa()(float64,error){if p==nil{return 0,errNilArgument("process")};return modelParameter(p.object,0,1)}
func(p *HestonProcess)Theta()(float64,error){if p==nil{return 0,errNilArgument("process")};return modelParameter(p.object,0,2)}
func(p *HestonProcess)Sigma()(float64,error){if p==nil{return 0,errNilArgument("process")};return modelParameter(p.object,0,3)}
func(p *HestonProcess)Rho()(float64,error){if p==nil{return 0,errNilArgument("process")};return modelParameter(p.object,0,4)}
func(p *HestonModel)V0()(float64,error){if p==nil{return 0,errNilArgument("model")};return modelParameter(p.object,1,0)}
func(p *HestonModel)Kappa()(float64,error){if p==nil{return 0,errNilArgument("model")};return modelParameter(p.object,1,1)}
func(p *HestonModel)Theta()(float64,error){if p==nil{return 0,errNilArgument("model")};return modelParameter(p.object,1,2)}
func(p *HestonModel)Sigma()(float64,error){if p==nil{return 0,errNilArgument("model")};return modelParameter(p.object,1,3)}
func(p *HestonModel)Rho()(float64,error){if p==nil{return 0,errNilArgument("model")};return modelParameter(p.object,1,4)}
func(p *HullWhite)A()(float64,error){if p==nil{return 0,errNilArgument("model")};return modelParameter(p.object,2,0)}
func(p *HullWhite)Sigma()(float64,error){if p==nil{return 0,errNilArgument("model")};return modelParameter(p.object,2,1)}
func(p *HullWhite)R0()(float64,error){if p==nil{return 0,errNilArgument("model")};return modelParameter(p.object,2,2)}
func(p *HullWhite)DiscountBondOption(kind OptionType,strike,maturity,bondMaturity float64)(float64,error){
 if p==nil{return 0,errNilArgument("model")};var value C.double
 err:=p.session.invoke(func()error{var e C.ItofinError;return ffiError(C.itofin_hullwhite_bond_option(p.session.ctx,C.uint64_t(p.id),C.int32_t(kind),C.double(strike),C.double(maturity),C.double(bondMaturity),&value,&e),&e)})
 return float64(value),err
}
// nil configuration selects the native/Python defaults (1e-8 tolerances).
type LevenbergMarquardtConfig struct{EpsFcn,XTol,GTol float64;UseCostFunctionsJacobian bool}
func(s *Session)NewLevenbergMarquardt(cfg *LevenbergMarquardtConfig)(*LevenbergMarquardt,error){
 c:=LevenbergMarquardtConfig{EpsFcn:1e-8,XTol:1e-8,GTol:1e-8};if cfg!=nil{c=*cfg};var jac C.int32_t;if c.UseCostFunctionsJacobian{jac=1}
 var id C.uint64_t;err:=s.invoke(func()error{var e C.ItofinError;return ffiError(C.itofin_levenberg_marquardt_new(s.ctx,C.double(c.EpsFcn),C.double(c.XTol),C.double(c.GTol),jac,&id,&e),&e)})
 if err!=nil{return nil,err};return &LevenbergMarquardt{object{s,uint64(id)}},nil
}
type EndCriteriaConfig struct{
 MaxIterations uint
 MaxStationaryStateIterations *uint
 RootEpsilon,FunctionEpsilon float64
 GradientNormEpsilon *float64
}
func(s *Session)NewEndCriteria(cfg EndCriteriaConfig)(*EndCriteria,error){
 c:=C.EndCriteriaConfig{max_iterations:C.size_t(cfg.MaxIterations),root_epsilon:C.double(cfg.RootEpsilon),function_epsilon:C.double(cfg.FunctionEpsilon)}
 if cfg.MaxStationaryStateIterations!=nil{c.has_stationary=1;c.stationary_iterations=C.size_t(*cfg.MaxStationaryStateIterations)}
 if cfg.GradientNormEpsilon!=nil{c.has_gradient=1;c.gradient_epsilon=C.double(*cfg.GradientNormEpsilon)}
 var id C.uint64_t;err:=s.invoke(func()error{var e C.ItofinError;return ffiError(C.itofin_end_criteria_new(s.ctx,c,&id,&e),&e)})
 if err!=nil{return nil,err};return &EndCriteria{object{s,uint64(id)}},nil
}
