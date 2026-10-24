// Встраивает target и короткий коммит в `version`.
fn main() {
    println!(
        "cargo:rustc-env=SHERPA_TARGET={}",
        std::env::var("TARGET").unwrap_or_default()
    );
    let commit = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    println!("cargo:rustc-env=SHERPA_COMMIT={commit}");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
}
