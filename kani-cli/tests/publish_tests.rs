#![allow(clippy::unwrap_used)]

use std::path::Path;

use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use ed25519_dalek::SigningKey;
use kani_cli::{
    commands::publish::{load_index, RepoIndex},
    signing,
};

fn gen_signing_key() -> SigningKey {
    let mut seed = [0u8; 32];
    use std::io::Read as _;
    std::fs::File::open("/dev/urandom")
        .unwrap()
        .read_exact(&mut seed)
        .unwrap();
    SigningKey::from_bytes(&seed)
}

fn write_key_file(dir: &Path, name: &str, key: &SigningKey) -> (std::path::PathBuf, std::path::PathBuf) {
    let pub_path = dir.join(format!("{name}.pub"));
    let key_path = dir.join(format!("{name}.key"));
    std::fs::write(&pub_path, signing::pubkey_b64(key)).unwrap();
    std::fs::write(&key_path, B64.encode(key.to_bytes())).unwrap();
    (pub_path, key_path)
}

const MINIMAL_YAML: &str = r#"
id: test-source
name: Test Source
base_url: https://example.com
version: "1.0.0"
search:
  path: /search?q={query}
  container: .item
  fields:
    title: h3
    url: a[href]
    cover: img[src]
manga_detail:
  container: .detail
  fields:
    title: h1
    description: .desc
    cover: img[src]
chapter_list:
  container: li
  fields:
    title: a
    url: a[href]
    number: .num
page_list:
  container: img
  fields:
    url: img[src]
"#;

#[test]
fn publish_yaml_creates_artifact_and_signature() {
    let tmp = tempfile::tempdir().unwrap();
    let author = gen_signing_key();
    let (_, author_key_path) = write_key_file(tmp.path(), "author", &author);

    let yaml_path = tmp.path().join("test-source.yaml");
    std::fs::write(&yaml_path, MINIMAL_YAML).unwrap();

    let repo_dir = tmp.path().join("repo");
    std::fs::create_dir_all(&repo_dir).unwrap();

    kani_cli::commands::publish::run(
        &yaml_path,
        &author_key_path,
        &repo_dir,
        None,
        None,
    )
    .unwrap();

    let artifact_path = repo_dir.join("extensions/test-source/1.0.0/extension.yaml");
    assert!(artifact_path.exists(), "artifact file should exist");

    let sig_path = repo_dir.join("extensions/test-source/1.0.0/extension.yaml.sig");
    assert!(sig_path.exists(), "signature file should exist");

    let artifact_bytes = std::fs::read(&artifact_path).unwrap();
    let sig_b64 = std::fs::read_to_string(&sig_path).unwrap();
    let sig_b64 = sig_b64.trim();
    let author_pub = signing::pubkey_b64(&author);

    signing::verify_artifact(&artifact_bytes, &author_pub, sig_b64).unwrap();
}

#[test]
fn publish_upserts_index_entry() {
    let tmp = tempfile::tempdir().unwrap();
    let author = gen_signing_key();
    let (_, author_key_path) = write_key_file(tmp.path(), "author", &author);

    let yaml_path = tmp.path().join("test-source.yaml");
    std::fs::write(&yaml_path, MINIMAL_YAML).unwrap();

    let repo_dir = tmp.path().join("repo");
    std::fs::create_dir_all(&repo_dir).unwrap();

    kani_cli::commands::publish::run(&yaml_path, &author_key_path, &repo_dir, None, None).unwrap();

    let (index, _) = load_index(&repo_dir).unwrap();
    assert_eq!(index.extensions.len(), 1);
    assert_eq!(index.extensions[0].id, "test-source");
    assert_eq!(index.extensions[0].version, "1.0.0");
    assert_eq!(index.extensions[0].format, "yaml");
}

#[test]
fn publish_with_repo_sign_key_creates_index_sig() {
    let tmp = tempfile::tempdir().unwrap();
    let author = gen_signing_key();
    let maintainer = gen_signing_key();
    let (maintainer_pub_path, maintainer_key_path) =
        write_key_file(tmp.path(), "maintainer", &maintainer);
    let (_, author_key_path) = write_key_file(tmp.path(), "author", &author);

    let yaml_path = tmp.path().join("test-source.yaml");
    std::fs::write(&yaml_path, MINIMAL_YAML).unwrap();

    let repo_dir = tmp.path().join("repo");
    std::fs::create_dir_all(&repo_dir).unwrap();

    let index = RepoIndex {
        name: "Test Repo".to_string(),
        maintainer_key: signing::pubkey_b64(&maintainer),
        extensions: vec![],
    };
    std::fs::write(
        repo_dir.join("index.json"),
        serde_json::to_string(&index).unwrap(),
    )
    .unwrap();

    kani_cli::commands::publish::run(
        &yaml_path,
        &author_key_path,
        &repo_dir,
        Some(&maintainer_key_path),
        None,
    )
    .unwrap();

    let sig_path = repo_dir.join("index.json.sig");
    assert!(sig_path.exists(), "index.json.sig should exist");

    let (_, index_bytes) = load_index(&repo_dir).unwrap();
    let sig_b64 = std::fs::read_to_string(&sig_path).unwrap();
    let maintainer_pub = signing::pubkey_b64(&maintainer);
    signing::verify_artifact(&index_bytes, &maintainer_pub, sig_b64.trim()).unwrap();

    kani_cli::commands::repo::run_verify(&repo_dir, Some(&maintainer_pub_path)).unwrap();
}

