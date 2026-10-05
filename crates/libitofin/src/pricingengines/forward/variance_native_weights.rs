use crate::option::OptionType;

pub(super) const LITERATURE_WEIGHTS: &[(OptionType, f64, f64)] = &[
    (OptionType::Call, 100.0, 0.001962622569588014),
    (OptionType::Call, 105.0, 0.0036826854004746415),
    (OptionType::Call, 110.0, 0.003355166081696002),
    (OptionType::Call, 115.0, 0.00306948477997236),
    (OptionType::Call, 120.0, 0.0028188056131883497),
    (OptionType::Call, 125.0, 0.0025976342175353974),
    (OptionType::Call, 130.0, 0.00240151372092659),
    (OptionType::Call, 135.0, 0.0022267981838663756),
    (OptionType::Put, 100.0, 0.0020980108953598223),
    (OptionType::Put, 95.0, 0.004499925831976314),
    (OptionType::Put, 90.0, 0.0050146012796917026),
    (OptionType::Put, 85.0, 0.005622959606299653),
    (OptionType::Put, 80.0, 0.006349214454288012),
    (OptionType::Put, 75.0, 0.007225946122328138),
    (OptionType::Put, 70.0, 0.008297829970538372),
    (OptionType::Put, 65.0, 0.009627459843254894),
    (OptionType::Put, 60.0, 0.011304730223884604),
    (OptionType::Put, 55.0, 0.013462502343838854),
    (OptionType::Put, 50.0, 0.016303878162346755),
];
