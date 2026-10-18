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

    /// Serves `body`, pausing `delay` before each chunk. Long enough that a
    /// test can observe the file mid-transfer, which is the only way to see
    /// that a download writes to a temporary name rather than to its real one.
    fn serving_slowly(body: Vec<u8>, chunk: usize, delay: std::time::Duration) -> Self {
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
                        let _ = stream.set_nonblocking(false);
                        counter.fetch_add(1, Ordering::SeqCst);
                        let mut buf = [0u8; 1024];
                        let _ = stream.read(&mut buf);
                        let head = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = stream.write_all(head.as_bytes());
                        for piece in body.chunks(chunk) {
                            if flag.load(Ordering::SeqCst) {
                                break;
                            }
                            let _ = stream.write_all(piece);
                            let _ = stream.flush();
                            std::thread::sleep(delay);
                        }
                    }
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

/// The multi-file form of `verified_path`: one answer for several files of a
/// set, with the set-wide policy intact — a file nobody asked for that no
/// longer matches its pin still blocks every path the set could hand out.
#[test]
fn verified_paths_answers_for_several_files_under_one_set_policy() {
    let dir = tempfile::tempdir().unwrap();
    let (one, two) = (b"first file".to_vec(), b"second file".to_vec());
    let manifest = Manifest::parse(&format!(
        r#"{{"sets":[{{"name":"a-set","arm":"image","files":[
             {{"name":"one.bin","url":"https://e/one","sha256":"{}","bytes":{}}},
             {{"name":"two.bin","url":"https://e/two","sha256":"{}","bytes":{}}}]}}]}}"#,
        sha256_of(&one),
        one.len(),
        sha256_of(&two),
        two.len()
    ))
    .expect("the test manifest is valid");
    let models = models_in(dir.path(), manifest);
    std::fs::write(dir.path().join("one.bin"), &one).unwrap();
    std::fs::write(dir.path().join("two.bin"), &two).unwrap();

    let paths = models
        .verified_paths("a-set", &["one.bin", "two.bin"])
        .unwrap();
    assert_eq!(
        paths,
        vec![dir.path().join("one.bin"), dir.path().join("two.bin")],
        "paths come back in the order they were asked for"
    );

    // Tamper with the file nobody asked for: `two.bin` is not in the request,
    // and the answer must still refuse to hand out `one.bin`.
    std::fs::write(dir.path().join("two.bin"), b"tampered here").unwrap();
    let err = models.verified_paths("a-set", &["one.bin"]).unwrap_err();
    assert!(format!("{err:#}").contains("two.bin"), "{err:#}");

    // A name that is not in the set is reported before any verification is
    // attempted, exactly as the single-file form reports it.
    let err = models.verified_paths("a-set", &["three.bin"]).unwrap_err();
    assert!(format!("{err:#}").contains("no file named"), "{err:#}");
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

