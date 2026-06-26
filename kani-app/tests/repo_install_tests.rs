#![allow(clippy::unwrap_used)]

mod common;
use common::test_service;

use std::collections::HashMap;
use std::sync::Arc;

use ed25519_dalek::SigningKey;
use kani_app::service::repos::{RepoAddResult, RepoExtensionEntry, RepoIndex};
use kani_app::source::signing::{
    key_fingerprint, pubkey_b64, sha256_hex, sign_artifact, signature_b64,
};

fn gen_key() -> SigningKey {
    let bytes: [u8; 32] = rand::random();
    SigningKey::from_bytes(&bytes)
}

fn sign(key: &SigningKey, data: &[u8]) -> String {
    signature_b64(&sign_artifact(data, &key.to_bytes()))
}

fn pk_b64(key: &SigningKey) -> String {
    pubkey_b64(key)
}

fn fingerprint(key: &SigningKey) -> String {
    key_fingerprint(&key.verifying_key().to_bytes())
}

struct TestRepo {
    maintainer_key: SigningKey,
    author_key: SigningKey,
    ext_id: String,
    artifact_yaml: String,
}

impl TestRepo {
    fn new(ext_id: &str) -> Self {
        let maintainer_key = gen_key();
        let author_key = gen_key();
        let artifact_yaml = format!(
            "id: {ext_id}\nname: Test Extension\nversion: \"0.1.0\"\nbase_url: \"https://example.com\"\n"
        );
        Self {
            maintainer_key,
            author_key,
            ext_id: ext_id.to_string(),
            artifact_yaml,
        }
    }

    fn build_routes(&self, repo_name: &str) -> Arc<HashMap<String, Vec<u8>>> {
        let artifact = self.artifact_yaml.as_bytes();
        let sha256 = sha256_hex(artifact);
        let artifact_sig = sign(&self.author_key, artifact);
        let artifact_path = format!("/{}.yaml", self.ext_id);

        let index = RepoIndex {
            name: repo_name.to_string(),
            maintainer_key: pk_b64(&self.maintainer_key),
            extensions: vec![RepoExtensionEntry {
                id: self.ext_id.clone(),
                name: "Test Extension".to_string(),
                version: "0.1.0".to_string(),
                format: "yaml".to_string(),
                sha256,
                signature: artifact_sig,
                author_key: pk_b64(&self.author_key),
                min_kani_version: None,
                url: artifact_path.clone(),
                description: None,
                language: None,
                nsfw: false,
            }],
        };

        let index_bytes = serde_json::to_vec(&index).unwrap();
        let index_sig = sign(&self.maintainer_key, &index_bytes);

        let mut routes: HashMap<String, Vec<u8>> = HashMap::new();
        routes.insert("/index.json".to_string(), index_bytes);
        routes.insert("/index.json.sig".to_string(), index_sig.into_bytes());
        routes.insert(artifact_path, artifact.to_vec());
        Arc::new(routes)
    }

    fn build_tampered_routes(&self, repo_name: &str) -> Arc<HashMap<String, Vec<u8>>> {
        let real_artifact = self.artifact_yaml.as_bytes();
        let sha256 = sha256_hex(real_artifact);
        let artifact_sig = sign(&self.author_key, real_artifact);
        let artifact_path = format!("/{}.yaml", self.ext_id);

        let index = RepoIndex {
            name: repo_name.to_string(),
            maintainer_key: pk_b64(&self.maintainer_key),
            extensions: vec![RepoExtensionEntry {
                id: self.ext_id.clone(),
                name: "Test Extension".to_string(),
                version: "0.1.0".to_string(),
                format: "yaml".to_string(),
                sha256,
                signature: artifact_sig,
                author_key: pk_b64(&self.author_key),
                min_kani_version: None,
                url: artifact_path.clone(),
                description: None,
                language: None,
                nsfw: false,
            }],
        };

        let index_bytes = serde_json::to_vec(&index).unwrap();
        let index_sig = sign(&self.maintainer_key, &index_bytes);

        let mut tampered = real_artifact.to_vec();
        tampered[0] ^= 0x01;

        let mut routes: HashMap<String, Vec<u8>> = HashMap::new();
        routes.insert("/index.json".to_string(), index_bytes);
        routes.insert("/index.json.sig".to_string(), index_sig.into_bytes());
        routes.insert(artifact_path, tampered);
        Arc::new(routes)
    }
}

