//! Logical monitor size on Linux.
//!
//! GTK only reports whole scale factors. At 125%, 150%, or any other fractional
//! setting it still says scale 1 and returns the panel's hardware pixels.
//! GNOME's display config has the real per-monitor scale. Monitors at 100%, and
//! desktops that do not expose this, keep the GTK size.

#[cfg(any(target_os = "linux", test))]
use serde_json::Value;
#[cfg(any(target_os = "linux", test))]
use std::sync::{LazyLock, Mutex};
#[cfg(target_os = "linux")]
use std::{
    process::Command,
    time::{Duration, Instant},
};
use tauri::Monitor;

#[derive(Clone, Debug)]
pub struct MonitorPixels {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    // Read on macOS and Windows. Linux zoning starts from the full screen
    // instead, so a moved dock does not leave its old gap behind.
    #[allow(dead_code)]
    pub work_x: f64,
    #[allow(dead_code)]
    pub work_y: f64,
    pub work_width: f64,
    pub work_height: f64,
    /// Multiply these logical pixels to get window coordinates Tao expects.
    pub window_scale: f64,
}

#[cfg(any(target_os = "linux", test))]
#[derive(Clone, Debug)]
struct GnomeMonitor {
    product: String,
    connector: String,
    x: f64,
    y: f64,
    scale: f64,
    mode_width: f64,
    mode_height: f64,
    transform: u32,
    primary: bool,
}

pub fn monitor_pixels(monitor: &Monitor) -> MonitorPixels {
    let window_scale = monitor.scale_factor().max(0.01);
    let position = monitor.position();
    let size = monitor.size();
    let work = monitor.work_area();
    let mut pixels = MonitorPixels {
        x: position.x as f64 / window_scale,
        y: position.y as f64 / window_scale,
        width: size.width as f64 / window_scale,
        height: size.height as f64 / window_scale,
        work_x: work.position.x as f64 / window_scale,
        work_y: work.position.y as f64 / window_scale,
        work_width: work.size.width as f64 / window_scale,
        work_height: work.size.height as f64 / window_scale,
        window_scale,
    };
    #[cfg(target_os = "linux")]
    if let Some(desktop) =
        matching_gnome_monitor(&pixels, monitor.name().as_deref().map(String::as_str))
    {
        correct_for_desktop_scale(&mut pixels, &desktop);
    }
    pixels
}

#[cfg(any(target_os = "linux", test))]
fn correct_for_desktop_scale(pixels: &mut MonitorPixels, desktop: &GnomeMonitor) {
    if desktop.scale <= 0.0 {
        return;
    }
    let factor = desktop.scale / pixels.window_scale;
    if (factor - 1.0).abs() < 0.01 {
        return;
    }
    let oriented = oriented_mode(desktop.mode_width, desktop.mode_height, desktop.transform);
    let raw = (desktop.mode_width, desktop.mode_height);
    let hardware = if near(pixels.width, oriented.0) && near(pixels.height, oriented.1) {
        oriented
    } else if near(pixels.width, raw.0) && near(pixels.height, raw.1) {
        raw
    } else {
        return;
    };
    pixels.width = hardware.0 / factor;
    pixels.height = hardware.1 / factor;
    pixels.work_width /= factor;
    pixels.work_height /= factor;
    note_correction(&desktop.product, desktop.scale, pixels.width, pixels.height);
}

#[cfg(any(target_os = "linux", test))]
fn oriented_mode(width: f64, height: f64, transform: u32) -> (f64, f64) {
    match transform {
        1 | 3 | 5 | 7 => (height, width),
        _ => (width, height),
    }
}

#[cfg(any(target_os = "linux", test))]
fn near(left: f64, right: f64) -> bool {
    (left - right).abs() <= 2.0
}

#[cfg(any(target_os = "linux", test))]
fn note_correction(product: &str, scale: f64, width: f64, height: f64) {
    static SEEN: LazyLock<Mutex<Vec<String>>> = LazyLock::new(|| Mutex::new(Vec::new()));
    let key = format!("{product}@{scale:.3}");
    let Ok(mut seen) = SEEN.lock() else {
        return;
    };
    if seen.iter().any(|item| item == &key) {
        return;
    }
    seen.push(key);
    eprintln!("[display] {product}: desktop scale {scale:.3} -> logical {width:.0}x{height:.0}");
}

