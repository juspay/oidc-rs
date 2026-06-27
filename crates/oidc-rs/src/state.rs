//! Shared auth-state types used by framework adapter middleware.

use crate::{BasicExchanger, Validator};

/// Shared, runtime-built state. Holds the configured mode.
#[derive(Clone)]
pub struct AuthState {
    /// Auth mode: disabled (synthetic [`crate::Identity::Disabled`]) or
    /// enabled with a [`Validator`] + [`BasicExchanger`].
    pub mode: AuthMode,
}

/// Auth mode discriminator.
#[derive(Clone)]
pub enum AuthMode {
    /// Bypass all auth.
    Disabled,
    /// Validate inbound credentials.
    Enabled {
        /// JWT validator.
        validator: Validator,
        /// Basic→JWT exchanger.
        exchanger: BasicExchanger,
    },
}

impl AuthState {
    /// Borrow the underlying exchanger, if any. Useful for cache-flush
    /// endpoints.
    ///
    /// # Returns
    ///
    /// `Some(&BasicExchanger)` when in enabled mode, `None` when disabled.
    pub fn exchanger(&self) -> Option<&BasicExchanger> {
        match &self.mode {
            AuthMode::Enabled { exchanger, .. } => Some(exchanger),
            AuthMode::Disabled => None,
        }
    }
}
