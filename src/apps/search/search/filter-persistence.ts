import { open, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { FilterModel, type Filter } from "../filter.svelte";
import { formFromFilters } from "./form-mapping";
import type { SearchFormState } from "./page-types";

type SaveLoadHandlers = {
    getSearchFilters: () => Filter[];
    setStatus: (status: string) => void;
    setSearchForm: (form: SearchFormState) => void;
    clearSearchResults: () => void;
};

export async function saveFilterProfile(
    handlers: SaveLoadHandlers,
): Promise<void> {
    try {
        const selectedPath = await saveDialog({
            title: "Save Filter",
            defaultPath: "bolt-filter.bsearch",
            filters: [{ name: "Bolt Search Filter", extensions: ["bsearch"] }],
        });

        if (typeof selectedPath !== "string" || !selectedPath.trim()) {
            return;
        }

        const payload = JSON.stringify(
            FilterModel.toSavedFile(handlers.getSearchFilters()),
            null,
            2,
        );

        await invoke("save_filter_file", {
            path: selectedPath,
            content: payload,
        });

        handlers.setStatus(`Search profile saved: ${selectedPath}`);
    } catch (error) {
        console.error("Save filter failed:", error);
        handlers.setStatus("Save filter failed");
    }
}

export async function loadFilterProfile(
    handlers: SaveLoadHandlers,
): Promise<void> {
    try {
        const selectedPath = await open({
            title: "Load Filter",
            multiple: false,
            directory: false,
            filters: [{ name: "Bolt Search Filter", extensions: ["bsearch"] }],
        });

        if (typeof selectedPath !== "string" || !selectedPath.trim()) {
            return;
        }

        const content = await invoke<string>("load_filter_file", {
            path: selectedPath,
        });
        const saved = FilterModel.parseSavedFile(content);
        const loadedFilters = FilterModel.fromSavedFile(saved, 0);

        handlers.setSearchForm(formFromFilters(loadedFilters));
        handlers.clearSearchResults();
        handlers.setStatus(`Search profile loaded: ${selectedPath}`);
    } catch (error) {
        console.error("Load filter failed:", error);
        handlers.setStatus("Load filter failed");
    }
}
