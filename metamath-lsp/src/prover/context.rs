use crate::prover::Tactics;
use metamath_knife::as_str;
use metamath_knife::formula::Substitutions;
use metamath_knife::formula::TypeCode;
use metamath_knife::formula::WorkVariableProvider;
use metamath_knife::proof::ProofTreeArray;
use metamath_knife::scopeck::FrameRef;
use metamath_knife::verify::ProofBuilder;
use metamath_knife::Database;
use metamath_knife::Formula;
use metamath_knife::Label;
use metamath_knife::StatementType;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::sync::Arc;

use super::TacticsError;
use super::proof_step::ProofStep;

/// A type for representing theorem essential hypotheses: a label and the corresponding formula.
pub struct Hypotheses(Box<[(Label, Formula)]>);

type HypothesesIter<'a> = std::iter::Map<
    std::slice::Iter<'a, (Label, Formula)>,
    for<'r> fn(&'r (Label, Formula)) -> ProofStep,
>;

impl Hypotheses {
    fn known_steps_iter(&self) -> HypothesesIter {
        fn build_hyp(t: &(Label, Formula)) -> ProofStep {
            ProofStep::Hyp {
                label: t.0,
                result: t.1.clone(),
            }
        }
        self.0
            .iter()
            .map(build_hyp as fn(t: &(Label, Formula)) -> ProofStep)
    }

    fn from_frame(frame: FrameRef) -> Self {
        let hyp_vec: Vec<(Label, Formula)> =
            frame.essentials().map(|(l, f)| (l, f.clone())).collect();
        Self(hyp_vec.into_boxed_slice())
    }
}

impl IntoIterator for Hypotheses {
    type Item = (Label, Formula);
    type IntoIter = std::vec::IntoIter<(Label, Formula)>;

    fn into_iter(self) -> Self::IntoIter {
        Vec::from(self.0).into_iter()
    }
}

#[derive(Clone)]
pub struct Context {
    pub(crate) db: Database,
    loc_after: Label,
    goal: Formula,
    known_steps: Vec<(usize, ProofStep)>,
    variables: Substitutions,
    label_variables: HashMap<String, Label>,
    tactics_variables: HashMap<String, Arc<dyn Tactics>>,
    used_variables: HashMap<TypeCode, BTreeSet<Label>>,
    last_step_id: usize,
}

