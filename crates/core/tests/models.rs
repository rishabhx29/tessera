//! Ticket 03 acceptance: pinned model delivery.
//!
//! Everything here runs against a real HTTP server on localhost, so the
//! production fetch path is what is under test — not a fake standing in for
//! it. The manifest is pointed at that server, which is what `--models-dir`
//! plus a test manifest exist for.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use tessera_core::models::{set_version, Manifest, Models};
use tessera_core::store::{Arm, Store};

/// A one-file HTTP server. Counts the requests it serves, so "a warm run
/// performs zero network access" is measured rather than assumed.
struct TestServer {
    base: String,
    requests: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl TestServer {
    /// Serves `body` at every path. `truncate_to` cuts the response short
    /// after that many bytes, standing in for a connection that dies
    /// mid-transfer.
    fn serving(body: Vec<u8>, truncate_to: Option<usize>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let base = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let counter = Arc::clone(&requests);
        let flag = Arc::clone(&stop);
        listener.set_nonblocking(true).expect("nonblocking");
        let handle = std::thread::spawn(move || {
            while !flag.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        // An accepted socket can inherit the listener's
                        // non-blocking mode, and a read that returns
                        // WouldBlock before the request has arrived looks
                        // exactly like a client that hung up.
                        let _ = stream.set_nonblocking(false);
                        counter.fetch_add(1, Ordering::SeqCst);
                        let mut buf = [0u8; 1024];
                        let _ = stream.read(&mut buf);
                        let head = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = stream.write_all(head.as_bytes());
                        let send = truncate_to.unwrap_or(body.len()).min(body.len());
                        let _ = stream.write_all(&body[..send]);
                        let _ = stream.flush();
                    }
                    // Nothing waiting yet; sleep rather than spin.
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(2));
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            base,
            requests,
            stop,
            handle: Some(handle),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    fn request_count(&self) -> usize {
        self.requests.load(Ordering::SeqCst)
    }

    /// Stop serving, so any further request fails loudly instead of hanging.
    fn shut_down(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.shut_down();
    }
}

fn sha256_of(bytes: &[u8]) -> String {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(bytes);
    bytes_to_hex(hasher.finalize().as_slice())
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    tessera_core::models::fetch::hex(bytes)
}

fn pinned_sha(body: &[u8]) -> String {
    sha256_of(body)
}

/// A manifest with one set, served by `server`, pinned to `body`.
fn manifest_for(server: &TestServer, body: &[u8], arm: &str) -> Manifest {
    Manifest::parse(&format!(
        r#"{{"sets":[{{"name":"test-set","arm":"{arm}","files":[
             {{"name":"model.int8.onnx","url":"{}","sha256":"{}","bytes":{}}}]}}]}}"#,
        server.url("/model"),
        sha256_of(body),
        body.len()
    ))
    .expect("the test manifest is valid")
}

fn models_in(dir: &Path, manifest: Manifest) -> Models {
    Models::with_manifest(dir, manifest)
}

fn downloaded_files(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect();
    names.sort();
    names
}

#[test]
fn first_use_downloads_and_a_warm_run_touches_no_network() {
    let body = b"a model, such as it is".to_vec();
    let mut server = TestServer::serving(body.clone(), None);
    let dir = tempfile::tempdir().unwrap();
    let models = models_in(dir.path(), manifest_for(&server, &body, "image"));

    let cold = models.ensure(&["test-set"]).unwrap();
    assert_eq!(cold.downloaded, vec!["model.int8.onnx"]);
    assert!(cold.touched_network());
    assert_eq!(cold.bytes, body.len() as u64);
    assert_eq!(
        std::fs::read(dir.path().join("model.int8.onnx")).unwrap(),
        body
    );

    // The warm run is the whole point of the AC: the server is shut down, so a
    // single request would fail the test rather than pass quietly.
    server.shut_down();
    let warm = models.ensure(&["test-set"]).unwrap();
    assert_eq!(warm.downloaded, Vec::<String>::new());
    assert_eq!(warm.already_present, vec!["model.int8.onnx"]);
    assert!(!warm.touched_network());
    assert_eq!(server.request_count(), 1, "the warm run hit the network");
}

