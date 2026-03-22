export type AppId = "search" | "download-master";

export const globalState = $state({
    activeApp: "search" as AppId,
});

export function isAppId(value: string): value is AppId {
    return value === "search" || value === "download-master";
}

export function setActiveApp(next: AppId): void {
    globalState.activeApp = next;
}
