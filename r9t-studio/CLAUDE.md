# R9T Studio — Next-Generation Linux Control Center for GameGaraj Slayer R9T

## Project Overview
`r9t-studio` is a complete, production-grade Linux hardware control daemon (`r9td`), CLI (`r9tctl`), kernel fan module enhancement (`r9t_fan`), and GPU-accelerated desktop application (`r9t-ui` built with Tauri v2 + Svelte 5 / HTML5 Canvas) for the **GameGaraj Slayer R9T** laptop (AMD Ryzen 9 9955HX3D + NVIDIA GeForce RTX 5070 Ti Laptop GPU + 2560x1600 240Hz display, running Arch Linux with Hyprland and KDE Plasma support).

Reference existing Python implementation in `../outputs/slayer_r9t/` and kernel module in `../outputs/fan-driver/r9t_fan.c` for all hardware paths, sysfs attributes, and ioctl constants. Do NOT modify `../outputs/slayer_r9t/`; build the entire new system cleanly inside `r9t-studio/`.

---

## Hardware & Sysfs Specification (Verified on Live System)

1. **Kernel Fan Module (`r9t_fan` & `/dev/tuxedo_io`):**
   - Kernel module parameters:
     - `/sys/module/r9t_fan/parameters/status` (read): returns `mode=%d cpu_target=%u gpu_target=%u cpu_rpm=%d gpu_rpm=%d cpu_temp=%u gpu_temp=%u mode_byte=%u cpu_pwm=%u gpu_pwm=%u lease_ms=%d error=%d thermal=%d`
     - `/sys/module/r9t_fan/parameters/control` (write): accepts `auto`, `boost`, or `manual <cpu_pct> <gpu_pct>` (15-second lease; must be refreshed every 2–5 seconds while manual curve/target is active).
   - `driver/r9t_fan.c` (v2 enhancement):
     - Cache the last EC read state (`cpu`, `gpu`, `r0`, `r1`, `reg`, `pwm0`, `pwm1`) in kernel memory with a `jiffies` timestamp (`last_sample_jiffies`). If `status_get` is called within `500ms` of the last sample, return the cached values immediately without issuing 11 slow WMI ACPI calls (`uniwill_read_ec_ram`), reducing read latency from ~550ms to <0.01ms.
     - Allow `manual <cpu_pct> <gpu_pct>` from `20` to `100` (instead of `50` to `100`) when temperatures are safe (`cpu < 80` and `gpu < 78`), enforcing minimum `60%` when `cpu >= 85` or `gpu >= 82`, and `thermal_override` (`100%`) at `cpu >= 95` or `gpu >= 87`.
   - `/dev/tuxedo_io` ioctl constants (fallback when `r9t_fan` status is unavailable):
     - `0x8004EC20` (Temp 1), `0x8004EC21` (Fan 1), `0x8004EC22` (Temp 2), `0x8004EC23` (Fan 2).
     - IMPORTANT: Never call `/dev/tuxedo_io` if `/sys/module/r9t_fan/parameters/status` already succeeded!

2. **Extended `hwmon` & Power Supply Sensors:**
   - Discover dynamically by reading `/sys/class/hwmon/hwmon*/name`:
     - `k10temp`: AMD Ryzen 9 9955HX3D temperatures (`temp1_input` = Tctl, `temp3_input` / `temp4_input` = CCD1/CCD2).
     - `amdgpu`: Radeon 610M iGPU temperature (`temp1_input`), power (`power1_average` / `power1_input`).
     - `nvme`: Two NVMe SSD temperature sensors (`temp1_input`, composite/sensor 1/2).
     - `spd5118`: Two DDR5 SODIMM temperature sensors (`temp1_input`).
     - `mt7921_phy0`: Wi-Fi 6E adapter temperature (`temp1_input`).
   - Power Supplies (`/sys/class/power_supply/`):
     - `AC0/online`: `1` (AC connected) or `0` (On battery).
     - `BAT0/`: `capacity`, `status`, `energy_now`, `energy_full`, `energy_full_design`, `power_now`, `voltage_now`, `cycle_count`, `charge_control_end_threshold`.
     - `ucsi-source-psy-USBC000:001` & `002`: USB-C PD status (`online`, `voltage_now`, `current_now`).

3. **NVIDIA RTX 5070 Ti (Direct NVML FFI via `nvml-wrapper` or `libloading` on `/usr/lib/libnvidia-ml.so.1`):**
   - Check `/sys/bus/pci/devices/0000:01:00.0/power/runtime_status` first.
   - If `runtime_status == "suspended"` (D3cold deep sleep), do NOT call NVML functions that wake the GPU; report GPU state as `Suspended (D3cold, 0W)`.
   - When active (`active`), read via NVML in <0.1ms (zero `nvidia-smi` subprocesses):
     - Temperature (`nvmlDeviceGetTemperature`), Power usage mW (`nvmlDeviceGetPowerUsage`), Power management limit (`nvmlDeviceGetPowerManagementLimit`, `nvmlDeviceGetPowerManagementLimitConstraints`), Clocks (`Graphics`, `Memory`), Utilization (`gpu`, `memory`), VRAM (`used`, `total`), Fan speed, Performance state (`P0`–`P8`), and BOTH Graphics + Compute running processes (`nvmlDeviceGetGraphicsRunningProcesses_v3` & `nvmlDeviceGetComputeRunningProcesses_v3`).