#[cfg(target_os = "linux")]
fn matching_gnome_monitor(pixels: &MonitorPixels, name: Option<&str>) -> Option<GnomeMonitor> {
    let monitors = gnome_monitors();
    if let Some(name) = name {
        let mut same = monitors.iter().filter(|monitor| monitor.product == name);
        if let Some(first) = same.next() {
            if same.next().is_none() {
                return Some(first.clone());
            }
        }
    }
    monitors
        .into_iter()
        .min_by(|left, right| {
            distance(left, pixels.x, pixels.y).total_cmp(&distance(right, pixels.x, pixels.y))
        })
        .filter(|monitor| distance(monitor, pixels.x, pixels.y) <= 4.0)
}

#[cfg(target_os = "linux")]
fn distance(monitor: &GnomeMonitor, x: f64, y: f64) -> f64 {
    let dx = monitor.x - x;
    let dy = monitor.y - y;
    dx * dx + dy * dy
}

#[cfg(target_os = "linux")]
static GNOME_MONITORS: LazyLock<Mutex<Option<(Instant, Vec<GnomeMonitor>)>>> =
    LazyLock::new(|| Mutex::new(None));

#[cfg(target_os = "linux")]
fn gnome_monitors() -> Vec<GnomeMonitor> {
    if let Ok(guard) = GNOME_MONITORS.lock() {
        if let Some((fetched, monitors)) = guard.as_ref() {
            if fetched.elapsed() < Duration::from_secs(2) {
                return monitors.clone();
            }
        }
    }
    let monitors = load_gnome_monitors();
    if let Ok(mut guard) = GNOME_MONITORS.lock() {
        *guard = Some((Instant::now(), monitors.clone()));
    }
    monitors
}

#[cfg(target_os = "linux")]
fn load_gnome_monitors() -> Vec<GnomeMonitor> {
    let output = Command::new("busctl")
        .args([
            "--user",
            "--json=short",
            "call",
            "org.gnome.Mutter.DisplayConfig",
            "/org/gnome/Mutter/DisplayConfig",
            "org.gnome.Mutter.DisplayConfig",
            "GetCurrentState",
        ])
        .stdin(std::process::Stdio::null())
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    let Ok(text) = String::from_utf8(output.stdout) else {
        return Vec::new();
    };
    parse_mutter_state(&text)
}

#[cfg(any(target_os = "linux", test))]
fn parse_mutter_state(text: &str) -> Vec<GnomeMonitor> {
    let Ok(root) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    let Some(data) = root.get("data").and_then(Value::as_array) else {
        return Vec::new();
    };
    let Some(hardware) = data.get(1).and_then(Value::as_array) else {
        return Vec::new();
    };
    let Some(logical) = data.get(2).and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut modes = Vec::new();
    for monitor in hardware {
        let Some(monitor) = monitor.as_array() else {
            continue;
        };
        let Some(product) = monitor
            .first()
            .and_then(Value::as_array)
            .and_then(|identity| identity.get(2))
            .and_then(Value::as_str)
        else {
            continue;
        };
        let Some(mode_list) = monitor.get(1).and_then(Value::as_array) else {
            continue;
        };
        for mode in mode_list {
            let Some(mode) = mode.as_array() else {
                continue;
            };
            let current = mode
                .get(6)
                .and_then(|props| props.get("is-current"))
                .and_then(|flag| flag.get("data"))
                .and_then(Value::as_bool)
                == Some(true);
            if !current {
                continue;
            }
            let (Some(width), Some(height)) = (json_f64(mode.get(1)), json_f64(mode.get(2))) else {
                continue;
            };
            modes.push((product.to_string(), width, height));
        }
    }
    let mut monitors = Vec::new();
    for layout in logical {
        let Some(layout) = layout.as_array() else {
            continue;
        };
        let (Some(x), Some(y), Some(scale)) = (
            json_f64(layout.get(0)),
            json_f64(layout.get(1)),
            json_f64(layout.get(2)),
        ) else {
            continue;
        };
        let transform = json_f64(layout.get(3)).unwrap_or(0.0) as u32;
        let Some(members) = layout.get(5).and_then(Value::as_array) else {
            continue;
        };
        for member in members {
            let Some(identity) = member.as_array() else {
                continue;
            };
            let Some(product) = identity.get(2).and_then(Value::as_str) else {
                continue;
            };
            let Some((_, mode_width, mode_height)) =
                modes.iter().find(|(name, _, _)| name == product)
            else {
                continue;
            };
            monitors.push(GnomeMonitor {
                product: product.to_string(),
                connector: identity
                    .first()
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                x,
                y,
                scale,
                mode_width: *mode_width,
                mode_height: *mode_height,
                transform,
                primary: layout.get(4).and_then(Value::as_bool).unwrap_or(false),
            });
        }
    }
    monitors
}