#[test]
fn repo_verify_detects_tampered_artifact() {
    let tmp = tempfile::tempdir().unwrap();
    let author = gen_signing_key();
    let maintainer = gen_signing_key();
    let (maintainer_pub_path, maintainer_key_path) =
        write_key_file(tmp.path(), "maintainer", &maintainer);
    let (_, author_key_path) = write_key_file(tmp.path(), "author", &author);

    let yaml_path = tmp.path().join("test-source.yaml");
    std::fs::write(&yaml_path, MINIMAL_YAML).unwrap();

    let repo_dir = tmp.path().join("repo");
    std::fs::create_dir_all(&repo_dir).unwrap();

    let index = RepoIndex {
        name: "Test Repo".to_string(),
        maintainer_key: signing::pubkey_b64(&maintainer),
        extensions: vec![],
    };
    std::fs::write(
        repo_dir.join("index.json"),
        serde_json::to_string(&index).unwrap(),
    )
    .unwrap();

    kani_cli::commands::publish::run(
        &yaml_path,
        &author_key_path,
        &repo_dir,
        Some(&maintainer_key_path),
        None,
    )
    .unwrap();

    let artifact_path = repo_dir.join("extensions/test-source/1.0.0/extension.yaml");
    let mut artifact_bytes = std::fs::read(&artifact_path).unwrap();
    artifact_bytes[0] ^= 0x01;
    std::fs::write(&artifact_path, &artifact_bytes).unwrap();

    let result = kani_cli::commands::repo::run_verify(&repo_dir, Some(&maintainer_pub_path));
    assert!(result.is_err(), "verify should fail on tampered artifact");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("failed verification") || err.contains("mismatch") || err.contains("invalid"),
        "unexpected error: {err}"
    );
}

#[test]
fn keygen_creates_pub_and_key_files() {
    let tmp = tempfile::tempdir().unwrap();
    kani_cli::commands::keygen::run(
        &tmp.path().to_path_buf(),
        "author",
        None,
    )
    .unwrap();

    let pub_path = tmp.path().join("author.pub");
    let key_path = tmp.path().join("author.key");
    assert!(pub_path.exists());
    assert!(key_path.exists());

    let (_, _) = signing::load_verifying_key(&pub_path).unwrap();
    let _ = signing::load_signing_key(&key_path).unwrap();
}

#[test]
fn keygen_pub_matches_private_key() {
    let tmp = tempfile::tempdir().unwrap();
    kani_cli::commands::keygen::run(
        &tmp.path().to_path_buf(),
        "test",
        None,
    )
    .unwrap();

    let signing_key = signing::load_signing_key(&tmp.path().join("test.key")).unwrap();
    let (verifying_bytes, verifying_b64) = signing::load_verifying_key(&tmp.path().join("test.pub")).unwrap();

    assert_eq!(signing_key.verifying_key().to_bytes(), verifying_bytes);
    assert_eq!(signing::pubkey_b64(&signing_key), verifying_b64);
}

#[test]
fn show_fingerprint_prints_sha256_prefix() {
    let tmp = tempfile::tempdir().unwrap();
    kani_cli::commands::keygen::run(&tmp.path().to_path_buf(), "test", None).unwrap();

    let (key_bytes, _) = signing::load_verifying_key(&tmp.path().join("test.pub")).unwrap();
    let fp = signing::key_fingerprint(&key_bytes);
    assert!(fp.starts_with("SHA256:"), "fingerprint should start with SHA256:");
}