4. **AMD CPU Power Limits & Platform Profiles:**
   - `/sys/firmware/acpi/platform_profile` (or `powerprofilesctl` D-Bus `net.hadess.PowerProfiles`): `quiet` / `power-saver`, `balanced`, `performance`.
   - AMD Ryzen PPT limits via `ryzenadj` (`--stapm-limit`, `--fast-limit`, `--slow-limit`, `--tctl-temp`), including presets:
     - `eco_silent`: 35W / 45W / 40W, GPU 80W
     - `sweet_spot`: 65W / 80W / 75W, GPU 100W
     - `balanced`: 75W / 100W / 90W, GPU 115W
     - `performance`: 100W / 125W / 115W, GPU 140W
     - `extreme`: 120W / 145W / 135W, GPU 140W

5. **126-Channel Per-Key RGB Keyboard (`/sys/class/leds/rgb:kbd_backlight*`):**
   - `rgb:kbd_backlight` and `rgb:kbd_backlight_1` .. `rgb:kbd_backlight_125` (126 total LED zones).
   - Keep file descriptors to `multi_intensity` and `brightness` cached in an open FD pool (`LedFdPool`) inside `r9td` so updating all 126 keys takes <0.3ms via `seek(0)` + `write_all` instead of opening/closing 378 sysfs files on every poll.
   - Support modes: Static, 4-Zone, Per-Key custom map (Turkish Q layout), Breathing, Wave, Thermal Gradient (CPU temp -> color), and PipeWire Audio Reactive.

6. **Dual Desktop Backend (Hyprland Native IPC + KDE Plasma Fallback):**
   - **Hyprland (`$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/`):**
     - Connect directly to `.socket.sock` to query `j/monitors`, `j/activewindow`, `j/clients` and to apply refresh rate (`keyword monitor eDP-1,2560x1600@240,auto,1` or `@60`) in <0.2ms.
     - Subscribe to `.socket2.sock` event stream (`activewindowv2`, `fullscreen`, `openwindow`, `closewindow`) for zero-polling game/fullscreen detection.
   - **KDE Plasma Fallback:**
     - Use `kscreen-doctor` when `XDG_CURRENT_DESKTOP` contains `KDE`.

---

## Architecture & Workspace Layout (`r9t-studio/`)

```text
r9t-studio/
├── Cargo.toml                  # Rust Workspace (r9t-protocol, r9t-hal, r9td, r9tctl)
├── driver/
│   ├── r9t_fan.c               # Enhanced v2 kernel module with cached status & 20-100% PWM
│   └── Makefile
├── crates/
│   ├── r9t-protocol/           # Shared serde types: TelemetrySnapshot, Profile, FanCurve, RgbConfig, RpcRequest, RpcResponse
│   ├── r9t-hal/                # Hardware Abstraction Layer: ec, nvml, hwmon, power, rgb_pool, ryzen, audio, sleep_diag, display
│   ├── r9td/                   # Async Tokio daemon: Single Thermal Authority (PID + Hysteresis), 300s RingBuffer, Event Automation, Unix Socket IPC (/tmp/r9td.sock & /run/r9td.sock)
│   └── r9tctl/                 # Sub-2ms CLI + Waybar JSON live streamer (`r9tctl waybar --follow`) + session event bridge
└── ui/                         # Modern Desktop GUI (Obsidian Dark theme, 6 Sidebar Modules)
```

## 6 Sidebar Modules in `ui/`
1. **Genel Bakış (Dashboard):** Header quick bar (Profile, AC/BAT, CPU/GPU °C & W, Max Fan Boost toggle, 60Hz/240Hz toggle), Extended Metric Cards (CPU, GPU, DDR5 RAM temps, Dual NVMe temps, Battery health/discharge rate), and Dual-Axis Live History Charts (pre-populated from daemon's 300s ring buffer so there is never "VERİ YOK" on startup or tab switch).
2. **Performans & Termal:** Power profiles (`Eco`, `Sweet Spot 65W/100W`, `Dengeli`, `Performans`, `Extreme`), AMD Ryzen PPT (`STAPM`, `Fast`, `Slow`, `Tctl`) & NVIDIA Power Limit sliders, and an **Interactive Drag-and-Drop Spline Fan Curve Canvas** + PID Target Temperature controller.
3. **Klavye & RGB Stüdyosu:** Interactive **GameGaraj Slayer R9T Turkish Q SVG Keyboard Map** (click or drag-paint keys, WASD/Arrows/Function quick groups, 4-zone mode, live effects: Static, Breathing, Rainbow Wave, CPU Thermal Gradient, Audio Reactive).
4. **Ekran, GPU & Otomasyon:** Hyprland & KDE display refresh rate (`60Hz` / `240Hz`), brightness, AC/Battery automation rules, Game Auto-Profile rules, and **GPU Wake Inspector** (shows `D3cold` vs `D0` state, lists both Graphics `G` and Compute `C` processes holding the NVIDIA GPU awake, and allows safe process termination).
5. **Donanım, Ses & Tanılama:** Battery charge threshold (`80%`/`90%`/`100%`), `Fn-Lock`, USB-C PD telemetry, PipeWire audio volume/mic/noise-cancellation controls, `s2idle` Sleep Blocker Analyzer, and Top CPU/RAM/GPU Process Monitor (async, never blocking UI).
6. **Profiller & Kıyaslama:** Dark-themed Profile Comparison Matrix, JSON Import/Export, and 1-Click Thermal/Power Stability Benchmark.
