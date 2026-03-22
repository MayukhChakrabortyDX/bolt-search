import SearchApp from "./search/SearchApp.svelte";
import { type AppId, isAppId } from "../global.svelte";

export type RegisteredAppDefinition = {
    id: AppId;
    label: string;
    description: string;
    available: boolean;
    component: typeof SearchApp;
};

export const APP_REGISTRY: ReadonlyArray<RegisteredAppDefinition> = [
    {
        id: "search",
        label: "Search",
        description: "File and folder discovery with filters",
        available: true,
        component: SearchApp,
    },
];

export function getRegisteredApp(appId: string): RegisteredAppDefinition {
    if (isAppId(appId)) {
        const found = APP_REGISTRY.find((entry) => entry.id === appId);
        if (found) {
            return found;
        }
    }

    return APP_REGISTRY[0];
}
