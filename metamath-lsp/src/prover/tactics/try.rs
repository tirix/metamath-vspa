use crate::prover::{Context, Tactics, TacticsError, TacticsResult};

/// A tactics which tries a list of tactics until one of them produces a proof.
pub struct Try {
    tactics: Vec<Box<dyn Tactics>>,
}

impl Try {
    /// Creates a new "Try" tactics with the given sub-tactics.
    pub fn new(tactics: Vec<Box<dyn Tactics>>) -> Self {
        Self { tactics }
    }
}

impl Tactics for Try {
    fn get_name(&self) -> String {
        "try".to_string()
    }

    fn elaborate(&self, context: &mut Context) -> TacticsResult {
        for t in self.tactics.iter() {
            if let Ok(step) = t.elaborate(context) {
                return Ok(step);
            }
        }
        Err(TacticsError::from(
            "None of the tactics tried produced a proof",
        ))
    }
}
