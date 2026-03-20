import type { Filter, FilterType } from "../filter.svelte";
import type { SearchFormState, ValidationIssue } from "./types";

export function createDefaultSearchForm(): SearchFormState {
    return {
        query: "",
        extensionInput: "",
        pathContainsInput: "",
        pathPrefix: "",
        scopeMode: "all",
        scopeDrive: "ALL",
        scopeFolders: [],
        kind: "any",
        includeHidden: false,
        readonlyOnly: false,
        sizeMin: "",
        sizeMinUnit: "MB",
        sizeMax: "",
        sizeMaxUnit: "MB",
        modifiedFrom: "",
        modifiedTo: "",
        createdFrom: "",
        createdTo: "",
    };
}

export function resolvePreferredDrive(roots: string[]): string {
    const normalizedPreferred = roots.find((root) => {
        const normalized = root.trim().replace(/\//g, "\\").toUpperCase();
        return normalized === "C:\\" || normalized === "C:";
    });

    if (normalizedPreferred) {
        return normalizedPreferred;
    }

    return roots[0] ?? "C:\\";
}

export function parseMultiValueInput(value: string): string[] {
    return value
        .split(/[\n,;]+/)
        .map((token) => token.trim())
        .filter((token) => token.length > 0);
}

export function normalizeExtension(value: string): string {
    const trimmed = value.trim().toLowerCase();
    if (!trimmed) return "";
    return trimmed.startsWith(".") ? trimmed : `.${trimmed}`;
}

export function parseExtensionTokens(value: string): string[] {
    const normalized = parseMultiValueInput(value)
        .map((token) => normalizeExtension(token))
        .filter((token) => token.length > 0);

    return Array.from(new Set(normalized));
}

export function parseSubfolderPaths(value: string): string[] {
    return value
        .split("\n")
        .map((v) => v.trim())
        .filter((v) => v.length > 0);
}

export function dedupePaths(paths: string[]): string[] {
    const seen = new Set<string>();
    const unique: string[] = [];

    for (const path of paths) {
        const normalized = path.trim();
        if (!normalized || seen.has(normalized)) continue;
        seen.add(normalized);
        unique.push(normalized);
    }

    return unique;
}

export function analyzeSearchForm(
    form: SearchFormState,
    options?: { enforceFolderScopeSelection?: boolean },
): ValidationIssue[] {
    const issues: ValidationIssue[] = [];
    const parseDate = (value: string): Date | null => {
        const trimmed = value.trim();
        if (!trimmed) {
            return null;
        }

        const parsed = new Date(trimmed);
        return isNaN(parsed.getTime()) ? null : parsed;
    };
    const toBytes = (value: string, unit: string = "B"): number => {
        const n = parseFloat(value);
        if (isNaN(n)) return -1;
        const map: Record<string, number> = {
            B: 1,
            KB: 1024,
            MB: 1024 ** 2,
            GB: 1024 ** 3,
        };
        return n * (map[unit] ?? 1);
    };

    if (form.scopeMode === "drive") {
        const selectedDrive = form.scopeDrive.trim();
        if (!selectedDrive || selectedDrive === "ALL") {
            issues.push({
                message: "Choose a specific drive or switch scope to All drives",
            });
        }
    }

    if (
        options?.enforceFolderScopeSelection &&
        form.scopeMode === "folder" &&
        form.scopeFolders.length === 0
    ) {
        issues.push({
            message: "Folder scope requires at least one selected folder",
        });
    }

    if (form.sizeMin.trim() && form.sizeMax.trim()) {
        const gtBytes = toBytes(form.sizeMin, form.sizeMinUnit);
        const ltBytes = toBytes(form.sizeMax, form.sizeMaxUnit);
        if (gtBytes !== -1 && ltBytes !== -1 && gtBytes >= ltBytes) {
            issues.push({
                message: "Minimum size must be smaller than maximum size",
            });
        }
    }

    const checkRange = (from: string, to: string, label: string) => {
        if (from.trim() && to.trim()) {
            const a = parseDate(from);
            const b = parseDate(to);
            if (a && b && a >= b) {
                issues.push({
                    message: `${label} start date must be earlier than end date`,
                });
            }
        }
    };

    checkRange(form.modifiedFrom, form.modifiedTo, "Modified");
    checkRange(form.createdFrom, form.createdTo, "Created");

    return issues;
}

function toDateRangeFilter(
    filters: Filter[],
    id: number,
    from: string,
    to: string,
    rangeType: Extract<FilterType, "modified_range" | "created_range">,
    afterType: Extract<FilterType, "modified_after" | "created_after">,
    beforeType: Extract<FilterType, "modified_before" | "created_before">,
): number {
    const hasFrom = from.trim().length > 0;
    const hasTo = to.trim().length > 0;

    if (hasFrom && hasTo) {
        filters.push({
            id,
            type: rangeType,
            value: from,
            value2: to,
        });
        return id + 1;
    }

    if (hasFrom) {
        filters.push({ id, type: afterType, value: from, value2: "" });
        id += 1;
    }

    if (hasTo) {
        filters.push({ id, type: beforeType, value: to, value2: "" });
        id += 1;
    }

    return id;
}

export function formToFilters(form: SearchFormState): Filter[] {
    const built: Filter[] = [];
    let id = 0;

    const push = (
        type: FilterType,
        value = "",
        value2 = "",
        unit?: string,
    ) => {
        built.push({ id: id++, type, value, value2, unit });
    };

    const queryValue = form.query.trim();
    if (queryValue) {
        push("name_contains", queryValue);
    }

    for (const extension of parseExtensionTokens(form.extensionInput)) {
        push("extension", extension);
    }

    for (const fragment of parseMultiValueInput(form.pathContainsInput)) {
        push("path_contains", fragment);
    }

    const pathPrefix = form.pathPrefix.trim();
    if (pathPrefix) {
        push("path_prefix", pathPrefix);
    }

    if (form.scopeMode === "drive") {
        push("drive", form.scopeDrive || "ALL");
    } else {
        push("drive", "ALL");
    }

    if (form.scopeMode === "folder" && form.scopeFolders.length > 0) {
        push("subfolder", dedupePaths(form.scopeFolders).join("\n"));
    }

    if (form.kind === "file") {
        push("file_only");
    } else if (form.kind === "folder") {
        push("folder_only");
    }

    if (form.includeHidden) {
        push("hidden");
    }

    if (form.readonlyOnly) {
        push("readonly");
    }

    const minSize = form.sizeMin.trim();
    if (minSize) {
        push("size_gt", minSize, "", form.sizeMinUnit || "B");
    }

    const maxSize = form.sizeMax.trim();
    if (maxSize) {
        push("size_lt", maxSize, "", form.sizeMaxUnit || "B");
    }

    id = toDateRangeFilter(
        built,
        id,
        form.modifiedFrom,
        form.modifiedTo,
        "modified_range",
        "modified_after",
        "modified_before",
    );

    toDateRangeFilter(
        built,
        id,
        form.createdFrom,
        form.createdTo,
        "created_range",
        "created_after",
        "created_before",
    );

    return built;
}

export function formFromFilters(filters: Filter[]): SearchFormState {
    const next = createDefaultSearchForm();
    const getFirst = (type: FilterType) =>
        filters.find((filter) => filter.type === type);
    const getMany = (type: FilterType) =>
        filters.filter((filter) => filter.type === type);

    next.query = getFirst("name_contains")?.value ?? "";
    next.extensionInput = getMany("extension")
        .map((filter) => filter.value)
        .join(", ");
    next.extensionInput = parseExtensionTokens(next.extensionInput).join(", ");
    next.pathContainsInput = getMany("path_contains")
        .map((filter) => filter.value.trim())
        .filter((value) => value.length > 0)
        .join(", ");
    next.pathPrefix = getFirst("path_prefix")?.value ?? "";

    const drive = (getFirst("drive")?.value ?? "ALL").trim();
    if (drive && drive !== "ALL") {
        next.scopeMode = "drive";
        next.scopeDrive = drive;
    }

    const folderPaths = dedupePaths(
        getMany("subfolder").flatMap((filter) =>
            parseSubfolderPaths(filter.value),
        ),
    );
    if (folderPaths.length > 0) {
        next.scopeMode = "folder";
        next.scopeFolders = folderPaths;
    }

    if (getMany("file_only").length > 0) {
        next.kind = "file";
    } else if (getMany("folder_only").length > 0) {
        next.kind = "folder";
    }

    next.includeHidden = getMany("hidden").length > 0;
    next.readonlyOnly = getMany("readonly").length > 0;

    const sizeMin = getFirst("size_gt");
    if (sizeMin) {
        next.sizeMin = sizeMin.value;
        next.sizeMinUnit = sizeMin.unit ?? "B";
    }

    const sizeMax = getFirst("size_lt");
    if (sizeMax) {
        next.sizeMax = sizeMax.value;
        next.sizeMaxUnit = sizeMax.unit ?? "B";
    }

    const modifiedRange = getFirst("modified_range");
    if (modifiedRange) {
        next.modifiedFrom = modifiedRange.value;
        next.modifiedTo = modifiedRange.value2;
    } else {
        next.modifiedFrom = getFirst("modified_after")?.value ?? "";
        next.modifiedTo = getFirst("modified_before")?.value ?? "";
    }

    const createdRange = getFirst("created_range");
    if (createdRange) {
        next.createdFrom = createdRange.value;
        next.createdTo = createdRange.value2;
    } else {
        next.createdFrom = getFirst("created_after")?.value ?? "";
        next.createdTo = getFirst("created_before")?.value ?? "";
    }

    return next;
}
