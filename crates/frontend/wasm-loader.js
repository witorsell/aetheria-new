// the network in front of the server chokes every connection after roughly 16-24KB, so a plain
// download of the 2MB wasm bundle dies partway and the app never starts. this swaps the one
// fetch trunk's init does for the wasm with a ranged download: small pieces, each on its own
// request, a few at once, retried when one stalls. it pulls the gzip copy the build writes (.gzip, because cloudflare caches .gz whole and chokes) next
// to the wasm (a third of the size) and unpacks it here, and falls back to the raw file. the
// finished file is kept in cache storage so later visits skip all of this, and a percentage
// shows while it runs because the page is blank until the wasm is in.
(() => {
  const PIECE = 12 * 1024;
  const PARALLEL = 10;
  const ATTEMPTS = 6;
  const PIECE_TIMEOUT_MS = 8000;
  const CACHE_NAME = "aetheria-wasm";
  const realFetch = window.fetch.bind(window);

  const wasmResponse = (bytes) =>
    new Response(bytes, { headers: { "Content-Type": "application/wasm" } });

  let label = null;
  function showProgress(done, total) {
    if (!document.body) return;
    if (!label) {
      label = document.createElement("div");
      label.style.cssText =
        "position:fixed;inset:0;display:flex;align-items:center;justify-content:center;" +
        "font:14px system-ui,sans-serif;color:#8a8a8a;pointer-events:none;z-index:2147483647";
      document.body.appendChild(label);
      addEventListener("TrunkApplicationStarted", () => label.remove(), { once: true });
    }
    label.textContent = `loading aetheria... ${Math.min(100, Math.floor((done / total) * 100))}%`;
  }

  async function fetchPiece(url, start, end) {
    let lastError;
    for (let attempt = 0; attempt < ATTEMPTS; attempt++) {
      const ctrl = new AbortController();
      const timer = setTimeout(() => ctrl.abort(), PIECE_TIMEOUT_MS);
      try {
        // the query keeps every attempt a distinct request, the server ignores it
        const res = await realFetch(`${url}?piece=${start}-${attempt}`, {
          headers: { Range: `bytes=${start}-${end}` },
          cache: "no-store",
          signal: ctrl.signal,
        });
        if (res.status !== 206) throw new Error(`piece answered ${res.status}`);
        const buf = new Uint8Array(await res.arrayBuffer());
        if (buf.length !== end - start + 1) throw new Error("short piece");
        return { buf, res };
      } catch (err) {
        lastError = err;
      } finally {
        clearTimeout(timer);
      }
    }
    throw lastError;
  }

  async function openCache() {
    try {
      return await caches.open(CACHE_NAME);
    } catch {
      return null;
    }
  }

  // the server answers a missing file with the app's index.html, so the gzip copy is only
  // trusted when its first bytes are the gzip magic
  async function download(url, wantGzip) {
    const first = await fetchPiece(url, 0, PIECE - 1);
    if (wantGzip && !(first.buf[0] === 0x1f && first.buf[1] === 0x8b)) throw new Error("not gzip");
    const total = Number((first.res.headers.get("Content-Range") || "").split("/")[1]);
    if (!total) throw new Error("no total size");

    const bytes = new Uint8Array(total);
    bytes.set(first.buf, 0);
    const count = Math.ceil(total / PIECE);
    let next = 1;
    let finished = 1;
    showProgress(finished, count);
    const worker = async () => {
      while (next < count) {
        const i = next++;
        const start = i * PIECE;
        const end = Math.min(start + PIECE, total) - 1;
        bytes.set((await fetchPiece(url, start, end)).buf, start);
        showProgress(++finished, count);
      }
    };
    await Promise.all(Array.from({ length: Math.min(PARALLEL, count - 1) }, worker));
    return bytes;
  }

  async function unzip(gz) {
    const stream = new Blob([gz]).stream().pipeThrough(new DecompressionStream("gzip"));
    return new Uint8Array(await new Response(stream).arrayBuffer());
  }

  async function fetchWasm(url) {
    if (typeof DecompressionStream === "function") {
      try {
        return await unzip(await download(`${url}.gzip`, true));
      } catch {}
    }
    return download(url, false);
  }

  async function loadWasm(url) {
    const cache = await openCache();
    if (cache) {
      const hit = await cache.match(url).catch(() => null);
      if (hit) return hit;
    }
    const bytes = await fetchWasm(url);
    if (cache) {
      // file names carry a content hash, so anything else in there is an old build
      cache.put(url, wasmResponse(bytes)).catch(() => {});
      cache.keys().then((keys) => keys.forEach((k) => k.url !== url && cache.delete(k))).catch(() => {});
    }
    return wasmResponse(bytes);
  }

  window.fetch = function (input, init) {
    const url = new URL(typeof input === "string" || input instanceof URL ? input : input.url, location.href);
    if (!url.pathname.endsWith("_bg.wasm")) return realFetch(input, init);
    return loadWasm(url.origin + url.pathname).catch(() => realFetch(input, init));
  };
  window.__aetheriaRealFetch = realFetch;
})();

