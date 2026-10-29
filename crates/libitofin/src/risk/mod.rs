//! Risk analytics over the live instrument graph.
//!
//! Home of [`sensitivity`], the bump-and-reprice PV01 helpers.

pub mod sensitivity;

pub use sensitivity::{
    DEFAULT_SHIFT, Rows, SensitivityAnalysis, aggregate_npv, bucket_analysis,
    bucket_analysis_matrix, bucket_analysis_vector, parallel_analysis,
};
