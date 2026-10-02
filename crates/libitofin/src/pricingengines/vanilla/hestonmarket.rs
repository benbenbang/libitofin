//! Shared validated market inputs for European Heston engines.

use super::analytichestonengine::HestonChf;
use crate::errors::QlResult;
use crate::exercise::ExerciseType;
use crate::instruments::{OptionArguments, PlainVanillaPayoff, StrikedTypePayoff, TypePayoff};
use crate::models::HestonModel;
use crate::option::OptionType;
use crate::processes::HestonProcess;
use crate::stochasticprocess::StochasticProcess;
use crate::time::date::Date;
use crate::types::Real;
use crate::{fail, require};
use std::any::Any;

pub(super) struct HestonMarket {
    pub chf: HestonChf,
    pub time: Real,
    pub discount: Real,
    pub forward: Real,
    pub strike: Real,
    pub option_type: OptionType,
}

impl HestonMarket {
    pub fn new(arguments: &OptionArguments, model: &HestonModel) -> QlResult<Self> {
        Self::from_process(arguments, &model.process())
    }

    pub fn from_process(arguments: &OptionArguments, process: &HestonProcess) -> QlResult<Self> {
        let Some(exercise) = &arguments.exercise else {
            fail!("no exercise given");
        };
        require!(
            exercise.exercise_type() == ExerciseType::European,
            "not an European option"
        );
        let Some(payoff) = &arguments.payoff else {
            fail!("no payoff given");
        };
        let payoff: &dyn StrikedTypePayoff = &**payoff;
        let Some(payoff) = (payoff as &dyn Any).downcast_ref::<PlainVanillaPayoff>() else {
            fail!("non plain vanilla payoff given");
        };
        let date = exercise.last_date();
        require!(date != Date::null(), "non-null exercise date required");
        require!(
            process.risk_free_rate().current_link()?.reference_date()? != Date::null()
                && process.dividend_yield().current_link()?.reference_date()? != Date::null(),
            "non-null curve reference dates required"
        );
        let time = process.time(&date)?;
        let spot = process.s0().current_link()?.value()?;
        let discount = process
            .risk_free_rate()
            .current_link()?
            .discount_date(date, false)?;
        let dividend = process
            .dividend_yield()
            .current_link()?
            .discount_date(date, false)?;
        let strike = payoff.strike();
        require!(
            spot.is_finite() && spot > 0.0,
            "positive finite underlying required"
        );
        require!(
            strike.is_finite() && strike > 0.0,
            "positive finite strike required"
        );
        require!(
            time.is_finite() && time > 0.0,
            "positive finite maturity required"
        );
        require!(
            discount.is_finite() && discount > 0.0 && dividend.is_finite() && dividend > 0.0,
            "positive finite discounts required"
        );
        let forward = spot * dividend / discount;
        require!(
            forward.is_finite() && forward > 0.0,
            "positive finite forward required"
        );
        Ok(Self {
            chf: HestonChf::new(
                process.kappa(),
                process.theta(),
                process.sigma(),
                process.rho(),
                process.v0(),
            ),
            time,
            discount,
            forward,
            strike,
            option_type: payoff.option_type(),
        })
    }
}
