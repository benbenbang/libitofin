use libitofin::legacy::libormarketmodels::{
    LfmCovarianceProxy, LmExponentialCorrelationModel, LmLinearExponentialVolatilityModel,
};
use libitofin::shared::shared;

#[test]
fn instantaneous_legacy_components_match_independent_decimal_fixtures() {
    let volatility = shared(
        LmLinearExponentialVolatilityModel::new(vec![0.5, 1.0, 2.0], 0.2, 0.3, 0.1, 0.15).unwrap(),
    );
    let correlation = shared(LmExponentialCorrelationModel::new(3, 0.25).unwrap());
    let proxy = LfmCovarianceProxy::new(volatility.clone(), correlation).unwrap();
    let cases: [(f64, [f64; 3], [[f64; 3]; 3]); 4] = [
        (
            0.0,
            [
                0.31517699410626443,
                0.35928637723860124,
                0.40184639985171455,
            ],
            [
                [0.09933653761386026, 0.0881904664266704, 0.07681877018782225],
                [0.0881904664266704, 0.12908670086923849, 0.11244165055667879],
                [
                    0.07681877018782225,
                    0.11244165055667879,
                    0.16148052907378405,
                ],
            ],
        ),
        (
            0.5,
            [0.0, 0.31517699410626443, 0.386932668229798],
            [
                [0.0, 0.0, 0.0],
                [0.0, 0.09933653761386026, 0.0949765274964502],
                [0.0, 0.0949765274964502, 0.14971688974343092],
            ],
        ),
        (
            1.0,
            [0.0, 0.0, 0.35928637723860124],
            [
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 0.12908670086923849],
            ],
        ),
        (
            1.5,
            [0.0, 0.0, 0.31517699410626443],
            [
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 0.09933653761386026],
            ],
        ),
    ];
    for (time, expected_volatility, expected_covariance) in cases {
        let actual_volatility = volatility.volatility(time).unwrap();
        let actual_covariance = proxy.covariance(time).unwrap();
        let diffusion = proxy.diffusion(time).unwrap();
        let product = &diffusion * &diffusion.transpose();
        for i in 0..3 {
            assert!((actual_volatility[i] - expected_volatility[i]).abs() < 2e-15);
            for j in 0..3 {
                assert!((actual_covariance[(i, j)] - expected_covariance[i][j]).abs() < 2e-15);
                assert!((product[(i, j)] - expected_covariance[i][j]).abs() < 2e-14);
            }
        }
    }
}
