// SPDX-License-Identifier: GPL-3.0-or-later
import { useEffect, useState } from "react";

/** A number kept in localStorage, such as a sidebar width: UI geometry, never document data. */
export function useStoredNumber(key: string, fallback: number) {
	const [value, setValue] = useState(() => {
		const n = Number(localStorage.getItem(key));
		return Number.isFinite(n) && n > 0 ? n : fallback;
	});
	useEffect(() => localStorage.setItem(key, String(value)), [key, value]);
	return [value, setValue] as const;
}
