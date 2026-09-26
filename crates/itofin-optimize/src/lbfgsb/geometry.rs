//! Projected-path Cauchy point and direct primal minimization on its free set.
//!
//! - Byrd, Lu, Nocedal and Zhu (1995), Sections 4 and 5.1: the piecewise
//!   quadratic projected path and direct primal subspace solve.

use super::{Compact, dot, solve};
use crate::problem::Bounds;

pub(super) struct Cauchy {
    pub point: Vec<f64>,
    pub free: Vec<usize>,
}

fn free_indices(x: &[f64], bounds: &Bounds) -> Vec<usize> {
    x.iter()
        .enumerate()
        .filter_map(|(i, &xi)| (xi > bounds.lower[i] && xi < bounds.upper[i]).then_some(i))
        .collect()
}

fn point_on_path(
    x: &[f64],
    displacement: &[f64],
    active: &[Option<f64>],
    bounds: &Bounds,
) -> Option<Cauchy> {
    let point: Vec<f64> = x
        .iter()
        .zip(displacement)
        .enumerate()
        .map(|(i, (&xi, &di))| {
            active[i].unwrap_or_else(|| (xi + di).clamp(bounds.lower[i], bounds.upper[i]))
        })
        .collect();
    if !point.iter().all(|xi| xi.is_finite()) {
        return None;
    }
    let free = free_indices(&point, bounds);
    Some(Cauchy { point, free })
}

pub(super) fn generalized_cauchy(
    x: &[f64],
    g: &[f64],
    bounds: &Bounds,
    b: &Compact,
) -> Option<Cauchy> {
    let mut direction = vec![0.0; x.len()];
    let mut breakpoints = Vec::new();
    for i in 0..x.len() {
        let velocity = -g[i];
        if velocity > 0.0 && x[i] < bounds.upper[i] {
            direction[i] = velocity;
            let time = (bounds.upper[i] - x[i]) / velocity;
            if time.is_finite() {
                breakpoints.push((time, i));
            }
        } else if velocity < 0.0 && x[i] > bounds.lower[i] {
            direction[i] = velocity;
            let time = (bounds.lower[i] - x[i]) / velocity;
            if time.is_finite() {
                breakpoints.push((time, i));
            }
        }
    }
    breakpoints.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut displacement = vec![0.0; x.len()];
    let mut active = vec![None; x.len()];
    let mut previous = 0.0;
    let mut event = 0;
    loop {
        if direction.iter().all(|&di| di == 0.0) {
            return point_on_path(x, &displacement, &active, bounds);
        }
        let bd = b.times(&direction)?;
        let bz = b.times(&displacement)?;
        let slope = dot(g, &direction) + dot(&bz, &direction);
        let curvature = dot(&direction, &bd);
        if !(slope.is_finite() && curvature.is_finite() && curvature > 0.0) {
            return None;
        }
        if slope >= 0.0 {
            return point_on_path(x, &displacement, &active, bounds);
        }
        let next = breakpoints.get(event).map_or(f64::INFINITY, |&(t, _)| t);
        let segment = next - previous;
        let minimizer = -slope / curvature;
        if !minimizer.is_finite() {
            return None;
        }
        if minimizer < segment {
            for (zi, di) in displacement.iter_mut().zip(&direction) {
                *zi += minimizer * di;
            }
            return point_on_path(x, &displacement, &active, bounds);
        }
        for (zi, di) in displacement.iter_mut().zip(&direction) {
            *zi += segment * di;
        }
        while event < breakpoints.len() && breakpoints[event].0 == next {
            let i = breakpoints[event].1;
            let bound = if direction[i] > 0.0 {
                bounds.upper[i]
            } else {
                bounds.lower[i]
            };
            displacement[i] = bound - x[i];
            active[i] = Some(bound);
            direction[i] = 0.0;
            event += 1;
        }
        previous = next;
    }
}

