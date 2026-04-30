//! @arch:layer(rpc)
//! @arch:role(transport)
//! @arch:thread(async_io)
//!
//! `yah-rpc-ssh` — JSON-RPC 2.0 client transport that drives a remote
//! `yah serve --stdio` over SSH.
//!
//! The crate is split into two layers so each can be tested in isolation:
//!
//! - [`session`] — transport-agnostic JSON-RPC session over any
//!   `AsyncRead + AsyncWrite` pair. Owns the request/response multiplex,
//!   the pending-id map, and the notification fan-out for `arch:event`
//!   frames the daemon emits. Tests construct it on top of in-memory
//!   pipes and drive a fake server task — no SSH required.
//!
//! - [`client`] — [`SshRpcClient`]: spawns the local `ssh` binary,
//!   launches `yah serve --stdio --rig <workspace>` on the remote, and
//!   wires the SSH channel's stdin/stdout into a [`JsonRpcSession`].
//!   Typed methods mirror the surface of `KgService` so the Tauri host's
//!   RigBackend dispatch (R019-F3) stays a thin match-on-enum.
//!
//! Reconnect on transport drop is handled at the [`SshRpcClient`] layer:
//! when the underlying session closes (ssh died, network stalled, kernel
//! sent SIGPIPE), [`SshRpcClient::call`] returns
//! [`RpcError::TransportClosed`] and the next call walks the
//! exponential-backoff reconnect path. The Tauri host's ConnectionStrip
//! already paints the red state from the resulting error events — no
//! extra plumbing here.
//!
//! See `architecture/rig-backend-dispatch.md` for how this slots into
//! the renderer/Tauri/daemon stack.

pub mod client;
pub mod session;

pub use client::{OpenRigResult, ReconnectPolicy, ReindexReasonWire, SshRpcClient, SshRpcConfig};
pub use session::{ArchEventStream, JsonRpcSession, RpcError};
