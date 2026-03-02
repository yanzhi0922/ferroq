//! Backend adapter implementations.

pub mod failover;
pub mod lagrange;

pub use failover::FailoverAdapter;
pub use lagrange::LagrangeAdapter;
