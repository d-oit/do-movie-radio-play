mod energy;
mod engine;
mod spectral;

use anyhow::{bail, Result};

pub use energy::EnergyVad;
pub use engine::VadEngine;
pub use spectral::SpectralVad;

pub fn create_engine(
    name: &str,
    threshold: f32,
    flatness_max: Option<f32>,
    entropy_min: Option<f32>,
    centroid_min: Option<f32>,
    centroid_max: Option<f32>,
) -> Result<Box<dyn VadEngine>> {
    match name {
        "energy" => Ok(Box::new(EnergyVad::new(threshold))),
        "spectral" => {
            let engine = match (flatness_max, entropy_min, centroid_min, centroid_max) {
                (Some(f), Some(e), Some(c_min), Some(c_max)) => {
                    SpectralVad::with_thresholds(threshold, f, e, c_min, c_max)
                }
                _ => SpectralVad::new(threshold),
            };
            Ok(Box::new(engine))
        }
        _ => bail!("unknown VAD engine '{name}'"),
    }
}
