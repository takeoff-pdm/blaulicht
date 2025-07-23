import tailwindcss from '@tailwindcss/vite';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';
import path from 'path';

export default defineConfig({
	plugins: [tailwindcss(), sveltekit()],
	resolve: {
		alias: {
			$lib: path.resolve('./src/lib')
		}
	},
	server: {
		proxy: {
			'/api': {
				target: 'http://localhost:1234',
				changeOrigin: true,
				secure: false,
				ws: true
			} // Forward all requests to /api to the backend server
		} // or you can use regular expressions to handle multiple endpoints// '/api': {
	} //   target: 'http://localhost:5000',//   changeOrigin: true,
});
