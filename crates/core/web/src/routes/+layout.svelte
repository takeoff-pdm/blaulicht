<script lang="ts">
	import '../app.css';

	import * as Sidebar from '$lib/components/ui/sidebar/index.js';
	import AppSidebar from '$lib/components/app-sidebar.svelte';

	// import type { LayoutLoad } from '@sveltejs/kit';
	// import type { Data } from '$lib/types';

	// let { children } = $props();

	import type { LayoutProps } from './$types';

	let { data, children }: LayoutProps = $props();
</script>


<Sidebar.Provider>
	<AppSidebar />
	<main class="w-full">
		<!-- <Sidebar.Trigger /> -->
		{@render children?.()}
	</main>
</Sidebar.Provider>
