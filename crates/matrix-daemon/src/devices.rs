use matrix_client::{
    Snapshot,
    devices::{Device, DeviceReply},
};
use matrix_core::{Frame, HEIGHT, WIDTH, protocol};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, Read, Write},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

struct Panel {
    port: Option<Box<dyn serialport::SerialPort>>,
    firmware: Option<String>,
    frame: Option<Frame>,
    last_write: Instant,
    retry_at: Instant,
    error: Option<&'static str>,
    needs_replug: bool,
    confirmed_off: bool,
    brightness: Option<u8>,
}
#[derive(Default)]
pub struct Manager {
    panels: Mutex<BTreeMap<String, Arc<Mutex<Panel>>>>,
    active: Mutex<BTreeMap<String, Arc<AtomicBool>>>,
    jobs: Mutex<BTreeSet<String>>,
    stopping: AtomicBool,
}
static MANAGER: OnceLock<Manager> = OnceLock::new();
pub fn manager() -> &'static Manager {
    MANAGER.get_or_init(Manager::default)
}

fn enumerate() -> Result<Vec<Device>, ()> {
    let ports = serialport::available_ports().map_err(|_| ())?;
    let mut result = vec![];
    for port in ports {
        if !matches!(port.port_type, serialport::SerialPortType::UsbPort(ref info) if info.vid == 0x32ac && info.pid == 0x0020)
        {
            continue;
        }
        #[cfg(target_os = "linux")]
        let location = std::path::Path::new(&port.port_name)
            .file_name()
            .and_then(|name| {
                std::fs::canonicalize(
                    std::path::Path::new("/sys/class/tty")
                        .join(name)
                        .join("device"),
                )
                .ok()
            })
            .map(|p| p.to_string_lossy().into_owned());
        #[cfg(not(target_os = "linux"))]
        let location = None; // COM order/duplicate serial is not a physical identity.
        result.push(Device {
            port: port.port_name,
            location,
            firmware: None,
            status: "detected; identify to query firmware".into(),
        });
    }
    result.sort_by(|a, b| a.port.cmp(&b.port));
    Ok(result)
}

