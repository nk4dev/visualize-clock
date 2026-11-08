# Sound Clock Widget (Linux / Ubuntu)

A blazing-fast, ultra-lightweight desktop clock widget with real-time sound visualizer for Ubuntu / Linux desktops.

Written natively in Rust (`winit` + `softbuffer` + `rustfft` + `libpulse-simple`), achieving extraordinary resource efficiency: **~1.5 MB RSS (daemon) / ~11 MB RSS (GUI)** and **0–1% CPU usage**.

---

## Key Features

- ⚡ **Ultra-Low Memory & CPU Footprint**:
  - Memory usage: **~1.5 MB RSS** for background daemon / **~11 MB RSS** for interactive GUI.
  - Zero heap allocation during rendering loops (pre-allocated glyph & FFT scratch buffers).
  - Adaptive framerate: smooth 60 FPS when audio plays, automatically throttles down when silent.
- 🎵 **PipeWire & PulseAudio Desktop Audio Capture**:
  - Automatically captures audio from browsers (YouTube, Twitch), music players (Spotify, Apple Music), games, and media players.
  - Automatic fallback to default input if the system audio monitor is unavailable.
- 🎶 **MPRIS Media Integration (Now Playing)**:
  - Automatically detects and displays currently playing song titles, artists, and playback state (Spotify, Firefox, Chrome, VLC, etc.).
  - **Dynamic CJK Font Fallback**: Seamless on-demand glyph rasterization supporting Japanese, Chinese, and Korean characters without missing glyphs.
  - Displayed in both the compact desktop bar and expanded full widget.
- 📌 **Default Compact Desktop Bar Mode with Hover Expansion**:
  - Launches by default as a sleek, compact floating bar (30px height) showing time, live mini-visualizer, and song title.
  - **Auto-expands on mouse hover**: Instantly unfolds into a full-sized visualizer and digital clock widget.
  - Automatically contracts back to the slim bar when the cursor leaves.
- 🔝 **Native GNOME Shell Top Bar Integration**:
  - Integrated into the GNOME top panel via dedicated extension (`sound-clock-visualizer@varius`).
  - **Interactive Popup Menu**: Clicking the top panel icon opens a native GNOME popup featuring:
    - Large digital clock with neon glow and full date
    - Now Playing media card (track title, artist, player badge)
    - High-resolution real-time audio visualizer canvas
    - Live audio signal meter (RMS progress bar)
    - One-click theme switcher
- 🔄 **One-Click Full Ecosystem Restart**:
  - Instantly restart all components (background audio capture daemon and GNOME extension) if audio disconnects or freezes.
  - Accessible via CLI (`sound-clock-widget --restart`), launcher shortcut, or directly inside the top bar popup menu.
- 🎨 **5 Neon Color Themes**:
  - `Neon Cyberpunk`: Cyan & Magenta
  - `Sunset Glow`: Amber Gold & Coral Red
  - `Matrix Terminal`: Emerald Matrix Green
  - `Ice Minimalist`: Polar White & Sky Blue
  - `Rainbow Aurora`: Dynamic spectrum gradient across 28 frequency bands
- 📊 **3 Visualizer Modes**:
  - `BARS`: Frequency equalizer bars with peak-hold dots
  - `MIRROR`: Center-symmetric bidirectional studio meters
  - `WAVE`: Raw audio waveform oscilloscope mode
- 🪟 **Modern Translucent Glass UI**:
  - Borderless, rounded frosted-glass acrylic appearance.
  - Left-click drag anywhere to reposition freely across the screen.
  - Mutter-safe window management with `Always-on-Top` and `Sticky` across all virtual desktops.

---

## Keyboard & Mouse Shortcuts

| Input | Action |
|---|---|
| **Mouse Hover** | Automatically expands compact bar into the full-featured widget |
| **Left Click + Drag** | Reposition widget freely anywhere on screen |
| **Right Click** or **[T]** | Cycle color themes (Cyberpunk, Sunset, Matrix, Ice, Rainbow) |
| **Middle Click** or **[M]** | Cycle visualizer modes (`BARS` / `MIRROR` / `WAVE`) |
| **Mouse Scroll** or **[+]** / **[-]** | Adjust visualizer sensitivity (Gain) |
| **[B]** | Toggle Bar Mode (auto-expand on hover) ⇔ Always-Expanded Floating Mode |
| **[D]** | Snap directly onto GNOME Top Bar area (Y = 32) |
| **[Arrow Keys]** | Fine-tune widget position pixel-by-pixel (automatically saved) |
| **[P]** | Toggle Always-on-Top (Pin) |
| **[H]** | Toggle on-screen shortcut help overlay |
| **[Q]** or **[Esc]** | Exit widget |

---

## Installation & Build

### Requirements (Ubuntu / Debian)

```bash
sudo apt update
sudo apt install -y build-essential pkg-config libpulse-dev playerctl wmctrl fonts-noto-cjk
```

### Build from Source

```bash
# Clone and enter workspace
cd /home/nknighta/_varius/apps/s-client

# Compile optimized release binary
cargo build --release
```

The resulting binary will be located at `./target/release/sound-clock-widget`.

---

## Running the Widget

### 1. Standard Launch (Default Bar Mode)
Starts as a compact top bar with auto-hover expansion, live track info, and mini visualizer:
```bash
./target/release/sound-clock-widget
```

### 2. Always-Expanded Floating Mode
Starts directly in the full-size clock and large visualizer card:
```bash
./target/release/sound-clock-widget --float
```

### 3. Background Audio Daemon (for GNOME Shell Top Bar Extension)
Generates high-speed audio FFT state at `/dev/shm/sound_clock_state.json`:
```bash
./target/release/sound-clock-widget --daemon
```
*(Automatically managed via systemd user service `sound-clock-daemon.service`)*

### 4. Full Ecosystem Restart
Restarts the audio daemon and reloads the GNOME Shell top panel extension in one step:
```bash
./target/release/sound-clock-widget --restart
# or
sound-clock-restart
```

---

## Architecture & Tech Stack

- **Language**: 100% Native Rust (Edition 2021)
- **Audio Capture**: PulseAudio Simple API FFI (`libpulse-simple`) connected to PipeWire monitor socket
- **DSP / FFT**: 1024-point Fast Fourier Transform via `rustfft` with Hann windowing and logarithmic band grouping
- **Renderer**: CPU software rasterizer via `softbuffer` (zero GPU overhead)
- **Typography**: Scalable glyph rasterization with `ab_glyph`, featuring automatic fallback to CJK fonts (`DroidSansFallback` / `NotoSansCJK`) and dynamic caching
- **Media Tracking**: MPRIS D-Bus metadata parsing via `playerctl`
- **Compositor Compatibility**: Fully compatible with GNOME Mutter 45–50 on Ubuntu Wayland & X11 sessions

---

## License

MIT License. Designed with care for high performance and low resource usage.
