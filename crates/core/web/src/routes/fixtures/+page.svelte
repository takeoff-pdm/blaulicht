<script lang="ts">
	import StageEditor from '$lib/components/stage_editor/StageEditor.svelte';
	import * as Card from '$lib/components/ui/card';
	import { BlaulichtWebsocket, BlaulichtWebsocketCallbacks, topicControl } from '$lib/websocket';
	import { onMount } from 'svelte';
	import { getData } from '$lib/fetch';
	import type { Data } from '$lib/types/state';

	export let data: Data | null = null;

	export async function fetchDataDaemon() {
		data = await getData();
		setTimeout(fetchDataDaemon, 100);
	}

	let socket: BlaulichtWebsocket | null = null;

	async function sendControl(data: any) {
		console.log('Send control', data);
		socket!.send({
			kind: 'Control',
			value: data
		});
	}

	onMount(() => {
		const callbacks = new BlaulichtWebsocketCallbacks();

		callbacks.subscribe(topicControl(), (event) => {
			console.log('control', event);
		});

		const socketTemp = new BlaulichtWebsocket(callbacks);
		socket = socketTemp;

		fetchDataDaemon();
	});
</script>

<StageEditor state={data?.dmx_engine} on_control={sendControl}></StageEditor>
