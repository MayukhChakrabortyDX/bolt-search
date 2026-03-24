import {
    type SearchFormState,
    type ValidationIssue,
} from "./page-types";

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

function joinWindowsPath(root: string, child: string): string {
    const base = root.trim().replace(/[\\/]+$/g, "");
    return `${base}\\${child}`;
}

export function defaultExcludedSystemFolders(roots: string[]): string[] {
    const normalizedRoots = dedupePaths(
        roots
            .map((root) => root.trim())
            .filter((root) => root.length > 0),
    );

    if (normalizedRoots.length === 0) {
        return [];
    }

    const perDriveFolders = ["$Recycle.Bin", "System Volume Information"];
    const preferredDrive = resolvePreferredDrive(normalizedRoots);
    const preferredDriveFolders = [
        "Windows",
        "Program Files",
        "Program Files (x86)",
        "ProgramData",
        "Recovery",
        "PerfLogs",
    ];

    const excluded: string[] = [];

    for (const root of normalizedRoots) {
        for (const child of perDriveFolders) {
            excluded.push(joinWindowsPath(root, child));
        }
    }

    for (const child of preferredDriveFolders) {
        excluded.push(joinWindowsPath(preferredDrive, child));
    }

    return dedupePaths(excluded);
}

export function normalizePath(path: string): string {
    return path.replace(/\\/g, "/").replace(/\/+/g, "/").replace(/\/$/, "");
}

export function displayPath(path: string): string {
    return path.replace(/\//g, "\\");
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

