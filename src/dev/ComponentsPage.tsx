// SPDX-License-Identifier: GPL-3.0-or-later
// Harness page (/__harness, screens "components" and "components-overlays"): every src/ui component
// in every state that applies. States are forced with `data-demo` and a `data-window="inactive"`
// wrapper (docs/design/DESIGN.md, Process). Overlays open one at a time through scenario steps.
import {
	Download,
	FileText,
	Folder,
	FolderOpen,
	Inbox,
	Plus,
	RefreshCw,
	Settings,
	SlidersHorizontal,
	Trash2,
} from "lucide-react";
import { type ReactNode, useState } from "react";
import { APP_NAME } from "../brand";
import { locale, t } from "../i18n";
import { AiLabel } from "../ui/AiLabel";
import { AirGapIndicator } from "../ui/AirGapIndicator";
import { Alert } from "../ui/Alert";
import { Badge, type BadgeTone } from "../ui/Badge";
import { Banner } from "../ui/Banner";
import { Button, type ButtonVariant, type ControlSize } from "../ui/Button";
import { Checkbox } from "../ui/Checkbox";
import { ContextMenu } from "../ui/ContextMenu";
import { EmptyState } from "../ui/EmptyState";
import { EvidenceLevel } from "../ui/EvidenceLevel";
import { IconButton } from "../ui/IconButton";
import { KeyboardShortcut } from "../ui/KeyboardShortcut";
import { List, ListRow } from "../ui/List";
import { Menu, type MenuEntry } from "../ui/Menu";
import { NotFoundState } from "../ui/NotFoundState";
import { PiiCount } from "../ui/PiiCount";
import { Popover } from "../ui/Popover";
import { ProgressBar } from "../ui/ProgressBar";
import { ProgressRing } from "../ui/ProgressRing";
import { SearchField } from "../ui/SearchField";
import { SegmentedControl } from "../ui/SegmentedControl";
import { Select } from "../ui/Select";
import { Sheet } from "../ui/Sheet";
import { Sidebar, type SidebarSection } from "../ui/Sidebar";
import { Skeleton } from "../ui/Skeleton";
import { Slider } from "../ui/Slider";
import { SourceChip } from "../ui/SourceChip";
import { SplitView } from "../ui/SplitView";
import { Stepper } from "../ui/Stepper";
import { Switch } from "../ui/Switch";
import { Table, type TableColumn } from "../ui/Table";
import { Tag } from "../ui/Tag";
import { Text } from "../ui/Text";
import { TextField } from "../ui/TextField";
import { Toast, ToastProvider } from "../ui/Toast";
import { Toolbar } from "../ui/Toolbar";
import "./components-page.css";