impl Manager {
    pub fn list(&self) -> DeviceReply {
        let Ok(mut devices) = enumerate() else {
            return DeviceReply {
                devices: vec![],
                message: "Serial enumeration failed; check drivers and access.".into(),
            };
        };
        let panels = self.panels.lock().expect("panel registry");
        for device in &mut devices {
            if let Some(panel) = panels.get(&device.port) {
                if let Ok(panel) = panel.try_lock() {
                    device.firmware = panel.firmware.clone();
                    device.status = if let Some(error) = panel.error {
                        error
                    } else if panel.confirmed_off {
                        "sleeping by configured policy"
                    } else if panel.port.is_some() {
                        "open; identification path ready"
                    } else {
                        panel.error.unwrap_or("disconnected; awaiting retry")
                    }
                    .into();
                } else {
                    device.status = "busy: display operation".into();
                }
            }
        }
        DeviceReply { devices, message: "Device identity is physical location, never enumeration order or serial number. Windows location remains unverified.".into() }
    }
    pub fn cancel(&self, port: &str) {
        if let Some(active) = self
            .active
            .lock()
            .expect("identification registry")
            .get(port)
        {
            active.store(false, Ordering::Release);
        }
    }
    pub fn begin(&self, port: &str) -> Option<Arc<AtomicBool>> {
        let present = enumerate().ok()?;
        if !present.iter().any(|device| device.port == port) {
            return None;
        }
        let mut active = self.active.lock().ok()?;
        active.retain(|name, flag| {
            flag.load(Ordering::Acquire) || present.iter().any(|device| &device.port == name)
        });
        if active
            .get(port)
            .is_some_and(|flag| flag.load(Ordering::Acquire))
        {
            return None;
        }
        let flag = Arc::new(AtomicBool::new(true));
        active.insert(port.into(), flag.clone());
        Some(flag)
    }
    fn panel(&self, port: &str) -> io::Result<Arc<Mutex<Panel>>> {
        let mut panels = self
            .panels
            .lock()
            .map_err(|_| io::Error::other("device registry"))?;
        // Bound retained ports and never evict a port while it is in use.
        let present = enumerate().unwrap_or_default();
        panels.retain(|name, panel| {
            present.iter().any(|d| &d.port == name) || Arc::strong_count(panel) > 1
        });
        if !panels.contains_key(port) && panels.len() >= 2 {
            return Err(io::Error::other("one/two panels supported"));
        }
        Ok(panels
            .entry(port.into())
            .or_insert_with(|| {
                Arc::new(Mutex::new(Panel {
                    port: None,
                    firmware: None,
                    frame: None,
                    last_write: Instant::now(),
                    retry_at: Instant::now(),
                    error: None,
                    needs_replug: false,
                    confirmed_off: false,
                    brightness: None,
                }))
            })
            .clone())
    }
    pub fn identify(
        &self,
        port_name: &str,
        frames: tokio::sync::watch::Receiver<Snapshot>,
        active: Arc<AtomicBool>,
        config: crate::Configuration,
    ) -> io::Result<()> {
        let panel = self.panel(port_name)?;
        let mut panel = panel
            .try_lock()
            .map_err(|_| io::Error::other("device busy"))?;
        if !panel.ready(Instant::now()) {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "device awaiting retry or physical reconnect",
            ));
        }
        let result = (|| {
            if panel.port.is_some() {
                panel
                    .port
                    .as_mut()
                    .expect("open port")
                    .write_all(&protocol::command(0x03, &[0]))?;
                std::thread::sleep(Duration::from_secs(1));
            }
            panel.open(port_name)?;
            let saved = config.lock().expect("configuration").read().config;
            let devices = enumerate().unwrap_or_default();
            let role = resolve(&saved, &devices)
                .into_iter()
                .find(|(_, port)| port == port_name)
                .and_then(|(name, _)| saved.panels.iter().find(|p| p.name == name));
            let policy = role
                .map(|panel| matrix_core::brightness::effective(&saved.brightness, panel))
                .unwrap_or(&saved.brightness);
            let flash = 32_u8.clamp(policy.minimum, policy.maximum);
            let port = panel.port.as_mut().expect("open port");
            identify_on(
                &mut **port,
                &active,
                || {
                    let config = config.lock().expect("configuration").read().config;
                    normal_frame(
                        port_name,
                        &config,
                        &frames.borrow(),
                        &enumerate().unwrap_or_default(),
                    )
                    .unwrap_or_else(blank)
                },
                Duration::from_secs(2),
                flash,
            )?;
            let saved = config.lock().expect("configuration").read().config;
            let devices = enumerate().unwrap_or_default();
            let role = resolve(&saved, &devices)
                .into_iter()
                .find(|(_, port)| port == port_name)
                .and_then(|(name, _)| saved.panels.iter().find(|p| p.name == name));
            if let Some(level) = role.and_then(|panel| {
                matrix_core::brightness::effective(&saved.brightness, panel).level()
            }) {
                panel
                    .port
                    .as_mut()
                    .expect("open port")
                    .write_all(&protocol::command(0x00, &[level]))?;
                panel.brightness = Some(level);
            } else {
                panel.sleep();
            }
            Ok(())
        })();
        active.store(false, Ordering::Release);
        panel.frame = None;
        if result.is_err() {
            panel.failed(Instant::now());
        }
        if self.stopping.load(Ordering::Acquire) {
            panel.sleep();
        }
        result
    }

    fn route(&'static self, config: matrix_core::config::Config, snapshot: Snapshot) {
        if self.stopping.load(Ordering::Acquire) {
            return;
        }
        let devices = enumerate().unwrap_or_default();
        let routes = resolve(&config, &devices);
        self.panels.lock().expect("panels").retain(|name, handle| {
            if routes.iter().any(|(_, port)| port == name) {
                return true;
            }
            if let Ok(mut panel) = handle.try_lock() {
                if panel.needs_replug && devices.iter().any(|device| &device.port == name) {
                    return true;
                }
                panel.sleep();
                false
            } else {
                true
            }
        });
        for (logical, name) in routes {
            let brightness = matrix_core::brightness::effective(
                &config.brightness,
                config
                    .panels
                    .iter()
                    .find(|p| p.name == logical)
                    .expect("resolved panel"),
            )
            .level();
            let Some(frame) = normal_frame(&name, &config, &snapshot, &devices) else {
                continue;
            };
            if !self.jobs.lock().expect("jobs").insert(name.clone()) {
                continue;
            }
            tokio::task::spawn_blocking(move || {
                if let Ok(handle) = self.panel(&name)
                    && let Ok(mut panel) = handle.try_lock()
                {
                    if !panel.ready(Instant::now()) || self.stopping.load(Ordering::Acquire) {
                        self.jobs.lock().expect("jobs").remove(&name);
                        return;
                    }
                    let result = (|| {
                        if brightness.is_none() && panel.confirmed_off {
                            return Ok(());
                        }
                        panel.open(&name)?;
                        let Some(brightness) = brightness else {
                            panel.sleep();
                            return Ok(());
                        };
                        if self.stopping.load(Ordering::Acquire) {
                            panel.sleep();
                            return Ok(());
                        }
                        if panel.frame.as_ref() != Some(&frame)
                            || panel.brightness != Some(brightness)
                        {
                            let port = panel.port.as_mut().expect("open port");
                            port.write_all(&protocol::command(0x00, &[brightness]))?;
                            for command in protocol::greyscale(&frame).expect("valid frame") {
                                port.write_all(&command)?;
                            }
                            panel.frame = Some(frame);
                            panel.brightness = Some(brightness);
                            panel.last_write = Instant::now();
                        } else if panel.last_write.elapsed() >= Duration::from_secs(25) {
                            panel
                                .port
                                .as_mut()
                                .expect("open port")
                                .write_all(&protocol::command(0x00, &[brightness]))?;
                            panel.last_write = Instant::now();
                        }
                        Ok::<_, io::Error>(())
                    })();
                    if result.is_err() {
                        panel.failed(Instant::now());
                    }
                    if self.stopping.load(Ordering::Acquire) {
                        panel.sleep();
                    }
                }
                self.jobs.lock().expect("jobs").remove(&name);
            });
        }
    }
}

