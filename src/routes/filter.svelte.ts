export type FilterType =
    | "extension"
    | "name_contains"
    | "path_contains"
    | "subfolder"
    | "size_gt"
    | "size_lt"
    | "modified_after"
    | "modified_before"
    | "created_after"
    | "created_before"
    | "drive"
    | "hidden"
    | "readonly"
    | "file_only"
    | "folder_only";

export interface Filter {
    id: number;
    type: FilterType;
    value: string;
    unit?: string;
}

export interface FilterMeta {
    label: string;
    placeholder?: string;
    hasValue: boolean;
    isSize?: boolean;
}

export type QueryFilter = {
    type: FilterType;
    value?: string;
    unit?: string;
};

export type SavedFilterFile = {
    version: 1;
    filters: Array<{
        type: FilterType;
        value?: string;
        unit?: string;
    }>;
};

export class FilterModel {
    static readonly meta: Record<FilterType, FilterMeta> = {
        extension:       { label: "Extension",         placeholder: ".rs, .toml", hasValue: true },
        name_contains:   { label: "Name contains",     placeholder: "config",     hasValue: true },
        path_contains:   { label: "Path contains",     placeholder: "src/",       hasValue: true },
        subfolder:       { label: "Subfolder",                                     hasValue: true },
        size_gt:         { label: "Size greater than",                             hasValue: true, isSize: true },
        size_lt:         { label: "Size less than",                                hasValue: true, isSize: true },
        modified_after:  { label: "Modified after",                                hasValue: true },
        modified_before: { label: "Modified before",                               hasValue: true },
        created_after:   { label: "Created after",                                 hasValue: true },
        created_before:  { label: "Created before",                                hasValue: true },
        drive:           { label: "Drive",                                         hasValue: true },
        hidden:          { label: "Hidden files",                                  hasValue: false },
        readonly:        { label: "Read only",                                     hasValue: false },
        file_only:       { label: "Files only",                                    hasValue: false },
        folder_only:     { label: "Folders only",                                  hasValue: false },
    };

    static readonly stackableTypes: FilterType[] = ["extension", "path_contains"];
    static readonly allTypes: FilterType[] = [
        "extension",
        "name_contains",
        "path_contains",
        "subfolder",
        "size_gt",
        "size_lt",
        "modified_after",
        "modified_before",
        "created_after",
        "created_before",
        "drive",
        "hidden",
        "readonly",
        "file_only",
        "folder_only",
    ];

    static isFilterType(value: unknown): value is FilterType {
        return typeof value === "string" && this.allTypes.includes(value as FilterType);
    }

    static create(id: number, type: FilterType = "extension"): Filter {
        const filter: Filter = { id, type, value: "" };
        this.applyTypeDefaults(filter);
        return filter;
    }

    static applyTypeDefaults(filter: Filter): void {
        if (filter.type === "drive") {
            filter.value = filter.value || "ALL";
            return;
        }

        if (filter.type === "subfolder") {
            filter.value = filter.value || "";
            return;
        }

        if (!this.meta[filter.type].hasValue) {
            filter.value = "";
        }

        if (this.meta[filter.type].isSize && !filter.unit) {
            filter.unit = "B";
        }
    }

    static toQuery(filters: Filter[]): { filters: QueryFilter[] } {
        return {
            filters: filters.map(({ type, value, unit }) => ({
                type,
                ...(this.meta[type].hasValue ? {
                    value,
                    ...(this.meta[type].isSize ? { unit: unit ?? "B" } : {}),
                } : {}),
            })),
        };
    }

    static toSavedFile(filters: Filter[]): SavedFilterFile {
        return {
            version: 1,
            filters: filters.map(({ type, value, unit }) => ({
                type,
                ...(this.meta[type].hasValue ? { value } : {}),
                ...(this.meta[type].isSize && unit ? { unit } : {}),
            })),
        };
    }

    static fromSavedFile(saved: SavedFilterFile, startId = 0): Filter[] {
        return saved.filters.map((item, index) => {
            const filter: Filter = {
                id: startId + index,
                type: item.type,
                value: item.value ?? "",
                unit: item.unit,
            };
            this.applyTypeDefaults(filter);
            return filter;
        });
    }

    static parseSavedFile(content: string): SavedFilterFile {
        const parsed = JSON.parse(content) as unknown;

        if (!parsed || typeof parsed !== "object") {
            throw new Error("Invalid filter file: expected an object");
        }

        const candidate = parsed as Partial<SavedFilterFile>;
        if (candidate.version !== 1 || !Array.isArray(candidate.filters)) {
            throw new Error("Invalid filter file: unsupported version or malformed filters");
        }

        for (const item of candidate.filters) {
            if (!item || typeof item !== "object") {
                throw new Error("Invalid filter file: filter entry is malformed");
            }

            const typed = item as { type?: unknown; value?: unknown; unit?: unknown };
            if (!this.isFilterType(typed.type)) {
                throw new Error("Invalid filter file: unknown filter type");
            }
            if (typed.value !== undefined && typeof typed.value !== "string") {
                throw new Error("Invalid filter file: filter value must be a string");
            }
            if (typed.unit !== undefined && typeof typed.unit !== "string") {
                throw new Error("Invalid filter file: filter unit must be a string");
            }
        }

        return candidate as SavedFilterFile;
    }
}
