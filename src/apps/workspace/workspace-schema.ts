import type { WorkspaceToolJson, WorkspaceToolKind } from "./workspace-tools";

export const WORKSPACE_BOLT_FILE_NAME = "workspace.bolt.json";
export const WORKSPACE_BOLT_SCHEMA_KIND = "workspace" as const;
export const WORKSPACE_BOLT_SCHEMA_VERSION = 1 as const;

export type WorkspaceBoltJson = {
    kind: typeof WORKSPACE_BOLT_SCHEMA_KIND;
    schemaVersion: typeof WORKSPACE_BOLT_SCHEMA_VERSION;
    name: string;
    createdAt: string;
    updatedAt: string;
    tools: WorkspaceToolJson[];
};

const WORKSPACE_TOOL_KINDS: WorkspaceToolKind[] = [
    "todo_board",
    "kanban_board",
    "youtube_player",
    "media_library",
];

function isRecord(value: unknown): value is Record<string, unknown> {
    return typeof value === "object" && value !== null && !Array.isArray(value);
}

function requireString(
    source: Record<string, unknown>,
    key: keyof WorkspaceBoltJson,
): string {
    const value = source[key];
    if (typeof value !== "string") {
        throw new Error(`workspace.bolt.json: \"${key}\" must be a string`);
    }

    const trimmed = value.trim();
    if (!trimmed) {
        throw new Error(`workspace.bolt.json: \"${key}\" cannot be empty`);
    }

    return trimmed;
}

function parseIsoDate(dateValue: string, fieldName: "createdAt" | "updatedAt"): Date {
    const parsed = new Date(dateValue);
    if (Number.isNaN(parsed.getTime())) {
        throw new Error(`workspace.bolt.json: \"${fieldName}\" must be an ISO date string`);
    }

    return parsed;
}

function parseTools(value: unknown): WorkspaceToolJson[] {
    if (!Array.isArray(value)) {
        throw new Error('workspace.bolt.json: "tools" must be an array');
    }

    return value.map((entry, index) => {
        if (!isRecord(entry)) {
            throw new Error(`workspace.bolt.json: tools[${index}] must be an object`);
        }

        const id = typeof entry.id === "string" ? entry.id.trim() : "";
        const kind = typeof entry.kind === "string" ? entry.kind.trim() : "";
        const name = typeof entry.name === "string" ? entry.name.trim() : "";
        const createdAtRaw = typeof entry.createdAt === "string" ? entry.createdAt.trim() : "";
        const config = isRecord(entry.config) ? entry.config : null;

        if (!id) {
            throw new Error(`workspace.bolt.json: tools[${index}].id cannot be empty`);
        }

        if (!WORKSPACE_TOOL_KINDS.includes(kind as WorkspaceToolKind)) {
            throw new Error(`workspace.bolt.json: tools[${index}].kind is not supported`);
        }

        if (!name) {
            throw new Error(`workspace.bolt.json: tools[${index}].name cannot be empty`);
        }

        if (!createdAtRaw) {
            throw new Error(`workspace.bolt.json: tools[${index}].createdAt cannot be empty`);
        }

        parseIsoDate(createdAtRaw, "createdAt");

        if (!config) {
            throw new Error(`workspace.bolt.json: tools[${index}].config must be an object`);
        }

        return {
            id,
            kind: kind as WorkspaceToolKind,
            name,
            createdAt: new Date(createdAtRaw).toISOString(),
            config,
        };
    });
}

export class WorkspaceManifest {
    readonly kind = WORKSPACE_BOLT_SCHEMA_KIND;
    readonly schemaVersion = WORKSPACE_BOLT_SCHEMA_VERSION;
    readonly name: string;
    readonly createdAt: Date;
    readonly updatedAt: Date;
    readonly tools: WorkspaceToolJson[];

    constructor(params: {
        name: string;
        createdAt?: Date;
        updatedAt?: Date;
        tools?: WorkspaceToolJson[];
    }) {
        const normalizedName = params.name.trim();
        if (!normalizedName) {
            throw new Error("Workspace name cannot be empty");
        }

        this.name = normalizedName;
        this.createdAt = params.createdAt ?? new Date();
        this.updatedAt = params.updatedAt ?? this.createdAt;
        this.tools = params.tools ?? [];

        if (this.updatedAt.getTime() < this.createdAt.getTime()) {
            throw new Error("workspace.bolt.json: updatedAt cannot be earlier than createdAt");
        }
    }

    static create(name: string): WorkspaceManifest {
        return new WorkspaceManifest({ name });
    }

    toJSON(): WorkspaceBoltJson {
        return {
            kind: this.kind,
            schemaVersion: this.schemaVersion,
            name: this.name,
            createdAt: this.createdAt.toISOString(),
            updatedAt: this.updatedAt.toISOString(),
            tools: this.tools,
        };
    }

    static parse(input: unknown): WorkspaceManifest {
        const raw =
            typeof input === "string"
                ? (() => {
                      try {
                          return JSON.parse(input) as unknown;
                      } catch (error) {
                          const message = error instanceof Error ? error.message : "Invalid JSON";
                          throw new Error(`workspace.bolt.json: ${message}`);
                      }
                  })()
                : input;

        if (!isRecord(raw)) {
            throw new Error("workspace.bolt.json: expected a JSON object");
        }

        if (raw.kind !== WORKSPACE_BOLT_SCHEMA_KIND) {
            throw new Error(`workspace.bolt.json: \"kind\" must be \"${WORKSPACE_BOLT_SCHEMA_KIND}\"`);
        }

        if (raw.schemaVersion !== WORKSPACE_BOLT_SCHEMA_VERSION) {
            throw new Error(
                `workspace.bolt.json: unsupported schemaVersion \"${String(raw.schemaVersion)}\"`,
            );
        }

        const name = requireString(raw, "name");
        const createdAtRaw = requireString(raw, "createdAt");
        const updatedAtRaw = requireString(raw, "updatedAt");
        const createdAt = parseIsoDate(createdAtRaw, "createdAt");
        const updatedAt = parseIsoDate(updatedAtRaw, "updatedAt");
        const tools = parseTools(raw.tools ?? []);

        return new WorkspaceManifest({
            name,
            createdAt,
            updatedAt,
            tools,
        });
    }
}
