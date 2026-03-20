import type { Filter, FilterType } from "../filter.svelte";
import { createDefaultSearchForm, type SearchFormState } from "./page-types";
import {
    dedupePaths,
    parseExtensionTokens,
    parseMultiValueInput,
    parseSubfolderPaths,
} from "./form-utils";

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

    const push = (type: FilterType, value = "", value2 = "", unit?: string) => {
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