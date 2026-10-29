use matrix_client::{
    Snapshot,
    devices::{Device, DeviceReply},
};
use matrix_core::{Frame, HEIGHT, WIDTH, protocol};
use std::{
    collections::BTreeMap,
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
}
#[derive(Default)]
pub struct Manager {
    panels: Mutex<BTreeMap<String, Arc<Mutex<Panel>>>>,
    active: Mutex<BTreeMap<String, Arc<AtomicBool>>>,
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
                    device.status = "identifying".into();
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
                }))
            })
            .clone())
    }
    pub fn identify(
        &self,
        port_name: &str,
        frames: tokio::sync::watch::Receiver<Snapshot>,
        active: Arc<AtomicBool>,
    ) -> io::Result<()> {
        let panel = self.panel(port_name)?;
        let mut panel = panel
            .try_lock()
            .map_err(|_| io::Error::other("device busy"))?;
        let result = (|| {
            if panel.port.is_none() {
                let builder =
                    serialport::new(port_name, 115200).timeout(Duration::from_millis(250));
                #[cfg(unix)]
                {
                    let mut port = builder.open_native()?;
                    port.set_exclusive(true)?;
                    panel.port = Some(Box::new(port));
                }
                #[cfg(windows)]
                {
                    panel.port = Some(builder.open()?);
                }
                let port = panel.port.as_mut().expect("open port");
                std::thread::sleep(Duration::from_millis(200));
                port.write_all(&protocol::command(0x03, &[0]))?;
                std::thread::sleep(Duration::from_secs(1));
                port.write_all(&protocol::command(0x20, &[]))?;
                let mut version = [0; 3];
                port.read_exact(&mut version)?;
                panel.firmware = protocol::firmware(version);
                if panel.firmware.is_none() {
                    return Err(io::Error::other("invalid firmware response"));
                }
            } else {
                panel
                    .port
                    .as_mut()
                    .expect("open port")
                    .write_all(&protocol::command(0x03, &[0]))?;
                std::thread::sleep(Duration::from_secs(1));
            }
            let port = panel.port.as_mut().expect("open port");
            identify_on(
                &mut **port,
                &active,
                || frames.borrow().frame.clone(),
                Duration::from_secs(2),
            )
        })();
        active.store(false, Ordering::Release);
        if result.is_err() {
            panel.port = None;
            panel.firmware = None;
        }
        result
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
}
