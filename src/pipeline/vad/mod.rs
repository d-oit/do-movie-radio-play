mod energy;
mod engine;

use anyhow::{bail, Result};

pub use energy::EnergyVad;
pub use engine::VadEngine;

pub fn create_engine(name: &str, threshold: f32) -> Result<Box<dyn VadEngine>> {
    match name {
        "energy" => Ok(Box::new(EnergyVad::new(threshold))),
        _ => bail!("unknown VAD engine '{name}'"),
    }
}
