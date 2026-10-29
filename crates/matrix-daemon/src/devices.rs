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
}
#[derive(Default)]
pub struct Manager {
    panels: Mutex<BTreeMap<String, Arc<Mutex<Panel>>>>,
    active: Mutex<BTreeMap<String, Arc<AtomicBool>>>,
    jobs: Mutex<BTreeSet<String>>,
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
                    device.status = if panel.port.is_some() {
                        "open; identification path ready"
                    } else {
                        "disconnected/error; retry identification"
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
            )
        })();
        active.store(false, Ordering::Release);
        panel.frame = None;
        if result.is_err() {
            panel.port = None;
            panel.firmware = None;
        }
        result
    }

    fn route(&'static self, config: matrix_core::config::Config, snapshot: Snapshot) {
        let devices = enumerate().unwrap_or_default();
        let routes = resolve(&config, &devices);
        self.panels.lock().expect("panels").retain(|name, handle| {
            if routes.iter().any(|(_, port)| port == name) {
                return true;
            }
            if let Ok(mut panel) = handle.try_lock() {
                if let Some(port) = panel.port.as_mut() {
                    let _ = port.write_all(&protocol::command(0x03, &[1]));
                }
                false
            } else {
                true
            }
        });
        for (_, name) in routes {
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
                    let result = (|| {
                        panel.open(&name)?;
                        if panel.frame.as_ref() != Some(&frame) {
                            let port = panel.port.as_mut().expect("open port");
                            port.write_all(&protocol::command(0x00, &[32]))?;
                            for command in protocol::greyscale(&frame).expect("valid frame") {
                                port.write_all(&command)?;
                            }
                            panel.frame = Some(frame);
                            panel.last_write = Instant::now();
                        } else if panel.last_write.elapsed() >= Duration::from_secs(25) {
                            panel
                                .port
                                .as_mut()
                                .expect("open port")
                                .write_all(&protocol::command(0x00, &[32]))?;
                            panel.last_write = Instant::now();
                        }
                        Ok::<_, io::Error>(())
                    })();
                    if result.is_err() {
                        panel.port = None;
                        panel.firmware = None;
                        panel.frame = None;
                    }
                }
                self.jobs.lock().expect("jobs").remove(&name);
            });
        }
    }
}

impl Panel {
    fn open(&mut self, name: &str) -> io::Result<()> {
        if self.port.is_some() {
            return Ok(());
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
        self.frame = None;
        Ok(())
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
    Some(matrix_core::layout::render(panel, &readings))
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
            writer.write_all(&protocol::command(0x00, &[32]))?;
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
}
