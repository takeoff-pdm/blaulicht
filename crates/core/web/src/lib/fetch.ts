// import { BlaulichtWebsocket } from "./websocket";

import type { Data } from "./types/state";

// export async function createWS(callbacks: B) {
// }


export async function getData(): Promise<Data> {
	const res = await (await fetch('/api/state')).json();
	return res;
}

