import type { ChipSelectOption } from "../../lib/components/ChipSelect.svelte";

export const MAX_RESULTS = 10_000;
export const WORKER_UI_DEBOUNCE_MS = 50;

export const SIZE_UNIT_OPTIONS: ChipSelectOption[] = [
    { value: "B", label: "B" },
    { value: "KB", label: "KB" },
    { value: "MB", label: "MB" },
    { value: "GB", label: "GB" },
];

export const POPULAR_EXTENSION_OPTIONS = [
    { value: ".pdf", label: "PDF" },
    { value: ".docx", label: "DOCX" },
    { value: ".xlsx", label: "XLSX" },
    { value: ".pptx", label: "PPTX" },
    { value: ".txt", label: "TXT" },
    { value: ".zip", label: "ZIP" },
    { value: ".jpg", label: "JPG" },
    { value: ".png", label: "PNG" },
    { value: ".mp4", label: "MP4" },
    { value: ".mp3", label: "MP3" },
    { value: ".js", label: "JS" },
    { value: ".ts", label: "TS" },
] as const;

export const DIR_INDENT_CLASSES = [
    "pl-2",
    "pl-[26px]",
    "pl-[44px]",
    "pl-[62px]",
    "pl-[80px]",
    "pl-[98px]",
    "pl-[116px]",
    "pl-[134px]",
    "pl-[152px]",
    "pl-[170px]",
    "pl-[188px]",
    "pl-[206px]",
    "pl-[224px]",
] as const;

export const FILE_INDENT_CLASSES = [
    "pl-5",
    "pl-[38px]",
    "pl-[56px]",
    "pl-[74px]",
    "pl-[92px]",
    "pl-[110px]",
    "pl-[128px]",
    "pl-[146px]",
    "pl-[164px]",
    "pl-[182px]",
    "pl-[200px]",
    "pl-[218px]",
    "pl-[236px]",
] as const;
