// SPDX-License-Identifier: GPL-3.0-or-later
// Settings pane layout, as in System Settings: titled groups of rows, a label on the left and
// its control on the right, with an optional note under the row.
import { type ReactNode, useId } from "react";
import { Text } from "../ui/Text";

export function Group({
	title,
	description,
	children,
}: {
	title: string;
	description?: ReactNode;
	children: ReactNode;
}) {
	const id = useId();
	return (
		<section className="form-group" aria-labelledby={id}>
			<Text as="h2" variant="headline" id={id}>
				{title}
			</Text>
			{description && <div className="form-group-description">{description}</div>}
			<div className="form-group-box">{children}</div>
		</section>
	);
}

export function Row({
	label,
	note,
	children,
}: {
	label?: ReactNode;
	note?: ReactNode;
	children?: ReactNode;
}) {
	return (
		<div className="form-row">
			<div className="form-row-main">
				{label !== undefined && <div className="form-row-label">{label}</div>}
				{children !== undefined && <div className="form-row-control">{children}</div>}
			</div>
			{note && <div className="form-row-note">{note}</div>}
		</div>
	);
}