#[test]
fn repo_list_shows_extensions() {
    let tmp = tempfile::tempdir().unwrap();
    let author = gen_signing_key();
    let (_, author_key_path) = write_key_file(tmp.path(), "author", &author);

    let yaml_path = tmp.path().join("test-source.yaml");
    std::fs::write(&yaml_path, MINIMAL_YAML).unwrap();

    let repo_dir = tmp.path().join("repo");
    std::fs::create_dir_all(&repo_dir).unwrap();

    kani_cli::commands::publish::run(&yaml_path, &author_key_path, &repo_dir, None, None).unwrap();

    kani_cli::commands::repo::run_list(&repo_dir).unwrap();

    let (index, _) = load_index(&repo_dir).unwrap();
    assert_eq!(index.extensions.len(), 1);
    assert_eq!(index.extensions[0].id, "test-source");
}

#[test]
fn repo_list_empty_repo() {
    let tmp = tempfile::tempdir().unwrap();
    kani_cli::commands::keygen::run(&tmp.path().to_path_buf(), "m", None).unwrap();

    let repo_dir = tmp.path().join("repo");
    kani_cli::commands::repo::run_init(&repo_dir, "Empty Repo", &tmp.path().join("m.pub")).unwrap();

    kani_cli::commands::repo::run_list(&repo_dir).unwrap();
}

#[test]
fn repo_add_copies_artifact_and_updates_index() {
    let tmp = tempfile::tempdir().unwrap();
    let author = gen_signing_key();
    let (author_pub_path, author_key_path) = write_key_file(tmp.path(), "author", &author);

    let yaml_path = tmp.path().join("test-source.yaml");
    std::fs::write(&yaml_path, MINIMAL_YAML).unwrap();

    let staging_dir = tmp.path().join("staging");
    std::fs::create_dir_all(&staging_dir).unwrap();

    kani_cli::commands::publish::run(&yaml_path, &author_key_path, &staging_dir, None, None)
        .unwrap();

    let staged_artifact = staging_dir.join("extensions/test-source/1.0.0/extension.yaml");

    let repo_dir = tmp.path().join("repo");
    kani_cli::commands::repo::run_init(&repo_dir, "Curated Repo", &author_pub_path).unwrap();

    kani_cli::commands::repo::run_add(
        &staged_artifact,
        &author_pub_path,
        &repo_dir,
        None,
        None,
    )
    .unwrap();

    let artifact_dest = repo_dir.join("extensions/test-source/1.0.0/extension.yaml");
    assert!(artifact_dest.exists(), "artifact should be copied to repo");

    let sig_dest = repo_dir.join("extensions/test-source/1.0.0/extension.yaml.sig");
    assert!(sig_dest.exists(), "sig should be copied to repo");

    let (index, _) = load_index(&repo_dir).unwrap();
    assert_eq!(index.extensions.len(), 1);
    assert_eq!(index.extensions[0].id, "test-source");
}

#[test]
fn repo_add_rejects_tampered_artifact() {
    let tmp = tempfile::tempdir().unwrap();
    let author = gen_signing_key();
    let (author_pub_path, author_key_path) = write_key_file(tmp.path(), "author", &author);

    let yaml_path = tmp.path().join("test-source.yaml");
    std::fs::write(&yaml_path, MINIMAL_YAML).unwrap();

    let staging_dir = tmp.path().join("staging");
    std::fs::create_dir_all(&staging_dir).unwrap();
    kani_cli::commands::publish::run(&yaml_path, &author_key_path, &staging_dir, None, None)
        .unwrap();

    let artifact_path = staging_dir.join("extensions/test-source/1.0.0/extension.yaml");
    let mut bytes = std::fs::read(&artifact_path).unwrap();
    bytes[0] ^= 0xFF;
    std::fs::write(&artifact_path, bytes).unwrap();

    let repo_dir = tmp.path().join("repo");
    std::fs::create_dir_all(&repo_dir).unwrap();

    let result = kani_cli::commands::repo::run_add(
        &artifact_path,
        &author_pub_path,
        &repo_dir,
        None,
        None,
    );
    assert!(result.is_err(), "add should fail on tampered artifact");
}

#[test]
fn repo_init_creates_index_and_extensions_dir() {
    let tmp = tempfile::tempdir().unwrap();
    kani_cli::commands::keygen::run(&tmp.path().to_path_buf(), "maintainer", None).unwrap();

    let repo_dir = tmp.path().join("my-repo");
    kani_cli::commands::repo::run_init(
        &repo_dir,
        "My Test Repo",
        &tmp.path().join("maintainer.pub"),
    )
    .unwrap();

    assert!(repo_dir.join("index.json").exists());
    assert!(repo_dir.join("extensions").is_dir());

    let raw = std::fs::read(repo_dir.join("index.json")).unwrap();
    let index: RepoIndex = serde_json::from_slice(&raw).unwrap();
    assert_eq!(index.name, "My Test Repo");
    assert!(index.extensions.is_empty());
}
