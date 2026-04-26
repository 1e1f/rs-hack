// Tiny static server for development. Same bundle works inside Tauri's webview;
// in production the Rust `yah` binary will serve these assets.

const PORT = parseInt(process.env.YAH_UI_PORT || "5173");
const ROOT = "./public";

const server = Bun.serve({
  port: PORT,
  async fetch(req) {
    const url = new URL(req.url);
    const path = url.pathname === "/" ? "/index.html" : url.pathname;
    const file = Bun.file(`${ROOT}${path}`);
    if (await file.exists()) return new Response(file);
    // SPA fallback
    return new Response(Bun.file(`${ROOT}/index.html`));
  },
});

console.log(`yah-ui dev → http://localhost:${server.port}`);