// the box-to-cloudflare path throttles anything that isn't a small finished response.
// Range on /uploads/ was getting dropped and coming back as a cut-off 200 of the whole
// file, so each slice is its own short URL: /media-piece/<file>/<start>/<len>
(() => {
  // 24KB is the biggest complete body this origin-to-CF path still delivers; 32KB dies at ~16KB
  const UPLOAD_PIECE = 24 * 1024;
  const ATTEMPTS = 3;
  const PIECE_TIMEOUT_MS = 8000;
  const CACHE_NAME = "aetheria-uploads";
  const MAX_INFLIGHT = 10;
  const IMAGE_SLOTS = 3;
  // proxied images go through the worker, which can rebuild about 600KB per response,
  // so a big one is pulled as slices that each fit inside that
  const PROXY_PIECE = 256 * 1024;
  const PROXY_PIECE_TIMEOUT_MS = 25000;
  const realFetch = window.__aetheriaRealFetch || window.fetch.bind(window);
  const blobs = new Map();
  const inflight = new Map();
  const jobs = [];
  let running = 0;
  let imageActive = 0;
  const imageWait = [];

  function limit(fn) {
    return new Promise((resolve, reject) => {
      jobs.push({ fn, resolve, reject });
      pump();
    });
  }
  function pump() {
    while (running < MAX_INFLIGHT && jobs.length) {
      const job = jobs.shift();
      running++;
      job.fn().then(job.resolve, job.reject).finally(() => {
        running--;
        pump();
      });
    }
  }
  function withImageSlot(fn) {
    return new Promise((resolve, reject) => {
      const run = () => {
        imageActive++;
        fn().then(resolve, reject).finally(() => {
          imageActive--;
          if (imageWait.length) imageWait.shift()();
        });
      };
      if (imageActive < IMAGE_SLOTS) run();
      else imageWait.push(run);
    });
  }

  const isProxy = (url) => url.startsWith("/api/proxy?");

  function pieceUrl(url, start, len) {
    if (isProxy(url)) return `${url}&off=${start}&len=${len}`;
    const u = new URL(url, location.href);
    const name = u.pathname.split("/").pop();
    return `/media-piece/${encodeURIComponent(name)}/${start}/${len}`;
  }

  async function fetchPiece(url, start, len, totalHint) {
    let lastError;
    for (let attempt = 0; attempt < ATTEMPTS; attempt++) {
      try {
        const res = await limit(() => {
          const ctrl = new AbortController();
          const timer = setTimeout(() => ctrl.abort(), isProxy(url) ? PROXY_PIECE_TIMEOUT_MS : PIECE_TIMEOUT_MS);
          return realFetch(pieceUrl(url, start, len), {
            cache: "no-store",
            credentials: "same-origin",
            headers: { "Accept-Encoding": "identity" },
            signal: ctrl.signal,
          }).finally(() => clearTimeout(timer));
        });
        if (!res.ok) throw new Error(`piece answered ${res.status}`);
        const buf = new Uint8Array(await res.arrayBuffer());
        const total = Number(res.headers.get("X-File-Size") || totalHint || 0);
        if (buf.length !== len && start + buf.length !== total) throw new Error("short piece");
        if (buf.length > len) throw new Error("fat piece");
        return { buf, total, type: res.headers.get("Content-Type") || "application/octet-stream" };
      } catch (err) {
        lastError = err;
      }
    }
    throw lastError;
  }

  async function download(url) {
    const PIECE = isProxy(url) ? PROXY_PIECE : UPLOAD_PIECE;
    return withImageSlot(async () => {
      const first = await fetchPiece(url, 0, PIECE);
      const total = first.total;
      if (!total) throw new Error("no total size");
      if (total <= first.buf.length) return new Blob([first.buf], { type: first.type });
      const bytes = new Uint8Array(total);
      bytes.set(first.buf, 0);
      const count = Math.ceil(total / PIECE);
      const rest = [];
      for (let i = 1; i < count; i++) {
        const start = i * PIECE;
        const len = Math.min(PIECE, total - start);
        rest.push(
          fetchPiece(url, start, len, total).then((p) => {
            bytes.set(p.buf, start);
          }),
        );
      }
      await Promise.all(rest);
      return new Blob([bytes], { type: first.type });
    });
  }

  function blobFor(url) {
    const parsed = new URL(url, location.href);
    const key = parsed.pathname + parsed.search;
    if (blobs.has(key)) return Promise.resolve(blobs.get(key));
    if (inflight.has(key)) return inflight.get(key);
    const job = (async () => {
      try {
        const cache = await caches.open(CACHE_NAME).catch(() => null);
        const hit = cache ? await cache.match(key).catch(() => null) : null;
        const blob = hit ? await hit.blob() : await download(url);
        if (cache && !hit) cache.put(key, new Response(blob)).catch(() => {});
        const obj = URL.createObjectURL(blob);
        blobs.set(key, obj);
        return obj;
      } finally {
        inflight.delete(key);
      }
    })();
    inflight.set(key, job);
    return job;
  }

  function uploadSrc(raw) {
    if (!raw || raw.startsWith("blob:") || raw.startsWith("data:")) return null;
    try {
      const u = new URL(raw, location.href);
      if (u.origin !== location.origin) return null;
      if (!u.pathname.startsWith("/uploads/")) return null;
      return u.pathname + u.search.replace(/&?piece=\d+-\d+/g, "").replace(/^\?&/, "?");
    } catch {
      return null;
    }
  }

  // only /api/proxy images that already failed to load come through here; the ones
  // that load on their own are left alone
  function proxySrc(raw) {
    if (!raw || raw.startsWith("blob:") || raw.startsWith("data:")) return null;
    try {
      const u = new URL(raw, location.href);
      if (u.origin !== location.origin || u.pathname !== "/api/proxy") return null;
      const params = new URLSearchParams(u.search);
      for (const key of ["retry", "off", "len"]) params.delete(key);
      return u.pathname + "?" + params.toString();
    } catch {
      return null;
    }
  }

  function upgrade(img) {
    const url = uploadSrc(img.getAttribute("src") || img.src);
    if (url) loadInto(img, url);
  }

  function loadInto(img, url) {
    if (img.dataset.rangedSrc === url && blobs.has(url)) {
      if (img.src !== blobs.get(url)) img.src = blobs.get(url);
      return;
    }
    if (img.dataset.rangedSrc === url && img.dataset.rangedPending === "1") return;
    img.dataset.rangedSrc = url;
    img.dataset.rangedPending = "1";
    // cancel the doomed full GET so it doesn't sit on a hung connection
    img.src =
      "data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7";
    blobFor(url)
      .then((obj) => {
        if (img.dataset.rangedSrc === url) img.src = obj;
      })
      .catch(() => {
        // do not fall back to the full /uploads/ GET, that is the request the throttle kills
        if (img.dataset.rangedSrc === url) img.alt = img.alt || "avatar";
      })
      .finally(() => {
        if (img.dataset.rangedSrc === url) delete img.dataset.rangedPending;
      });
  }

  function scan(root) {
    if (!root) return;
    if (root.nodeName === "IMG") upgrade(root);
    if (root.querySelectorAll) root.querySelectorAll("img").forEach(upgrade);
  }

  const obs = new MutationObserver((muts) => {
    for (const m of muts) {
      if (m.type === "attributes" && m.target.nodeName === "IMG") upgrade(m.target);
      else m.addedNodes.forEach(scan);
    }
  });
  // image errors don't bubble, so listen in the capture phase
  addEventListener(
    "error",
    (e) => {
      const img = e.target;
      if (!img || img.nodeName !== "IMG") return;
      const url = proxySrc(img.getAttribute("src") || img.src);
      if (url) loadInto(img, url);
    },
    true,
  );
  const start = () => {
    obs.observe(document.documentElement, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ["src"],
    });
    scan(document);
  };
  if (document.documentElement) start();
  else addEventListener("DOMContentLoaded", start, { once: true });
})();