/// The atomicity claim, made observable.
///
/// Every other failure test here asserts the *destination* is clean afterwards,
/// which a regression that streamed straight into the real name and deleted on
/// mismatch would also satisfy. This one watches the file mid-transfer: while
/// bytes are still arriving the models directory holds a `.part` and NOT the
/// real name. That is the property — no reader can ever see a half-written
/// model under the name the engine loads from — and it is the only thing the
/// mechanism buys.
#[test]
fn a_download_is_invisible_under_its_real_name_until_it_is_verified() {
    let body = vec![b'x'; 512 * 1024];
    let server =
        TestServer::serving_slowly(body.clone(), 4096, std::time::Duration::from_millis(12));
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("model.int8.onnx");

    let worker = {
        let models_path = dir.path().to_path_buf();
        std::thread::spawn(move || {
            // A fresh handle in this thread so the download is genuinely in
            // flight while the main thread looks at the directory.
            let manifest = Manifest::parse(&format!(
                r#"{{"sets":[{{"name":"test-set","arm":"image","files":[
                     {{"name":"model.int8.onnx","url":"{}","sha256":"{}","bytes":{}}}]}}]}}"#,
                server.url("/model"),
                sha256_of(&body),
                body.len()
            ))
            .expect("valid manifest");
            let m = Models::with_manifest(&models_path, manifest);
            m.ensure(&["test-set"])
        })
    };

    // Poll until the temporary file shows up, which is proof the bytes are
    // landing somewhere before the rename.
    let mut saw_partial = false;
    for _ in 0..400 {
        if downloaded_files(dir.path())
            .iter()
            .any(|name| name.ends_with(".part"))
        {
            saw_partial = true;
            // The point of the test: mid-transfer, the real name does not exist.
            assert!(
                !dest.exists(),
                "a partial download was visible under its real name"
            );
            break;
        }
        if worker.is_finished() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        saw_partial,
        "the download never showed a temporary file, so atomicity is unproven"
    );

    worker.join().unwrap().expect("the download succeeds");
    assert!(dest.exists(), "the verified model was not installed");
    assert_eq!(
        downloaded_files(dir.path()),
        vec!["model.int8.onnx".to_string()],
        "the temporary file outlived the rename"
    );
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
    const EXPECTED_SETS: [&str; 4] = ["siglip", "minilm", "caption", "ppocr"];
    let dir = tempfile::tempdir().unwrap();
    // No network: this only reads the manifest compiled into the binary.
    let models = Models::open(dir.path()).unwrap();
    let names = models.set_names();
    for expected in EXPECTED_SETS {
        assert!(
            names.contains(&expected),
            "the manifest is missing {expected}"
        );
    }
    let status = models.status().unwrap();
    assert_eq!(status.len(), EXPECTED_SETS.len());
    assert!(
        status.iter().all(|set| !set.is_complete()),
        "an empty models dir cannot have anything installed"
    );
}

// ------------------------------------------------------------- pin provenance

/// Where a pinned file comes from, and how to ask that host what it holds.
///
/// A checksum is only worth its provenance. The pin is what stands between a
/// user and a file that arrived truncated or substituted, and it can only do
/// that job if the value was read from the host rather than computed from
/// whatever the first download produced. A pin taken from the local artefact
/// certifies the local artefact, corruption included — which is how a decoder
/// 268 KB short of upstream sat in this manifest looking verified, and how a
/// captioning model came to be pinned to a config file its own URL never
/// served.
enum Host<'a> {
    /// `https://huggingface.co/{repo}/resolve/{revision}/{path}`
    HuggingFace {
        repo: &'a str,
        revision: &'a str,
        path: &'a str,
    },
    /// `https://github.com/{repo}/releases/download/{tag}/{asset}`
    GitHub {
        repo: &'a str,
        tag: &'a str,
        asset: &'a str,
    },
}

/// Split a pinned URL into the host and the coordinates needed to ask about it.
/// `None` means an unrecognised host, which is itself a finding: a pin nobody
/// can check is a pin nobody has checked.
fn parse_pin(url: &str) -> Option<Host<'_>> {
    if let Some(rest) = url.strip_prefix("https://huggingface.co/") {
        let (repo, tail) = rest.split_once("/resolve/")?;
        let (revision, path) = tail.split_once('/')?;
        return Some(Host::HuggingFace {
            repo,
            revision,
            path,
        });
    }
    if let Some(rest) = url.strip_prefix("https://github.com/") {
        let (repo, tail) = rest.split_once("/releases/download/")?;
        let (tag, asset) = tail.split_once('/')?;
        return Some(Host::GitHub { repo, tag, asset });
    }
    None
}

/// A full git commit id. Short revisions are ambiguous, and a branch or tag
/// name is not a revision at all.
fn is_commit_sha(text: &str) -> bool {
    text.len() == 40 && text.chars().all(|c| c.is_ascii_hexdigit())
}

fn get_json(agent: &ureq::Agent, url: &str) -> Result<serde_json::Value, String> {
    let mut response = agent
        .get(url)
        .call()
        .map_err(|err| format!("GET {url}: {err}"))?;
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|err| format!("reading {url}: {err}"))?;
    serde_json::from_str(&body).map_err(|err| format!("parsing {url}: {err}"))
}

