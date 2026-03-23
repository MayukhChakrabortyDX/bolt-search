export type FolderContextMenuAction = "preview-folder" | "open-in-explorer";

export type FolderContextMenuTarget = {
    path: string;
    name: string;
};

export type FolderContextMenuHandlers = {
    onPreviewFolder: (target: FolderContextMenuTarget) => void | Promise<void>;
    onOpenInExplorer: (target: FolderContextMenuTarget) => void | Promise<void>;
};

export function createFolderContextMenuController() {
    const state = $state({
        open: false,
        x: 0,
        y: 0,
        target: null as FolderContextMenuTarget | null,
    });

    function close(): void {
        state.open = false;
        state.target = null;
    }

    function open(event: MouseEvent, target: FolderContextMenuTarget): void {
        event.preventDefault();
        state.open = true;
        state.x = event.clientX;
        state.y = event.clientY;
        state.target = target;
    }

    async function select(
        action: FolderContextMenuAction,
        handlers: FolderContextMenuHandlers,
    ): Promise<void> {
        const target = state.target;
        close();

        if (!target) {
            return;
        }

        if (action === "preview-folder") {
            await handlers.onPreviewFolder(target);
            return;
        }

        await handlers.onOpenInExplorer(target);
    }

    return {
        state,
        open,
        close,
        select,
    };
}
