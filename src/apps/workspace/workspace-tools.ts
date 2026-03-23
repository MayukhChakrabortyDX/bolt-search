export type WorkspaceToolKind =
    | "todo_board"
    | "kanban_board"
    | "youtube_player"
    | "media_library";

export type WorkspaceToolJson = {
    id: string;
    kind: WorkspaceToolKind;
    name: string;
    createdAt: string;
    config: Record<string, unknown>;
};

export const BOLT_INTERNAL_DIR = ".bolt";
export const BOLT_TOOLS_DIR = `${BOLT_INTERNAL_DIR}/tools`;
export const BOLT_TOOLS_INDEX_FILE = `${BOLT_TOOLS_DIR}/index.json`;

function createToolId(prefix: string): string {
    const random = Math.random().toString(36).slice(2, 8);
    return `${prefix}-${Date.now()}-${random}`;
}

abstract class WorkspaceToolModule {
    readonly kind: WorkspaceToolKind;
    readonly title: string;
    readonly description: string;

    protected constructor(params: {
        kind: WorkspaceToolKind;
        title: string;
        description: string;
    }) {
        this.kind = params.kind;
        this.title = params.title;
        this.description = params.description;
    }

    protected abstract defaultConfig(): Record<string, unknown>;

    createEntity(nameOverride?: string): WorkspaceToolJson {
        return {
            id: createToolId(this.kind),
            kind: this.kind,
            name: (nameOverride ?? this.title).trim(),
            createdAt: new Date().toISOString(),
            config: this.defaultConfig(),
        };
    }
}

class TodoBoardToolModule extends WorkspaceToolModule {
    constructor() {
        super({
            kind: "todo_board",
            title: "Todo Board",
            description: "Track actionable tasks and daily priorities.",
        });
    }

    protected defaultConfig(): Record<string, unknown> {
        return {
            defaultView: "list",
            statuses: ["todo", "doing", "done"],
        };
    }
}

class KanbanBoardToolModule extends WorkspaceToolModule {
    constructor() {
        super({
            kind: "kanban_board",
            title: "Kanban Board",
            description: "Plan workflow with columns and cards.",
        });
    }

    protected defaultConfig(): Record<string, unknown> {
        return {
            columns: ["Backlog", "In Progress", "Review", "Done"],
            wipLimit: 0,
        };
    }
}

class YouTubePlayerToolModule extends WorkspaceToolModule {
    constructor() {
        super({
            kind: "youtube_player",
            title: "YouTube Player",
            description: "Save and play useful learning videos inside the workspace.",
        });
    }

    protected defaultConfig(): Record<string, unknown> {
        return {
            playlists: [],
            autoplay: false,
        };
    }
}

class MediaLibraryToolModule extends WorkspaceToolModule {
    constructor() {
        super({
            kind: "media_library",
            title: "Media Library",
            description: "Organize local media assets and references.",
        });
    }

    protected defaultConfig(): Record<string, unknown> {
        return {
            folders: [],
            showPreviews: true,
        };
    }
}

export const WORKSPACE_TOOL_CATALOG = [
    new TodoBoardToolModule(),
    new KanbanBoardToolModule(),
    new YouTubePlayerToolModule(),
    new MediaLibraryToolModule(),
] as const;

export function findWorkspaceToolModule(kind: WorkspaceToolKind): (typeof WORKSPACE_TOOL_CATALOG)[number] {
    const tool = WORKSPACE_TOOL_CATALOG.find((entry) => entry.kind === kind);
    if (!tool) {
        throw new Error(`Unknown workspace tool kind: ${kind}`);
    }

    return tool;
}

export function listMissingToolKinds(installed: WorkspaceToolJson[]): WorkspaceToolKind[] {
    const installedKinds = new Set(installed.map((tool) => tool.kind));
    return WORKSPACE_TOOL_CATALOG.filter((tool) => !installedKinds.has(tool.kind)).map(
        (tool) => tool.kind,
    );
}

export function toolStorageFilePath(tool: WorkspaceToolJson): string {
    return `${BOLT_TOOLS_DIR}/${tool.id}.tool.json`;
}

export function isBoltInternalPath(relativePath: string): boolean {
    const normalized = relativePath.trim().replace(/\\/g, "/").replace(/^\/+/, "");
    return normalized === BOLT_INTERNAL_DIR || normalized.startsWith(`${BOLT_INTERNAL_DIR}/`);
}
