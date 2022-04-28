//! A Collection of Tactics

mod apply;
mod assumption;
mod sorry;
mod r#try;

pub use apply::Apply;
pub use assumption::Assumption;
pub use sorry::Sorry;
pub use r#try::Try;