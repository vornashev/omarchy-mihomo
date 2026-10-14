//! Profiles and runtime config generation.
//!
//! The pipeline follows Clash Verge Rev's "enhance" semantics so profiles
//! and override files migrate unchanged:
//!
//! 1. profile YAML (anchors and `<<` merge keys resolved)
//! 2. seq overrides of `rules`, `proxies`, `proxy-groups` (prepend/append/delete)
//! 3. global merge, then the profile's merge (deep merge)
//! 4. the user's own rules, on top of everything
//! 5. managed settings (ports, TUN, controller...) that overrides cannot change
//! 6. dangling group members pruned
//!
//! The result is validated with `mihomo -t` before it replaces the live config.

pub mod build;
pub mod core_bin;
pub mod error;
pub mod fetch;
pub mod merge;
pub mod paths;
pub mod profiles;
pub mod rules;
pub mod runtime;
pub mod seq;
pub mod settings;
pub mod yaml;

pub use error::{Error, Result};