#[test]
fn a_tampered_file_is_not_usable_even_before_anything_replaces_it() {
    let body = b"the pinned bytes".to_vec();
    let server = TestServer::serving(body.clone(), None);
    let dir = tempfile::tempdir().unwrap();
    let models = models_in(dir.path(), manifest_for(&server, &body, "image"));
    models.ensure(&["test-set"]).unwrap();

    // Same length, different content: the size check cannot save us here, only
    // the checksum can — which is why the checksum is the thing that is checked.
    let installed = dir.path().join("model.int8.onnx");
    let tampered = b"the pinned BYTES".to_vec();
    std::fs::write(&installed, &tampered).unwrap();
    assert_eq!(installed.metadata().unwrap().len(), body.len() as u64);
    let pinned = &models.manifest().sets()[0].files[0];
    assert!(
        !models.is_verified(pinned),
        "a tampered model passed as verified: the engine would have run it"
    );
}

/// A file that was tampered with on disk is replaced, loudly: a pin moving is a
/// routine event during development, and making every user hand-delete files to
/// get past one is a bug report waiting to happen. What must never happen is the
/// engine *using* bytes it cannot verify.
#[test]
fn a_tampered_file_is_replaced_rather_than_blocking_the_install() {
    let body = b"the pinned bytes".to_vec();
    let server = TestServer::serving(body.clone(), None);
    let dir = tempfile::tempdir().unwrap();
    let models = models_in(dir.path(), manifest_for(&server, &body, "image"));
    models.ensure(&["test-set"]).unwrap();

    let installed = dir.path().join("model.int8.onnx");
    std::fs::write(&installed, b"something else!").unwrap();

    let report = models.ensure(&["test-set"]).unwrap();
    assert_eq!(report.replaced, vec!["model.int8.onnx"]);
    assert_eq!(report.downloaded, vec!["model.int8.onnx"]);
    assert!(
        report
            .lines()
            .iter()
            .any(|line| line.contains("did not match its pin")),
        "replacing a model should be reported: {:?}",
        report.lines()
    );
    assert_eq!(std::fs::read(&installed).unwrap(), body);
    assert!(models.is_verified(&models.manifest().sets()[0].files[0]));
}

/// When the host keeps serving bytes that do not match the pin, that is the
/// failure a user must see: named file, expected checksum, what arrived.
#[test]
fn bytes_that_do_not_match_the_pin_fail_loudly_every_time() {
    let served = b"whatever the server felt like".to_vec();
    let pinned = b"what we expected".to_vec();
    let server = TestServer::serving(served, None);
    let dir = tempfile::tempdir().unwrap();
    let models = models_in(dir.path(), manifest_for(&server, &pinned, "text"));

    for attempt in 1..=2 {
        let err = models.ensure(&["test-set"]).unwrap_err();
        let message = format!("{err:#}");
        assert!(
            message.contains("failed verification"),
            "attempt {attempt} was not loud about unpinned bytes: {message}"
        );
        assert!(
            message.contains(&pinned_sha(&pinned)) || message.contains("expected"),
            "the message does not say what was expected: {message}"
        );
        assert!(
            !dir.path().join("model.int8.onnx").exists(),
            "unverified bytes were left under the real name"
        );
        assert_eq!(downloaded_files(dir.path()), Vec::<String>::new());
    }
}

