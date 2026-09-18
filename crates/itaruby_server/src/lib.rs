//! Synchronous LSP server for itaruby (rust-analyzer style: no async runtime).

pub mod main_loop;

/// Runs the language server over stdio until the client disconnects.
///
/// # Errors
///
/// Returns `Err` if the LSP handshake or message loop fails (malformed
/// params, a broken stdio channel), or if the stdio I/O threads fail to
/// join cleanly.
pub fn run_server() -> anyhow::Result<()> {
    main_loop::run(lsp_server::Connection::stdio())
}