impl Host<'_> {
    /// What the host declares: the file's length, and its sha256 when the host
    /// publishes one. Sizes cover every pinned file; hashes cover the ones held
    /// in large-file storage, which is all of the expensive ones.
    fn declared(&self, agent: &ureq::Agent) -> Result<(Option<u64>, Option<String>), String> {
        match self {
            Host::HuggingFace { repo, path, .. } => {
                let api = format!("https://huggingface.co/api/models/{repo}?blobs=true");
                let body = get_json(agent, &api)?;
                let siblings = body["siblings"]
                    .as_array()
                    .ok_or_else(|| format!("{api} carries no file list"))?;
                let entry = siblings
                    .iter()
                    .find(|sibling| sibling["rfilename"].as_str() == Some(*path))
                    .ok_or_else(|| format!("{repo} has no file {path} at this revision"))?;
                Ok((
                    entry["size"].as_u64(),
                    entry["lfs"]["sha256"].as_str().map(str::to_owned),
                ))
            }
            Host::GitHub { repo, tag, asset } => {
                let api = format!("https://api.github.com/repos/{repo}/releases/tags/{tag}");
                let body = get_json(agent, &api)?;
                let assets = body["assets"]
                    .as_array()
                    .ok_or_else(|| format!("{api} carries no asset list"))?;
                let entry = assets
                    .iter()
                    .find(|candidate| candidate["name"].as_str() == Some(*asset))
                    .ok_or_else(|| format!("{repo}@{tag} has no asset {asset}"))?;
                // Release assets come with a size but no published hash.
                Ok((entry["size"].as_u64(), None))
            }
        }
    }
}

/// Every pin in the shipped manifest has to match what its host says, on the
/// one property every host publishes: the length. A pin can be wrong in two
/// ways and this catches both — never having matched the host, and having
/// stopped matching after upstream moved.
#[test]
#[ignore = "hits the network: audits every pin against what its host declares"]
fn every_pin_matches_the_size_its_host_declares() {
    let manifest = Manifest::pinned().expect("the built-in manifest parses");
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(60)))
        .build()
        .new_agent();

    let mut audited = 0usize;
    let mut problems: Vec<String> = Vec::new();
    for set in manifest.sets() {
        for file in &set.files {
            let Some(host) = parse_pin(&file.url) else {
                problems.push(format!(
                    "{}/{}: nothing knows how to check {}",
                    set.name, file.name, file.url
                ));
                continue;
            };
            match host.declared(&agent) {
                Err(why) => problems.push(format!("{}/{}: {why}", set.name, file.name)),
                Ok((size, sha256)) => {
                    audited += 1;
                    if size != Some(file.bytes) {
                        problems.push(format!(
                            "{}/{}: pinned at {} bytes, host declares {size:?}",
                            set.name, file.name, file.bytes
                        ));
                    }
                    // Only when the host publishes one: a size that agrees with
                    // a hash that was never checked is half a proof.
                    if let Some(sha256) = sha256 {
                        if sha256 != file.sha256 {
                            problems.push(format!(
                                "{}/{}: pinned sha256 {}, host declares {sha256}",
                                set.name, file.name, file.sha256
                            ));
                        }
                    }
                }
            }
        }
    }

    assert!(
        audited >= 10,
        "only {audited} pins were auditable, which cannot be the whole manifest"
    );
    assert!(
        problems.is_empty(),
        "pins that do not match their own source:\n  {}",
        problems.join("\n  ")
    );
}

/// A pin that follows a moving reference can go stale while nobody edits the
/// manifest: `resolve/main` is whatever `main` is today, so the first time
/// upstream commits to one of these files every fresh install starts failing
/// its checksum, and the manifest looks like the broken part. Naming a commit
/// makes a pin mean one thing forever.
#[test]
fn every_pinned_url_names_an_immutable_revision() {
    let manifest = Manifest::pinned().expect("the built-in manifest parses");
    let mut moving: Vec<String> = Vec::new();
    for set in manifest.sets() {
        for file in &set.files {
            let where_it_points = format!("{}/{}", set.name, file.name);
            match parse_pin(&file.url) {
                Some(Host::HuggingFace { revision, .. }) if !is_commit_sha(revision) => {
                    moving.push(format!("{where_it_points}: {revision} is not a commit"));
                }
                // A release tag is a fixed thing; `latest` is not a tag at all.
                Some(Host::GitHub { tag: "latest", .. }) => {
                    moving.push(format!("{where_it_points}: latest is not a release"));
                }
                Some(_) => {}
                None => moving.push(format!("{where_it_points}: unknown host in {}", file.url)),
            }
        }
    }
    assert!(
        moving.is_empty(),
        "these pins follow a moving reference:\n  {}",
        moving.join("\n  ")
    );
}