/** Demo-only content: fixture data, not UI strings, so it lives here instead of i18n.ts. */
const TEXT = {
	en: {
		save: "Save",
		addFolder: "Add Folder",
		remove: "Remove",
		more: "More",
		settings: "Settings",
		name: "Name",
		nameValue: "Receipts 2026",
		nameHelp: "Shown in the sidebar.",
		query: "budget",
		pages: "Pages per batch",
		view: "View",
		list: "List",
		grid: "Grid",
		columns: "Columns",
		sortBy: "Sort by",
		relevance: "Relevance",
		modified: "Date Modified",
		kind: "Kind",
		indexPdfs: "Index PDFs",
		ocr: "Recognize text in images",
		everything: "Select all",
		watch: "Watch for changes",
		lowPower: "Pause on battery",
		weight: "Keyword weight",
		library: "Library",
		libraryInactive: "Library (inactive window)",
		librarySplit: "Library (split view)",
		folders: "Folders",
		smart: "Smart Folders",
		all: "All Documents",
		recent: "Recent",
		receipts: "Receipts",
		contracts: "Contracts",
		research: "Research Notes",
		pdfs: "PDFs this month",
		results: "Results",
		toolbar: "Main toolbar",
		indexing: "Indexing",
		downloading: "Downloading model",
		folderTable: "Watched folders",
		errorTable: "Files that failed",
		colFolder: "Folder",
		colDocs: "Documents",
		colStatus: "Status",
		colFile: "File",
		colReason: "Reason",
		active: "Active",
		paused: "Paused",
		unavailable: "Unavailable",
		encrypted: "Password protected",
		corrupt: "Damaged file",
		tooLarge: "Larger than 200 MB",
		bannerInfo: "Indexing 342 files. Results may be incomplete.",
		bannerSuccess: "All folders are up to date.",
		bannerWarning:
			"Air-Gap is off. Model downloads and update checks may use the network.",
		bannerError: "Could not read 2 files.",
		bannerTitle: "Index needs attention",
		retry: "Retry",
		new: "New",
		beta: "Beta",
		ok: "Indexed",
		warn: "Partial",
		fail: "Failed",
		typePdf: "type:pdf",
		after: "after:2026-01",
		removeFilter: "Remove filter",
		emptyTitle: "No folders yet",
		emptyBody: "Add a folder to search its documents on this Mac.",
		notFoundBody: "Try other words, or add the folder that holds the file.",
		chipLong:
			"Quarterly operating budget review for the regional offices, final draft.pdf",
		chipShort: "Lease agreement.pdf",
		excerpt: "The approved budget for Q3 is 12% lower than the draft.",
		popoverTitle: "Indexing options",
		popoverBody: "Changes apply to the next scan.",
		open: "Open",
		reveal: "Show in Finder",
		copy: "Copy Citation",
		showHidden: "Show hidden files",
		delete: "Delete Index",
		contextTarget: "Right-click this file",
		sheetTitle: "Add a folder",
		sheetBody: "{app} indexes the files in this folder on this Mac only.",
		alertTitle: "Rebuild the index?",
		alertBody: "Search works again when the rebuild finishes.",
		rebuild: "Rebuild",
		deleteTitle: "Delete all indexes?",
		deleteBody:
			"Your documents stay where they are. Only the search index is removed.",
		toastTitle: "Citation copied",
		toastBody: "Paste it into any document.",
		undo: "Undo",
		undoAlt: "Undo copying the citation",
		popover: "Popover",
		menu: "Menu",
		select: "Select",
		tooltip: "Tooltip",
		sheet: "Sheet",
		alert: "Alert",
		alertDestructive: "Destructive Alert",
		toast: "Toast",
		shortcutOpen: "Command Shift Space",
		shortcutCopy: "Command C",
	},
	ko: {
		save: "저장",
		addFolder: "폴더 추가",
		remove: "제거",
		more: "더 보기",
		settings: "설정",
		name: "이름",
		nameValue: "2026 영수증",
		nameHelp: "사이드바에 표시됩니다.",
		query: "예산",
		pages: "한 번에 처리할 쪽 수",
		view: "보기",
		list: "목록",
		grid: "격자",
		columns: "열",
		sortBy: "정렬 기준",
		relevance: "관련도",
		modified: "수정한 날짜",
		kind: "종류",
		indexPdfs: "PDF 색인",
		ocr: "이미지 속 텍스트 인식",
		everything: "모두 선택",
		watch: "변경 사항 감시",
		lowPower: "배터리 사용 시 일시 정지",
		weight: "키워드 가중치",
		library: "보관함",
		libraryInactive: "보관함 (비활성 창)",
		librarySplit: "보관함 (분할 보기)",
		folders: "폴더",
		smart: "스마트 폴더",
		all: "모든 문서",
		recent: "최근 항목",
		receipts: "영수증",
		contracts: "계약서",
		research: "연구 노트",
		pdfs: "이번 달 PDF",
		results: "결과",
		toolbar: "기본 도구 막대",
		indexing: "색인 중",
		downloading: "모델 내려받는 중",
		folderTable: "감시 중인 폴더",
		errorTable: "실패한 파일",
		colFolder: "폴더",
		colDocs: "문서",
		colStatus: "상태",
		colFile: "파일",
		colReason: "이유",
		active: "사용 중",
		paused: "일시 정지",
		unavailable: "사용할 수 없음",
		encrypted: "암호로 보호됨",
		corrupt: "손상된 파일",
		tooLarge: "200MB 초과",
		bannerInfo:
			"파일 342개를 색인하는 중입니다. 결과가 일부만 보일 수 있습니다.",
		bannerSuccess: "모든 폴더가 최신 상태입니다.",
		bannerWarning:
			"에어갭이 꺼져 있습니다. 모델 내려받기와 업데이트 확인에 네트워크를 쓸 수 있습니다.",
		bannerError: "파일 2개를 읽지 못했습니다.",
		bannerTitle: "색인을 확인해야 합니다",
		retry: "다시 시도",
		new: "새 항목",
		beta: "베타",
		ok: "색인됨",
		warn: "일부",
		fail: "실패",
		typePdf: "type:pdf",
		after: "after:2026-01",
		removeFilter: "필터 제거",
		emptyTitle: "아직 폴더가 없습니다",
		emptyBody: "폴더를 추가하면 이 Mac에서 문서를 검색할 수 있습니다.",
		notFoundBody: "다른 단어로 찾거나 파일이 있는 폴더를 추가하세요.",
		chipLong: "지역 사무소 분기별 운영 예산 검토 최종안.pdf",
		chipShort: "임대차 계약서.pdf",
		excerpt: "3분기 승인 예산은 초안보다 12% 적습니다.",
		popoverTitle: "색인 옵션",
		popoverBody: "다음 검사부터 적용됩니다.",
		open: "열기",
		reveal: "Finder에서 보기",
		copy: "인용 복사",
		showHidden: "숨김 파일 보기",
		delete: "색인 삭제",
		contextTarget: "이 파일을 오른쪽 클릭",
		sheetTitle: "폴더 추가",
		sheetBody: "{app}는 이 폴더의 파일을 이 Mac 안에서만 색인합니다.",
		alertTitle: "색인을 다시 만들까요?",
		alertBody: "다시 만들기가 끝나면 검색을 다시 쓸 수 있습니다.",
		rebuild: "다시 만들기",
		deleteTitle: "모든 색인을 삭제할까요?",
		deleteBody: "문서는 그대로 남습니다. 검색 색인만 지워집니다.",
		toastTitle: "인용을 복사했습니다",
		toastBody: "아무 문서에나 붙여 넣으세요.",
		undo: "실행 취소",
		undoAlt: "인용 복사 실행 취소",
		popover: "팝오버",
		menu: "메뉴",
		select: "선택",
		tooltip: "도움말",
		sheet: "시트",
		alert: "경고",
		alertDestructive: "파괴적 경고",
		toast: "토스트",
		shortcutOpen: "Command Shift Space",
		shortcutCopy: "Command C",
	},
} as const;
const x = TEXT[locale.toLowerCase().startsWith("ko") ? "ko" : "en"];

