// SPDX-License-Identifier: GPL-3.0-or-later
//! Menu bar and tray labels in English and Korean. Each key's `match` must cover both
//! languages, so a missing translation does not compile.

use std::sync::OnceLock;

use crate::brand::APP_NAME;
use crate::ingest::queue::IndexProgress;
use crate::ingest::scheduler::{Current, Reason, SchedState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En,
    Ko,
}

/// The first macOS preferred language Locite has, the one WKWebView reports to `navigator.language`.
pub fn lang() -> Lang {
    static LANG: OnceLock<Lang> = OnceLock::new();
    *LANG.get_or_init(|| pick(preferred_languages()))
}

fn pick(preferred: Vec<String>) -> Lang {
    preferred
        .iter()
        .find_map(|l| match l.split(['-', '_']).next() {
            Some("ko") => Some(Lang::Ko),
            Some("en") => Some(Lang::En),
            _ => None,
        })
        .unwrap_or(Lang::En)
}

#[cfg(target_os = "macos")]
fn preferred_languages() -> Vec<String> {
    objc2_foundation::NSLocale::preferredLanguages()
        .iter()
        .map(|s| s.to_string())
        .collect()
}

#[cfg(not(target_os = "macos"))]
fn preferred_languages() -> Vec<String> {
    Vec::new()
}

#[derive(Debug, Clone, Copy)]
pub enum S {
    About,
    Settings,
    Sponsor,
    Services,
    Hide,
    HideOthers,
    ShowAll,
    Quit,
    File,
    AddFolder,
    OpenQuickSearch,
    ExportPack,
    CloseWindow,
    Edit,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
    CopyCitation,
    CopyMasked,
    Find,
    View,
    ShowSidebar,
    HideSidebar,
    Search,
    Ask,
    Compare,
    Changes,
    EnterFullScreen,
    Indexing,
    PauseIndexing,
    ResumeIndexing,
    IndexNow,
    RescanFolder,
    Window,
    Minimize,
    Zoom,
    BringAllToFront,
    Help,
    AppHelp,
    KeyboardShortcuts,
    VerifyAirGap,
    AirGapOn,
    AirGapOff,
    IndexNotStarted,
    UpToDate,
    ScheduleNotStarted,
    SponsorSubmenu,
    IndexingPaused,
}

