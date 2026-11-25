// SPDX-License-Identifier: GPL-3.0-or-later
// Single source of brand strings. Plain consts only: build.rs `include!`s this file. (So no `//!` doc here.)

pub const APP_NAME: &str = "Locite";
pub const APP_SLUG: &str = "locite";
pub const BUNDLE_ID: &str = "io.github.skyoo2003.locite";
pub const KEYCHAIN_SERVICE: &str = "io.github.skyoo2003.locite";
pub const PACK_EXT: &str = "locitepack";
/// The public source repository; the About screen links the exact commit under it.
pub const REPO_URL: &str = "https://github.com/skyoo2003/locite";
pub const SITE_URL: &str = "https://skyoo2003.github.io/locite";
/// The signed update manifest of the latest GitHub release (ADR-022).
pub const UPDATE_MANIFEST_URL: &str =
    "https://github.com/skyoo2003/locite/releases/latest/download/latest.json";
pub const SPONSOR_URLS: &[&str] = &[
    "https://github.com/sponsors/skyoo2003",
    "https://buymeacoffee.com/skyoo2003",
];
