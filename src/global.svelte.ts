export type AppId = "search";

export const globalState = $state({
    activeApp: "search" as AppId,
});

export function isAppId(value: string): value is AppId {
    return value === "search";
}

export function setActiveApp(next: AppId): void {
    globalState.activeApp = next;
}