pub fn s(lang: Lang, key: S) -> String {
    use Lang::{En, Ko};
    let text = match (key, lang) {
        (S::About, En) => return format!("About {APP_NAME}"),
        (S::About, Ko) => return format!("{APP_NAME}에 관하여"),
        (S::Settings, En) => "Settings…",
        (S::Settings, Ko) => "설정…",
        (S::Sponsor, En) => return format!("Sponsor {APP_NAME}…"),
        (S::Sponsor, Ko) => return format!("{APP_NAME} 후원하기…"),
        (S::Services, En) => "Services",
        (S::Services, Ko) => "서비스",
        (S::Hide, En) => return format!("Hide {APP_NAME}"),
        (S::Hide, Ko) => return format!("{APP_NAME} 가리기"),
        (S::HideOthers, En) => "Hide Others",
        (S::HideOthers, Ko) => "기타 가리기",
        (S::ShowAll, En) => "Show All",
        (S::ShowAll, Ko) => "모두 보기",
        (S::Quit, En) => return format!("Quit {APP_NAME}"),
        (S::Quit, Ko) => return format!("{APP_NAME} 종료"),
        (S::File, En) => "File",
        (S::File, Ko) => "파일",
        (S::AddFolder, En) => "Add Folder…",
        (S::AddFolder, Ko) => "폴더 추가…",
        (S::OpenQuickSearch, En) => "Open Quick Search",
        (S::OpenQuickSearch, Ko) => "빠른 검색 열기",
        (S::ExportPack, En) => "Export as .locitepack…",
        (S::ExportPack, Ko) => ".locitepack으로 내보내기…",
        (S::CloseWindow, En) => "Close Window",
        (S::CloseWindow, Ko) => "윈도우 닫기",
        (S::Edit, En) => "Edit",
        (S::Edit, Ko) => "편집",
        (S::Undo, En) => "Undo",
        (S::Undo, Ko) => "실행 취소",
        (S::Redo, En) => "Redo",
        (S::Redo, Ko) => "실행 복귀",
        (S::Cut, En) => "Cut",
        (S::Cut, Ko) => "오려두기",
        (S::Copy, En) => "Copy",
        (S::Copy, Ko) => "복사하기",
        (S::Paste, En) => "Paste",
        (S::Paste, Ko) => "붙여넣기",
        (S::SelectAll, En) => "Select All",
        (S::SelectAll, Ko) => "전체 선택",
        (S::CopyCitation, En) => "Copy Citation",
        (S::CopyCitation, Ko) => "인용 복사",
        (S::CopyMasked, En) => "Copy Masked",
        (S::CopyMasked, Ko) => "가려서 복사",
        (S::Find, En) => "Find",
        (S::Find, Ko) => "찾기",
        (S::View, En) => "View",
        (S::View, Ko) => "보기",
        (S::ShowSidebar, En) => "Show Sidebar",
        (S::ShowSidebar, Ko) => "사이드바 보기",
        (S::HideSidebar, En) => "Hide Sidebar",
        (S::HideSidebar, Ko) => "사이드바 가리기",
        (S::Search, En) => "Search",
        (S::Search, Ko) => "검색",
        (S::Ask, En) => "Ask",
        (S::Ask, Ko) => "질문",
        (S::Compare, En) => "Compare",
        (S::Compare, Ko) => "비교",
        (S::Changes, En) => "Changes",
        (S::Changes, Ko) => "변경 사항",
        (S::EnterFullScreen, En) => "Enter Full Screen",
        (S::EnterFullScreen, Ko) => "전체 화면 시작",
        (S::Indexing, En) => "Indexing",
        (S::Indexing, Ko) => "색인",
        (S::PauseIndexing, En) => "Pause Indexing",
        (S::PauseIndexing, Ko) => "색인 일시 정지",
        (S::ResumeIndexing, En) => "Resume Indexing",
        (S::ResumeIndexing, Ko) => "색인 재개",
        (S::IndexNow, En) => "Index Everything Now",
        (S::IndexNow, Ko) => "지금 전체 색인",
        (S::RescanFolder, En) => "Rescan Folder",
        (S::RescanFolder, Ko) => "폴더 다시 검사",
        (S::Window, En) => "Window",
        (S::Window, Ko) => "윈도우",
        (S::Minimize, En) => "Minimize",
        (S::Minimize, Ko) => "최소화",
        (S::Zoom, En) => "Zoom",
        (S::Zoom, Ko) => "확대/축소",
        (S::BringAllToFront, En) => "Bring All to Front",
        (S::BringAllToFront, Ko) => "모두 앞으로 가져오기",
        (S::Help, En) => "Help",
        (S::Help, Ko) => "도움말",
        (S::AppHelp, En) => return format!("{APP_NAME} Help"),
        (S::AppHelp, Ko) => return format!("{APP_NAME} 도움말"),
        (S::KeyboardShortcuts, En) => "Keyboard Shortcuts",
        (S::KeyboardShortcuts, Ko) => "키보드 단축키",
        (S::VerifyAirGap, En) => "How to Verify Air-Gap",
        (S::VerifyAirGap, Ko) => "에어갭 확인 방법",
        (S::AirGapOn, En) => "Air-Gap: On",
        (S::AirGapOn, Ko) => "에어갭: 켜짐",
        (S::AirGapOff, En) => "Air-Gap: Off",
        (S::AirGapOff, Ko) => "에어갭: 꺼짐",
        (S::IndexNotStarted, En) => "Indexing not started",
        (S::IndexNotStarted, Ko) => "색인 시작 전",
        (S::UpToDate, En) => "Up to date",
        (S::UpToDate, Ko) => "모두 최신 상태",
        (S::ScheduleNotStarted, En) => "Schedule: not started",
        (S::ScheduleNotStarted, Ko) => "일정: 시작 전",
        (S::SponsorSubmenu, En) => "Sponsor",
        (S::SponsorSubmenu, Ko) => "후원",
        (S::IndexingPaused, En) => "Indexing paused",
        (S::IndexingPaused, Ko) => "색인 멈춤",
    };
    text.into()
}

