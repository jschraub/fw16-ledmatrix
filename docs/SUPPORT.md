# Capability evidence and qualification

Targets: Linux and Windows. A successful build is not a claim that all desktop,
driver, or panel capabilities work. The daemon `--probe` and desktop capability
screen distinguish available, unavailable, error, and not verified. Available
means a prerequisite was observed; it does not mean a full adapter is qualified.

| Environment | Evidence | Still unverified |
| --- | --- | --- |
| Arch Linux x86_64 (development machine) | Native builds, IPC/runtime tests, executable capability probe | No panels; no accessible graphical session for native-window checks |
| Ubuntu 24.04 x86_64 | CI build and automated tests | Interactive desktop features and real USB panels |
| Windows hosted runner | CI build, IPC security/restricted-token tests | Actual Windows 11 laptop, brightness/audio/session APIs, USB panel identity |

No Windows 11 hardware-support promise is made yet. Choose the published runtime
support matrix only after interactive qualification. Keep serialport for serial
enumeration/transport, sysfs for Linux power/backlight, bounded pactl queries for
Linux audio, and platform-specific Windows interfaces. Probe prerequisites before
adding feature controls. Failed/missing probes never masquerade as zero readings.

Credential probes inspect existence only; they do not open credentials, export
home paths, refresh tokens, or query provider endpoints. Display/session event
coverage and physical bay location identity remain explicitly not verified.