impl Panel {
    fn ready(&self, now: Instant) -> bool {
        !self.needs_replug && now >= self.retry_at
    }
    fn failed(&mut self, now: Instant) {
        self.needs_replug = self.port.is_some();
        self.port = None;
        self.firmware = None;
        self.frame = None;
        self.retry_at = now + Duration::from_secs(5);
        self.error = Some(if self.needs_replug {
            "device write failed: reconnect the module (leave disconnected at least two seconds); no commands will follow a potentially partial payload"
        } else {
            "reconnecting: check access/competing POC/device; retry no sooner than five seconds"
        });
    }
    fn sleep(&mut self) {
        if let Some(port) = self.port.as_mut() {
            self.confirmed_off = port
                .write_all(&protocol::command(0x03, &[1]))
                .and_then(|_| port.flush())
                .is_ok();
            if !self.confirmed_off {
                self.needs_replug = true;
                self.error = Some("sleep command failed; reconnect the module");
            }
        }
        self.port = None;
        self.frame = None;
        self.brightness = None;
    }
    fn open(&mut self, name: &str) -> io::Result<()> {
        if self.port.is_some() {
            return Ok(());
        }
        #[cfg(target_os = "linux")]
        if another_owner(
            std::path::Path::new(name),
            std::path::Path::new("/proc"),
            std::process::id(),
        ) {
            return Err(io::Error::new(
                io::ErrorKind::AddrInUse,
                "another process owns this serial device",
            ));
        }
        let builder = serialport::new(name, 115200).timeout(Duration::from_millis(250));
        #[cfg(unix)]
        let mut port: Box<dyn serialport::SerialPort> = {
            let mut port = builder.open_native()?;
            port.set_exclusive(true)?;
            Box::new(port)
        };
        #[cfg(windows)]
        let mut port = builder.open()?;
        std::thread::sleep(Duration::from_millis(200));
        port.write_all(&protocol::command(0x03, &[0]))?;
        std::thread::sleep(Duration::from_secs(1));
        port.write_all(&protocol::command(0x20, &[]))?;
        let mut version = [0; 3];
        port.read_exact(&mut version)?;
        self.firmware =
            Some(protocol::firmware(version).ok_or_else(|| io::Error::other("invalid firmware"))?);
        self.port = Some(port);
        self.confirmed_off = false;
        self.brightness = None;
        self.error = None;
        self.frame = None;
        Ok(())
    }
}