/// "Indexing 11/18", "Indexing paused 3/8", or "Up to date".
pub fn index_label(lang: Lang, progress: Option<IndexProgress>) -> String {
    let Some(p) = progress else {
        return s(lang, S::IndexNotStarted);
    };
    let finished = p.done + p.failed;
    let total = finished + p.queued + p.running;
    match (p.paused, total > finished, lang) {
        (true, _, Lang::En) => format!("Indexing paused {finished}/{total}"),
        (true, _, Lang::Ko) => format!("색인 멈춤 {finished}/{total}"),
        (false, true, Lang::En) => format!("Indexing {finished}/{total}"),
        (false, true, Lang::Ko) => format!("색인 중 {finished}/{total}"),
        (false, false, _) => s(lang, S::UpToDate),
    }
}

pub fn schedule_label(lang: Lang, current: Option<Current>) -> String {
    let Some(c) = current else {
        return s(lang, S::ScheduleNotStarted);
    };
    let ko = lang == Lang::Ko;
    let state = match (c.state, ko) {
        (SchedState::Paused, false) => "Paused",
        (SchedState::Paused, true) => "멈춤",
        (SchedState::RunNow, false) => "Running now",
        (SchedState::RunNow, true) => "지금 실행 중",
        (SchedState::LowPower, false) => "Low power",
        (SchedState::LowPower, true) => "절전",
        (SchedState::Interactive, false) => "Interactive",
        (SchedState::Interactive, true) => "사용 중",
        (SchedState::Idle, false) => "Idle",
        (SchedState::Idle, true) => "유휴",
        (SchedState::Normal, false) => "Normal",
        (SchedState::Normal, true) => "보통",
    };
    let why = match (c.reason, ko) {
        (Reason::UserPaused, false) => "paused by you",
        (Reason::UserPaused, true) => "직접 멈춤",
        (Reason::UserRunNow, false) => "run now",
        (Reason::UserRunNow, true) => "지금 실행",
        (Reason::OutsideHours, false) => "outside allowed hours",
        (Reason::OutsideHours, true) => "허용 시간 밖",
        (Reason::Thermal, false) => "Mac is hot",
        (Reason::Thermal, true) => "Mac 과열",
        (Reason::LowPowerMode, false) => "Low Power Mode",
        (Reason::LowPowerMode, true) => "저전력 모드",
        (Reason::LowBattery, false) => "battery low",
        (Reason::LowBattery, true) => "배터리 부족",
        (Reason::Searching, false) => "searching",
        (Reason::Searching, true) => "검색 중",
        (Reason::UserIdle, false) => "you are away",
        (Reason::UserIdle, true) => "자리 비움",
        (Reason::Default, _) => "",
    };
    let head = if ko { "일정" } else { "Schedule" };
    if why.is_empty() {
        format!("{head}: {state}")
    } else {
        format!("{head}: {state} ({why})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_first_supported_preferred_language() {
        let v = |l: &[&str]| l.iter().map(|s| s.to_string()).collect();
        assert_eq!(pick(v(&["ko-KR", "en-US"])), Lang::Ko);
        assert_eq!(pick(v(&["fr-FR", "en-GB", "ko"])), Lang::En);
        assert_eq!(pick(v(&["ja-JP"])), Lang::En);
        assert_eq!(pick(v(&[])), Lang::En);
    }

    #[test]
    fn index_label_counts_finished_of_total() {
        let p = |queued, running, done, failed, paused| IndexProgress {
            queued,
            running,
            done,
            failed,
            paused,
        };
        let en = |p| index_label(Lang::En, p);
        assert_eq!(en(None), "Indexing not started");
        assert_eq!(en(Some(p(5, 2, 10, 1, false))), "Indexing 11/18");
        assert_eq!(en(Some(p(5, 0, 3, 0, true))), "Indexing paused 3/8");
        assert_eq!(en(Some(p(0, 0, 9, 1, false))), "Up to date");
        assert_eq!(en(Some(p(0, 0, 0, 0, false))), "Up to date");
        assert_eq!(
            index_label(Lang::Ko, Some(p(5, 2, 10, 1, false))),
            "색인 중 11/18"
        );
    }
}
