//! Live frames for the inspector. One producer per device runs only while the
//! window keeps asking for frames: AXe's MJPEG stream when installed, otherwise
//! `simctl` screenshots in a loop. The window long-polls for the newest frame.
use std::{
    collections::HashMap,
    io::Read,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Condvar, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

/// A producer stops once nobody has asked for a frame this long.
const IDLE: Duration = Duration::from_secs(6);
const POLL_GAP: Duration = Duration::from_millis(120);
const MAX_FRAME: usize = 8 * 1024 * 1024;
/// Frame numbers keep rising across producers, so a window that polled the cover
/// stream still sees the first inner-display frame after a pose change.
static SEQ: AtomicU64 = AtomicU64::new(0);

#[derive(Default)]
struct Latest {
    seq: u64,
    jpeg: Arc<Vec<u8>>,
    error: Option<String>,
    finished: bool,
}
struct Producer {
    latest: Mutex<Latest>,
    ready: Condvar,
    pulled: Mutex<Instant>,
}
impl Producer {
    fn idle(&self) -> bool {
        self.pulled.lock().map(|at| at.elapsed() > IDLE).unwrap_or(true)
    }
    fn push(&self, jpeg: Vec<u8>) {
        if let Ok(mut latest) = self.latest.lock() {
            latest.seq = SEQ.fetch_add(1, Ordering::Relaxed) + 1;
            latest.jpeg = Arc::new(jpeg);
            latest.error = None;
        }
        self.ready.notify_all();
    }
    fn finish(&self, error: Option<String>) {
        if let Ok(mut latest) = self.latest.lock() {
            latest.finished = true;
            latest.error = error;
        }
        self.ready.notify_all();
    }
}

#[derive(Default)]
pub struct Streams(Mutex<HashMap<String, Arc<Producer>>>);

pub struct Frame {
    pub seq: u64,
    pub jpeg: Arc<Vec<u8>>,
}

/// Complete JPEGs in `buffer`, by start and end markers; the rest stays buffered.
pub fn take_jpegs(buffer: &mut Vec<u8>) -> Vec<Vec<u8>> {
    let mut frames = Vec::new();
    loop {
        let Some(start) = buffer.windows(3).position(|w| w == [0xFF, 0xD8, 0xFF]) else {
            // Keep a possible partial marker only.
            let keep = buffer.len().min(2);
            buffer.drain(..buffer.len() - keep);
            return frames;
        };
        let Some(end) = buffer[start + 2..].windows(2).position(|w| w == [0xFF, 0xD9]).map(|i| start + 2 + i + 2) else {
            buffer.drain(..start);
            if buffer.len() > MAX_FRAME {
                buffer.clear();
            }
            return frames;
        };
        frames.push(buffer[start..end].to_vec());
        buffer.drain(..end);
    }
}

fn run_axe(producer: &Arc<Producer>, udid: &str) -> bool {
    let Some(axe) = super::axe_path() else { return false };
    let child = Command::new(axe)
        .args(["stream-video", "--udid", udid, "--fps", "12", "--quality", "72", "--scale", "0.5"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut child) = child else { return false };
    let Some(mut stdout) = child.stdout.take() else {
        let _ = child.kill();
        return false;
    };
    let child: Arc<Mutex<Child>> = Arc::new(Mutex::new(child));
    // A stalled stream blocks the read below, so idleness is judged on its own thread.
    let watchdog = {
        let (child, producer) = (child.clone(), producer.clone());
        thread::spawn(move || {
            loop {
                thread::sleep(Duration::from_millis(500));
                let exited = child.lock().map(|mut c| c.try_wait().map(|s| s.is_some()).unwrap_or(true)).unwrap_or(true);
                if exited {
                    return;
                }
                if producer.idle() {
                    if let Ok(mut c) = child.lock() {
                        let _ = c.kill();
                    }
                    return;
                }
            }
        })
    };
    let mut buffer = Vec::with_capacity(512 * 1024);
    let mut chunk = vec![0u8; 256 * 1024];
    let mut delivered = false;
    loop {
        match stdout.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(read) => {
                buffer.extend_from_slice(&chunk[..read]);
                for frame in take_jpegs(&mut buffer) {
                    delivered = true;
                    producer.push(frame);
                }
            }
        }
    }
    if let Ok(mut c) = child.lock() {
        let _ = c.kill();
        let _ = c.wait();
    }
    let _ = watchdog.join();
    // A stream that never produced a frame falls back to screenshots.
    delivered
}

fn run_screenshots(producer: &Producer, udid: &str, display: Option<&str>) {
    let mut failures = 0;
    while !producer.idle() {
        let started = Instant::now();
        match super::frame(udid, display) {
            Ok(png) => {
                failures = 0;
                producer.push(png);
            }
            Err(error) => {
                failures += 1;
                if failures >= 3 {
                    producer.finish(Some(error));
                    return;
                }
            }
        }
        thread::sleep(POLL_GAP.saturating_sub(started.elapsed()));
    }
}

impl Streams {
    fn producer(&self, udid: &str, display: Option<&'static str>) -> Arc<Producer> {
        let key = format!("{udid}|{}", display.unwrap_or_default());
        let udid = key.as_str();
        let mut map = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        if let Some(existing) = map.get(&key) {
            let finished = existing.latest.lock().map(|l| l.finished).unwrap_or(true);
            if !finished {
                *existing.pulled.lock().unwrap_or_else(|p| p.into_inner()) = Instant::now();
                return existing.clone();
            }
        }
        let producer = Arc::new(Producer { latest: Mutex::default(), ready: Condvar::new(), pulled: Mutex::new(Instant::now()) });
        map.insert(key.clone(), producer.clone());
        let (worker, udid) = (producer.clone(), udid.split('|').next().unwrap_or_default().to_string());
        thread::spawn(move || {
            // An AXe stream that ends after delivering frames was stopped on purpose
            // or lost the device; only one that never delivered falls back.
            // AXe streams iOS only; Android frames come from screencap. Its stream likely
            // shows only the main display, so a Duo's inner display uses screenshots.
            let streamed = super::android::avd(&udid).is_none() && display != Some(super::duo::INNER) && run_axe(&worker, &udid);
            if !streamed && !worker.idle() {
                run_screenshots(&worker, &udid, display);
            }
            worker.finish(None);
        });
        producer
    }

    /// Start capturing ahead of the first request, e.g. right after a fold.
    pub fn start(&self, udid: &str, display: Option<&'static str>) {
        self.producer(udid, display);
    }

    /// The newest frame after `after`, waiting up to `wait` for one to arrive.
    pub fn next(&self, udid: &str, display: Option<&'static str>, after: u64, wait: Duration) -> Result<Option<Frame>, String> {
        let producer = self.producer(udid, display);
        let deadline = Instant::now() + wait;
        let mut latest = producer.latest.lock().map_err(|e| e.to_string())?;
        loop {
            if latest.seq > after {
                return Ok(Some(Frame { seq: latest.seq, jpeg: latest.jpeg.clone() }));
            }
            if latest.finished {
                return match latest.error.clone() {
                    Some(error) => Err(error),
                    None => Ok(None),
                };
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(None);
            }
            latest = producer.ready.wait_timeout(latest, left).map_err(|e| e.to_string())?.0;
        }
    }

    /// Stop a device's producer, e.g. when it shuts down.
    pub fn stop(&self, udid: &str) {
        let prefix = format!("{udid}|");
        let mut map = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        let keys: Vec<String> = map.keys().filter(|k| k.starts_with(&prefix)).cloned().collect();
        for producer in keys.iter().filter_map(|k| map.remove(k)) {
            if let Ok(mut pulled) = producer.pulled.lock() {
                *pulled = Instant::now() - IDLE * 2;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jpegs_are_cut_from_a_multipart_stream_across_reads() {
        let frame = |n: u8| vec![0xFF, 0xD8, 0xFF, 0xE0, n, 0xFF, 0x00, n, 0xFF, 0xD9];
        let mut stream = b"HTTP/1.1 200 OK\r\nContent-Type: multipart/x-mixed-replace; boundary=--mjpegstream\r\n\r\n--mjpegstream\r\nContent-Type: image/jpeg\r\n\r\n".to_vec();
        stream.extend(frame(1));
        stream.extend(b"\r\n--mjpegstream\r\nContent-Type: image/jpeg\r\n\r\n");
        stream.extend(frame(2));
        let (first, second) = stream.split_at(stream.len() - 4);
        let mut buffer = first.to_vec();
        assert_eq!(take_jpegs(&mut buffer), vec![frame(1)]);
        buffer.extend_from_slice(second);
        assert_eq!(take_jpegs(&mut buffer), vec![frame(2)]);
        assert!(buffer.len() <= 2);
    }
}
