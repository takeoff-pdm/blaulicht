import { svelte } from '@sveltejs/vite-plugin-svelte'
import { resolve } from 'path'
import { defineConfig } from 'vite'

// https://vitejs.dev/config/
export default defineConfig({
    plugins: [svelte()],
    build: {
        rollupOptions: {
            input: {
                dash: resolve(__dirname, 'html/dash.html'),
            },
            // output: {
            //     manualChunks: (id: any) => {
            //         if (id.includes('node_modules')) {
            //             if (id.includes('@smui') || id.includes('material')) {
            //                 return 'vendor_mui'
            //             }
            //         }
            //     },
            // },
        },
    },
})