const noop = () => {};

type StateName =
	| "default"
	| "hover"
	| "pressed"
	| "focus"
	| "disabled"
	| "inactive";
type Demo = { "data-demo"?: "hover" | "pressed" | "focus"; disabled?: true };
const DEMO: Record<StateName, Demo> = {
	default: {},
	hover: { "data-demo": "hover" },
	pressed: { "data-demo": "pressed" },
	focus: { "data-demo": "focus" },
	disabled: { disabled: true },
	inactive: {},
};
const ALL: StateName[] = [
	"default",
	"hover",
	"pressed",
	"focus",
	"disabled",
	"inactive",
];
const FIELD: StateName[] = ["default", "focus", "disabled", "inactive"];

/** `only` limits a row to the states its forced attributes can show (see the slider). */
type Row = { label: string; only?: StateName[]; render(demo: Demo): ReactNode };
// Slider and segmented control put `data-demo` on the group, not the thumb or segment.
const GROUP: StateName[] = ["default", "disabled", "inactive"];

/** Rows are variants, columns are states; the inactive column is a forced background window. */
function Grid({ states, rows }: { states: StateName[]; rows: Row[] }) {
	return (
		<div
			className="cp-grid"
			style={{ gridTemplateColumns: `120px repeat(${states.length}, 1fr)` }}
		>
			<span />
			{states.map((s) => (
				<Text key={s} variant="subheadline" tone="secondary">
					{s}
				</Text>
			))}
			{rows.map((row) => (
				<div key={row.label} className="cp-grid-row">
					<Text variant="callout" tone="secondary">
						{row.label}
					</Text>
					{states.map((s) => (
						<div
							key={s}
							className="cp-cell"
							data-window={s === "inactive" ? "inactive" : undefined}
						>
							{(row.only ?? states).includes(s) && row.render(DEMO[s])}
						</div>
					))}
				</div>
			))}
		</div>
	);
}

