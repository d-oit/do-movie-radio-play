mod energy;
mod engine;
mod spectral;

use anyhow::{bail, Result};

pub use energy::EnergyVad;
pub use engine::VadEngine;
pub use spectral::SpectralVad;

pub fn create_engine(name: &str, threshold: f32) -> Result<Box<dyn VadEngine>> {
    match name {
        "energy" => Ok(Box::new(EnergyVad::new(threshold))),
        "spectral" => Ok(Box::new(SpectralVad::new(threshold))),
        _ => bail!("unknown VAD engine '{name}'"),
    }
}