/// GNOME's top bar sits on the primary screen. GTK's work area does not
/// include it, but a normal window is kept below it. Zone drawing uses the
/// same top edge as that window.
#[cfg(target_os = "linux")]
pub fn primary_top_bar_inset(pixels: &MonitorPixels, name: Option<&str>) -> f64 {
    let Some(desktop) = matching_gnome_monitor(pixels, name) else {
        return 0.0;
    };
    if !desktop.primary {
        return 0.0;
    }
    gnome_shell_panel_px()
}

#[cfg(any(target_os = "linux", test))]
fn gnome_panel_px(points: f64, text_scale: f64) -> f64 {
    // Upstream GNOME shell CSS: `#panel { height: 2.2em; }`.
    (2.2 * points * 96.0 / 72.0 * text_scale).round()
}

#[cfg(target_os = "linux")]
fn gnome_shell_panel_px() -> f64 {
    let Some(points) = gsettings_font_points() else {
        return 0.0;
    };
    let scale = gsettings_number("org.gnome.desktop.interface", "text-scaling-factor").unwrap_or(1.0);
    if points <= 0.0 || scale <= 0.0 {
        return 0.0;
    }
    gnome_panel_px(points, scale)
}

/// Usable zone rectangle inside one monitor, relative to that monitor's origin.
/// Excludes the top bar and the Ubuntu sidebar.
#[cfg(target_os = "linux")]
pub fn zone_work_area(pixels: &MonitorPixels, name: Option<&str>) -> (f64, f64, f64, f64) {
    // Start from the full screen. The desktop work area can still describe the
    // dock's previous side after it is moved, and that would stack with the
    // side read from the current dock setting.
    let mut left = 0.0;
    let mut top = 0.0;
    let mut width = pixels.width;
    let mut height = pixels.height;
    let bar = primary_top_bar_inset(pixels, name);
    if bar > 0.0 {
        top += bar;
        height = (height - bar).max(1.0);
    }
    if let Some(dock) = dock_on_monitor(pixels, name) {
        (left, top, width, height) = apply_dock_edge(left, top, width, height, dock.0, dock.1);
    }
    (left, top, width.max(1.0), height.max(1.0))
}

#[cfg(any(target_os = "linux", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DockEdge {
    Left,
    Right,
    Top,
    Bottom,
}

/// Icon size comes from the user's dock setting. The screen-edge side adds
/// Ubuntu's 4px margin and 14px padding, plus 1px so the window border shows.
#[cfg(any(target_os = "linux", test))]
fn dock_thickness(icon_size: f64) -> f64 {
    (icon_size + 19.0).max(0.0)
}

#[cfg(any(target_os = "linux", test))]
fn apply_dock_edge(
    left: f64,
    top: f64,
    width: f64,
    height: f64,
    edge: DockEdge,
    thickness: f64,
) -> (f64, f64, f64, f64) {
    match edge {
        DockEdge::Left => (left + thickness, top, (width - thickness).max(1.0), height),
        DockEdge::Right => (left, top, (width - thickness).max(1.0), height),
        DockEdge::Top => (left, top + thickness, width, (height - thickness).max(1.0)),
        DockEdge::Bottom => (left, top, width, (height - thickness).max(1.0)),
    }
}

