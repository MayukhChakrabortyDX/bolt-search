<script lang="ts">
	import './layout.css';
	import { getCurrentWindow } from '@tauri-apps/api/window';
	import { Minus, Square, X } from 'lucide-svelte';

	const { children } = $props();
	const appWindow = getCurrentWindow();

	async function minimizeWindow() {
		await appWindow.minimize();
	}

	async function toggleMaximizeWindow() {
		await appWindow.toggleMaximize();
	}

	async function closeWindow() {
		await appWindow.close();
	}
</script>

<div class="app-shell">
	<header class="window-titlebar">
		<div class="window-drag-region" data-tauri-drag-region>
			<span class="window-dot" aria-hidden="true"></span>
			<span class="window-title">Bolt Search</span>
		</div>
		<div class="window-controls">
			<button class="window-control-button" type="button" aria-label="Minimize" onclick={minimizeWindow}>
				<Minus size={14} strokeWidth={2} />
			</button>
			<button class="window-control-button" type="button" aria-label="Maximize" onclick={toggleMaximizeWindow}>
				<Square size={12} strokeWidth={2} />
			</button>
			<button class="window-control-button danger" type="button" aria-label="Close" onclick={closeWindow}>
				<X size={14} strokeWidth={2} />
			</button>
		</div>
	</header>

	<main class="app-content">
		{@render children()}
	</main>
</div>