impl<'a> Context {
    pub fn new(
        db: Database,
        loc_after: Label,
        goal: Formula
    ) -> Self {
        let mut context = Context {
            db,
            loc_after,
            goal: goal.clone(),
            known_steps: Vec::default(),
            variables: Substitutions::default(),
            label_variables: HashMap::default(),
            tactics_variables: HashMap::default(),
            used_variables: HashMap::default(),
            last_step_id: 0,
        };
        context.build_used_variables_list(&goal);
        context
    }

    pub fn with_goal(&self, goal: Formula) -> Self {
        Self {
            db: self.db.clone(),
            loc_after: self.loc_after,
            goal,
            known_steps: self.known_steps.clone(),
            variables: self.variables.clone(),
            label_variables: self.label_variables.clone(),
            tactics_variables: self.tactics_variables.clone(),
            used_variables: self.used_variables.clone(),
            last_step_id: self.last_step_id,
        }
    }

    pub fn with_variables(&self, v: &Substitutions) -> Self {
        let mut variables = self.variables.clone();
        variables.extend(v);
        Self {
            db: self.db.clone(),
            loc_after: self.loc_after,
            goal: self.goal.clone(),
            known_steps: self.known_steps.clone(),
            variables,
            label_variables: self.label_variables.clone(),
            tactics_variables: self.tactics_variables.clone(),
            used_variables: self.used_variables.clone(),
            last_step_id: self.last_step_id,
        }
    }

    pub fn without_variables(&self) -> Self {
        Self {
            db: self.db.clone(),
            loc_after: self.loc_after,
            goal: self.goal.clone(),
            known_steps: self.known_steps.clone(),
            variables: Substitutions::default(),
            label_variables: self.label_variables.clone(),
            tactics_variables: self.tactics_variables.clone(),
            used_variables: self.used_variables.clone(),
            last_step_id: self.last_step_id,
        }
    }

    /// Builds the list of available variables, 
    /// removing variables occurring in the given formula from the list of available variables from which new variables can be taken.
    /// Think for example `syl: ( ph -> ps ) , ( ps -> ch ) |- ( ph -> ch )`
    /// When only unifying with the final statement, there is no substitution for the variable ` ps `.
    /// It might however already be in use in the rest of the proof.
    /// Therefore, a new variable with the same typecode is chosen for substitution (MMJ2 would have used `&W1`)
    fn build_used_variables_list(&mut self, formula: &Formula) {
        for (label, is_variable) in formula.labels_iter() {
            if is_variable {
                let typecode = self.db.label_typecode(label);
                self.used_variables.entry(typecode).or_insert_with(|| BTreeSet::new()).insert(label);
            }
        }
    }

    pub fn add_known_step(&mut self, step_id: Option<usize>, step: ProofStep) {
        self.build_used_variables_list(step.result());
        let step_id = step_id.unwrap_or_else(|| {
            self.last_step_id += 1;
            self.last_step_id
        });
        self.known_steps.push((step_id, step));
    }

    pub fn add_label_variable(&mut self, id: String, label: Label) {
        self.label_variables.insert(id, label);
    }

    pub fn add_tactics_variable(&mut self, id: String, tactics: Arc<dyn Tactics>) {
        self.tactics_variables.insert(id, tactics);
    }

    pub fn get_label_variable(&self, id: String) -> Option<Label> {
        self.label_variables.get(&id).copied()
    }

    pub fn get_tactics_variable(&self, id: String) -> Option<Arc<dyn Tactics>> {
        self.tactics_variables.get(&id).cloned()
    }

    /// Returns whether the given theorem is not allowed (too far down the database)
    pub fn loc_after(&self, theorem: Label) -> bool {
        self.db
            .cmp(&theorem, &self.loc_after)
            .unwrap_or(std::cmp::Ordering::Greater)
            != std::cmp::Ordering::Less
    }

    pub fn goal(&self) -> &Formula {
        &self.goal
    }
    pub fn known_steps(&self) -> &Vec<(usize, ProofStep)> {
        &self.known_steps
    }
    pub fn variables(&self) -> &Substitutions {
        &self.variables
    }

    pub fn get_theorem_formulas(&self, label: Label) -> Option<(Formula, Hypotheses)> {
        let sref = self.db.statement_by_label(label)?;
        let formula = self.db.stmt_parse_result().get_formula(&sref)?.clone();
        let frame = self.db.get_frame(label)?;
        Some((formula, Hypotheses::from_frame(frame)))
    }

    pub fn statements(&self) -> impl Iterator<Item = (Label, Formula, Hypotheses)> + '_ {
        // We can safely unwrap here because loc_after is known to be a known statement
        self.statements_until(self.loc_after).unwrap()
    }

    pub fn statements_until(
        &self,
        theorem: Label,
    ) -> Option<impl Iterator<Item = (Label, Formula, Hypotheses)> + '_> {
        let nset = self.db.name_result().clone();
        let iter = self
            .db
            .statements_range(..theorem)
            .filter_map(move |sref| match sref.statement_type() {
                StatementType::Axiom | StatementType::Provable => {
                    let name = sref.label();
                    let label = nset.lookup_label(name)?.atom;
                    let (formula, hyps) = self.get_theorem_formulas(label)?;
                    Some((label, formula, hyps))
                }
                _ => None,
            })
            .collect::<Vec<_>>()
            .into_iter();
        Some(iter)
    }

    /// Add a hypothesis step to a proof array
    pub fn build_proof_hyp(
        &self,
        label: Label,
        formula: Formula,
        stack_buffer: &mut Vec<u8>,
        arr: &mut ProofTreeArray,
    ) -> Option<usize> {
        let nset = self.db.name_result().clone();
        let token = nset.atom_name(label);
        let address = nset.lookup_label(token)?.address;
        let range = formula
            .as_ref(&self.db)
            .append_to_stack_buffer(stack_buffer);
        Some(arr.build(address, Default::default(), stack_buffer, range))
    }

    /// Add a normal step to a proof array
    pub fn build_proof_step(
        &self,
        label: Label,
        formula: Formula,
        mand_hyps: Vec<usize>,
        substitutions: &Substitutions,
        stack_buffer: &mut Vec<u8>,
        arr: &mut ProofTreeArray,
    ) -> Option<usize> {
        let token = self.db.name_result().atom_name(label);
        let address = self.db.name_result().lookup_label(token)?.address;
        let range = formula
            .as_ref(&self.db)
            .append_to_stack_buffer(stack_buffer);
        let frame = self.db.get_frame(label)?;
        let mut hyps = vec![];
        for label in frame.floating() {
            let formula = &substitutions.get(label).unwrap_or_else(|| {
                panic!(
                    "While building proof using {}: No substitution for {}",
                    as_str(token),
                    as_str(self.db.name_result().atom_name(label))
                );
            });
            let proof_tree_index = formula
                .as_ref(&self.db)
                .build_syntax_proof::<usize, Vec<usize>>(stack_buffer, arr);
            hyps.push(proof_tree_index);
        }
        hyps.extend(mand_hyps);
        Some(arr.build(address, hyps, stack_buffer, range))
    }
}

impl WorkVariableProvider<TacticsError> for Context {
    /// Finds the next available variable with the given typecode
    fn new_work_variable(&mut self, typecode: TypeCode) -> Result<Label, TacticsError> {
        // This is a naive implementation which goes through all the statements, and finds the first float not yet used.
        // TODO Cache! 
        let tc_token = self.db.name_result().atom_name(typecode);
        let used_variables = self.used_variables.entry(typecode).or_insert_with(|| BTreeSet::new());
        for sref in self.db.statements_range(..) {
            if sref.statement_type() != StatementType::Floating { continue; }
            if sref.math_at(0).slice != tc_token { continue; }
            let label = self.db.name_result().get_atom(sref.label());
            if used_variables.contains(&label) { continue; }
            used_variables.insert(label);
            return Ok(label);
        }
        Err(TacticsError::from("No available work variable left"))
    }
}