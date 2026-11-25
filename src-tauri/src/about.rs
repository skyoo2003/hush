// SPDX-License-Identifier: GPL-3.0-or-later
//! The About screen: the Appropriate Legal Notices of GPL-3.0 section 5(d) (copyright, no
//! warranty, the license and where to read it) and the exact source this build came from.
//! The license and the third-party notices are compiled in, so they read offline.

use serde::{Deserialize, Serialize};

use crate::brand;

/// The commit this binary was built from (build.rs), or `v<version>` when built outside git.
pub const SOURCE_REV: &str = env!("SOURCE_REV");
pub const LICENSE_ID: &str = "GPL-3.0-or-later";
pub const LICENSE_URL: &str = "https://www.gnu.org/licenses/gpl-3.0.html";
const COPYRIGHT_HOLDER: &str = "Sung-Kyu Yoo";
const LICENSE_TEXT: &str = include_str!("../../LICENSE");
const NOTICES_TEXT: &str = include_str!("../../THIRD_PARTY_NOTICES");

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct About {
    pub name: String,
    pub version: String,
    pub copyright: String,
    pub license: String,
    pub license_url: String,
    /// The repository tree at `SOURCE_REV`.
    pub source_url: String,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum LegalText {
    License,
    ThirdPartyNotices,
}

pub fn about() -> About {
    About {
        name: brand::APP_NAME.into(),
        version: env!("CARGO_PKG_VERSION").into(),
        copyright: format!(
            "Copyright (C) 2026 {COPYRIGHT_HOLDER} and the {} contributors",
            brand::APP_NAME
        ),
        license: LICENSE_ID.into(),
        license_url: LICENSE_URL.into(),
        source_url: format!("{}/tree/{SOURCE_REV}", brand::REPO_URL),
    }
}

pub fn legal_text(which: LegalText) -> &'static str {
    match which {
        LegalText::License => LICENSE_TEXT,
        LegalText::ThirdPartyNotices => NOTICES_TEXT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn about_names_the_exact_source_and_the_license() {
        let a = about();
        assert_eq!(a.license, "GPL-3.0-or-later");
        assert!(
            a.source_url.ends_with(&format!("/tree/{SOURCE_REV}")),
            "{}",
            a.source_url
        );
        let rev_is_commit =
            SOURCE_REV.len() == 40 && SOURCE_REV.bytes().all(|b| b.is_ascii_hexdigit());
        assert!(
            rev_is_commit || SOURCE_REV == concat!("v", env!("CARGO_PKG_VERSION")),
            "{SOURCE_REV}"
        );
        assert!(legal_text(LegalText::License).contains("GNU GENERAL PUBLIC LICENSE"));
        assert!(legal_text(LegalText::License).contains("Version 3, 29 June 2007"));
    }

    #[test]
    fn notices_cover_crates_npm_pdfium_and_models() {
        let n = legal_text(LegalText::ThirdPartyNotices);
        for section in [
            "Rust crates",
            "npm packages bundled into the app's web UI",
            "PDFium (libpdfium.dylib)",
            "Embedding models",
        ] {
            assert!(n.contains(section), "missing section {section}");
        }
        for name in ["rusqlite", "react", "bge-small-en-v1.5"] {
            assert!(n.contains(name), "missing {name}");
        }
    }
}
