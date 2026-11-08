use std::process::Command;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Default)]
#[allow(dead_code)]
pub struct TrackInfo {
    pub artist: String,
    pub title: String,
    pub status: String,
    pub player: String,
}

impl TrackInfo {
    pub fn display_string(&self) -> String {
        if self.title.is_empty() {
            return String::new();
        }
        if self.artist.is_empty() {
            self.title.clone()
        } else {
            format!("{} - {}", self.artist, self.title)
        }
    }

    pub fn is_playing(&self) -> bool {
        self.status.eq_ignore_ascii_case("playing")
    }
}

pub struct MprisTracker {
    pub current_track: TrackInfo,
    last_check: Instant,
    check_interval: Duration,
}

impl MprisTracker {
    pub fn new() -> Self {
        Self {
            current_track: TrackInfo::default(),
            last_check: Instant::now() - Duration::from_secs(10),
            check_interval: Duration::from_millis(1000), // Check once every 1s
        }
    }

    pub fn update(&mut self) -> &TrackInfo {
        if self.last_check.elapsed() < self.check_interval {
            return &self.current_track;
        }
        self.last_check = Instant::now();

        let output = Command::new("playerctl")
            .args(["metadata", "--format", "{{ artist }}///{{ title }}///{{ status }}///{{ playerName }}"])
            .output();

        if let Ok(out) = output {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
                let parts: Vec<&str> = text.split("///").collect();
                if parts.len() >= 3 {
                    self.current_track = TrackInfo {
                        artist: parts[0].trim().to_string(),
                        title: parts[1].trim().to_string(),
                        status: parts[2].trim().to_string(),
                        player: if parts.len() >= 4 { parts[3].trim().to_string() } else { String::new() },
                    };
                    return &self.current_track;
                }
            }
        }

        self.current_track = TrackInfo::default();
        &self.current_track
    }
}
