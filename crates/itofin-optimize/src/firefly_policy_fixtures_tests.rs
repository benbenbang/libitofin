use super::*;
use crate::GlobalOptions;
use serde_json::Value;
use std::convert::Infallible;

fn records() -> Vec<Value> {
    include_str!("../tests/fixtures/firefly.jsonl")
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn vector(value: &Value) -> Vec<f64> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect()
}

fn space_bounds(value: &Value) -> Bounds {
    let pairs = value.as_array().unwrap();
    Bounds {
        lower: pairs.iter().map(|p| p[0].as_f64().unwrap()).collect(),
        upper: pairs.iter().map(|p| p[1].as_f64().unwrap()).collect(),
    }
}

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 1e-12 * expected.abs().max(1.0),
        "{actual:?} != {expected:?}"
    );
}

#[test]
fn primary_attraction_records_exercise_production_equation_and_physical_move() {
    let mut count = 0;
    for record in records().iter().filter(|r| r["kind"] == "attraction") {
        count += 1;
        let record = &record["value"];
        let options = FireflyOptions {
            beta0: record["beta0"].as_f64().unwrap(),
            gamma: record["gamma"].as_f64().unwrap(),
            ..FireflyOptions::default()
        };
        let beta = attraction(record["distance_squared"].as_f64().unwrap(), &options);
        close(beta, record["beta"].as_f64().unwrap());
        if !record["current"].is_null() {
            let current = vector(&record["current"]);
            let target = vector(&record["target"]);
            let expected = vector(&record["expected_move"]);
            let uniforms = vector(&record["uniforms"]);
            let bounds = space_bounds(&record["bounds"]);
            let space = BoxSpace::new(&bounds);
            let distance: f64 = space
                .free
                .iter()
                .map(|&i| ((target[i] - current[i]) / space.width(i)).powi(2))
                .sum();
            close(distance, record["distance_squared"].as_f64().unwrap());
            for &i in &space.free {
                let noise = record["alpha"].as_f64().unwrap() * (uniforms[i] - 0.5);
                close(
                    coordinate_move(current[i], target[i], beta, noise, &space, i),
                    expected[i],
                );
            }
        }
    }
    assert_eq!(count, 3);
}

fn options(input: &Value) -> FireflyOptions {
    let config = &input["options"];
    let mut options = FireflyOptions {
        global: GlobalOptions {
            seed: config["seed"].as_u64().unwrap_or(0),
            population_size: input["population_size"].as_u64().map(|n| n as usize),
            initial_population: input["initial_population"]
                .as_array()
                .map(|rows| rows.iter().map(vector).collect()),
            xatol: config["xatol"].as_f64(),
            fatol: config["fatol"].as_f64(),
        },
        ..FireflyOptions::default()
    };
    for (name, field) in [
        ("alpha", &mut options.alpha),
        ("beta0", &mut options.beta0),
        ("gamma", &mut options.gamma),
        ("alpha_decay", &mut options.alpha_decay),
    ] {
        if let Some(value) = config[name].as_f64() {
            *field = value;
        }
    }
    options
}

fn value(name: &str, x: &[f64]) -> f64 {
    match name {
        "signed_quadratic" => (x[0] - 1.25).powi(2) + 4.0 * (x[1] + 0.75).powi(2) - 3.0,
        "quadratic" => x[0].powi(2),
        "boundary_quadratic" => (x[0] - 3.0).powi(2) - 20.0,
        "constant" => -7.0,
        "linear" => x[0],
        other => panic!("unrecognized traced objective {other}"),
    }
}

#[test]
fn every_completed_generation_matches_frozen_population_noise_and_rng_draws() {
    let mut fixtures: Vec<(Value, Vec<Value>)> = Vec::new();
    for record in records() {
        match record["kind"].as_str().unwrap() {
            "input" => fixtures.push((record["value"].clone(), vec![])),
            "generation" => {
                let (input, generations) = fixtures.last_mut().unwrap();
                assert_eq!(input["name"], record["name"]);
                generations.push(record["value"].clone());
            }
            _ => {}
        }
    }
    let mut count = 0;
    for (input, generations) in fixtures {
        if generations.is_empty() {
            continue;
        }
        let bounds = space_bounds(&input["bounds"]);
        let space = BoxSpace::new(&bounds);
        let options = options(&input);
        let mut random = Random::new(options.global.seed);
        let mut search = Search {
            counters: Counters::new(&Common::default()),
            best: None,
            first: None,
        };
        let mut objective = |x: &[f64]| -> Result<f64, Infallible> {
            Ok(value(input["objective"].as_str().unwrap(), x))
        };
        let mut population = Vec::new();
        for row in 0..options.global.population_count(&bounds) {
            let x = match &options.global.initial_population {
                Some(points) => points[row].clone(),
                None if row == 0 => vector(&input["x0"]),
                None => space.sample(&mut random),
            };
            let f = search.value(&mut objective, &x).unwrap();
            population.push(Firefly { x, f });
        }
        let mut alpha = options.alpha;
        for (i, expected) in generations.iter().enumerate() {
            count += 1;
            population = generation(
                &mut search,
                &mut objective,
                &population,
                &space,
                &options,
                alpha,
                &mut random,
            )
            .unwrap();
            alpha *= options.alpha_decay;
            assert_eq!(expected["generation"].as_u64(), Some(i as u64 + 1));
            close(alpha, expected["alpha"].as_f64().unwrap());
            let expected_population = expected["population"].as_array().unwrap();
            assert_eq!(population.len(), expected_population.len());
            for (actual, expected) in population.iter().zip(expected_population) {
                close(actual.f, expected["fun"].as_f64().unwrap());
                let point = vector(&expected["x"]);
                for (actual, expected) in actual.x.iter().zip(point) {
                    if input["name"] == "microscopic_physical_distance_survives_normalization" {
                        assert_eq!(actual.to_bits(), expected.to_bits());
                    } else {
                        close(*actual, expected);
                    }
                }
            }
        }
        let mut independent_position = Random::new(options.global.seed);
        let draws = generations.last().unwrap()["rng_draws"].as_u64().unwrap();
        for _ in 0..draws {
            independent_position.unit();
        }
        assert_eq!(
            random.unit(),
            independent_position.unit(),
            "{}",
            input["name"]
        );
    }
    assert_eq!(count, 28);
}
