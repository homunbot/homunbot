//! Cross-platform sandbox E2E tests.
//!
//! These tests validate basic sandbox behavior on any platform
//! without depending on internal crate types.
//!
//! Run with: `cargo test --test sandbox_e2e -- --nocapture`

use std::process::Command;

fn docker_available() -> bool {
    Command::new("docker")
        .arg("info")
        .arg("--format")
        .arg("{{.ServerVersion}}")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(target_os = "linux")]
fn bwrap_available() -> bool {
    Command::new("bwrap")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// ─────────────────────────────────────────────────────────────────────────────
// Test: shell execution via native path
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_native_echo() {
    if cfg!(target_os = "windows") {
        let output = Command::new("cmd")
            .args(["/C", "echo", "e2e-native-ok"])
            .output()
            .expect("cmd echo");
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("e2e-native-ok"));
    } else {
        let output = Command::new("echo")
            .arg("e2e-native-ok")
            .output()
            .expect("echo");
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("e2e-native-ok"));
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Test: platform-appropriate backend exists
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_platform_backend_detection() {
    if cfg!(target_os = "linux") {
        #[cfg(target_os = "linux")]
        {
            if bwrap_available() {
                eprintln!("Linux: bwrap available — linux_native backend expected");
            } else {
                eprintln!("Linux: bwrap NOT available — will fall back to docker or none");
            }
        }
    } else if cfg!(target_os = "windows") {
        eprintln!("Windows: windows_native backend not yet implemented — will fall back to docker or none");
    } else {
        eprintln!("macOS/other: no native backend — will fall back to docker or none");
    }

    if docker_available() {
        eprintln!("Docker: available");
    } else {
        eprintln!("Docker: NOT available");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Test: docker sandbox echo (if docker available)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_docker_sandbox_echo() {
    if !docker_available() {
        eprintln!("SKIP: Docker not available");
        return;
    }

    let output = Command::new("docker")
        .args([
            "run", "--rm",
            "--network", "none",
            "node:22-alpine",
            "echo", "docker-sandbox-ok",
        ])
        .output()
        .expect("docker run echo");

    assert!(
        output.status.success(),
        "docker sandbox echo should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("docker-sandbox-ok"),
        "should see output, got: {stdout}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Test: bwrap sandbox echo (Linux only)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
#[test]
fn test_bwrap_sandbox_echo() {
    if !bwrap_available() {
        eprintln!("SKIP: bwrap not available");
        return;
    }

    let output = Command::new("bwrap")
        .args([
            "--die-with-parent",
            "--ro-bind", "/", "/",
            "--proc", "/proc",
            "--dev", "/dev",
            "--clearenv",
            "--setenv", "PATH", "/usr/local/bin:/usr/bin:/bin",
            "echo", "bwrap-sandbox-ok",
        ])
        .output()
        .expect("bwrap echo");

    assert!(output.status.success(), "bwrap sandbox echo should succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("bwrap-sandbox-ok"),
        "should see output, got: {stdout}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Test: env isolation via bwrap --clearenv (Linux only)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
#[test]
fn test_bwrap_env_isolation() {
    if !bwrap_available() {
        eprintln!("SKIP: bwrap not available");
        return;
    }

    let output = Command::new("bwrap")
        .args([
            "--die-with-parent",
            "--ro-bind", "/", "/",
            "--proc", "/proc",
            "--dev", "/dev",
            "--clearenv",
            "--setenv", "PATH", "/usr/local/bin:/usr/bin:/bin",
            "--setenv", "ALLOWED_VAR", "visible",
            "env",
        ])
        .env("SENSITIVE_TOKEN", "should-not-leak")
        .output()
        .expect("bwrap env isolation");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("SENSITIVE_TOKEN"),
        "SENSITIVE_TOKEN should not leak through --clearenv"
    );
    assert!(
        stdout.contains("ALLOWED_VAR=visible"),
        "explicitly set vars should be present"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Test: docker env isolation
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_docker_env_isolation() {
    if !docker_available() {
        eprintln!("SKIP: Docker not available");
        return;
    }

    let output = Command::new("docker")
        .args([
            "run", "--rm",
            "-e", "ALLOWED_VAR=visible",
            "node:22-alpine",
            "env",
        ])
        .env("HOST_SECRET", "should-not-leak")
        .output()
        .expect("docker env isolation");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("HOST_SECRET"),
        "host env vars should not leak into docker container"
    );
    assert!(
        stdout.contains("ALLOWED_VAR=visible"),
        "explicitly passed vars should be present"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Test: macOS fallback (no native backend)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
#[test]
fn test_macos_no_native_backend() {
    // On macOS, neither linux_native nor windows_native is available
    // Only Docker (if installed) should work
    eprintln!("macOS: native sandbox backends are not available");
    eprintln!("Docker available: {}", docker_available());
    // This test documents the expected behavior on macOS
}
