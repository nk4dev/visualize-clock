mod audio;
mod dsp;
mod mpris;
mod renderer;
mod theme;

use audio::AudioCapture;
use chrono::Local;
use dsp::VisualizerDsp;
use mpris::MprisTracker;
use renderer::{
    draw_card_background, draw_mini_visualizer, draw_mirror_visualizer, draw_visualizer_bars,
    draw_waveform, FontRenderer, VisualizerMode,
};
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use theme::{Color, Theme, ThemeMode};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalPosition, LogicalSize, PhysicalPosition};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId, WindowLevel};

#[cfg(target_os = "linux")]
use winit::platform::x11::{EventLoopBuilderExtX11, WindowAttributesExtX11, WindowType};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WindowDisplayMode {
    Floating,
    Bar,
}

fn get_pos_file_path() -> PathBuf {
    let mut path = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"));
    path.push(".config");
    path.push("sound-clock-widget");
    let _ = std::fs::create_dir_all(&path);
    path.push("pos.txt");
    path
}

fn load_saved_position() -> Option<(i32, i32)> {
    let content = std::fs::read_to_string(get_pos_file_path()).ok()?;
    let parts: Vec<&str> = content.trim().split(',').collect();
    if parts.len() == 2 {
        let x = parts[0].parse::<i32>().ok()?;
        let y = parts[1].parse::<i32>().ok()?;
        Some((x, y))
    } else {
        None
    }
}

fn save_position(x: i32, y: i32) {
    let _ = std::fs::write(get_pos_file_path(), format!("{},{}", x, y));
}

struct SoundClockApp {
    window: Option<Arc<Window>>,
    context: Option<softbuffer::Context<Arc<Window>>>,
    surface: Option<softbuffer::Surface<Arc<Window>, Arc<Window>>>,
    audio: AudioCapture,
    dsp: VisualizerDsp,
    font_renderer: FontRenderer,
    theme: Theme,
    visualizer_mode: VisualizerMode,
    display_mode: WindowDisplayMode,
    is_pinned: bool,
    is_hovered: bool,
    is_expanded: bool,
    hover_leave_time: Instant,
    show_help: bool,
    samples_buf: Vec<f32>,
    last_frame_time: Instant,
    last_sound_time: Instant,
    cursor_pos: PhysicalPosition<f64>,
    win_pos: (i32, i32),
    initialized_window_state: bool,
    mpris: MprisTracker,
}

impl SoundClockApp {
    fn new(start_floating: bool, start_top: bool) -> Self {
        // Default mode is Bar Mode (compact bar, expands to full widget on hover)
        let display_mode = if start_floating {
            WindowDisplayMode::Floating
        } else {
            WindowDisplayMode::Bar
        };
        let is_expanded = display_mode == WindowDisplayMode::Floating;
        // Default to pinned (Always on Top) for desktop widget experience
        let is_pinned = true;

        // Position: use start_top, saved pos, or default bar top position (160, 32)
        let win_pos = if start_top {
            (160, 32)
        } else if let Some((x, y)) = load_saved_position() {
            (x, y)
        } else {
            (160, 32)
        };

        Self {
            window: None,
            context: None,
            surface: None,
            audio: AudioCapture::new(),
            dsp: VisualizerDsp::new(AudioCapture::BUFFER_SIZE),
            font_renderer: FontRenderer::new(),
            theme: Theme::get(ThemeMode::NeonCyberpunk),
            visualizer_mode: VisualizerMode::Bars,
            display_mode,
            is_pinned,
            is_hovered: false,
            is_expanded,
            hover_leave_time: Instant::now(),
            show_help: false,
            samples_buf: vec![0.0; AudioCapture::BUFFER_SIZE],
            last_frame_time: Instant::now(),
            last_sound_time: Instant::now(),
            cursor_pos: PhysicalPosition::new(0.0, 0.0),
            win_pos,
            initialized_window_state: false,
            mpris: MprisTracker::new(),
        }
    }

