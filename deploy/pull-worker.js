// runs on aetheria.silt.im/*. the network in front of the server chokes every connection after
// roughly 16KB, so any response bigger than that dies on its way to cloudflare (fonts, avatars,
// the character list, the wasm). this pulls each plain GET from the origin in small ranges, each
// on its own connection, glues them together here and sends the whole thing to the visitor.
// nginx answers byte ranges for every path (proxy_force_ranges). anything that isn't a plain GET
// goes straight through, and so does anything that doesn't fit the pieces budget
const PIECE = 14 * 1024;
// free plan allows 50 subrequests per invocation, leave room for retries
const MAX_PIECES = 42;
const RETRY_BUDGET = 6;
const PIECE_TIMEOUT_MS = 6000;

const isStatic = (path) => /^\/assets\//.test(path) || /^\/(frontend|style)-[0-9a-f]+/.test(path) || path === "/wasm-loader.js";

function eligible(request) {
  if (request.method !== "GET") return false;
  if (request.headers.has("Range") || request.headers.has("Upgrade")) return false;
  const path = new URL(request.url).pathname;
  // avatars are assembled by wasm-loader.js in 12KB ranges; this worker's
  // 42-piece cap cannot cover a multi-MB character-card PNG
  if (path.startsWith("/uploads/")) return false;
  if ((request.headers.get("Accept") || "").includes("text/event-stream")) return false;
  return true;
}

async function fetchPiece(request, start, end, budget, total) {
  let lastError;
  for (let attempt = 0; ; attempt++) {
    const ctrl = new AbortController();
    const timer = setTimeout(() => ctrl.abort(), PIECE_TIMEOUT_MS);
    try {
      const headers = new Headers(request.headers);
      headers.set("Range", `bytes=${start}-${end}`);
      // the runtime unzips Content-Encoding gzip, which breaks on a partial piece
      headers.set("Accept-Encoding", "identity");
      headers.delete("If-None-Match");
      headers.delete("If-Modified-Since");
      const res = await fetch(request.url, { headers, cache: "no-store", signal: ctrl.signal });
      // the first piece decides what the whole thing is, anything but a 206 is handed back as is
      if (start === 0 && res.status !== 206) {
        clearTimeout(timer);
        return { res, buf: null };
      }
      if (res.status !== 206) throw new Error(`piece answered ${res.status}`);
      const size = Number((res.headers.get("Content-Range") || "").split("/")[1]);
      if (total && size !== total) throw new Error("size changed");
      const buf = new Uint8Array(await res.arrayBuffer());
      clearTimeout(timer);
      if (buf.length !== end - start + 1 && start + buf.length !== size) throw new Error("short piece");
      return { res, buf };
    } catch (err) {
      clearTimeout(timer);
      lastError = err;
      if (budget.retries-- <= 0 || attempt >= 2) throw lastError;
    }
  }
}

async function pull(request) {
  const budget = { retries: RETRY_BUDGET };
  const first = await fetchPiece(request, 0, PIECE - 1, budget);
  if (!first.buf) return { passthrough: first.res };

  const total = Number((first.res.headers.get("Content-Range") || "").split("/")[1]);
  if (!total || total > PIECE * MAX_PIECES) return null;
  const bytes = new Uint8Array(total);
  bytes.set(first.buf, 0);
  const count = Math.ceil(total / PIECE);
  await Promise.all(
    Array.from({ length: count - 1 }, async (_, i) => {
      const start = (i + 1) * PIECE;
      const end = Math.min(start + PIECE, total) - 1;
      const piece = await fetchPiece(request, start, end, budget, total);
      bytes.set(piece.buf, start);
    }),
  );
  return { bytes, headers: first.res.headers, count };
}

export default {
  async fetch(request, env, ctx) {
    const url = new URL(request.url);
    // russian origin to cloudflare dies once a response gets big. never let this
    // worker turn an avatar into a full-file pull. forward Range explicitly;
    // fetch(request) was dropping it and origin answered 200 with a cut-off body.
    if (url.pathname.startsWith("/uploads/") || url.pathname.startsWith("/media-piece/")) {
      const headers = new Headers(request.headers);
      const range = request.headers.get("Range");
      if (range) headers.set("Range", range);
      headers.set("Accept-Encoding", "identity");
      headers.delete("If-None-Match");
      headers.delete("If-Modified-Since");
      return fetch(request.url, { method: "GET", headers, redirect: "follow" });
    }
    if (!eligible(request)) return fetch(request);

    const cacheable = isStatic(url.pathname);
    const cache = caches.default;
    if (cacheable) {
      const hit = await cache.match(request);
      if (hit) return hit;
    }

    let out = null;
    for (let attempt = 0; attempt < 2 && !out; attempt++) {
      try {
        out = await pull(request);
      } catch {}
    }
    // a direct fetch of the whole body dies around 16KB and the browser
    // sits on that truncated response. fail fast so the client can retry.
    if (!out) return new Response(null, { status: 502, headers: { "cache-control": "no-store" } });
    if (out.passthrough) return out.passthrough;

    const headers = new Headers(out.headers);
    for (const h of ["Content-Range", "Content-Length", "Accept-Ranges", "Content-Encoding"]) headers.delete(h);
    headers.set("X-Pulled-Pieces", String(out.count));
    if (cacheable) {
      headers.set("Cache-Control", /^\/(frontend|style)-[0-9a-f]+/.test(url.pathname) ? "public, max-age=31536000, immutable" : "public, max-age=3600");
    }
    const response = new Response(out.bytes, { status: 200, headers });
    if (cacheable) ctx.waitUntil(cache.put(request, response.clone()));
    return response;
  },
};