#[cfg(target_os = "linux")]
fn another_owner(device: &std::path::Path, proc_root: &std::path::Path, own_pid: u32) -> bool {
    let Ok(target) = std::fs::canonicalize(device) else {
        return false;
    };
    let Ok(processes) = std::fs::read_dir(proc_root) else {
        return false;
    };
    for process in processes.filter_map(Result::ok) {
        let Some(pid) = process
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        if pid == own_pid {
            continue;
        }
        if let Ok(files) = std::fs::read_dir(process.path().join("fd")) {
            for file in files.filter_map(Result::ok) {
                if std::fs::read_link(file.path()).is_ok_and(|path| path == target) {
                    return true;
                }
            }
        }
    }
    false
}

pub async fn shutdown() {
    manager().stopping.store(true, Ordering::Release);
    for flag in manager().active.lock().expect("active").values() {
        flag.store(false, Ordering::Release);
    }
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let complete = tokio::task::spawn_blocking(|| {
            let mut complete = true;
            for panel in manager().panels.lock().expect("panels").values() {
                if let Ok(mut panel) = panel.try_lock() {
                    panel.sleep();
                } else {
                    complete = false;
                }
            }
            complete && manager().jobs.lock().expect("jobs").is_empty()
        })
        .await
        .unwrap_or(false);
        if complete || Instant::now() >= until {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn blank() -> Frame {
    Frame {
        width: WIDTH as u8,
        height: HEIGHT as u8,
        pixels: vec![0; WIDTH * HEIGHT],
    }
}
fn resolve(config: &matrix_core::config::Config, devices: &[Device]) -> Vec<(String, String)> {
    config
        .panels
        .iter()
        .filter_map(|panel| {
            let binding = panel.binding.as_ref()?;
            let matching: Vec<_> = devices
                .iter()
                .filter(|device| device.location.as_ref() == Some(binding))
                .collect();
            (matching.len() == 1).then(|| (panel.name.clone(), matching[0].port.clone()))
        })
        .collect()
}
fn normal_frame(
    port: &str,
    config: &matrix_core::config::Config,
    snapshot: &Snapshot,
    devices: &[Device],
) -> Option<Frame> {
    let routes = resolve(config, devices);
    let (name, _) = routes.iter().find(|(_, value)| value == port)?;
    let panel = config.panels.iter().find(|panel| &panel.name == name)?;
    let readings = matrix_core::layout::Readings {
        hour: snapshot.hour,
        minute: snapshot.minute,
        default_clock_intensity: config.clock_intensity,
        battery: snapshot
            .power
            .as_ref()
            .and_then(|p| p.percent)
            .map(f64::from),
        ..Default::default()
    };
    Some(matrix_core::layout::render_configured(
        panel,
        &readings,
        &config.brightness,
    ))
}
pub async fn route_loop(
    frames: tokio::sync::watch::Receiver<Snapshot>,
    config: crate::Configuration,
) {
    let mut timer = tokio::time::interval(Duration::from_secs(2));
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        timer.tick().await;
        let snapshot = frames.borrow().clone();
        let config = config.lock().expect("configuration").read().config;
        let _ = tokio::task::spawn_blocking(move || manager().route(config, snapshot)).await;
    }
}

fn identify_on<W: Write + ?Sized>(
    writer: &mut W,
    active: &AtomicBool,
    latest: impl Fn() -> Frame,
    duration: Duration,
    brightness: u8,
) -> io::Result<()> {
    let mut pattern = Frame {
        width: WIDTH as u8,
        height: HEIGHT as u8,
        pixels: vec![0; WIDTH * HEIGHT],
    };
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            if x == 0 || x == WIDTH - 1 || y == 0 || y == HEIGHT - 1 {
                pattern.pixels[y * WIDTH + x] = 180;
            }
        }
    }
    let result = (|| {
        if active.load(Ordering::Acquire) {
            writer.write_all(&protocol::command(0x00, &[brightness]))?;
            writer.write_all(&protocol::bw(&pattern).expect("valid pattern"))?;
        }
        let until = Instant::now() + duration;
        while active.load(Ordering::Acquire) && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(20));
        }
        Ok::<_, io::Error>(())
    })();
    // Restore current normal content after a successful or cancelled identification.
    // Do not write another command behind a partially failed command: its bytes
    // could be interpreted as the remainder of that command's payload.
    result?;
    protocol::greyscale(&latest())
        .ok_or_else(|| io::Error::other("invalid normal frame"))?
        .iter()
        .try_for_each(|command| writer.write_all(command))
}

