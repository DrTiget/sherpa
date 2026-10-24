//! `sherpa-core` — тонкая обёртка: логи в stderr + `sherpa_lib::run_cli`.

use tracing_subscriber::EnvFilter;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let level = sherpa_lib::commands::log_level_from_args(&args).unwrap_or_else(|| "warn".into());
    let filter = EnvFilter::try_new(format!("{level},russh=warn")).unwrap_or_else(|_| EnvFilter::new("warn"));
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(filter)
        .with_target(false)
        .init();

    let code = sherpa_lib::run_cli(args, &mut std::io::stdin().lock(), &mut std::io::stdout().lock());
    std::process::exit(code);
}
