//! Plico immediate declarations projected to NSH's native element tree.
//!
//! The application keeps an ordinary [`plico::Engine`] and [`plico::View`].
//! NSH owns reconciliation, Taffy layout, native text, painting and windows;
//! this crate does not use Plico's renderer or layout backends.

mod projection;

pub use projection::{Adapter, Error};

#[cfg(feature = "native")]
pub mod native;