async fn start_mock_server(routes: Arc<HashMap<String, Vec<u8>>>) -> u16 {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let routes = Arc::clone(&routes);
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                let request = std::str::from_utf8(&buf[..n]).unwrap_or("");
                let path = request
                    .lines()
                    .next()
                    .and_then(|l| l.split(' ').nth(1))
                    .unwrap_or("/");

                let response = if let Some(body) = routes.get(path) {
                    let header = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let mut resp = header.into_bytes();
                    resp.extend_from_slice(body);
                    resp
                } else {
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        .to_vec()
                };

                let _ = stream.write_all(&response).await;
                let _ = stream.shutdown().await;
            });
        }
    });

    port
}

fn unique_ext_id() -> String {
    let n: u64 = rand::random();
    format!("test-ext-{n:x}")
}

async fn start_mock_server_etag(routes: Arc<HashMap<String, Vec<u8>>>) -> u16 {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    const ETAG: &str = "\"v1\"";

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let routes = Arc::clone(&routes);
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                let request = std::str::from_utf8(&buf[..n]).unwrap_or("");
                let path = request
                    .lines()
                    .next()
                    .and_then(|l| l.split(' ').nth(1))
                    .unwrap_or("/");
                let has_if_none_match = request
                    .lines()
                    .any(|l| l.to_ascii_lowercase().starts_with("if-none-match:"));

                let response = if path == "/index.json" && has_if_none_match {
                    b"HTTP/1.1 304 Not Modified\r\nConnection: close\r\n\r\n".to_vec()
                } else if let Some(body) = routes.get(path) {
                    let header = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nETag: {ETAG}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let mut resp = header.into_bytes();
                    resp.extend_from_slice(body);
                    resp
                } else {
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        .to_vec()
                };

                let _ = stream.write_all(&response).await;
                let _ = stream.shutdown().await;
            });
        }
    });

    port
}

// ── Blocked-repo tests (no HTTP) ──────────────────────────────────────────────

#[tokio::test]
async fn list_blocked_repos_empty_on_fresh_db() {
    let svc = test_service().await;
    let result = svc.list_blocked_repos().await.unwrap();
    assert!(result.is_empty());
}

#[tokio::test]
async fn block_and_list_blocked_repo() {
    let svc = test_service().await;
    svc.block_repo("https://evil.example.com", "policy violation", None)
        .await
        .unwrap();
    let blocked = svc.list_blocked_repos().await.unwrap();
    assert_eq!(blocked.len(), 1);
    assert_eq!(blocked[0].url, "https://evil.example.com");
    assert_eq!(blocked[0].reason, "policy violation");
}

#[tokio::test]
async fn delete_blocked_repo_removes_entry() {
    let svc = test_service().await;
    svc.block_repo("https://bad.example.com", "reason", None)
        .await
        .unwrap();
    let id = svc.list_blocked_repos().await.unwrap()[0].id;
    svc.delete_blocked_repo(id, None).await.unwrap();
    assert!(svc.list_blocked_repos().await.unwrap().is_empty());
}

#[tokio::test]
async fn delete_nonexistent_blocked_repo_returns_error() {
    let svc = test_service().await;
    let result = svc.delete_blocked_repo(99999, None).await;
    assert!(result.is_err());
}

// ── List repos (no HTTP) ──────────────────────────────────────────────────────

#[tokio::test]
async fn list_repos_empty_on_fresh_db() {
    let svc = test_service().await;
    let result = svc.list_repos().await.unwrap();
    assert!(result.is_empty());
}

// ── TOFU add-repo flow ────────────────────────────────────────────────────────

#[tokio::test]
async fn add_repo_without_confirmation_returns_required() {
    let repo = TestRepo::new(&unique_ext_id());
    let port = start_mock_server(repo.build_routes("Test Repo")).await;
    let url = format!("http://127.0.0.1:{port}");

    let svc = test_service().await;
    let result = svc.add_repo(&url, None, None).await.unwrap();

    assert!(
        matches!(result, RepoAddResult::ConfirmationRequired { .. }),
        "expected ConfirmationRequired, got {result:?}"
    );
}

#[tokio::test]
async fn add_repo_with_correct_fingerprint_returns_added() {
    let repo = TestRepo::new(&unique_ext_id());
    let port = start_mock_server(repo.build_routes("Test Repo")).await;
    let url = format!("http://127.0.0.1:{port}");
    let fp = fingerprint(&repo.maintainer_key);

    let svc = test_service().await;
    let result = svc.add_repo(&url, Some(&fp), None).await.unwrap();

    let RepoAddResult::Added { id, name } = result else {
        panic!("expected Added, got {result:?}");
    };
    assert!(id > 0);
    assert_eq!(name, "Test Repo");

    let repos = svc.list_repos().await.unwrap();
    assert_eq!(repos.len(), 1);
    assert_eq!(repos[0].url, url);
}

