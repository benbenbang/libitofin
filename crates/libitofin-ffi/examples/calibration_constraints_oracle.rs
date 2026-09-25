#[allow(dead_code)]
mod fixture {
    include!("../tests/calibration_constraints.rs");

    pub fn core_parameters() -> [[f64; 5]; 3] {
        [
            direct_core(Case::Boundary),
            direct_core(Case::FixedRho),
            direct_core(Case::Weighted),
        ]
    }
}

fn main() {
    println!("{:?}", fixture::core_parameters());
}