#[test]
fn bytes_that_do_not_match_the_pin_never_reach_the_models_dir() {
    // The server serves something other than what the manifest pins — a
    // tampered mirror, a CDN serving the wrong build, a hostile network.
    let served = b"whatever the server felt like".to_vec();
    let pinned = b"what we expected".to_vec();
    let server = TestServer::serving(served, None);
    let dir = tempfile::tempdir().unwrap();
    let models = models_in(dir.path(), manifest_for(&server, &pinned, "text"));

    let err = models.ensure(&["test-set"]).unwrap_err();
    let message = format!("{err:#}");
    assert!(
        message.contains("failed verification"),
        "an unpinned download was not reported: {message}"
    );
    assert!(
        !dir.path().join("model.int8.onnx").exists(),
        "unverified bytes were left under the real name"
    );
    // and nothing partial is left to be mistaken for a model later
    assert_eq!(downloaded_files(dir.path()), Vec::<String>::new());
}

#[test]
fn an_interrupted_download_leaves_nothing_that_looks_real() {
    let body = vec![b'x'; 4096];
    let mut server = TestServer::serving(body.clone(), Some(100));
    let dir = tempfile::tempdir().unwrap();
    let models = models_in(dir.path(), manifest_for(&server, &body, "image"));

    let err = models.ensure(&["test-set"]).unwrap_err();
    assert!(format!("{err:#}").contains("model.int8.onnx"));

    // Half a model must never appear under its real name, and must not be left
    // behind for the next run to find.
    assert_eq!(
        downloaded_files(dir.path()),
        Vec::<String>::new(),
        "an interrupted download left something behind"
    );

    // And the next run recovers rather than tripping over the wreckage.
    server.shut_down();
    let mut whole = TestServer::serving(body.clone(), None);
    let models = models_in(dir.path(), manifest_for(&whole, &body, "image"));
    assert_eq!(models.ensure(&["test-set"]).unwrap().downloaded.len(), 1);
    whole.shut_down();
}

#[test]
fn a_native_engine_needs_no_download_at_all() {
    let body = b"never fetched".to_vec();
    let mut server = TestServer::serving(body.clone(), None);
    let dir = tempfile::tempdir().unwrap();
    let models = models_in(dir.path(), manifest_for(&server, &body, "image"));

    // A consumer that ships with the operating system asks for nothing, so it
    // performs no network I/O and writes nothing.
    let report = models.ensure(&[]).unwrap();
    assert!(!report.touched_network());
    assert!(report.downloaded.is_empty() && report.already_present.is_empty());
    assert_eq!(server.request_count(), 0);
    assert!(!dir.path().exists() || downloaded_files(dir.path()).is_empty());
    server.shut_down();
}

#[test]
fn a_mirror_is_tried_without_weakening_the_pin() {
    let body = b"the same bytes from a second place".to_vec();
    // Primary: nothing listening. Mirror: a working server.
    let mirror = TestServer::serving(body.clone(), None);
    let manifest = Manifest::parse(&format!(
        r#"{{"sets":[{{"name":"test-set","arm":null,"files":[
             {{"name":"m.onnx","url":"http://127.0.0.1:1/primary",
               "mirrors":["{}"],"sha256":"{}","bytes":{}}}]}}]}}"#,
        mirror.url("/m"),
        sha256_of(&body),
        body.len()
    ))
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let models = models_in(dir.path(), manifest);
    assert_eq!(models.ensure(&["test-set"]).unwrap().downloaded.len(), 1);
    assert_eq!(std::fs::read(dir.path().join("m.onnx")).unwrap(), body);
}

#[test]
fn status_reports_what_is_present_and_what_is_not() {
    let body = b"one model".to_vec();
    let server = TestServer::serving(body.clone(), None);
    let dir = tempfile::tempdir().unwrap();
    let models = models_in(dir.path(), manifest_for(&server, &body, "image"));

    let status = models.status().unwrap();
    assert_eq!(status.len(), 1);
    assert!(!status[0].is_complete());
    assert_eq!(status[0].missing, vec!["model.int8.onnx"]);

    models.ensure(&["test-set"]).unwrap();
    let status = models.status().unwrap();
    assert!(status[0].is_complete());
    assert_eq!(status[0].installed, vec!["model.int8.onnx"]);
    assert_eq!(status[0].version, set_version(&models.manifest().sets()[0]));
}

