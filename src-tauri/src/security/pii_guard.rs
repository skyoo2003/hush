// SPDX-License-Identifier: GPL-3.0-or-later
//! Checks text before it leaves the app through the clipboard.

/// Returns the text to put on the clipboard. A no-op for now; the PII warning and the masked
/// copy attach here.
pub fn before_copy(text: String) -> String {
    text
}
