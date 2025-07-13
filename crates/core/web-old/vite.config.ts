import tailwindcss from "@tailwindcss/vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { resolve } from "path";
import { defineConfig } from "vite";

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [tailwindcss(), svelte()],
  resolve: {
    alias: {
      $lib: resolve("./src/lib"),
    },
  },
  // build: {
  //   rollupOptions: {
  //     input: { dash: resolve(__dirname, "html/dash.html") }
  //   } // output: {//     manualChunks: (id: any) => {
  // }, //         if (id.includes('node_modules')) {
  //             if (id.includes('@smui') || id.includes('material')) {
  //                 return 'vendor_mui'
  //             }
  //         }
  //     },
  // },
  server: {
    proxy: {
      "/api": {
        target: "http://localhost:1234",
        changeOrigin: true,
        secure: false,
        ws: true,
      }, // Forward all requests to /api to the backend server
    }, // or you can use regular expressions to handle multiple endpoints// '/api': {
  }, //   target: 'http://localhost:5000',//   changeOrigin: true,
}); //   secure: false, // Set to false if your API does not use HTTPS
// },
