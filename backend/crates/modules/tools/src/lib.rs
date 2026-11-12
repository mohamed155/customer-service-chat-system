pub mod approval;
pub mod audit;
pub mod credential;
pub mod executor;
pub mod model;
pub mod policy;
pub mod queries;
pub mod registry;
pub mod routes;

// Tool executions emit `tool.executed` audit entries via `audit::record_execution`.