#[cfg(target_os = "linux")]
fn dock_on_monitor(pixels: &MonitorPixels, name: Option<&str>) -> Option<(DockEdge, f64)> {
    let dock = load_dock()?;
    let desktop = matching_gnome_monitor(pixels, name)?;
    let here = if dock.all_monitors {
        true
    } else if dock.connector.eq_ignore_ascii_case("primary") || dock.connector.is_empty() {
        desktop.primary
    } else {
        desktop.connector.eq_ignore_ascii_case(&dock.connector)
    };
    if !here {
        return None;
    }
    Some((dock.edge, dock_thickness(dock.icon)))
}

#[cfg(target_os = "linux")]
struct DockReserve {
    edge: DockEdge,
    icon: f64,
    connector: String,
    all_monitors: bool,
}

#[cfg(target_os = "linux")]
fn load_dock() -> Option<DockReserve> {
    const SCHEMA: &str = "org.gnome.shell.extensions.dash-to-dock";
    let position = gsettings_string(SCHEMA, "dock-position")?;
    let icon = gsettings_number(SCHEMA, "dash-max-icon-size")?;
    if icon <= 0.0 {
        return None;
    }
    let edge = match position.to_ascii_uppercase().as_str() {
        "LEFT" => DockEdge::Left,
        "RIGHT" => DockEdge::Right,
        "TOP" => DockEdge::Top,
        "BOTTOM" => DockEdge::Bottom,
        _ => return None,
    };
    Some(DockReserve {
        edge,
        icon,
        connector: gsettings_string(SCHEMA, "preferred-monitor-by-connector")
            .unwrap_or_else(|| "primary".to_string()),
        all_monitors: gsettings_bool(SCHEMA, "multi-monitor").unwrap_or(false),
    })
}

#[cfg(target_os = "linux")]
fn gsettings_font_points() -> Option<f64> {
    let text = gsettings_get("org.gnome.desktop.interface", "font-name")?;
    let text = text.trim().trim_matches('\'').trim_matches('"');
    text.split_whitespace()
        .next_back()
        .and_then(|points| points.parse::<f64>().ok())
        .filter(|points| *points > 0.0)
}

