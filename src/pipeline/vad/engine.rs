use crate::types::frame::Frame;

pub trait VadEngine: Send + Sync {
    fn classify(&self, frames: &[Frame]) -> Vec<bool>;
    fn name(&self) -> &'static str;
}
