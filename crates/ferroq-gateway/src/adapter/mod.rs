//! Backend adapter implementations.

pub mod failover;
pub mod lagrange;
pub mod official;

pub use failover::FailoverAdapter;
pub use lagrange::LagrangeAdapter;
pub use official::OfficialAdapter;
