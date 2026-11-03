mod renderer;
mod wayland;

use anyhow::{bail, Result};
use clap::Parser;
use std::path::PathBuf;

/// Verdant: a moonlit ASCII tree, animated quietly behind your windows.
#[derive(Parser, Debug)]
#[command(version, about)]
pub struct Options {
    /// Maximum frames per second (1–60)
    #[arg(long, default_value_t = 20, value_parser = clap::value_parser!(u32).range(1..=60))]
    fps: u32,
    /// Maximum render width; compositor scales to the output
    #[arg(long, default_value_t = 1920, value_parser = clap::value_parser!(u32).range(320..=7680))]
    max_width: u32,
    /// Number of character columns (80–480)
    #[arg(long, default_value_t = 240, value_parser = clap::value_parser!(u32).range(80..=480))]
    columns: u32,
    /// Overall light level (0.1–2.0)
    #[arg(long, default_value_t = 0.95)]
    brightness: f32,
    /// Animation speed (0–4)
    #[arg(long, default_value_t = 1.0)]
    speed: f32,
    /// Output name, as shown by --list-outputs (default: compositor chooses)
    #[arg(long)]
    output: Option<String>,
    /// List connected Wayland outputs and exit
    #[arg(long)]
    list_outputs: bool,
    /// Optional replacement image; original tree is embedded in the binary
    #[arg(long)]
    image: Option<PathBuf>,
    /// Render one PNG without creating a wallpaper or connecting to Wayland
    #[arg(long)]
    snapshot: Option<PathBuf>,
    /// Snapshot width
    #[arg(long, default_value_t = 1920, value_parser = clap::value_parser!(u32).range(320..=7680))]
    width: u32,
    /// Snapshot height
    #[arg(long, default_value_t = 1080, value_parser = clap::value_parser!(u32).range(180..=4320))]
    height: u32,
    /// Initial animation time in seconds (also controls snapshots)
    #[arg(long, default_value_t = 0.0)]
    time: f32,
    /// Draw once, then sleep until the output changes
    #[arg(long = "static")]
    still: bool,
    /// Exit after this many seconds (useful for profiling)
    #[arg(long)]
    duration: Option<f64>,
}

fn main() -> Result<()> {
    // Nix's library wrapper renames the executable to .verdant-wrapped.
    // Keep process-name-based pause/stop commands consistent with cargo runs.
    unsafe {
        libc::prctl(libc::PR_SET_NAME, c"verdant".as_ptr(), 0, 0, 0);
    }
    let options = Options::parse();
    if !options.brightness.is_finite() || !(0.1..=2.0).contains(&options.brightness) {
        bail!("--brightness must be between 0.1 and 2.0");
    }
    if !options.speed.is_finite() || !(0.0..=4.0).contains(&options.speed) {
        bail!("--speed must be between 0 and 4");
    }
    if !options.time.is_finite() || !(0.0..=86400.0).contains(&options.time) {
        bail!("--time must be between 0 and 86400 seconds");
    }
    if options.duration.is_some_and(|v| {
        !v.is_finite() || v <= 0.0 || std::time::Duration::try_from_secs_f64(v).is_err()
    }) {
        bail!("--duration must be finite and positive");
    }
    if let Some(path) = &options.snapshot {
        let mut renderer = renderer::Renderer::headless(&options)?;
        renderer.draw(options.time, &options);
        renderer.snapshot(path)?;
        eprintln!("Saved {}", path.display());
        Ok(())
    } else {
        wayland::run(&options)
    }
}

/// Keep aspect ratio while limiting the number of shaded pixels.
fn render_size(width: u32, height: u32, max_width: u32) -> (u32, u32) {
    let scale = (max_width as f64 / width.max(1) as f64).min(1.0);
    (
        (width as f64 * scale).round().max(1.0) as u32,
        (height as f64 * scale).round().max(1.0) as u32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resolution_cap_preserves_aspect_and_never_upscales() {
        assert_eq!(render_size(3840, 2160, 1920), (1920, 1080));
        assert_eq!(render_size(1280, 720, 1920), (1280, 720));
        assert_eq!(render_size(3440, 1440, 1920), (1920, 804));
        assert_eq!(render_size(1, 1, 1920), (1, 1));
    }
}