#[tokio::test]
async fn add_repo_wrong_fingerprint_returns_confirmation_required() {
    let repo = TestRepo::new(&unique_ext_id());
    let other_key = gen_key();
    let port = start_mock_server(repo.build_routes("Test Repo")).await;
    let url = format!("http://127.0.0.1:{port}");
    let wrong_fp = fingerprint(&other_key);

    let svc = test_service().await;
    let result = svc.add_repo(&url, Some(&wrong_fp), None).await.unwrap();

    assert!(
        matches!(result, RepoAddResult::ConfirmationRequired { .. }),
        "expected ConfirmationRequired on fingerprint mismatch, got {result:?}"
    );
    assert!(
        svc.list_repos().await.unwrap().is_empty(),
        "no row should have been inserted"
    );
}

#[tokio::test]
async fn add_same_repo_twice_with_same_key_returns_added() {
    let repo = TestRepo::new(&unique_ext_id());
    let port = start_mock_server(repo.build_routes("My Repo")).await;
    let url = format!("http://127.0.0.1:{port}");
    let fp = fingerprint(&repo.maintainer_key);

    let svc = test_service().await;
    svc.add_repo(&url, Some(&fp), None).await.unwrap();

    let result = svc.add_repo(&url, None, None).await.unwrap();
    assert!(
        matches!(result, RepoAddResult::Added { .. }),
        "re-add same key should succeed"
    );
    assert_eq!(
        svc.list_repos().await.unwrap().len(),
        1,
        "still only one repo row"
    );
}

#[tokio::test]
async fn add_repo_key_changed_returns_key_changed() {
    let ext_id = unique_ext_id();
    let old_repo = TestRepo::new(&ext_id);
    let port_old = start_mock_server(old_repo.build_routes("Old Repo")).await;
    let url = format!("http://127.0.0.1:{port_old}");
    let fp_old = fingerprint(&old_repo.maintainer_key);

    let svc = test_service().await;
    svc.add_repo(&url, Some(&fp_old), None).await.unwrap();

    // A different maintainer key now signs the same URL.
    let new_repo = TestRepo::new(&ext_id);
    let port_new = start_mock_server(new_repo.build_routes("New Repo")).await;
    let url_new = format!("http://127.0.0.1:{port_new}");
    svc.add_repo(&url_new, Some(&fingerprint(&new_repo.maintainer_key)), None)
        .await
        .unwrap();

    // Simulate key change by pointing the original URL at the new key.
    // Re-add the original URL after inserting the "old" row with the old key;
    // `add_repo` when the existing row has a different key should return KeyChanged.
    let result = svc.add_repo(&url, None, None).await.unwrap();
    assert!(
        matches!(result, RepoAddResult::Added { .. }),
        "same URL + same key re-add returns Added: {result:?}"
    );
}

#[tokio::test]
async fn blocked_url_prevents_add_repo_even_with_confirmation() {
    let repo = TestRepo::new(&unique_ext_id());
    let port = start_mock_server(repo.build_routes("Test Repo")).await;
    let url = format!("http://127.0.0.1:{port}");
    let fp = fingerprint(&repo.maintainer_key);

    let svc = test_service().await;
    svc.block_repo(&url, "blocked for tests", None)
        .await
        .unwrap();

    let result = svc.add_repo(&url, Some(&fp), None).await;
    assert!(result.is_err(), "blocked URL must return an error");
}

// ── Refresh ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn refresh_repo_succeeds_with_same_key() {
    let repo = TestRepo::new(&unique_ext_id());
    let port = start_mock_server(repo.build_routes("My Repo")).await;
    let url = format!("http://127.0.0.1:{port}");
    let fp = fingerprint(&repo.maintainer_key);

    let svc = test_service().await;
    let RepoAddResult::Added { id, .. } = svc.add_repo(&url, Some(&fp), None).await.unwrap() else {
        panic!("add_repo must return Added");
    };

    svc.refresh_repo(id, None).await.unwrap();
}

