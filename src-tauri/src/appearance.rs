// SPDX-License-Identifier: GPL-3.0-or-later
//! Theme and accent for every window: the saved settings plus the macOS accent color, pushed to
//! the frontend, which turns them into `data-theme` and `--accent` (src/ui/appearance.ts).

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Runtime};

/// Payload `String`: the system accent as `#rrggbb` (sRGB), after the user changes it.
pub const ACCENT_EVENT: &str = "ui://accent";
/// Payload [`Appearance`], after the theme or accent setting changes.
pub const APPEARANCE_EVENT: &str = "ui://appearance";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    /// Light or dark, following the system.
    #[default]
    System,
    Light,
    Dark,
    OledDark,
    Terminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum Accent {
    /// The accent color picked in System Settings > Appearance.
    #[default]
    System,
    /// The app's own muted indigo.
    Brand,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Appearance {
    pub theme: Theme,
    pub accent: Accent,
    /// `#rrggbb`, or None where the platform has no accent color.
    pub system_accent: Option<String>,
}

impl Appearance {
    pub fn new(theme: Theme, accent: Accent) -> Self {
        Self {
            theme,
            accent,
            system_accent: system_accent(),
        }
    }

    pub fn emit<R: Runtime>(&self, app: &impl Emitter<R>) {
        if let Err(e) = app.emit(APPEARANCE_EVENT, self) {
            tracing::warn!(error = %e, "appearance event not sent");
        }
    }
}

/// sRGB components in 0..=1 as `#rrggbb`; out-of-range (extended sRGB) values are clamped.
fn hex(r: f64, g: f64, b: f64) -> String {
    let byte = |c: f64| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", byte(r), byte(g), byte(b))
}

#[cfg(target_os = "macos")]
pub fn system_accent() -> Option<String> {
    use objc2_app_kit::{NSColor, NSColorSpace};
    let c = NSColor::controlAccentColor().colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())?;
    Some(hex(c.redComponent(), c.greenComponent(), c.blueComponent()))
}

#[cfg(not(target_os = "macos"))]
pub fn system_accent() -> Option<String> {
    None
}

/// Emits [`ACCENT_EVENT`] each time macOS reports a system color change.
#[cfg(target_os = "macos")]
pub fn watch_accent<R: Runtime>(app: tauri::AppHandle<R>) {
    use block2::RcBlock;
    use objc2_app_kit::NSSystemColorsDidChangeNotification;
    use objc2_foundation::{NSNotification, NSNotificationCenter};
    use std::ptr::NonNull;

    let block = RcBlock::new(move |_: NonNull<NSNotification>| {
        if let Some(accent) = system_accent() {
            if let Err(e) = app.emit(ACCENT_EVENT, accent) {
                tracing::warn!(error = %e, "accent event not sent");
            }
        }
    });
    // SAFETY: the name is AppKit's constant, there is no sender filter, and with no queue the
    // block runs on the posting (main) thread; AppHandle is Send + Sync.
    // The center holds the block for the app's lifetime; the returned token is not needed.
    let _ = unsafe {
        NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
            Some(NSSystemColorsDidChangeNotification),
            None,
            None,
            &block,
        )
    };
}

#[cfg(not(target_os = "macos"))]
pub fn watch_accent<R: Runtime>(_app: tauri::AppHandle<R>) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_rounds_and_clamps_extended_srgb() {
        assert_eq!(hex(0.0, 0.478, 1.0), "#007aff");
        assert_eq!(hex(-0.1, 0.5, 1.2), "#0080ff");
    }

    #[test]
    fn settings_values_are_snake_case() {
        let t: Theme = serde_json::from_str("\"oled_dark\"").unwrap();
        assert_eq!(t, Theme::OledDark);
        assert_eq!(serde_json::to_string(&Accent::Brand).unwrap(), "\"brand\"");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_system_accent_is_an_srgb_hex() {
        let a = system_accent().expect("macOS always has an accent color");
        assert!(a.len() == 7 && a.starts_with('#'), "got {a}");
    }
}
