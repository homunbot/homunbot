//! Runtime image build validation tests.
//!
//! These tests require Docker to be available. They validate that
//! `docker/sandbox-runtime/Dockerfile` produces a working image.
//!
//! Run with: `cargo test --test sandbox_runtime_image -- --nocapture`

use std::process::Command;

fn docker_available() -> bool {
    let has_docker = Command::new("docker")
        .arg("info")
        .arg("--format")
        .arg("{{.ServerVersion}}")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !has_docker {
        return false;
    }
    // Verify Linux containers work (Windows Docker can't run alpine images)
    Command::new("docker")
        .args(["run", "--rm", "alpine:3.20", "true"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn image_exists(image: &str) -> bool {
    Command::new("docker")
        .args(["image", "inspect", image])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

const BASELINE_IMAGE: &str = "homun/runtime-core:2026.03";

// ─────────────────────────────────────────────────────────────────────────────
// Test: build canonical baseline image
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_build_canonical_baseline() {
    if !docker_available() {
        eprintln!("SKIP: Docker not available");
        return;
    }

    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let script_path = format!("{manifest_dir}/scripts/build_sandbox_runtime_image.sh");

    let output = Command::new("bash")
        .arg(&script_path)
        .arg(BASELINE_IMAGE)
        .current_dir(manifest_dir)
        .output()
        .expect("build script should run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "build script should exit 0.\nstdout: {stdout}\nstderr: {stderr}"
    );

    assert!(
        image_exists(BASELINE_IMAGE),
        "image {BASELINE_IMAGE} should be present after build"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Test: runtime has Node.js
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_runtime_has_node() {
    if !docker_available() || !image_exists(BASELINE_IMAGE) {
        eprintln!("SKIP: Docker or image not available (run test_build_canonical_baseline first)");
        return;
    }

    let output = Command::new("docker")
        .args(["run", "--rm", BASELINE_IMAGE, "node", "--version"])
        .output()
        .expect("docker run node --version");

    assert!(output.status.success(), "node should be available");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.starts_with('v'),
        "node version should start with 'v', got: {stdout}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Test: runtime has Python3
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_runtime_has_python() {
    if !docker_available() || !image_exists(BASELINE_IMAGE) {
        eprintln!("SKIP: Docker or image not available");
        return;
    }

    let output = Command::new("docker")
        .args(["run", "--rm", BASELINE_IMAGE, "python3", "--version"])
        .output()
        .expect("docker run python3 --version");

    assert!(output.status.success(), "python3 should be available");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Python"),
        "should report Python version, got: {stdout}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Test: runtime has bash
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_runtime_has_bash() {
    if !docker_available() || !image_exists(BASELINE_IMAGE) {
        eprintln!("SKIP: Docker or image not available");
        return;
    }

    let output = Command::new("docker")
        .args(["run", "--rm", BASELINE_IMAGE, "bash", "-c", "echo ok"])
        .output()
        .expect("docker run bash -c 'echo ok'");

    assert!(output.status.success(), "bash should be available");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "ok", "bash should echo 'ok', got: {stdout}");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test: runtime has tsx
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_runtime_has_tsx() {
    if !docker_available() || !image_exists(BASELINE_IMAGE) {
        eprintln!("SKIP: Docker or image not available");
        return;
    }

    let output = Command::new("docker")
        .args(["run", "--rm", BASELINE_IMAGE, "npx", "tsx", "--version"])
        .output()
        .expect("docker run npx tsx --version");

    assert!(
        output.status.success(),
        "tsx should be available via npx. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Test: docker sandbox execution with baseline image
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_docker_sandbox_execution_in_baseline() {
    if !docker_available() || !image_exists(BASELINE_IMAGE) {
        eprintln!("SKIP: Docker or image not available");
        return;
    }

    let output = Command::new("docker")
        .args([
            "run",
            "--rm",
            "--network",
            "none",
            BASELINE_IMAGE,
            "bash",
            "-c",
            "echo sandbox-baseline-ok",
        ])
        .output()
        .expect("docker run sandbox test");

    assert!(
        output.status.success(),
        "docker sandbox execution should succeed"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("sandbox-baseline-ok"),
        "should see 'sandbox-baseline-ok', got: {stdout}"
    );
}
