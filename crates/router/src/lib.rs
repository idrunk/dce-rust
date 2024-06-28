//! # Dce Router
//!
//! Dce router is the core feature of dce framework, it can help you do api routing in any type and any framework (not only with Dce) program coding.
//!
//! ## Features
//!
//! - `default`: `["async"]`
//! - `async`: You can define both async and normal sync fn as controller if this enabled, or just allow sync controller
//!
//! ## Examples
//!
//! ```passed
//! use dce_macro::api;
//! use dce_router::router::Router;
//! use dce_cli::protocol::{CliProtocol, CliRaw};
//!
//! #[tokio::main]
//! async fn main() {
//!     let router = Router::new()
//!         .push(sync)
//!         .push(a_sync)
//!         .ready();
//!     CliProtocol::new(1).route(router.clone(), Default::default()).await;
//! }
//!
//! #[api]
//! pub fn sync(req: CliRaw) {
//!     req.end(None)
//! }
//!
//! #[api]
//! pub async fn a_sync(req: CliRaw) {
//!     req.raw_resp("Hello world !".to_string())
//! }
//! ```
//!


pub mod api;
pub mod request;
pub mod router;
pub mod serializer;
pub mod protocol;