    fn find_window_id(&self) -> Option<String> {
        let pid_str = std::process::id().to_string();
        let display_val = std::env::var("DISPLAY").unwrap_or_else(|_| ":0".into());
        let output = std::process::Command::new("wmctrl")
            .env("DISPLAY", &display_val)
            .args(["-l", "-p"])
            .output()
            .ok()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 && parts[2] == pid_str {
                return Some(parts[0].to_string());
            }
        }
        None
    }

    fn apply_pinned_state(&self) -> bool {
        let action = if self.is_pinned { "add" } else { "remove" };
        let flag = format!("{},above,sticky", action);
        let display_val = std::env::var("DISPLAY").unwrap_or_else(|_| ":0".into());
        if let Some(wid) = self.find_window_id() {
            let _ = std::process::Command::new("wmctrl")
                .env("DISPLAY", &display_val)
                .args(["-i", "-r", &wid, "-b", &flag])
                .spawn();
            true
        } else {
            let _ = std::process::Command::new("wmctrl")
                .env("DISPLAY", &display_val)
                .args(["-r", "Sound Clock Widget", "-b", &flag])
                .spawn();
            false
        }
    }

    fn set_pinned(&mut self, pinned: bool) {
        self.is_pinned = pinned;
        if let Some(window) = &self.window {
            let level = if self.is_pinned {
                WindowLevel::AlwaysOnTop
            } else {
                WindowLevel::Normal
            };
            window.set_window_level(level);
        }

        self.apply_pinned_state();
    }

    fn toggle_pin(&mut self) {
        self.set_pinned(!self.is_pinned);
    }

    fn move_to(&mut self, x: i32, y: i32) {
        self.win_pos = (x, y);
        save_position(x, y);
        if let Some(window) = &self.window {
            window.set_outer_position(LogicalPosition::new(x as f64, y as f64));
        }
        let pos_arg = format!("0,{},{},-1,-1", x, y);
        if let Some(wid) = self.find_window_id() {
            let _ = std::process::Command::new("wmctrl")
                .args(["-i", "-r", &wid, "-e", &pos_arg])
                .spawn();
        } else {
            let _ = std::process::Command::new("wmctrl")
                .args(["-r", "Sound Clock Widget", "-e", &pos_arg])
                .spawn();
        }
    }

    fn dock_to_top_bar(&mut self) {
        // Snap directly onto the GNOME top bar at Y=0
        let target_x = if self.win_pos.0 >= 0 { self.win_pos.0.max(120) } else { 160 };
        self.display_mode = WindowDisplayMode::Bar;
        self.is_expanded = false;
        self.set_pinned(true);
        if let Some(window) = &self.window {
            let _ = window.request_inner_size(LogicalSize::new(360.0, 30.0));
        }
        self.move_to(target_x, 32);
    }

    fn toggle_bar_mode(&mut self) {
        if self.display_mode == WindowDisplayMode::Floating {
            self.display_mode = WindowDisplayMode::Bar;
            self.set_pinned(true);
            if !self.is_hovered {
                self.is_expanded = false;
                if let Some(window) = &self.window {
                    let _ = window.request_inner_size(LogicalSize::new(360.0, 30.0));
                }
            }
        } else {
            self.display_mode = WindowDisplayMode::Floating;
            self.is_expanded = true;
            if let Some(window) = &self.window {
                let _ = window.request_inner_size(LogicalSize::new(360.0, 165.0));
            }
        }
    }

    fn get_target_width(&self) -> f64 {
        if !self.mpris.current_track.title.is_empty() {
            440.0
        } else {
            360.0
        }
    }

    fn render(&mut self) {
        let (Some(surface), Some(window)) = (&mut self.surface, &self.window) else {
            return;
        };

        let size = window.inner_size();
        let (Some(width_nz), Some(height_nz)) =
            (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            return;
        };

        surface.resize(width_nz, height_nz).unwrap();
        let mut buffer = surface.buffer_mut().unwrap();

        let w = size.width;
        let h = size.height;

        buffer.fill(0);

        let (is_active, rms, source_name) = self.audio.get_samples(&mut self.samples_buf);
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame_time).as_secs_f32().min(0.1);
        self.last_frame_time = now;

        if is_active {
            self.last_sound_time = now;
        }

        self.dsp.process(&self.samples_buf, dt);

        let local_time = Local::now();
        let time_str = local_time.format("%H:%M:%S").to_string();
        let date_str = local_time.format("%Y.%m.%d · %a").to_string().to_uppercase();

        let track = self.mpris.update();
        let track_str = track.display_string();
        let has_track = !track_str.is_empty();

        if h <= 42 {
            // === Collapsed Desktop Bar View ===
            draw_card_background(
                &mut buffer,
                w,
                h,
                6.0,
                self.theme.bg,
                self.theme.border,
            );

            // Time text (Medium font, vertically centered)
            self.font_renderer.draw_text_medium(
                &mut buffer,
                w,
                h,
                10,
                21,
                &time_str,
                self.theme.time_text,
            );

            if has_track {
                // When track is playing: Show Mini Viz + Track Title + Hover arrow
                let mini_viz_x = 98u32;
                let mini_viz_w = 58u32;
                draw_mini_visualizer(
                    &mut buffer,
                    w,
                    h,
                    mini_viz_x,
                    7,
                    mini_viz_w,
                    16,
                    &self.dsp.smooth_bars,
                    &self.theme,
                );

                // Safe multi-byte track title truncate
                let mut disp_track = format!("♪ {}", track_str);
                let char_count = disp_track.chars().count();
                let max_chars = if w >= 440 { 28 } else { 18 };
                if char_count > max_chars {
                    let truncated: String = disp_track.chars().take(max_chars.saturating_sub(2)).collect();
                    disp_track = format!("{}..", truncated);
                }

                self.font_renderer.draw_text_small(
                    &mut buffer,
                    w,
                    h,
                    164,
                    20,
                    &disp_track,
                    self.theme.accent,
                );

                // Right hover arrow
                self.font_renderer.draw_text_small(
                    &mut buffer,
                    w,
                    h,
                    (w as i32) - 34,
                    20,
                    "[▾]",
                    self.theme.date_text,
                );
            } else {
                // When no track playing: Show centered visualizer + Date
                let mini_viz_x = 106u32;
                let mini_viz_w = w.saturating_sub(224).min(130);
                if mini_viz_w > 20 {
                    draw_mini_visualizer(
                        &mut buffer,
                        w,
                        h,
                        mini_viz_x,
                        7,
                        mini_viz_w,
                        16,
                        &self.dsp.smooth_bars,
                        &self.theme,
                    );
                }

                let hint_str = format!("{} · [▾]", local_time.format("%m.%d"));
                self.font_renderer.draw_text_small(
                    &mut buffer,
                    w,
                    h,
                    (w as i32) - 114,
                    20,
                    &hint_str,
                    self.theme.accent,
                );
            }
        } else {
            // === Full Expanded Widget View (Shown on hover or in Floating mode) ===
            draw_card_background(
                &mut buffer,
                w,
                h,
                16.0,
                self.theme.bg,
                self.theme.border,
            );

            // 1. Draw Time text (Large)
            self.font_renderer.draw_text_large(
                &mut buffer,
                w,
                h,
                22,
                46,
                &time_str,
                self.theme.time_text,
            );

            // 2. Draw Date text
            self.font_renderer.draw_text_small(
                &mut buffer,
                w,
                h,
                24,
                68,
                &date_str,
                self.theme.date_text,
            );

            // 3. Badges & Status
            let mode_badge = if self.display_mode == WindowDisplayMode::Bar {
                "[TOPBAR ▴]"
            } else {
                "[FLOAT]"
            };
            let pin_badge = if self.is_pinned { "[PIN]" } else { "" };
            let status_badge = format!(
                "{} {} {} | {}",
                mode_badge,
                pin_badge,
                self.visualizer_mode.name(),
                self.theme.name
            );
            self.font_renderer.draw_text_small(
                &mut buffer,
                w,
                h,
                (w as i32) - 225,
                26,
                &status_badge,
                self.theme.accent,
            );

            let src_badge = format!("{}: {:.0}%", source_name, (rms * 100.0).clamp(0.0, 99.0));
            self.font_renderer.draw_text_small(
                &mut buffer,
                w,
                h,
                (w as i32) - 170,
                46,
                &src_badge,
                self.theme.date_text,
            );

            // 3.5 Now Playing Track info (UTF-8 safe)
            if has_track {
                let mut disp = format!(">> {}", track_str);
                let char_count = disp.chars().count();
                if char_count > 34 {
                    let truncated: String = disp.chars().take(31).collect();
                    disp = format!("{}...", truncated);
                }
                self.font_renderer.draw_text_small(
                    &mut buffer,
                    w,
                    h,
                    24,
                    84,
                    &disp,
                    self.theme.accent,
                );
            }

            // 4. Sound Visualizer Area
            let viz_x = 20;
            let viz_y = if has_track { 94 } else { 82 };
            let viz_w = w.saturating_sub(40);
            let viz_h = h.saturating_sub(if has_track { 116 } else { 104 });

            if viz_w > 10 && viz_h > 10 {
                match self.visualizer_mode {
                    VisualizerMode::Bars => {
                        draw_visualizer_bars(
                            &mut buffer,
                            w,
                            h,
                            viz_x,
                            viz_y,
                            viz_w,
                            viz_h,
                            &self.dsp.smooth_bars,
                            &self.dsp.peak_bars,
                            &self.theme,
                        );
                    }
                    VisualizerMode::Mirror => {
                        draw_mirror_visualizer(
                            &mut buffer,
                            w,
                            h,
                            viz_x,
                            viz_y,
                            viz_w,
                            viz_h,
                            &self.dsp.smooth_bars,
                            &self.theme,
                        );
                    }
                    VisualizerMode::Waveform => {
                        draw_waveform(
                            &mut buffer,
                            w,
                            h,
                            viz_x,
                            viz_y,
                            viz_w,
                            viz_h,
                            &self.samples_buf,
                            &self.theme,
                        );
                    }
                }
            }

            // 5. Help / Controls
            if self.show_help {
                let help_text = "[D]DockTop [Arrows]Move [B]BarMode [T]Theme [M]Mode [P]Pin [Q]Quit";
                self.font_renderer.draw_text_small(
                    &mut buffer,
                    w,
                    h,
                    18,
                    (h as i32) - 8,
                    help_text,
                    Color::rgba(255, 255, 200, 240),
                );
            } else {
                let hint = if self.display_mode == WindowDisplayMode::Bar {
                    "[D] Top Bar | [Arrows] Adjust | Hover out to collapse"
                } else {
                    "[D] Dock Top | [B] Bar | [H] Help"
                };
                self.font_renderer.draw_text_small(
                    &mut buffer,
                    w,
                    h,
                    (w as i32) - 250,
                    (h as i32) - 8,
                    hint,
                    Color::rgba(140, 150, 170, 160),
                );
            }
        }

        buffer.present().unwrap();

        if !self.initialized_window_state {
            if self.is_pinned {
                if self.apply_pinned_state() {
                    self.initialized_window_state = true;
                }
            } else {
                self.initialized_window_state = true;
            }
        }
    }
}

