use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::fd::FromRawFd;
use std::process::{Command, Stdio};
use std::sync::{Arc, Barrier};
use std::time::Instant;

use sabiql_app::model::app_state::AppState;
use sabiql_app::ports::outbound::renderer::Renderer;
use sabiql_app::services::AppServices;
use sabiql_infra::adapters::TomlSettingsStore;
use sabiql_ui::adapters::TuiAdapter;
use sabiql_ui::tui::TuiRunner;

#[test]
fn osc52_and_real_renderer_share_stdout_exclusion() {
    if std::env::var_os("SABIQL_OSC52_PTY_CHILD").is_some() {
        exercise_output();
        return;
    }
    let mut master = -1;
    let mut slave = -1;
    let mut size = libc::winsize {
        ws_row: 30,
        ws_col: 120,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // openpty initializes both descriptors on success; each is adopted exactly once below.
    assert_eq!(
        unsafe {
            libc::openpty(
                &raw mut master,
                &raw mut slave,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &raw mut size,
            )
        },
        0
    );
    let (mut master, slave) = unsafe { (File::from_raw_fd(master), File::from_raw_fd(slave)) };
    let child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "tests::clipboard::osc52_and_real_renderer_share_stdout_exclusion",
            "--nocapture",
        ])
        .env("SABIQL_OSC52_PTY_CHILD", "1")
        .stdout(Stdio::from(slave.try_clone().unwrap()))
        .stdin(Stdio::from(slave))
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut buffer = [0; 8192];
        loop {
            match master.read(&mut buffer) {
                Ok(0) => break,
                Ok(length) => bytes.extend_from_slice(&buffer[..length]),
                Err(error) if error.raw_os_error() == Some(libc::EIO) => break,
                Err(error) => panic!("PTY read failed: {error}"),
            }
        }
        bytes
    });

    let result = child.wait_with_output().unwrap();
    let output = reader.join().unwrap();

    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let sequence = b"\x1b]52;c;5pel5pys6Kqe8J+Zgg==\x07";
    assert_eq!(
        output
            .windows(sequence.len())
            .filter(|bytes| *bytes == sequence)
            .count(),
        100
    );
    let text = String::from_utf8(output).unwrap();
    let frame = text
        .split("FRAME_BEGIN")
        .nth(1)
        .unwrap()
        .split("FRAME_END")
        .next()
        .unwrap();
    assert!(!frame.contains("\x1b]52;"));
    assert!(frame.contains("\x1b["));
}

fn exercise_output() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("connections.toml"),
        "version = 3\nclipboard_backend = \"osc52\"\nconnections = []\n",
    )
    .unwrap();
    let clipboard = TomlSettingsStore::with_config_dir(dir.path().to_path_buf())
        .load_clipboard()
        .unwrap();
    let mut tui = TuiRunner::new().unwrap();
    let services = AppServices::stub();
    let mut state = AppState::new("osc52-pty".into());
    let barrier = Arc::new(Barrier::new(2));
    let stdout = std::io::stdout();
    let mut output_lock = stdout.lock();
    let worker_barrier = Arc::clone(&barrier);
    let worker = std::thread::spawn(move || {
        worker_barrier.wait();
        for _ in 0..100 {
            assert!(clipboard.copy_text("日本語🙂").is_ok());
            std::thread::yield_now();
        }
    });
    barrier.wait();
    output_lock.write_all(b"FRAME_BEGIN").unwrap();
    TuiAdapter::new(&mut tui)
        .draw(&state, &services, Instant::now())
        .unwrap();
    output_lock.write_all(b"FRAME_END").unwrap();
    output_lock.flush().unwrap();
    drop(output_lock);
    for index in 0..100 {
        state
            .messages
            .set_success_at(format!("frame {index}"), Instant::now());
        TuiAdapter::new(&mut tui)
            .draw(&state, &services, Instant::now())
            .unwrap();
        std::thread::yield_now();
    }
    worker.join().unwrap();
}