#[test]
fn an_unknown_set_name_is_an_error_rather_than_a_silent_no_op() {
    let body = b"x".to_vec();
    let server = TestServer::serving(body.clone(), None);
    let dir = tempfile::tempdir().unwrap();
    let models = models_in(dir.path(), manifest_for(&server, &body, "image"));
    let err = models.ensure(&["typo"]).unwrap_err();
    assert!(
        format!("{err:#}").contains("typo"),
        "a typo should name itself: {err:#}"
    );
    assert_eq!(server.request_count(), 0);
}

#[test]
#[ignore = "hits the network: run with --ignored to prove the pins still match upstream"]
fn the_pinned_checksums_still_match_upstream() {
    // The smallest pinned file: SigLIP's preprocessor config, 368 bytes. Small
    // enough to check often, real enough to prove the whole path — HTTPS, the
    // live URL, the recorded checksum — still holds against upstream.
    let manifest = Manifest::pinned().unwrap();
    let file = manifest
        .set("siglip")
        .unwrap()
        .files
        .iter()
        .min_by_key(|f| f.bytes)
        .expect("the siglip set has files")
        .clone();
    assert_eq!(file.bytes, 368, "expected the preprocessor config");

    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join(&file.name);
    let digest = tessera_core::models::fetch::download_verified(
        &file.url,
        &file.name,
        &dest,
        &file.sha256,
        file.bytes,
    )
    .expect("the pinned checksum no longer matches what the host serves");
    assert_eq!(digest, file.sha256);
    assert_eq!(std::fs::read(&dest).unwrap().len() as u64, file.bytes);
}

#[test]
fn a_changed_pin_marks_only_its_own_arm_for_re_embedding() {
    let data = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(data.path()).unwrap();

    // Everything below goes through real manifests and the real version
    // derivation, so this is what the engine does, not a simulation of it.
    let base = models_in(dir.path(), manifest_with_pins("a", "b"));
    let image_version = set_version(base.manifest().set_for_arm(Arm::Image).unwrap());
    let text_version = set_version(base.manifest().set_for_arm(Arm::Text).unwrap());
    assert_ne!(image_version, text_version);

    // Nothing embedded yet: both arms are outstanding work, and neither holds
    // vectors "from a different model" — they hold none. OCR is never
    // outstanding work, because it has no vector space.
    let stale = base.stale_arms(&store).unwrap();
    assert_eq!(stale.len(), 2);
    assert!(stale.iter().all(|s| !s.is_from_a_different_model()));

    // Both arms embedded by the models now pinned.
    store.set_arm_version(Arm::Image, &image_version).unwrap();
    store.set_arm_version(Arm::Text, &text_version).unwrap();
    assert!(base.stale_arms(&store).unwrap().is_empty());

    // The SigLIP pin moves. The MiniLM pin has not, so the text arm's vectors
    // are still perfectly comparable with each other and must not be touched.
    let moved_image = models_in(dir.path(), manifest_with_pins("c", "b"));
    let stale = moved_image.stale_arms(&store).unwrap();
    assert_eq!(
        stale.len(),
        1,
        "only the arm whose pin moved is outstanding"
    );
    assert_eq!(stale[0].arm, Arm::Image);
    assert!(stale[0].is_from_a_different_model());
    assert_eq!(stale[0].built_by.as_deref(), Some(image_version.as_str()));
    assert_ne!(stale[0].pinned, image_version);

    // The same for the MiniLM pin, in the other direction.
    let moved_text = models_in(dir.path(), manifest_with_pins("a", "d"));
    let stale = moved_text.stale_arms(&store).unwrap();
    assert_eq!(stale.len(), 1);
    assert_eq!(stale[0].arm, Arm::Text);
    assert_eq!(stale[0].built_by.as_deref(), Some(text_version.as_str()));

    // Re-embedding settles it: the arm is recorded against the new pin and the
    // index goes back to being searchable. Each direction gets its own index —
    // one index cannot satisfy two different manifests at once, which is the
    // point of pinning per arm.
    let image_store = Store::open(tempfile::tempdir().unwrap().path()).unwrap();
    image_store
        .set_arm_version(Arm::Image, &image_version)
        .unwrap();
    image_store
        .set_arm_version(Arm::Text, &text_version)
        .unwrap();
    let moved = models_in(dir.path(), manifest_with_pins("c", "b"));
    assert_eq!(moved.stale_arms(&image_store).unwrap().len(), 1);
    image_store
        .set_arm_version(
            Arm::Image,
            &set_version(moved.manifest().set_for_arm(Arm::Image).unwrap()),
        )
        .unwrap();
    assert!(moved.stale_arms(&image_store).unwrap().is_empty());

    let text_store = Store::open(tempfile::tempdir().unwrap().path()).unwrap();
    text_store
        .set_arm_version(Arm::Image, &image_version)
        .unwrap();
    text_store
        .set_arm_version(Arm::Text, &text_version)
        .unwrap();
    let moved = models_in(dir.path(), manifest_with_pins("a", "d"));
    assert_eq!(moved.stale_arms(&text_store).unwrap().len(), 1);
    text_store
        .set_arm_version(
            Arm::Text,
            &set_version(moved.manifest().set_for_arm(Arm::Text).unwrap()),
        )
        .unwrap();
    assert!(moved.stale_arms(&text_store).unwrap().is_empty());
}

