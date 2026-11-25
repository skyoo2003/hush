// SPDX-License-Identifier: GPL-3.0-or-later
import type { ElementType, ReactNode } from "react";
import "./Text.css";

/** The macOS text styles in DESIGN.md; sizes live only in tokens.css. */
export const TEXT_VARIANTS = [
	"large-title",
	"title1",
	"title2",
	"title3",
	"headline",
	"body",
	"callout",
	"subheadline",
	"footnote",
] as const;
export type TextVariant = (typeof TEXT_VARIANTS)[number];
export type TextTone = "primary" | "secondary" | "tertiary";

type Props = {
	variant?: TextVariant;
	tone?: TextTone;
	/** The element to render, `span` by default. */
	as?: ElementType;
	/** Content text (snippets, answers) that the user may select. */
	selectable?: boolean;
	id?: string;
	children: ReactNode;
};

export function Text({
	variant = "body",
	tone = "primary",
	as: Tag = "span",
	selectable,
	id,
	children,
}: Props) {
	return (
		<Tag
			id={id}
			className={`text text-${variant} tone-${tone}`}
			data-selectable={selectable || undefined}
		>
			{children}
		</Tag>
	);
}