function Section({ title, children }: { title: string; children: ReactNode }) {
	return (
		<section className="cp-section">
			<Text as="h2" variant="headline">
				{title}
			</Text>
			{children}
		</section>
	);
}

/** One view shown in an active and a forced inactive window, side by side. */
function ActiveInactive({ render }: { render(inactive: boolean): ReactNode }) {
	return (
		<div className="cp-pair">
			<div className="cp-pane">{render(false)}</div>
			<div className="cp-pane" data-window="inactive">
				{render(true)}
			</div>
		</div>
	);
}

const SIZES: ControlSize[] = ["regular", "small"];
const VARIANTS: ButtonVariant[] = [
	"primary",
	"secondary",
	"plain",
	"destructive",
];

const buttonRows: Row[] = [
	...VARIANTS.flatMap((variant) =>
		SIZES.map((size) => ({
			label: `${variant} ${size}`,
			render: (d: Demo) => (
				<Button variant={variant} size={size} {...d}>
					{variant === "destructive" ? x.remove : x.save}
				</Button>
			),
		})),
	),
	{
		label: "with icon",
		render: (d) => (
			<Button icon={Plus} {...d}>
				{x.addFolder}
			</Button>
		),
	},
	...(["plain", "secondary"] as const).flatMap((variant) =>
		SIZES.map((size) => ({
			label: `icon ${variant} ${size}`,
			render: (d: Demo) => (
				<IconButton
					label={x.settings}
					icon={Settings}
					variant={variant}
					size={size}
					{...d}
				/>
			),
		})),
	),
];

const SORT_OPTIONS = [
	{ value: "relevance", label: x.relevance },
	{ value: "modified", label: x.modified },
	{ value: "kind", label: x.kind },
] as const;
const VIEW_OPTIONS = [
	{ value: "list", label: x.list },
	{ value: "grid", label: x.grid },
	{ value: "columns", label: x.columns },
] as const;

const fieldRows: Row[] = [
	{
		label: "text field",
		render: (d) => (
			<TextField
				label={x.name}
				description={x.nameHelp}
				defaultValue={x.nameValue}
				{...d}
			/>
		),
	},
	{
		label: "text invalid",
		render: (d) => (
			<TextField aria-label={x.name} invalid defaultValue="" {...d} />
		),
	},
	...SIZES.map((size) => ({
		label: `search ${size}`,
		render: (d: Demo) => (
			<SearchField
				aria-label={t("search.label")}
				placeholder={t("search.placeholder")}
				clearLabel={t("ui.clear")}
				value=""
				onChange={noop}
				size={size}
				{...d}
			/>
		),
	})),
	{
		label: "search + chips",
		render: (d) => (
			<SearchField
				aria-label={t("search.label")}
				clearLabel={t("ui.clear")}
				value={x.query}
				onChange={noop}
				chips={<Tag label={x.typePdf} />}
				{...d}
			/>
		),
	},
	...SIZES.map((size) => ({
		label: `stepper ${size}`,
		render: (d: Demo) => (
			<Stepper
				label={x.pages}
				incrementLabel={t("ui.increment")}
				decrementLabel={t("ui.decrement")}
				value={8}
				onValueChange={noop}
				min={1}
				max={64}
				size={size}
				{...d}
			/>
		),
	})),
	...SIZES.map((size) => ({
		label: `select ${size}`,
		render: (d: Demo) => (
			<Select
				label={x.sortBy}
				options={SORT_OPTIONS}
				value="modified"
				onValueChange={noop}
				size={size}
				{...d}
			/>
		),
	})),
];

