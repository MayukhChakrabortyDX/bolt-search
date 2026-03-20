export type CalendarCell = {
    iso: string;
    day: number;
    inCurrentMonth: boolean;
    isToday: boolean;
};

export const WEEKDAY_LABELS = ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"];

const MIN_YEAR = 1950;
const MAX_YEAR = new Date().getFullYear();
const monthLabelFormatter = new Intl.DateTimeFormat(undefined, {
    month: "short",
});

export const dateLabelFormatter = new Intl.DateTimeFormat(undefined, {
    year: "numeric",
    month: "short",
    day: "2-digit",
});

export const MONTH_OPTIONS = Array.from({ length: 12 }, (_, monthIndex) => ({
    value: monthIndex,
    label: monthLabelFormatter.format(new Date(2000, monthIndex, 1)),
}));

export const YEAR_OPTIONS = Array.from(
    { length: MAX_YEAR - MIN_YEAR + 1 },
    (_, index) => MIN_YEAR + index,
);

function pad2(value: number): string {
    return String(value).padStart(2, "0");
}

export function formatIsoDate(date: Date): string {
    return `${date.getFullYear()}-${pad2(date.getMonth() + 1)}-${pad2(date.getDate())}`;
}

export function parseIsoDate(raw: string): Date | null {
    const match = raw.trim().match(/^(\d{4})-(\d{2})-(\d{2})$/);
    if (!match) return null;

    const year = Number(match[1]);
    const monthIndex = Number(match[2]) - 1;
    const day = Number(match[3]);
    const parsed = new Date(year, monthIndex, day);

    if (
        parsed.getFullYear() !== year ||
        parsed.getMonth() !== monthIndex ||
        parsed.getDate() !== day
    ) {
        return null;
    }

    return parsed;
}

export function startOfMonth(date: Date): Date {
    return new Date(date.getFullYear(), date.getMonth(), 1);
}

function clampYear(year: number): number {
    return Math.max(MIN_YEAR, Math.min(MAX_YEAR, year));
}

export function clampMonthToBounds(date: Date): Date {
    return new Date(clampYear(date.getFullYear()), date.getMonth(), 1);
}

function isSameDate(a: Date, b: Date): boolean {
    return (
        a.getFullYear() === b.getFullYear() &&
        a.getMonth() === b.getMonth() &&
        a.getDate() === b.getDate()
    );
}

export function buildCalendarCells(month: Date): CalendarCell[] {
    const year = month.getFullYear();
    const monthIndex = month.getMonth();
    const firstOfMonth = new Date(year, monthIndex, 1);
    const gridStart = new Date(year, monthIndex, 1 - firstOfMonth.getDay());
    const today = new Date();
    const cells: CalendarCell[] = [];

    for (let offset = 0; offset < 42; offset += 1) {
        const date = new Date(gridStart);
        date.setDate(gridStart.getDate() + offset);
        cells.push({
            iso: formatIsoDate(date),
            day: date.getDate(),
            inCurrentMonth: date.getMonth() === monthIndex,
            isToday: isSameDate(date, today),
        });
    }

    return cells;
}