impl ApplicationHandler for SoundClockApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        self.mpris.update();
        let initial_width = self.get_target_width();
        let initial_height = if self.display_mode == WindowDisplayMode::Bar && !self.is_expanded {
            30.0
        } else {
            165.0
        };

        let mut attributes = Window::default_attributes()
            .with_title("Sound Clock Widget")
            .with_inner_size(LogicalSize::new(initial_width, initial_height))
            .with_min_inner_size(LogicalSize::new(280.0, 28.0))
            .with_position(LogicalPosition::new(self.win_pos.0 as f64, self.win_pos.1 as f64))
            .with_decorations(false)
            .with_transparent(true);

        #[cfg(target_os = "linux")]
        {
            // Use Utility/Normal window type so Mutter allows _NET_WM_ACTION_MOVE.
            // Dock type prevents moving and causes Mutter to pass clicks through to background windows!
            attributes = attributes
                .with_x11_window_type(vec![WindowType::Utility, WindowType::Normal])
                .with_name("sound-clock-widget", "sound-clock-widget");
        }

        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .expect("Failed to create desktop widget window"),
        );

        let context = softbuffer::Context::new(window.clone())
            .expect("Failed to initialize softbuffer context");
        let surface = softbuffer::Surface::new(&context, window.clone())
            .expect("Failed to initialize softbuffer surface");

        self.window = Some(window.clone());
        self.context = Some(context);
        self.surface = Some(surface);

        // Position window
        window.set_outer_position(LogicalPosition::new(self.win_pos.0 as f64, self.win_pos.1 as f64));
        let _ = std::process::Command::new("wmctrl")
            .args(["-r", "Sound Clock Widget", "-e", &format!("0,{},{},-1,-1", self.win_pos.0, self.win_pos.1)])
            .spawn();

        if self.is_pinned {
            window.set_window_level(WindowLevel::AlwaysOnTop);
            let _ = std::process::Command::new("wmctrl")
                .args(["-r", "Sound Clock Widget", "-b", "add,above,sticky"])
                .spawn();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }

            WindowEvent::CursorEntered { .. } => {
                self.is_hovered = true;
                if self.display_mode == WindowDisplayMode::Bar && !self.is_expanded {
                    self.is_expanded = true;
                    if let Some(window) = &self.window {
                        let _ = window.request_inner_size(LogicalSize::new(self.get_target_width(), 165.0));
                        window.request_redraw();
                    }
                }
            }

            WindowEvent::CursorLeft { .. } => {
                self.is_hovered = false;
                self.hover_leave_time = Instant::now();
            }

            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_pos = position;
                self.is_hovered = true;
                if self.display_mode == WindowDisplayMode::Bar && !self.is_expanded {
                    self.is_expanded = true;
                    if let Some(window) = &self.window {
                        let _ = window.request_inner_size(LogicalSize::new(self.get_target_width(), 165.0));
                        window.request_redraw();
                    }
                }
            }

            WindowEvent::MouseInput { state, button, .. } => {
                if state == ElementState::Pressed {
                    match button {
                        MouseButton::Left => {
                            if let Some(window) = &self.window {
                                let _ = window.drag_window();
                            }
                        }
                        MouseButton::Right => {
                            self.theme = self.theme.next();
                            if let Some(window) = &self.window {
                                window.request_redraw();
                            }
                        }
                        MouseButton::Middle => {
                            self.visualizer_mode = self.visualizer_mode.next();
                            if let Some(window) = &self.window {
                                window.request_redraw();
                            }
                        }
                        _ => (),
                    }
                } else if state == ElementState::Released && button == MouseButton::Left {
                    // Update and save window position on drag release
                    if let Some(window) = &self.window {
                        if let Ok(pos) = window.outer_position() {
                            let x = pos.x;
                            let mut y = pos.y;
                            // Auto-snap to top bar if released near top (Y < 25)
                            if y < 25 && y >= -5 {
                                y = 0;
                                window.set_outer_position(LogicalPosition::new(x as f64, 0.0));
                            }
                            self.win_pos = (x, y);
                            save_position(x, y);
                        }
                    }
                }
            }

            WindowEvent::MouseWheel { delta, .. } => {
                let diff = match delta {
                    MouseScrollDelta::LineDelta(_x, y) => y * 0.15,
                    MouseScrollDelta::PixelDelta(pos) => (pos.y as f32) * 0.01,
                };
                self.dsp.gain = (self.dsp.gain + diff).clamp(0.2, 5.0);
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }

            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    match &event.logical_key {
                        Key::Named(NamedKey::Escape) => event_loop.exit(),
                        Key::Named(NamedKey::ArrowUp) => {
                            self.move_to(self.win_pos.0, (self.win_pos.1 - 4).max(0));
                        }
                        Key::Named(NamedKey::ArrowDown) => {
                            self.move_to(self.win_pos.0, self.win_pos.1 + 4);
                        }
                        Key::Named(NamedKey::ArrowLeft) => {
                            self.move_to((self.win_pos.0 - 10).max(0), self.win_pos.1);
                        }
                        Key::Named(NamedKey::ArrowRight) => {
                            self.move_to(self.win_pos.0 + 10, self.win_pos.1);
                        }
                        Key::Character(s) => match s.as_str() {
                            "q" | "Q" => event_loop.exit(),
                            "d" | "D" => {
                                // Snap to top bar!
                                self.dock_to_top_bar();
                                if let Some(window) = &self.window {
                                    window.request_redraw();
                                }
                            }
                            "b" | "B" => {
                                self.toggle_bar_mode();
                                if let Some(window) = &self.window {
                                    window.request_redraw();
                                }
                            }
                            "t" | "T" => {
                                self.theme = self.theme.next();
                                if let Some(window) = &self.window {
                                    window.request_redraw();
                                }
                            }
                            "m" | "M" => {
                                self.visualizer_mode = self.visualizer_mode.next();
                                if let Some(window) = &self.window {
                                    window.request_redraw();
                                }
                            }
                            "p" | "P" => {
                                self.toggle_pin();
                                if let Some(window) = &self.window {
                                    window.request_redraw();
                                }
                            }
                            "h" | "H" => {
                                self.show_help = !self.show_help;
                                if let Some(window) = &self.window {
                                    window.request_redraw();
                                }
                            }
                            "+" | "=" => {
                                self.dsp.gain = (self.dsp.gain + 0.2).min(5.0);
                                if let Some(window) = &self.window {
                                    window.request_redraw();
                                }
                            }
                            "-" | "_" => {
                                self.dsp.gain = (self.dsp.gain - 0.2).max(0.2);
                                if let Some(window) = &self.window {
                                    window.request_redraw();
                                }
                            }
                            _ => (),
                        },
                        _ => (),
                    }
                }
            }

            WindowEvent::RedrawRequested => {
                self.render();
            }

            _ => (),
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.display_mode == WindowDisplayMode::Bar {
            let target_w = self.get_target_width();
            if self.is_hovered && !self.is_expanded {
                self.is_expanded = true;
                if let Some(window) = &self.window {
                    let _ = window.request_inner_size(LogicalSize::new(target_w, 165.0));
                    window.request_redraw();
                }
            } else if !self.is_hovered && self.is_expanded {
                if self.hover_leave_time.elapsed() >= Duration::from_millis(250) {
                    self.is_expanded = false;
                    if let Some(window) = &self.window {
                        let _ = window.request_inner_size(LogicalSize::new(target_w, 30.0));
                        window.request_redraw();
                    }
                }
            }
        }

        let is_silent = self.last_sound_time.elapsed() > Duration::from_secs(2);

        let target_interval = if is_silent {
            Duration::from_millis(100)
        } else {
            Duration::from_millis(16)
        };

        let elapsed = self.last_frame_time.elapsed();
        if elapsed < target_interval {
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + (target_interval - elapsed),
            ));
        } else {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
            event_loop.set_control_flow(ControlFlow::Poll);
        }
    }
}