/// A two-arm manifest with the given pin characters, so a pin move can be
/// exercised through the real version-derivation path.
fn manifest_with_pins(image: &str, text: &str) -> Manifest {
    Manifest::parse(&format!(
        r#"{{"sets":[
            {{"name":"siglip","arm":"image","files":[
                {{"name":"vision.onnx","url":"https://e/v","sha256":"{}","bytes":10}}]}},
            {{"name":"minilm","arm":"text","files":[
                {{"name":"text.onnx","url":"https://e/t","sha256":"{}","bytes":20}}]}}]}}"#,
        image.repeat(64),
        text.repeat(64)
    ))
    .unwrap()
}

#[test]
fn forgetting_an_arm_makes_it_outstanding_work_again() {
    let data = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(data.path()).unwrap();
    let models = models_in(dir.path(), Manifest::pinned().unwrap());
    let image_version = set_version(models.manifest().set_for_arm(Arm::Image).unwrap());
    let text_version = set_version(models.manifest().set_for_arm(Arm::Text).unwrap());

    store.set_arm_version(Arm::Image, &image_version).unwrap();
    store.set_arm_version(Arm::Text, &text_version).unwrap();
    assert!(models.stale_arms(&store).unwrap().is_empty());

    store.clear_arm(Arm::Image).unwrap();
    let stale = models.stale_arms(&store).unwrap();
    assert_eq!(stale.len(), 1);
    assert_eq!(stale[0].arm, Arm::Image);
    assert_eq!(stale[0].built_by, None);
    // The text arm is untouched by anything that happened to the image arm.
    assert_eq!(
        store.arm_version(Arm::Text).unwrap().as_deref(),
        Some(text_version.as_str())
    );
}
#[test]
fn the_pinned_manifest_names_the_models_v0_1_actually_runs() {
    let dir = tempfile::tempdir().unwrap();
    // No network: this only reads the manifest compiled into the binary.
    let models = Models::open(dir.path()).unwrap();
    let names = models.set_names();
    for expected in ["siglip", "minilm", "ppocr"] {
        assert!(
            names.contains(&expected),
            "the manifest is missing {expected}"
        );
    }
    let status = models.status().unwrap();
    assert_eq!(status.len(), 3);
    assert!(
        status.iter().all(|set| !set.is_complete()),
        "an empty models dir cannot have anything installed"
    );
}
