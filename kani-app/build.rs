fn main() {
    println!("cargo:rerun-if-changed=../migrations");
    println!("cargo:rerun-if-changed=proto/tachiyomi_backup.proto");

    let fds = protox::compile(["proto/tachiyomi_backup.proto"], ["proto/"]).unwrap();
    prost_build::Config::new()
        .compile_fds(fds)
        .unwrap();
}
