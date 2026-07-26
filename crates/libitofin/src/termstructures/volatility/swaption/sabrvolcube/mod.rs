//! SABR swaption volatility cube internals (#596). Ticket #601 (T3a) lands the
//! inner [`Cube`] parameter store; the cube surface and calibration (T3b-d) follow
//! as siblings here.

mod cube;

#[allow(unused_imports)]
pub(crate) use cube::Cube;
