//! TOML+ language server entry point. Speaks LSP over stdio.

use std::fs::OpenOptions;
use std::io::Write;
use std::panic;
use std::path::PathBuf;

use tower_lsp::{LspService, Server};

mod server;

fn log_path() -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push("tomlplus-lsp.log");
    p
}

fn log_line(s: &str) {
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(log_path()) {
        let _ = writeln!(f, "{}", s);
    }
}

fn install_panic_hook() {
    panic::set_hook(Box::new(|info| {
        let bt = std::backtrace::Backtrace::force_capture();
        let msg = format!("PANIC: {}\nBACKTRACE:\n{}", info, bt);
        log_line(&msg);
        eprintln!("{}", msg);
    }));
}

fn main() {
    install_panic_hook();
    log_line(&format!(
        "─── tomlplus-lsp v{} starting (pid={}, args={:?}) ───",
        env!("CARGO_PKG_VERSION"),
        std::process::id(),
        std::env::args().collect::<Vec<_>>(),
    ));

    let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(r) => r,
        Err(e) => {
            log_line(&format!("FATAL: tokio runtime build failed: {}", e));
            std::process::exit(2);
        }
    };

    runtime.block_on(async {
        let stdin  = tokio::io::stdin();
        let stdout = tokio::io::stdout();
        let (service, socket) = LspService::new(server::Backend::new);
        log_line("server: entering serve loop");
        Server::new(stdin, stdout, socket).serve(service).await;
        log_line("server: serve loop exited normally");
    });
}