fn restart_everything() {
    println!("Restarting Sound Clock Widget ecosystem...");

    // 1. Restart background systemd user service
    let s1 = std::process::Command::new("systemctl")
        .args(["--user", "restart", "sound-clock-daemon.service"])
        .status();
    match s1 {
        Ok(st) if st.success() => println!("  [✓] Audio background daemon restarted"),
        _ => eprintln!("  [!] Warning: Failed to restart sound-clock-daemon service"),
    }

    // 2. Reload GNOME Shell Extension
    let _ = std::process::Command::new("gnome-extensions")
        .args(["disable", "sound-clock-visualizer@varius"])
        .status();
    std::thread::sleep(Duration::from_millis(150));
    let s2 = std::process::Command::new("gnome-extensions")
        .args(["enable", "sound-clock-visualizer@varius"])
        .status();
    match s2 {
        Ok(st) if st.success() => println!("  [✓] GNOME Shell Top Bar extension reloaded"),
        _ => eprintln!("  [!] Warning: Failed to reload GNOME Shell extension"),
    }

    // 3. Desktop Notification
    let _ = std::process::Command::new("notify-send")
        .args(["Sound Clock Widget", "すべてのコンポーネントを正常に再起動しました", "-i", "audio-volume-high"])
        .spawn();

    println!("All Sound Clock components successfully restarted!");
}