const toggleRows: Row[] = [
	...SIZES.flatMap((size) =>
		[false, true].map((on) => ({
			label: `switch ${size} ${on ? "on" : "off"}`,
			render: (d: Demo) => (
				<Switch label={x.watch} size={size} checked={on} {...d} />
			),
		})),
	),
	...([false, true, "indeterminate"] as const).map((checked) => ({
		label: `checkbox ${String(checked)}`,
		render: (d: Demo) => (
			<Checkbox
				label={checked === "indeterminate" ? x.everything : x.indexPdfs}
				checked={checked}
				{...d}
			/>
		),
	})),
	...SIZES.map((size) => ({
		label: `segmented ${size}`,
		only: GROUP,
		render: (d: Demo) => (
			<SegmentedControl
				label={x.view}
				options={VIEW_OPTIONS}
				value="list"
				onValueChange={noop}
				size={size}
				{...d}
			/>
		),
	})),
	{
		label: "slider",
		only: GROUP,
		render: (d) => (
			<Slider
				label={x.weight}
				value={40}
				onValueChange={noop}
				min={0}
				max={100}
				{...d}
			/>
		),
	},
];

type Doc = { id: string; name: string; meta: string; when: string };
const DOCS: Doc[] = [
	{ id: "a", name: x.chipShort, meta: "~/Documents/Legal", when: "2026-11-02" },
	{
		id: "b",
		name: x.chipLong,
		meta: "~/Documents/Finance",
		when: "2026-10-28",
	},
	{
		id: "c",
		name: "Q3 planning notes.md",
		meta: "~/Notes",
		when: "2026-10-12",
	},
	{
		id: "d",
		name: "Receipt 0412.jpg",
		meta: "~/Documents/Receipts",
		when: "2026-09-30",
	},
];

function DocList({
	density,
	label,
}: {
	density: "dense" | "two-line";
	label: string;
}) {
	return (
		<List
			label={label}
			items={DOCS}
			itemKey={(d) => d.id}
			itemText={(d) => d.name}
			selectedKey="b"
			onSelectedKeyChange={noop}
			density={density}
		>
			{(d) => (
				<ListRow
					icon={FileText}
					title={d.name}
					subtitle={density === "two-line" ? d.meta : undefined}
					trailing={d.when}
				/>
			)}
		</List>
	);
}

const SIDEBAR: SidebarSection[] = [
	{
		id: "library",
		items: [
			{ id: "all", label: x.all, icon: Inbox, count: 1542 },
			{ id: "recent", label: x.recent, icon: RefreshCw },
		],
	},
	{
		id: "folders",
		title: x.folders,
		collapsible: true,
		items: [
			{ id: "receipts", label: x.receipts, icon: Folder, count: 412 },
			{
				id: "contracts",
				label: x.contracts,
				icon: Folder,
				count: 88,
				badge: <Badge tone="danger">2</Badge>,
			},
			{ id: "research", label: x.research, icon: FolderOpen, count: 1042 },
		],
	},
	{
		id: "smart",
		title: x.smart,
		collapsible: true,
		items: [{ id: "pdfs", label: x.pdfs, icon: SlidersHorizontal }],
	},
];

type FolderRow = { id: string; name: string; docs: number; status: string };
const FOLDER_ROWS: FolderRow[] = [
	{ id: "r", name: "~/Documents/Research", docs: 1042, status: x.active },
	{ id: "c", name: "~/Documents/Receipts", docs: 412, status: x.paused },
	{ id: "l", name: "~/Documents/Legal", docs: 88, status: x.unavailable },
];
const FOLDER_COLUMNS: TableColumn<FolderRow>[] = [
	{ key: "name", header: x.colFolder, sortable: true, render: (r) => r.name },
	{
		key: "docs",
		header: x.colDocs,
		sortable: true,
		align: "end",
		width: "120px",
		render: (r) => r.docs.toLocaleString(),
	},
	{
		key: "status",
		header: x.colStatus,
		width: "160px",
		render: (r) => r.status,
	},
];
type ErrorRow = { id: string; file: string; reason: string };
const ERROR_ROWS: ErrorRow[] = [
	{ id: "1", file: "Scan 2026-03.pdf", reason: x.encrypted },
	{ id: "2", file: "Old ledger.xls", reason: x.corrupt },
	{ id: "3", file: "Archive video.mov", reason: x.tooLarge },
];
const ERROR_COLUMNS: TableColumn<ErrorRow>[] = [
	{ key: "file", header: x.colFile, sortable: true, render: (r) => r.file },
	{ key: "reason", header: x.colReason, render: (r) => r.reason },
];

