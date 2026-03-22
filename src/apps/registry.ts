import SearchApp from "./search/SearchApp.svelte";
import { type AppId, isAppId } from "../global.svelte";
import type { Component } from "svelte";

export type RegisteredAppDefinition = {
    id: AppId;
    label: string;
    description: string;
    available: boolean;
    component: Component;
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