fn run_daemon() {
    println!("Starting sound-clock-widget in background daemon mode for GNOME top bar...");
    let audio = AudioCapture::new();
    let mut dsp = VisualizerDsp::new(AudioCapture::BUFFER_SIZE);
    let mut mpris = MprisTracker::new();
    let mut samples_buf = vec![0.0f32; AudioCapture::BUFFER_SIZE];
    let mut last_frame = Instant::now();
    let mut last_sound = Instant::now();

    let tmp_path = "/dev/shm/sound_clock_state.tmp";
    let final_path = "/dev/shm/sound_clock_state.json";

    loop {
        let (is_active, rms, source_name) = audio.get_samples(&mut samples_buf);
        let now = Instant::now();
        let dt = now.duration_since(last_frame).as_secs_f32().min(0.1);
        last_frame = now;

        if is_active {
            last_sound = now;
        }

        dsp.process(&samples_buf, dt);

        let local_time = Local::now();
        let time_str = local_time.format("%H:%M:%S").to_string();
        let date_str = local_time.format("%Y.%m.%d %a").to_string().to_uppercase();

        // 14 bars normalized to integer percentages 0..100
        let mut bars = [0u8; 14];
        for i in 0..14 {
            let val = (dsp.smooth_bars[i * 2] * 100.0).clamp(0.0, 100.0) as u8;
            bars[i] = val;
        }

        let is_silent = last_sound.elapsed() > Duration::from_secs(2);

        // MPRIS Track Info
        let track = mpris.update();
        let track_display = track.display_string().replace('"', "\\\"").replace('\n', " ");
        let track_artist = track.artist.replace('"', "\\\"").replace('\n', " ");
        let track_title = track.title.replace('"', "\\\"").replace('\n', " ");
        let track_status = track.status.replace('"', "\\\"").replace('\n', " ");
        let track_player = track.player.replace('"', "\\\"").replace('\n', " ");

        let bars_str: Vec<String> = bars.iter().map(|b| b.to_string()).collect();
        let json_payload = format!(
            r#"{{"time":"{}","date":"{}","source":"{}","track":"{}","artist":"{}","title":"{}","status":"{}","player":"{}","rms":{:.3},"active":{},"bars":[{}]}}"#,
            time_str,
            date_str,
            source_name,
            track_display,
            track_artist,
            track_title,
            track_status,
            track_player,
            rms,
            !is_silent,
            bars_str.join(",")
        );

        let _ = std::fs::write(tmp_path, json_payload.as_bytes());
        let _ = std::fs::rename(tmp_path, final_path);

        if is_silent {
            std::thread::sleep(Duration::from_millis(500));
        } else {
            std::thread::sleep(Duration::from_millis(33)); // ~30 FPS
        }
    }
}

