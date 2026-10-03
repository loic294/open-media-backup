//! Safely erasing original/temporary devices once enough final copies exist.
mod format;
mod plan;
mod run;

pub use plan::{assess, plan_wipe, WipePlan};
pub use run::{wipe, WipeMethod};

#[cfg(test)]
mod tests;