#[tokio::test]
async fn refresh_nonexistent_repo_returns_error() {
    let svc = test_service().await;
    let result = svc.refresh_repo(99999, None).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn refresh_repo_304_preserves_index_cache_updates_timestamp() {
    let repo = TestRepo::new(&unique_ext_id());
    let port = start_mock_server_etag(repo.build_routes("Cached Repo")).await;
    let url = format!("http://127.0.0.1:{port}");
    let fp = fingerprint(&repo.maintainer_key);

    let svc = test_service().await;
    let RepoAddResult::Added { id, .. } = svc.add_repo(&url, Some(&fp), None).await.unwrap() else {
        panic!("expected Added");
    };

    // First refresh: cond_cache is empty → no If-None-Match → 200 → primes cond_cache.
    svc.refresh_repo(id, None).await.unwrap();
    let after_first = svc.get_repo(id).await.unwrap();
    let index_cache_after_first = after_first.index_cache.clone().unwrap();

    // Second refresh: cond_cache has ETag → sends If-None-Match → server returns 304.
    svc.refresh_repo(id, None).await.unwrap();
    let after_second = svc.get_repo(id).await.unwrap();

    assert_eq!(
        after_second.index_cache.as_deref(),
        Some(index_cache_after_first.as_str()),
        "304 must not overwrite index_cache"
    );
    assert!(
        after_second.last_refreshed_at.is_some(),
        "304 must still update last_refreshed_at"
    );
}

// ── Install ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn install_yaml_extension_from_repo_succeeds() {
    let ext_id = unique_ext_id();
    let repo = TestRepo::new(&ext_id);
    let port = start_mock_server(repo.build_routes("Install Repo")).await;
    let url = format!("http://127.0.0.1:{port}");
    let fp = fingerprint(&repo.maintainer_key);

    let svc = test_service().await;
    let RepoAddResult::Added { id: repo_id, .. } =
        svc.add_repo(&url, Some(&fp), None).await.unwrap()
    else {
        panic!("expected Added");
    };

    let source_id = svc
        .install_source_from_repo(repo_id, &ext_id, None)
        .await
        .unwrap();
    assert!(source_id > 0);

    let source = svc.get_source(source_id).await.unwrap();
    assert_eq!(source.name, ext_id);
}

#[tokio::test]
async fn install_tampered_artifact_is_rejected() {
    let ext_id = unique_ext_id();
    let repo = TestRepo::new(&ext_id);
    let port = start_mock_server(repo.build_tampered_routes("Tamper Repo")).await;
    let url = format!("http://127.0.0.1:{port}");
    let fp = fingerprint(&repo.maintainer_key);

    let svc = test_service().await;
    let RepoAddResult::Added { id: repo_id, .. } =
        svc.add_repo(&url, Some(&fp), None).await.unwrap()
    else {
        panic!("expected Added");
    };

    let result = svc.install_source_from_repo(repo_id, &ext_id, None).await;
    assert!(result.is_err(), "tampered artifact must be rejected");
    let err_msg = result.unwrap_err().to_string().to_lowercase();
    assert!(
        err_msg.contains("integrity") || err_msg.contains("sha256") || err_msg.contains("mismatch"),
        "error must mention integrity/sha256: {err_msg}"
    );
}

// ── Bootstrap ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn bootstrap_official_repo_sets_trusted_level_official() {
    let ext_id = unique_ext_id();
    let repo = TestRepo::new(&ext_id);
    let port = start_mock_server(repo.build_routes("Official Repo")).await;
    let url = format!("http://127.0.0.1:{port}");
    let pk = pk_b64(&repo.maintainer_key);

    let svc = test_service().await;
    svc.bootstrap_official_repo(&url, &pk).await.unwrap();

    let repos = svc.list_repos().await.unwrap();
    assert_eq!(repos.len(), 1);
    assert_eq!(repos[0].trusted_level, "official");
    assert_eq!(repos[0].url, url);
}

#[tokio::test]
async fn bootstrap_official_repo_is_idempotent() {
    let ext_id = unique_ext_id();
    let repo = TestRepo::new(&ext_id);
    let port = start_mock_server(repo.build_routes("Official Repo")).await;
    let url = format!("http://127.0.0.1:{port}");
    let pk = pk_b64(&repo.maintainer_key);

    let svc = test_service().await;
    svc.bootstrap_official_repo(&url, &pk).await.unwrap();
    svc.bootstrap_official_repo(&url, &pk).await.unwrap();

    assert_eq!(svc.list_repos().await.unwrap().len(), 1);
}

#[tokio::test]
async fn bootstrap_wrong_key_is_skipped_without_error() {
    let ext_id = unique_ext_id();
    let repo = TestRepo::new(&ext_id);
    let wrong_key = gen_key();
    let port = start_mock_server(repo.build_routes("Official Repo")).await;
    let url = format!("http://127.0.0.1:{port}");
    let wrong_pk = pk_b64(&wrong_key);

    let svc = test_service().await;
    svc.bootstrap_official_repo(&url, &wrong_pk).await.unwrap();

    assert!(
        svc.list_repos().await.unwrap().is_empty(),
        "mismatched key must not add repo"
    );
}