fn main() {
    if std::env::var("DISPLAY").is_err() {
        std::env::set_var("DISPLAY", ":0");
    }
    println!("Starting Sound Clock Widget for Ubuntu (Bar Mode Default)...");
    println!("Controls:");
    println!("  Mouse Hover      : Auto-expand full widget (collapses when cursor leaves)");
    println!("  Left Click + Drag: Move widget on desktop");
    println!("  [B]              : Toggle Bar Mode / Always-Expanded Floating Mode");
    println!("  [D]              : Dock directly to GNOME Top Bar area");
    println!("  Right Click / [T]: Cycle color themes");
    println!("  Middle Click /[M]: Cycle visualizer modes (Bars / Mirror / Wave)");
    println!("  [P]              : Toggle Always-on-Top (Pin)");
    println!("  + / - / Scroll   : Adjust visualizer sensitivity (Gain)");
    println!("  [Arrow Keys]     : Fine-tune position (Up/Down/Left/Right)");
    println!("  [H]              : Toggle help overlay");
    println!("  [Q] / [Esc]      : Exit widget");

    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--restart" || a == "-r") {
        restart_everything();
        return;
    }
    if args.iter().any(|a| a == "--daemon" || a == "-d") {
        run_daemon();
        return;
    }
    let start_floating = args.iter().any(|a| a == "--float" || a == "-f");
    let start_top = args.iter().any(|a| a == "--top" || a == "-t");

    let event_loop = {
        let mut builder = EventLoop::builder();
        #[cfg(target_os = "linux")]
        {
            builder.with_x11();
        }
        builder.build().or_else(|_| EventLoop::new())
    }.expect("Failed to initialize event loop");

    let mut app = SoundClockApp::new(start_floating, start_top);
    let _ = event_loop.run_app(&mut app);
}
