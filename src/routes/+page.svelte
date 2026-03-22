<script lang="ts">
    import { getRegisteredApp } from "../apps/registry";
    import { globalState } from "../global.svelte";

    const activeApp = $derived(
        getRegisteredApp(globalState.activeApp),
    );
    const ActiveAppComponent = $derived(activeApp.component);
</script>

<ActiveAppComponent />