pub(super) fn primal_minimize(
    x: &[f64],
    g: &[f64],
    cauchy: &Cauchy,
    bounds: &Bounds,
    b: &Compact,
) -> Option<Vec<f64>> {
    if cauchy.free.is_empty() {
        return Some(cauchy.point.clone());
    }
    let displacement: Vec<f64> = cauchy.point.iter().zip(x).map(|(ci, xi)| ci - xi).collect();
    let r: Vec<f64> = b
        .times(&displacement)?
        .iter()
        .zip(g)
        .map(|(bi, gi)| bi + gi)
        .collect();
    let mut step = vec![0.0; x.len()];
    if b.pairs.is_empty() {
        for &i in &cauchy.free {
            step[i] = -r[i] / b.theta;
        }
    } else {
        let (columns, mut block) = b.columns_and_block();
        let size = columns.len();
        let mut rhs = vec![0.0; size];
        for i in 0..size {
            rhs[i] = cauchy.free.iter().map(|&k| columns[i][k] * r[k]).sum();
            for j in 0..size {
                let gram: f64 = cauchy
                    .free
                    .iter()
                    .map(|&k| columns[i][k] * columns[j][k])
                    .sum();
                block[i][j] -= gram / b.theta;
            }
        }
        let weights = solve(block, rhs)?;
        for &i in &cauchy.free {
            let correction: f64 = columns.iter().zip(&weights).map(|(w, z)| w[i] * z).sum();
            step[i] = -(r[i] + correction / b.theta) / b.theta;
        }
    }
    if !step.iter().all(|si| si.is_finite()) {
        return None;
    }
    let mut alpha: f64 = 1.0;
    for &i in &cauchy.free {
        if step[i] > 0.0 {
            alpha = alpha.min((bounds.upper[i] - cauchy.point[i]) / step[i]);
        } else if step[i] < 0.0 {
            alpha = alpha.min((bounds.lower[i] - cauchy.point[i]) / step[i]);
        }
    }
    let mut point = cauchy.point.clone();
    for &i in &cauchy.free {
        point[i] = (point[i] + alpha * step[i]).clamp(bounds.lower[i], bounds.upper[i]);
    }
    point.iter().all(|xi| xi.is_finite()).then_some(point)
}

#[cfg(test)]
mod tests {
    use super::{generalized_cauchy, primal_minimize};
    use crate::lbfgsb::Compact;
    use crate::problem::Bounds;

    #[test]
    fn a_coupled_quadratic_uses_the_face_then_solves_its_free_coordinate() {
        let mut b = Compact::new(2, 3);
        assert!(b.update(vec![1.0, 0.0], vec![2.0, 1.0]));
        let bounds = Bounds {
            lower: vec![0.0, -2.0],
            upper: vec![1.0, 2.0],
        };
        let x = [0.0, 0.0];
        let g = [-5.0, -1.0];
        let cauchy = generalized_cauchy(&x, &g, &bounds, &b).expect("finite Cauchy point");
        assert_eq!(cauchy.free, vec![1]);
        assert!((cauchy.point[0] - 1.0).abs() < 1e-12);
        assert!((cauchy.point[1] - 0.2).abs() < 1e-12);
        let solution = primal_minimize(&x, &g, &cauchy, &bounds, &b).expect("finite subspace step");
        assert!((solution[0] - 1.0).abs() < 1e-12);
        assert!(solution[1].abs() < 1e-12);
        let clipped_newton = [1.0, -0.6];
        assert!((clipped_newton[1] - solution[1]).abs() > 0.5);
    }

    #[test]
    fn the_path_respects_fixed_and_open_coordinates() {
        let b = Compact::new(3, 2);
        let bounds = Bounds {
            lower: vec![1.0, f64::NEG_INFINITY, 0.0],
            upper: vec![1.0, f64::INFINITY, 2.0],
        };
        let x = [1.0, 0.0, 0.5];
        let g = [-10.0, -2.0, 1.0];
        let cauchy = generalized_cauchy(&x, &g, &bounds, &b).expect("finite Cauchy point");
        assert_eq!(cauchy.point[0], 1.0);
        assert!(cauchy.point[2] >= 0.0 && cauchy.point[2] <= 2.0);
        assert!(!cauchy.free.contains(&0));
    }

    #[test]
    fn a_breakpoint_retains_the_exact_active_bound_after_cancellation() {
        let b = Compact::new(1, 2);
        let upper = 6.09363334202361e-158;
        let bounds = Bounds {
            lower: vec![f64::NEG_INFINITY],
            upper: vec![upper],
        };
        let point = generalized_cauchy(&[-32.82972637320486], &[-100.0], &bounds, &b)
            .expect("finite Cauchy point");
        assert_eq!(point.point, vec![upper]);
        assert!(point.free.is_empty());
    }

    #[test]
    fn a_minimizer_on_tied_breakpoints_activates_both_faces() {
        let b = Compact::new(2, 2);
        let bounds = Bounds {
            lower: vec![f64::NEG_INFINITY; 2],
            upper: vec![1.0; 2],
        };
        let point = generalized_cauchy(&[0.0, 0.0], &[-1.0, -1.0], &bounds, &b)
            .expect("finite Cauchy point");
        assert_eq!(point.point, vec![1.0, 1.0]);
        assert!(point.free.is_empty());
    }
}
