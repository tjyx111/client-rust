//! Shared foundation for the incremental VPN client implementation.
//!
//! S0 intentionally contains no VPN production behavior. This crate exists so
//! formatting, linting, testing, and Release builds exercise a real workspace.

#![forbid(unsafe_code)]

#[cfg(test)]
mod tests {
    #[test]
    fn workspace_test_harness_is_active() {
        assert_eq!(env!("CARGO_PKG_NAME"), "simple-vpn-foundation");
        assert_eq!(env!("CARGO_PKG_VERSION"), "0.1.0");
    }
}
