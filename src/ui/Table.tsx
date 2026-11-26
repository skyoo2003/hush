// SPDX-License-Identifier: GPL-3.0-or-later
import { ChevronDown, ChevronUp } from "lucide-react";
import type { ComponentPropsWithRef, ReactNode } from "react";
import "./Table.css";

export type TableSort = {
	key: string;
	direction: "ascending" | "descending";
};

export type TableColumn<T> = {
	key: string;
	header: string;
	sortable?: boolean;
	align?: "start" | "end";
	width?: string;
	render(row: T): ReactNode;
};

export type TableProps<T> = Omit<ComponentPropsWithRef<"table">, "children"> & {
	label: string;
	rows: readonly T[];
	rowKey(row: T): string;
	columns: readonly TableColumn<T>[];
	/** Controlled: the caller sorts `rows`. */
	sort: TableSort | null;
	onSortChange(sort: TableSort): void;
};

export function Table<T>({
	label,
	rows,
	rowKey,
	columns,
	sort,
	onSortChange,
	className,
	...rest
}: TableProps<T>) {
	return (
		<table
			{...rest}
			aria-label={label}
			className={`table${className ? ` ${className}` : ""}`}
		>
			<thead>
				<tr>
					{columns.map((col) => {
						const direction =
							sort?.key === col.key ? sort.direction : undefined;
						const Chevron =
							direction === "descending" ? ChevronDown : ChevronUp;
						return (
							<th
								key={col.key}
								scope="col"
								aria-sort={direction}
								data-align={col.align}
								style={col.width ? { width: col.width } : undefined}
							>
								{col.sortable ? (
									<button
										type="button"
										className="table-sort"
										onClick={() =>
											onSortChange({
												key: col.key,
												direction:
													direction === "ascending"
														? "descending"
														: "ascending",
											})
										}
									>
										{col.header}
										{direction && (
											<Chevron size={12} strokeWidth={2} aria-hidden />
										)}
									</button>
								) : (
									col.header
								)}
							</th>
						);
					})}
				</tr>
			</thead>
			<tbody>
				{rows.map((row) => (
					<tr key={rowKey(row)}>
						{columns.map((col) => (
							<td key={col.key} data-align={col.align}>
								{col.render(row)}
							</td>
						))}
					</tr>
				))}
			</tbody>
		</table>
	);
}
