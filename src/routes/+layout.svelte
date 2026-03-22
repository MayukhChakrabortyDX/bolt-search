<script lang="ts">
	import './layout.css';
	import { onMount } from 'svelte';
	import { getCurrentWindow } from '@tauri-apps/api/window';
	import { FileDown, Minus, Moon, Save, Square, Sun, X } from 'lucide-svelte';

	const { children } = $props();
	const appWindow = getCurrentWindow();
	let isDarkMode = $state(false);
	let streamingEnabled = $state(true);
	let intentEnabled = $state(false);
	let dragRegionEl: HTMLDivElement | null = null;
	type ThemePreference = 'system' | 'light' | 'dark';
	let themePreference: ThemePreference = 'system';
	let themeMediaQuery: MediaQueryList | null = null;

	function applyTheme(darkMode: boolean) {
		if (typeof document === 'undefined') return;
		document.documentElement.dataset.theme = darkMode ? 'dark' : 'light';
		document.documentElement.classList.toggle('dark', darkMode);
	}

	function resolveDarkMode(): boolean {
		if (themePreference === 'dark') return true;
		if (themePreference === 'light') return false;
		return themeMediaQuery?.matches ?? false;
	}

	function applyResolvedTheme() {
		isDarkMode = resolveDarkMode();
		applyTheme(isDarkMode);
	}

	function syncStreamingPreference(enabled: boolean) {
		syncModePreferences({ streaming: enabled, intent: enabled ? false : intentEnabled });
	}

	function syncIntentPreference(enabled: boolean) {
		syncModePreferences({ streaming: enabled ? false : streamingEnabled, intent: enabled });
 	}

	function syncModePreferences(next: { streaming: boolean; intent: boolean }) {
		streamingEnabled = next.streaming;
		intentEnabled = next.intent;

		localStorage.setItem('bolt-search-streaming-enabled', streamingEnabled ? '1' : '0');
		localStorage.setItem('bolt-search-intent-enabled', intentEnabled ? '1' : '0');

		window.dispatchEvent(
			new CustomEvent('bolt-streaming-mode-changed', {
				detail: { enabled: streamingEnabled },
			})
		);
		window.dispatchEvent(
			new CustomEvent('bolt-intent-mode-changed', {
				detail: { enabled: intentEnabled },
			})
		);
	}

	onMount(() => {
		let hasShownWindow = false;

		const showWindowOnce = () => {
			if (hasShownWindow) return;
			hasShownWindow = true;

			void (async () => {
				await new Promise<void>((resolve) => {
					requestAnimationFrame(() => {
						requestAnimationFrame(() => resolve());
					});
				});

				await appWindow.show().catch((error) => {
					console.error('Failed to show startup window:', error);
				});
			})();
		};

		const onUiReady = () => {
			showWindowOnce();
		};

		window.addEventListener('bolt-ui-ready', onUiReady, { once: true });

		themeMediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
		const stored = localStorage.getItem('bolt-search-theme-preference');
		if (stored === 'dark' || stored === 'light' || stored === 'system') {
			themePreference = stored;
		}

		const onSystemThemeChange = () => {
			if (themePreference === 'system') {
				applyResolvedTheme();
			}
		};

		themeMediaQuery.addEventListener('change', onSystemThemeChange);
		applyResolvedTheme();

		const storedStreaming = localStorage.getItem('bolt-search-streaming-enabled');
		streamingEnabled = !(storedStreaming === '0' || storedStreaming === 'false');
		const storedIntent = localStorage.getItem('bolt-search-intent-enabled');
		intentEnabled = storedIntent === '1' || storedIntent === 'true';
		if (intentEnabled) {
			streamingEnabled = false;
		}
		localStorage.removeItem('bolt-search-backend-mode');
		syncModePreferences({ streaming: streamingEnabled, intent: intentEnabled });

		void appWindow.setContentProtected(false).catch(() => {
			// Ignore permission/platform failures; app remains usable.
		});

		if (dragRegionEl) {
			dragRegionEl.addEventListener('mousedown', startDrag);
		}

		return () => {
			window.removeEventListener('bolt-ui-ready', onUiReady);
			themeMediaQuery?.removeEventListener('change', onSystemThemeChange);
			themeMediaQuery = null;
			if (dragRegionEl) {
				dragRegionEl.removeEventListener('mousedown', startDrag);
			}
		};
	});

	function toggleDarkMode() {
		themePreference = isDarkMode ? 'light' : 'dark';
		localStorage.setItem('bolt-search-theme-preference', themePreference);
		applyResolvedTheme();
	}

	function toggleStreamingMode() {
		syncStreamingPreference(!streamingEnabled);
	}

	function toggleIntentMode() {
		syncIntentPreference(!intentEnabled);
	}

	async function startDrag(event: MouseEvent) {
		if (event.button !== 0) return;
		const target = event.target as HTMLElement | null;
		if (target?.closest('.window-controls')) return;

		try {
			await appWindow.startDragging();
		} catch {
			// Ignore drag errors; data-tauri-drag-region remains as fallback.
		}
	}

	async function minimizeWindow() {
		await appWindow.minimize();
	}

	async function toggleMaximizeWindow() {
		await appWindow.toggleMaximize();
	}

	async function closeWindow() {
		await appWindow.close();
	}

	function requestSaveFilter() {
		window.dispatchEvent(new CustomEvent('bolt-save-filter'));
	}

	function requestLoadFilter() {
		window.dispatchEvent(new CustomEvent('bolt-load-filter'));
	}
</script>

<div class="app-shell">
	<header class="window-titlebar" data-tauri-drag-region>
		<div class="window-drag-region" data-tauri-drag-region bind:this={dragRegionEl}>
			<span class="window-title">Bolt Search Software</span>
		</div>
		<div class="window-controls">
			<button
				class="window-control-button topbar-action"
				type="button"
				aria-label="Save Filter"
				onclick={requestSaveFilter}
				title="Save Filter"
			>
				<Save size={13} strokeWidth={2} />
			</button>
			<button
				class="window-control-button topbar-action"
				type="button"
				aria-label="Load Filter"
				onclick={requestLoadFilter}
				title="Load Filter"
			>
				<FileDown size={13} strokeWidth={2} />
			</button>
			<button
				class="window-control-button topbar-toggle"
				type="button"
				aria-label={streamingEnabled ? 'Disable streaming mode' : 'Enable streaming mode'}
				title={streamingEnabled ? 'Streaming mode enabled' : 'Streaming mode disabled'}
				onclick={toggleStreamingMode}
			>
				<span class={`streaming-indicator ${streamingEnabled ? 'on' : ''}`}></span>
				<span class="topbar-toggle-label">Stream</span>
			</button>
			<button
				class="window-control-button topbar-toggle"
				type="button"
				aria-label={intentEnabled ? 'Disable intent explorer mode' : 'Enable intent explorer mode'}
				title={intentEnabled ? 'Intent explorer enabled' : 'Intent explorer disabled'}
				onclick={toggleIntentMode}
			>
				<span class={`streaming-indicator ${intentEnabled ? 'on' : ''}`}></span>
				<span class="topbar-toggle-label">Intent</span>
			</button>
			<button
				class="window-control-button theme-toggle"
				type="button"
				aria-label={isDarkMode ? 'Switch to light mode' : 'Switch to dark mode'}
				onclick={toggleDarkMode}
			>
				{#if isDarkMode}
					<Sun size={14} strokeWidth={2} />
				{:else}
					<Moon size={14} strokeWidth={2} />
				{/if}
			</button>
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