pub struct CancelOnDrop(pub Arc<AtomicBool>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_restores_latest_greyscale_frame_without_truncating_commands() {
        let mut writes = vec![];
        let frame = matrix_core::render_clock(12, 34).unwrap();
        identify_on(
            &mut writes,
            &AtomicBool::new(false),
            || frame.clone(),
            Duration::from_secs(10),
            32,
        )
        .unwrap();
        let expected: Vec<u8> = protocol::greyscale(&frame)
            .unwrap()
            .into_iter()
            .flatten()
            .collect();
        assert_eq!(writes, expected); // Cancellation before start never flashes.
    }
    #[test]
    fn forged_ports_cannot_trigger_arbitrary_device_writes() {
        assert!(
            Manager::default()
                .begin("/not/a/Framework/device")
                .is_none()
        );
    }
    #[test]
    fn physical_locations_survive_reordering_and_ambiguity_never_guesses() {
        let mut config = matrix_core::config::Config::default();
        config.panels[0].binding = Some("bay-left".into());
        config.panels[1].binding = Some("bay-right".into());
        let device = |port: &str, location: &str| Device {
            port: port.into(),
            location: Some(location.into()),
            firmware: None,
            status: "fixture".into(),
        };
        let first = vec![device("COM2", "bay-right"), device("COM9", "bay-left")];
        assert_eq!(
            resolve(&config, &first),
            vec![
                ("left".into(), "COM9".into()),
                ("right".into(), "COM2".into())
            ]
        );
        let reordered = vec![device("COM4", "bay-left"), device("COM3", "bay-right")];
        assert_eq!(resolve(&config, &reordered)[0].1, "COM4");
        assert!(resolve(&config, &[]).is_empty());
        let ambiguous = vec![device("COM1", "bay-left"), device("COM2", "bay-left")];
        assert!(resolve(&config, &ambiguous).is_empty());
        let snapshot = crate::snapshot(0, 12, 34);
        assert_eq!(
            normal_frame("COM9", &config, &snapshot, &first).unwrap(),
            matrix_core::render_clock(12, 34).unwrap()
        );
    }
    #[test]
    fn retry_deadlines_are_bounded_and_quarantine_requires_replug() {
        let now = Instant::now();
        let mut panel = Panel {
            port: None,
            firmware: None,
            frame: None,
            last_write: now,
            retry_at: now,
            error: None,
            needs_replug: false,
            confirmed_off: false,
            brightness: None,
        };
        assert!(panel.ready(now));
        panel.failed(now);
        assert!(!panel.ready(now + Duration::from_secs(4)));
        assert!(panel.ready(now + Duration::from_secs(5)));
        panel.needs_replug = true;
        assert!(!panel.ready(now + Duration::from_secs(600)));
    }
    #[test]
    fn partial_write_failure_never_appends_another_protocol_command() {
        struct Broken {
            bytes: Vec<u8>,
            remaining: usize,
        }
        impl Write for Broken {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                if self.remaining == 0 {
                    return Err(io::Error::new(io::ErrorKind::BrokenPipe, "fixture"));
                }
                let count = bytes.len().min(self.remaining);
                self.bytes.extend_from_slice(&bytes[..count]);
                self.remaining -= count;
                Ok(count)
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut broken = Broken {
            bytes: vec![],
            remaining: 5,
        };
        assert!(
            identify_on(
                &mut broken,
                &AtomicBool::new(true),
                || matrix_core::render_clock(0, 0).unwrap(),
                Duration::ZERO,
                32
            )
            .is_err()
        );
        assert_eq!(broken.bytes, vec![0x32, 0xac, 0, 32, 0x32]);
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn competing_owner_probe_uses_file_descriptors_not_process_names() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("device");
        std::fs::write(&target, "fixture").unwrap();
        let fd = root.path().join("123/fd");
        std::fs::create_dir_all(&fd).unwrap();
        symlink(&target, fd.join("0")).unwrap();
        assert!(another_owner(&target, root.path(), 999));
        assert!(!another_owner(&target, root.path(), 123));
    }
}