const BADGE_TONES: BadgeTone[] = [
	"neutral",
	"accent",
	"success",
	"warning",
	"danger",
];

function MainToolbar({ airGap }: { airGap: boolean }) {
	return (
		<Toolbar
			label={x.toolbar}
			leading={<IconButton label={x.addFolder} icon={Plus} />}
			center={
				<SearchField
					aria-label={t("search.label")}
					placeholder={t("search.placeholder")}
					clearLabel={t("ui.clear")}
					value=""
					onChange={noop}
				/>
			}
			trailing={
				<>
					<ProgressRing label={x.indexing} value={0.6} />
					<AirGapIndicator
						on={airGap}
						label={t(airGap ? "airgap.on" : "airgap.off")}
						onClick={noop}
					/>
				</>
			}
		/>
	);
}

function AllComponents() {
	return (
		<>
			<Section title="Buttons">
				<Grid states={ALL} rows={buttonRows} />
			</Section>

			<Section title="Fields">
				<Grid states={FIELD} rows={fieldRows} />
			</Section>

			<Section title="Toggles">
				<Grid
					states={["default", "pressed", "focus", "disabled", "inactive"]}
					rows={toggleRows}
				/>
			</Section>

			<Section title="Lists (dense, two-line; active, inactive)">
				<ActiveInactive
					render={(inactive) => (
						<DocList
							density="dense"
							label={`${x.results} ${inactive ? 2 : 1}`}
						/>
					)}
				/>
				<ActiveInactive
					render={(inactive) => (
						<DocList
							density="two-line"
							label={`${x.results} ${inactive ? 4 : 3}`}
						/>
					)}
				/>
			</Section>

			<Section title="Sidebar (active, inactive)">
				<ActiveInactive
					render={(inactive) => (
						<Sidebar
							className="cp-sidebar"
							label={inactive ? x.libraryInactive : x.library}
							sections={SIDEBAR}
							selectedId="contracts"
							onSelect={noop}
						/>
					)}
				/>
			</Section>

			<Section title="Toolbar (Air-Gap on, off) and SplitView">
				<div className="cp-window">
					<MainToolbar airGap />
				</div>
				<div className="cp-window">
					<MainToolbar airGap={false} />
					<div className="cp-split">
						<SplitView
							size={220}
							onSizeChange={noop}
							dividerLabel={t("ui.resize")}
							leading={
								<Sidebar
									className="cp-fill"
									label={x.librarySplit}
									sections={SIDEBAR}
									selectedId="all"
									onSelect={noop}
								/>
							}
							trailing={
								<div className="cp-fill">
									<DocList density="two-line" label={`${x.results} 5`} />
								</div>
							}
						/>
					</div>
				</div>
			</Section>

			<Section title="Table (sorted by documents, descending; sorted by file)">
				<Table
					label={x.folderTable}
					rows={FOLDER_ROWS}
					rowKey={(r) => r.id}
					columns={FOLDER_COLUMNS}
					sort={{ key: "docs", direction: "descending" }}
					onSortChange={noop}
				/>
				<Table
					label={x.errorTable}
					rows={ERROR_ROWS}
					rowKey={(r) => r.id}
					columns={ERROR_COLUMNS}
					sort={{ key: "file", direction: "ascending" }}
					onSortChange={noop}
				/>
			</Section>

			<Section title="Banners">
				<Banner tone="info">{x.bannerInfo}</Banner>
				<Banner tone="success">{x.bannerSuccess}</Banner>
				<Banner tone="warning" onDismiss={noop} dismissLabel={t("ui.dismiss")}>
					{x.bannerWarning}
				</Banner>
				<Banner
					tone="error"
					title={x.bannerTitle}
					action={<Button size="small">{x.retry}</Button>}
					onDismiss={noop}
					dismissLabel={t("ui.dismiss")}
				>
					{x.bannerError}
				</Banner>
			</Section>

			<Section title="Progress and loading">
				<div className="cp-stack">
					<ProgressBar label={x.downloading} value={0.4} />
					<ProgressBar label={x.indexing} />
					<div className="cp-row">
						<ProgressRing label={x.indexing} value={0.25} />
						<ProgressRing label={x.indexing} value={0.75} />
						<ProgressRing label={x.indexing} />
					</div>
					<div className="cp-row">
						<Skeleton width={32} height={32} />
						<Skeleton lines={3} width={320} />
					</div>
				</div>
			</Section>

			<Section title="Badges, tags, shortcuts">
				<div className="cp-row">
					{BADGE_TONES.map((tone) => (
						<Badge key={tone} tone={tone}>
							{tone === "neutral"
								? 12
								: tone === "accent"
									? x.new
									: tone === "success"
										? x.ok
										: tone === "warning"
											? x.warn
											: x.fail}
						</Badge>
					))}
					<span data-window="inactive">
						<Badge tone="accent">{x.beta}</Badge>
					</span>
				</div>
				<div className="cp-row">
					<Tag label={x.typePdf} />
					<Tag label={x.after} onRemove={noop} removeLabel={x.removeFilter} />
					<KeyboardShortcut
						keys={{ meta: true, shift: true, key: " " }}
						label={x.shortcutOpen}
					/>
					<KeyboardShortcut
						keys={{ meta: true, key: "c" }}
						label={x.shortcutCopy}
					/>
					<KeyboardShortcut keys={{ key: "Enter" }} />
				</div>
			</Section>

			<Section title="Trust">
				<div className="cp-row">
					<AirGapIndicator on label={t("airgap.on")} onClick={noop} />
					<AirGapIndicator on={false} label={t("airgap.off")} onClick={noop} />
					<span data-window="inactive">
						<AirGapIndicator on label={t("airgap.on")} onClick={noop} />
					</span>
				</div>
				<Grid
					states={["default", "hover", "pressed", "focus", "inactive"]}
					rows={[
						{
							label: "source chip",
							render: (d) => (
								<SourceChip
									index={1}
									label={x.chipShort}
									location={t("card.page", { page: 4 })}
									onOpen={noop}
									{...d}
								/>
							),
						},
						{
							label: "long label",
							render: (d) => (
								<SourceChip
									index={2}
									label={x.chipLong}
									location={t("card.page", { page: 12 })}
									onOpen={noop}
									{...d}
								/>
							),
						},
					]}
				/>
				<div className="cp-row">
					<EvidenceLevel level="strong" label={t("evidence.strong")} />
					<EvidenceLevel level="weak" label={t("evidence.weak")} />
					<EvidenceLevel level="none" label={t("evidence.none")} />
					<AiLabel label={t("answer.aiLabel")} />
					<PiiCount count={0} label={t("pii.count", { count: 0 })} />
					<PiiCount count={3} label={t("pii.count", { count: 3 })} />
				</div>
				<div className="cp-pair">
					<NotFoundState
						title={t("answer.notFound")}
						description={x.notFoundBody}
					/>
					<EmptyState
						icon={Folder}
						title={x.emptyTitle}
						description={x.emptyBody}
						action={
							<Button variant="primary" icon={Plus}>
								{x.addFolder}
							</Button>
						}
					/>
				</div>
			</Section>
		</>
	);
}

