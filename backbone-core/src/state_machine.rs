//! Generic state machine infrastructure for domain state machines.
//!
//! Phase 0 generic base for the `state_machine.rs` generator (Category C).
//!
//! Every generated `{Name}StateMachine` used to re-define the same structural
//! boilerplate. This module provides:
//!
//! - `StateMachineError` — shared error type (identical across all modules)
//! - `TransitionMeta<S>` — required trait for Transition enums
//! - `StateMachineBehavior` — trait with default impls for all mechanical methods
//!
//! Generated files collapse to:
//!
//! ```rust,ignore
//! use backbone_core::state_machine::{
//!     StateMachineError, StateMachineBehavior, TransitionMeta,
//! };
//!
//! pub enum AgentState { PendingVerification, Active, Terminated }
//! pub enum AgentTransition { Approve, Suspend, Terminate }
//!
//! impl TransitionMeta<AgentState> for AgentTransition {
//!     fn target_state(&self) -> AgentState { ... }
//!     fn all() -> Vec<Self> { vec![...] }
//!     fn allowed_roles(&self) -> &'static [&'static str] { ... }
//! }
//!
//! pub struct AgentStateMachine { current_state: AgentState }
//!
//! impl StateMachineBehavior for AgentStateMachine {
//!     type State = AgentState;
//!     type Transition = AgentTransition;
//!     fn current_state(&self) -> AgentState { self.current_state }
//!     fn set_current_state(&mut self, s: AgentState) { self.current_state = s; }
//!     fn can_transition(&self, t: AgentTransition) -> bool { /* unique table */ }
//! }
//! ```

// ─── StateMachineError ────────────────────────────────────────────────────────

/// Shared error type for all state machines.
///
/// Generated modules re-export this via:
/// `pub use backbone_core::state_machine::StateMachineError;`
#[derive(Debug, Clone, thiserror::Error)]
pub enum StateMachineError {
    #[error("Invalid state: {0}")]
    InvalidState(String),

    #[error("Invalid transition: {0}")]
    InvalidTransition(String),

    #[error("Transition '{transition}' not allowed from state '{from}'")]
    TransitionNotAllowed {
        transition: String,
        from: String,
    },

    #[error("Role '{role}' not authorized for transition '{transition}'")]
    RoleNotAuthorized {
        role: String,
        transition: String,
    },

    #[error("Guard condition failed for transition '{0}'")]
    GuardFailed(String),

    #[error("Cannot transition from final state '{0}'")]
    FinalStateReached(String),
}

// ─── TransitionMeta ──────────────────────────────────────────────────────────

/// Required trait for Transition enums — provides the metadata that
/// `StateMachineBehavior` default methods need.
///
/// Implement this on each `{Name}Transition` enum:
///
/// ```rust,ignore
/// impl TransitionMeta<AgentState> for AgentTransition {
///     fn target_state(&self) -> AgentState { match self { ... } }
///     fn all() -> Vec<Self> { vec![...] }
///     fn allowed_roles(&self) -> &'static [&'static str] { match self { ... } }
/// }
/// ```
pub trait TransitionMeta<S>: Copy + std::fmt::Display {
    /// The state this transition leads to.
    fn target_state(&self) -> S;

    /// All possible transitions (used by `available_transitions`).
    fn all() -> Vec<Self>;

    /// Roles allowed to fire this transition. Empty slice means unrestricted.
    fn allowed_roles(&self) -> &'static [&'static str];
}

// ─── StateMachineBehavior ────────────────────────────────────────────────────

/// Trait that provides all mechanical state machine methods as default impls.
///
/// Implementors only need to define three methods:
/// - `current_state()` — read the current state field
/// - `set_current_state()` — write the current state field
/// - `can_transition()` — the unique per-entity transition table
///
/// All other methods (`transition`, `transition_with_role`,
/// `available_transitions`, `available_transitions_for_role`,
/// `transition_to_state`, `can_transition_with_role`) are provided for free.
pub trait StateMachineBehavior: Sized {
    type State: Copy + PartialEq + Default + std::fmt::Display;
    type Transition: TransitionMeta<Self::State>;

    // ── Required ──────────────────────────────────────────────────────────────

    fn current_state(&self) -> Self::State;
    fn set_current_state(&mut self, state: Self::State);

    /// The unique per-entity transition table.
    fn can_transition(&self, transition: Self::Transition) -> bool;

    // ── Provided (default impls) ──────────────────────────────────────────────

    fn new() -> Self where Self: Default { Self::default() }

    fn from_state(state: Self::State) -> Self where Self: Default {
        let mut sm = Self::default();
        sm.set_current_state(state);
        sm
    }

    /// Check if a transition is allowed for the given role.
    fn can_transition_with_role(&self, transition: Self::Transition, role: &str) -> bool {
        if !self.can_transition(transition) {
            return false;
        }
        let allowed = transition.allowed_roles();
        allowed.is_empty() || allowed.iter().any(|r| *r == role || *r == "*")
    }

    /// Apply a transition, returning the new state on success.
    fn transition(&mut self, transition: Self::Transition) -> Result<Self::State, StateMachineError> {
        if !self.can_transition(transition) {
            return Err(StateMachineError::TransitionNotAllowed {
                transition: transition.to_string(),
                from: self.current_state().to_string(),
            });
        }
        let next = transition.target_state();
        self.set_current_state(next);
        Ok(next)
    }

    /// Apply a transition with role authorization check.
    fn transition_with_role(
        &mut self,
        transition: Self::Transition,
        role: &str,
    ) -> Result<Self::State, StateMachineError> {
        if !self.can_transition(transition) {
            return Err(StateMachineError::TransitionNotAllowed {
                transition: transition.to_string(),
                from: self.current_state().to_string(),
            });
        }
        if !self.can_transition_with_role(transition, role) {
            return Err(StateMachineError::RoleNotAuthorized {
                role: role.to_string(),
                transition: transition.to_string(),
            });
        }
        let next = transition.target_state();
        self.set_current_state(next);
        Ok(next)
    }

    /// All transitions valid from the current state.
    fn available_transitions(&self) -> Vec<Self::Transition> {
        Self::Transition::all()
            .into_iter()
            .filter(|t| self.can_transition(*t))
            .collect()
    }

    /// All transitions valid from the current state for the given role.
    fn available_transitions_for_role(&self, role: &str) -> Vec<Self::Transition> {
        Self::Transition::all()
            .into_iter()
            .filter(|t| self.can_transition_with_role(*t, role))
            .collect()
    }

    /// Transition directly to a target state by finding any valid transition
    /// that leads there from the current state.
    fn transition_to_state(
        &mut self,
        target: Self::State,
    ) -> Result<Self::State, StateMachineError> {
        let valid = Self::Transition::all()
            .into_iter()
            .filter(|t| self.can_transition(*t))
            .find(|t| t.target_state() == target);
        match valid {
            Some(t) => self.transition(t),
            None => Err(StateMachineError::TransitionNotAllowed {
                transition: target.to_string(),
                from: self.current_state().to_string(),
            }),
        }
    }
}
