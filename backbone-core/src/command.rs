//! CQRS Command Pattern
//!
//! Provides traits for implementing the Command side of CQRS.
//! Commands represent intentions to change state in the system.
//!
//! # Example
//!
//! ```ignore
//! use backbone_core::{Command, CommandHandler};
//!
//! // Define a command
//! pub struct CreateUserCommand {
//!     pub email: String,
//!     pub name: String,
//! }
//!
//! impl Command for CreateUserCommand {
//!     type Result = UserId;
//! }
//!
//! // Implement the handler
//! pub struct CreateUserHandler {
//!     user_repository: Arc<dyn UserRepository>,
//! }
//!
//! #[async_trait::async_trait]
//! impl CommandHandler<CreateUserCommand> for CreateUserHandler {
//!     type Error = UserError;
//!
//!     async fn handle(&self, command: CreateUserCommand) -> Result<UserId, Self::Error> {
//!         let user = User::new(command.email, command.name)?;
//!         self.user_repository.create(user).await
//!     }
//! }
//! ```

use async_trait::async_trait;

/// Marker trait for CQRS commands.
///
/// Commands represent an intent to change the system state.
/// Each command should have a single handler.
pub trait Command: Send + Sync {
    /// The result type returned after successful command execution.
    type Result: Send + Sync;
}

/// Handler for executing commands.
///
/// Each command type should have exactly one handler.
/// Handlers contain the business logic for processing commands.
#[async_trait]
pub trait CommandHandler<C: Command>: Send + Sync {
    /// Error type for command execution failures.
    type Error: std::error::Error + Send + Sync;

    /// Execute the command and return the result.
    async fn handle(&self, command: C) -> Result<C::Result, Self::Error>;
}

/// Command dispatcher for routing commands to their handlers.
///
/// Provides a central point for command execution with
/// optional middleware support (logging, validation, etc.).
#[async_trait]
pub trait CommandDispatcher: Send + Sync {
    /// Dispatch a command to its handler.
    async fn dispatch<C: Command>(
        &self,
        command: C,
    ) -> Result<C::Result, Box<dyn std::error::Error + Send + Sync>>;
}

/// Trait for commands that can be validated before execution.
pub trait ValidatableCommand: Command {
    /// Validation error type.
    type ValidationError: std::error::Error + Send + Sync;

    /// Validate the command before execution.
    fn validate(&self) -> Result<(), Self::ValidationError>;
}

/// Extension trait for handlers that support validated commands.
#[async_trait]
pub trait ValidatingCommandHandler<C: ValidatableCommand + 'static>: CommandHandler<C> {
    /// Handle the command with validation.
    async fn handle_validated(&self, command: C) -> Result<C::Result, Self::Error> {
        // Note: Validation should be called by the dispatcher
        self.handle(command).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestCommand {
        value: i32,
    }

    impl Command for TestCommand {
        type Result = i32;
    }

    struct TestHandler;

    #[async_trait]
    impl CommandHandler<TestCommand> for TestHandler {
        type Error = std::io::Error;

        async fn handle(&self, command: TestCommand) -> Result<i32, Self::Error> {
            Ok(command.value * 2)
        }
    }

    #[tokio::test]
    async fn test_command_handler() {
        let handler = TestHandler;
        let command = TestCommand { value: 21 };
        let result = handler.handle(command).await.unwrap();
        assert_eq!(result, 42);
    }
}