#[cfg(target_os = "linux")]
fn gsettings_string(schema: &str, key: &str) -> Option<String> {
    let text = gsettings_get(schema, key)?;
    let text = text.trim().trim_matches('\'').trim_matches('"').trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

#[cfg(target_os = "linux")]
fn gsettings_bool(schema: &str, key: &str) -> Option<bool> {
    match gsettings_string(schema, key)?.as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

#[cfg(target_os = "linux")]
fn gsettings_number(schema: &str, key: &str) -> Option<f64> {
    let text = gsettings_get(schema, key)?;
    text.trim()
        .trim_matches('\'')
        .trim_matches('"')
        .parse::<f64>()
        .ok()
}

#[cfg(target_os = "linux")]
fn gsettings_get(schema: &str, key: &str) -> Option<String> {
    let output = Command::new("gsettings")
        .args(["get", schema, key])
        .stdin(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

#[cfg(any(target_os = "linux", test))]
fn json_f64(value: Option<&Value>) -> Option<f64> {
    value.and_then(Value::as_f64)
}

#[cfg(test)]
mod tests {
    use super::{correct_for_desktop_scale, parse_mutter_state, GnomeMonitor, MonitorPixels};

    const SAMPLE: &str = r#"{
      "data": [1,
        [
          [["eDP-1","BOE","PANEL","0"],[["1920x1200@60",1920,1200,60.0,1.25,[1.0,1.25,1.5],{"is-current":{"type":"b","data":true}}]],{}],
          [["DP-1","RTK","SIDE","0"],[["1920x1080@60",1920,1080,60.0,1.0,[1.0],{"is-current":{"type":"b","data":true}}]],{}]
        ],
        [
          [0,0,1.0,0,true,[["DP-1","RTK","SIDE","0"]],{}],
          [1920,180,1.25,0,false,[["eDP-1","BOE","PANEL","0"]],{}]
        ],
        {}
      ]
    }"#;

    fn pixels(width: f64, height: f64, scale: f64) -> MonitorPixels {
        MonitorPixels {
            x: 0.0,
            y: 0.0,
            width,
            height,
            work_x: 0.0,
            work_y: 0.0,
            work_width: width,
            work_height: height,
            window_scale: scale,
        }
    }

    #[test]
    fn fractional_scale_changes_only_that_monitor() {
        let monitors = parse_mutter_state(SAMPLE);
        let panel = monitors
            .iter()
            .find(|monitor| monitor.product == "PANEL")
            .unwrap();
        let side = monitors
            .iter()
            .find(|monitor| monitor.product == "SIDE")
            .unwrap();
        assert!((panel.scale - 1.25).abs() < 0.001);
        assert!((side.scale - 1.0).abs() < 0.001);
        assert!(!panel.primary);
        assert!(side.primary);

        let mut laptop = pixels(1920.0, 1200.0, 1.0);
        correct_for_desktop_scale(&mut laptop, panel);
        assert!((laptop.width - 1536.0).abs() < 0.5);
        assert!((laptop.height - 960.0).abs() < 0.5);
        assert!((laptop.window_scale - 1.0).abs() < 0.001);

        let mut portable = pixels(1920.0, 1080.0, 1.0);
        correct_for_desktop_scale(&mut portable, side);
        assert!((portable.width - 1920.0).abs() < 0.5);
        assert!((portable.height - 1080.0).abs() < 0.5);
    }

    #[test]
    fn one_hundred_thirty_three_percent_uses_that_scale() {
        let desktop = GnomeMonitor {
            product: "PANEL".into(),
            connector: "eDP-1".into(),
            x: 1920.0,
            y: 180.0,
            scale: 4.0 / 3.0,
            mode_width: 1920.0,
            mode_height: 1200.0,
            transform: 0,
            primary: true,
        };
        let mut laptop = pixels(1920.0, 1200.0, 1.0);
        correct_for_desktop_scale(&mut laptop, &desktop);
        assert!((laptop.width - 1440.0).abs() < 1.0);
        assert!((laptop.height - 900.0).abs() < 1.0);
    }

    #[test]
    fn already_logical_size_is_not_divided_again() {
        let desktop = GnomeMonitor {
            product: "PANEL".into(),
            connector: "eDP-1".into(),
            x: 0.0,
            y: 0.0,
            scale: 2.0,
            mode_width: 1920.0,
            mode_height: 1200.0,
            transform: 0,
            primary: true,
        };
        let mut laptop = pixels(960.0, 600.0, 2.0);
        correct_for_desktop_scale(&mut laptop, &desktop);
        assert!((laptop.width - 960.0).abs() < 0.5);
        assert!((laptop.height - 600.0).abs() < 0.5);
    }

    #[test]
    fn dock_reserves_its_edge() {
        assert!((super::dock_thickness(48.0) - 67.0).abs() < 0.1);
        assert!((super::dock_thickness(64.0) - 83.0).abs() < 0.1);
        let (left, top, width, height) =
            super::apply_dock_edge(0.0, 32.0, 1536.0, 928.0, super::DockEdge::Left, 76.0);
        assert!((left - 76.0).abs() < 0.1);
        assert!((top - 32.0).abs() < 0.1);
        assert!((width - 1460.0).abs() < 0.1);
        assert!((height - 928.0).abs() < 0.1);
        let (left, _, width, height) =
            super::apply_dock_edge(0.0, 0.0, 1920.0, 1080.0, super::DockEdge::Bottom, 76.0);
        assert!((left - 0.0).abs() < 0.1);
        assert!((width - 1920.0).abs() < 0.1);
        assert!((height - 1004.0).abs() < 0.1);
    }

    #[test]
    fn gnome_panel_matches_default_font() {
        assert!((super::gnome_panel_px(11.0, 1.0) - 32.0).abs() < 0.1);
        assert!((super::gnome_panel_px(11.0, 1.5) - 48.0).abs() < 0.6);
    }

    #[test]
    fn bad_payload_is_ignored() {
        assert!(parse_mutter_state("not json").is_empty());
        assert!(parse_mutter_state(r#"{"data":[]}"#).is_empty());
    }
}