const MENU_ITEMS: MenuEntry[] = [
	{
		type: "item",
		id: "open",
		label: x.open,
		icon: FileText,
		shortcut: "↩",
		onSelect: noop,
	},
	{
		type: "item",
		id: "reveal",
		label: x.reveal,
		icon: FolderOpen,
		onSelect: noop,
	},
	{
		type: "item",
		id: "copy",
		label: x.copy,
		shortcut: "⌘C",
		onSelect: noop,
	},
	{
		type: "item",
		id: "export",
		label: x.save,
		icon: Download,
		disabled: true,
		onSelect: noop,
	},
	{ type: "separator", id: "s1" },
	{
		type: "checkbox",
		id: "hidden",
		label: x.showHidden,
		checked: true,
		onCheckedChange: noop,
	},
	{ type: "separator", id: "s2" },
	{
		type: "item",
		id: "delete",
		label: x.delete,
		icon: Trash2,
		destructive: true,
		onSelect: noop,
	},
];

/** Triggers for each overlay; a scenario step opens exactly one (ids are the step selectors). */
function Overlays() {
	const [open, setOpen] = useState<
		"sheet" | "alert" | "alert-destructive" | "toast" | null
	>(null);
	const close = (o: boolean) => {
		if (!o) setOpen(null);
	};
	return (
		<ToastProvider label={t("ui.notifications")}>
			{/* The toolbar sits at the window top, where a sheet slides out from under it. */}
			<header>
				<MainToolbar airGap />
			</header>
			<main className="cp cp-overlays">
				{TITLE}
				<div className="cp-row">
					<Popover
						trigger={<Button id="ov-popover">{x.popover}</Button>}
						arrow
						aria-label={x.popoverTitle}
					>
						<div className="cp-stack">
							<Text variant="headline">{x.popoverTitle}</Text>
							<Text tone="secondary">{x.popoverBody}</Text>
							<Switch label={x.ocr} checked onCheckedChange={noop} />
						</div>
					</Popover>
					<Menu
						items={MENU_ITEMS}
						trigger={
							<Button id="ov-menu" icon={SlidersHorizontal}>
								{x.menu}
							</Button>
						}
					/>
					<Select
						id="ov-select"
						label={x.sortBy}
						options={SORT_OPTIONS}
						value="modified"
						onValueChange={noop}
					/>
					<SourceChip
						id="ov-tooltip"
						index={1}
						label={x.chipShort}
						location={t("card.page", { page: 4 })}
						excerpt={x.excerpt}
						onOpen={noop}
					/>
					<Button id="ov-sheet" onClick={() => setOpen("sheet")}>
						{x.sheet}
					</Button>
					<Button id="ov-alert" onClick={() => setOpen("alert")}>
						{x.alert}
					</Button>
					<Button
						id="ov-alert-destructive"
						onClick={() => setOpen("alert-destructive")}
					>
						{x.alertDestructive}
					</Button>
					<Button id="ov-toast" onClick={() => setOpen("toast")}>
						{x.toast}
					</Button>
				</div>
				<ContextMenu items={MENU_ITEMS}>
					<div id="ov-context" className="cp-context-target">
						<FileText size={16} strokeWidth={1.5} aria-hidden />
						{x.contextTarget}
					</div>
				</ContextMenu>
			</main>
			<Sheet
				open={open === "sheet"}
				onOpenChange={close}
				title={x.sheetTitle}
				description={x.sheetBody.replace("{app}", APP_NAME)}
				footer={
					<>
						<Button onClick={() => setOpen(null)}>{t("action.cancel")}</Button>
						<Button variant="primary">{x.addFolder}</Button>
					</>
				}
			>
				<TextField label={x.name} defaultValue={x.nameValue} />
			</Sheet>
			<Alert
				open={open === "alert"}
				onOpenChange={close}
				title={x.alertTitle}
				message={x.alertBody}
				confirmLabel={x.rebuild}
				cancelLabel={t("action.cancel")}
				onConfirm={noop}
				tone="default"
			/>
			<Alert
				open={open === "alert-destructive"}
				onOpenChange={close}
				icon={Trash2}
				title={x.deleteTitle}
				message={x.deleteBody}
				confirmLabel={x.delete}
				cancelLabel={t("action.cancel")}
				onConfirm={noop}
				tone="destructive"
			/>
			<Toast
				open={open === "toast"}
				onOpenChange={close}
				title={x.toastTitle}
				description={x.toastBody}
				action={{ label: x.undo, altText: x.undoAlt, onClick: noop }}
				closeLabel={t("ui.close")}
			/>
		</ToastProvider>
	);
}

const TITLE = (
	<Text as="h1" variant="title2">
		Components
	</Text>
);

export function ComponentsPage({ overlays = false }: { overlays?: boolean }) {
	return overlays ? (
		<Overlays />
	) : (
		<main className="cp">
			{TITLE}
			<AllComponents />
		</main>
	);
}
