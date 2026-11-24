// SPDX-License-Identifier: GPL-3.0-or-later
//! Sponsor links. The app never makes a request for sponsorship: with Air-Gap off the external
//! browser opens the URL, and with Air-Gap on a QR code generated here is shown instead.

use std::process::Command;

use qrcode::render::svg;
use qrcode::QrCode;
use serde::Serialize;

use crate::brand;

/// Tray labels, in `brand::SPONSOR_URLS` order. build.rs checks those URLs against FUNDING.yml.
pub const CHANNEL_NAMES: [&str; 2] = ["GitHub Sponsors (monthly)", "Buy Me a Coffee (one-time)"];

/// The main window shows a QR it receives on this event (sent by the tray with Air-Gap on).
pub const QR_EVENT: &str = "sponsor://qr";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SponsorLink {
    /// The external browser was asked to open the URL.
    Browser,
    /// Air-Gap is on: nothing was opened. `svg` encodes `url`.
    Qr { url: String, svg: String },
}

/// Opens channel `channel` with `browser` when Air-Gap is off; otherwise returns its QR code and
/// never calls `browser`. `None` for an unknown channel.
pub fn link(channel: usize, air_gap: bool, browser: impl FnOnce(&str)) -> Option<SponsorLink> {
    let url = *brand::SPONSOR_URLS.get(channel)?;
    if !air_gap {
        browser(url);
        return Some(SponsorLink::Browser);
    }
    // A URL of a few dozen bytes always fits a QR code; `ok()?` only covers a future long URL.
    let svg = QrCode::new(url)
        .ok()?
        .render::<svg::Color>()
        .min_dimensions(200, 200)
        .build();
    Some(SponsorLink::Qr {
        url: url.into(),
        svg,
    })
}

/// The browser makes the request; the app sends nothing.
pub fn open_in_browser(url: &str) {
    let url = url.to_owned();
    std::thread::spawn(move || Command::new("/usr/bin/open").arg(url).status());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn air_gap_on_shows_only_a_qr_and_never_opens_the_browser() {
        let mut calls = 0;
        for channel in 0..brand::SPONSOR_URLS.len() {
            let got = link(channel, true, |_| calls += 1).unwrap();
            let SponsorLink::Qr { url, svg } = got else {
                panic!("expected a QR, got {got:?}");
            };
            assert_eq!(url, brand::SPONSOR_URLS[channel]);
            assert!(svg.contains("<svg") && svg.contains("<path"), "{svg}");
        }
        assert_eq!(calls, 0);
    }

    #[test]
    fn air_gap_off_opens_the_browser_once_with_the_channel_url() {
        let mut opened = Vec::new();
        assert_eq!(
            link(1, false, |u| opened.push(u.to_owned())),
            Some(SponsorLink::Browser)
        );
        assert_eq!(opened, [brand::SPONSOR_URLS[1]]);
    }

    #[test]
    fn unknown_channel_opens_nothing() {
        let mut calls = 0;
        assert_eq!(link(2, false, |_| calls += 1), None);
        assert_eq!(calls, 0);
    }

    #[test]
    fn exactly_two_channels_with_names() {
        assert_eq!(brand::SPONSOR_URLS.len(), CHANNEL_NAMES.len());
    }
}
