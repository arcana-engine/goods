fn main() {
    let version_meta = rustc_version::version_meta().unwrap();

    match version_meta.commit_hash {
        Some(commit_hash) => {
            println!(
                "cargo:rustc-env=RELIQUARY_IMPORT_RUSTC_VERSION={}.{}",
                version_meta.semver, commit_hash
            )
        }
        None => {
            println!(
                "cargo:rustc-env=RELIQUARY_IMPORT_RUSTC_VERSION={}",
                version_meta.semver
            )
        }
    }
}
