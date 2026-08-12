<p align="center">
  <img src="assets/main.gif" alt="Aetheria" width="600"/>
</p>

<p align="center">
  <img src="https://readme-typing-svg.demolab.com?font=Fira+Code&pause=1000&color=8B5CF6&width=435&lines=private+roleplay+and+chat;no+cloud.+your+data+stays+yours;built+in+rust"/>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/rust-%23ff69b4.svg?style=for-the-badge&logo=rust&logoColor=white"/>
  <img src="https://img.shields.io/badge/sqlite-%23dda0dd.svg?style=for-the-badge&logo=sqlite&logoColor=white"/>
  <img src="https://img.shields.io/badge/leptos-%23c084fc.svg?style=for-the-badge&logo=webassembly&logoColor=white"/>
</p>

This is the Rust rewrite of Aetheria, my old fork of Agnaistic, into a high-performance, private roleplaying and chat interface.

Aetheria is an in-progress local roleplay/chat interface. It keeps everything in a local SQLite DB so nothing goes anywhere except the actual LLM API calls. One binary, one process, no node_modules.

## Why I made it

I'm a primarily mobile user, and I tried a bunch of the cloud options out there. Started on Agnaistic, ran into some issues, so I forked it to fix them. That turned into more and more modifications, but something always felt off, it didn't feel like mine because it was someone else's foundation.

Switched to SillyTavern after seeing all the positive opinions on it, but it was so laggy and not user-friendly enough that it made me realize I could just build my own and it'd be better. Their codebase is public so I can see how they implement stuff while I make it faster and more user-friendly.

So here we are.

## Recent updates
- Full theme system: SillyTavern-style token editor with instant live preview, import/export (including direct SillyTavern theme file import), plus a redesigned default look and a mascot (Aug 12)
- Group chats with multiple activation strategies (List, Natural) and per-character talkativeness (Aug 6-7)
- Character card import/export as SillyTavern-compatible PNG cards (Aug 5-6)
- Character alternate greetings, tags, and folders (Aug 5-6)
- Lorebook auto-creation from imported character cards (Aug 5)
- In-chat member management panel for groups (Aug 7)
- Fixed a frontend loop that was hammering the backend's config route on every character list render (Aug 5)
- AI-assisted generation of character fields (scenario, appearance, persona, greeting, example dialogue) (Aug 7)

## Stuff that works

**Core**
- SQLite backend via Axum, Leptos frontend compiled to WebAssembly
- SSE streaming for real-time responses
- AES-256 encryption for API keys (bring your own key)
- Argon2 auth for local users
- Multi-tenancy (`user_id` scoping across every db op and query)
- Safe image proxy. External images (avatars, character art) are fetched and cached server-side, with SSRF protection that blocks private/loopback addresses so opening a character never leaks your IP to the image host
- Remote media stripping. When `forbid_external_media` is on, the frontend just doesn't render remote images and embedded media in messages
- Server-side model listing. The provider's `/models` list is fetched behind your API key so only model IDs reach the browser
- SSE error events. Provider errors surface in-stream as `error` events without killing the connection

**Chat**
- Characters, chats, and lorebooks CRUD
- Group chats with activation strategies (List, Natural) and talkativeness
- In-chat group member management. Add or remove members, and a 1:1 chat promoted to a group (or stripped back down to a single member) keeps its full history
- Message branching and regeneration. Regenerate by branching off a prior turn. In group chats you can reroll a single named member instead
- `/continue` to extend the last reply
- `/respond-as-user` to generate the next line as the user instead of the character
- Message editing, and soft/hard deletion plus hide/unhide (visibility toggle)
- Per-character system prompt and post-history instructions that override the account-wide defaults when filled in
- Reasoning effort control for thinking models (OpenAI, Anthropic, Gemini)
- Thought block display for reasoning models, with thinking blocks stripped from what the model sees on the next turn

**Character Management**
- Alternate greetings, tags, folders, and avatars
- AI-assisted generation of character fields: scenario, appearance (booru tags), persona (optionally a single trait), first greeting, and example dialogue
- Character card import/export (SillyTavern PNG format, v2)

**Memory**
- Long-term vector/RAG memory with a free local embedding option (in-process `nomic-embed-text-v1.5` via candle, or an OpenAI-compatible `/embeddings` endpoint)
- Chat memory summaries. An incremental, threshold-gated LLM "story so far" prepended to the prompt as `[Story so far: …]`. See the dedicated section below
- Per-message `raw_prompt`, `prompt_tokens`, and `context_limit` stored on each message, which the RAG and summary logic use to work out what has already fallen out of context

**Lorebooks** (see the dedicated section below).

**Import/Export**
- Lorebook import/export (SillyTavern world-info v2 format)
- SillyTavern-compatible completion presets, lorebooks, and regex scripts
- Settings, preset, and regex-script export and import. The settings export is a keyless snapshot meant for backup or migration
- Regex scripts support prompt-only and markdown-only placements, run-on-edit re-application, the `substitute_regex` flag, and min/max depth gating
- Completion presets: marker prompts (`worldInfoBefore`, `charDescription`, `chatHistory`, …) are resolved from live content at assembly time and ordered by the preset's `prompt_order` list, which honors enabled toggles plus injection depth and injection position
- Real GPT-2 BPE tokenizer for accurate context-window management
- Full sampling controls (temperature, top_p, top_k, frequency/presence penalty, max response length)

**Theming**
- Full custom theme system: colors, typography, shape, effects, and mascot visibility, all stored as a flat token set and applied live as CSS custom properties, no reload needed
- Built-in default and light themes, plus unlimited custom themes per account, each with its own live-preview editor (every color/slider/checkbox edit applies instantly, nothing is written until you hit save)
- Theme import/export, including direct SillyTavern UI theme JSON import, auto-mapped onto Aetheria's token set (an `@import` in imported `custom_css` is stripped with a warning, since it's a tracking/XSS vector)
- Aeth, a small mascot who reacts to what's actually happening: a thinking pose while a reply streams, a startled pose on a failed generation, an empty-state pose when your character list is empty, and a corner easter egg that peeks up on hover or click

## Architecture
```
browser (Leptos + WASM)
        │
        │  SSE / fetch
        ▼
   Axum (Rust)
        │
        ├── SQLite (accounts, characters, chats, lorebooks, settings)
        ├── candle / nomic-embed-text (local embeddings)
        ├── image proxy (SSRF-safe external fetch + cache)
        └── LLM providers (OpenAI, Anthropic, Gemini, Horde, NovelAI)
```

## Security & privacy

Nothing leaves your machine except the HTTP requests to your configured LLM provider(s). Every bit of state (accounts, characters, chats, lorebooks, settings, and even the encrypted API keys) lives in one local SQLite file. API keys are AES-256 encrypted at rest and decrypted on demand from `AETHERIA_ENCRYPTION_KEY` (32 bytes). Authentication is Argon2. `forbid_external_media` keeps remote images and embedded media out of messages on the client side, and the image proxy blocks private/loopback targets so avatars can't be used as an SSRF vector. All reads and writes are `user_id`-scoped, so users only ever touch their own data.

## Long-term memory (RAG)

Settings → Long-Term Memory. Off by default. When enabled, every message gets embedded and stored. Once a message falls out of the active context window (based on the real context limit, not a fixed message count), retrieval pulls back whatever is still semantically relevant to the current turn by cosine similarity.

Two embedding backends:
- **Local** runs `nomic-embed-text-v1.5` on the server via candle. Free, no API key. The model (~500MB) downloads to `~/.cache/huggingface` on first use.
- **API** calls an OpenAI-compatible `/embeddings` endpoint. Base URL and key fall back to the main provider's if left blank, but the model name does not, since the main chat model usually isn't an embedding model.

`rag_top_k` and `rag_score_threshold` in Settings control how many matches get pulled in and how similar they have to be.

## Chat memory summaries

A second, separate memory system folds conversation into a running narrative summary, so the character stops forgetting the plot once the context window fills up.

A pass only fires after a reply is saved, and only once the unsummarized messages since the last pass reach about 3000 tokens. So a couple of turns never kick off a summarization call. Each chat stores `memory_summary_message_id`, a cursor advanced to the last message actually folded in, so the next pass picks up exactly where it left off and nothing is summarized twice. The result is capped at about 250 words, and thinking/reasoning blocks are stripped from both the source messages and the summary model's output before it is persisted.

The summary can reuse your main provider or run on a separate, cheaper one. Leave the `summary_*` fields blank to inherit the main provider, key, model, and context limit. Or point it at its own provider, base URL, API key, model, and `summary_context_limit` (falls back to the main context limit when unset). The key is decrypted on demand, the same way the main key is.

On every turn the summary is prepended into the prompt as `[Story so far: …]`, right before the message history. Context trimming then drops the oldest raw messages first. Because those are exactly the messages the summary just compressed, the narrative stays in context in their place.

The whole flow, including the thinking-block handling, lives in `crates/server/src/memory.rs`.

## Lorebooks

Keyword-triggered context injection that round-trips with SillyTavern. Import or export a world-info book (v2) and entries, positions, priorities, and ST-format `extensions` (weight, probability, selectiveLogic, excludeRecursion, …) are preserved on the record.

An entry activates when one of its `keywords` shows up in the recent scan window (`scan_depth`). Keywords can be a JSON array or comma-separated. `constant` entries inject on every turn regardless of keywords. Active entries are ordered by priority (desc) then weight (asc) and placed into the `before_char` or `after_char` prompt bucket per entry `position`.

The advanced ST fields (`use_probability`, `selective` + `selective_logic`, `secondary_keys`, `recursive_scanning`, `token_budget`, `exclude_recursion`) are written through on import/export so a book keeps working if you move it back into SillyTavern, but aetheria's trigger stays deliberately simple. It evaluates keywords, constants, and priority/weight only.

## Sampling parameters

Settings → Sampling Parameters, sent with every generation request to the main model: temperature, top_p, top_k, frequency/presence penalty, max response length. A value of 0 (except temperature/top_p) means "don't send it," since some providers reject an explicit 0.

**Reasoning effort** (low/medium/high, or unset for the provider default) caps how much of the response budget a reasoning model spends on `thinking` output before the actual reply, so it doesn't burn the whole `max_tokens` allowance on thinking and leave nothing for the answer. The mapping is per provider since none of them agree on the shape:
- OpenAI-compatible (NanoGPT, OpenRouter-style): sent directly as `reasoning_effort`.
- Anthropic: mapped to `thinking.budget_tokens` (low/medium/high → 4096/10000/24000). Extended thinking rejects `temperature`/`top_p`/`top_k` in the same request, so those get dropped automatically whenever this is set.
- Gemini: mapped to `thinkingConfig.thinkingBudget`, same token values.
- Novel/Horde: no reasoning concept, setting is ignored.

## Environment variables

See `.env.example` for a template. Required:

| Variable | Description |
|---|---|
| `AETHERIA_SESSION_SECRET` | Cookie encryption key (>= 64 random characters) |
| `AETHERIA_ENCRYPTION_KEY` | AES-256 key for encrypted API keys (exactly 32 bytes) |

Optional (on a fresh install, at least one onboarding method is needed, either bootstrapping an initial account or setting `ENABLE_REGISTRATION=true`):

| Variable | Default | Description |
|---|---|---|
| `INITIAL_USERNAME` / `INITIAL_PASSWORD` | `admin` / `password` | Bootstrap an initial user on first startup |
| `ENABLE_REGISTRATION` | `false` | Set to `true` to allow self-registration at `/register` |
| `AETHERIA_BIND` | `127.0.0.1:4310` | IP and port to bind server listener |
| `AETHERIA_DOMAIN` | `yourdomain.com` | Domain name used in Nginx proxy configuration |
| `MAX_UPLOAD_SIZE_MB` | `25` | Max body/upload size limit in megabytes |
| `AETHERIA_GENERATE_BURST` | `20.0` | Max burst capacity for generation rate limit per user |
| `AETHERIA_GENERATE_PER_SEC` | `0.5` | Token refill rate per second for generation rate limiter (e.g. 30/min) |
| `AETHERIA_IMAGE_CACHE_MAX_CAPACITY` | `500` | Max item capacity for cached proxy images in moka cache |
| `AETHERIA_IMAGE_CACHE_TTL_SECS` | `3600` | Time-to-live in seconds for cached proxy images |

## Getting started

### Prerequisites

- [Rust](https://rustup.rs/) (stable)
- [Trunk](https://trunkrs.dev/) (`cargo install trunk`)
- Node.js is NOT needed

### Build

```bash
cd crates/frontend && trunk build --release && cd ../..
cargo build --release -p server
```

### Configure

Copy `.env.example` to `.env` and fill in your keys:

```bash
cp .env.example .env
openssl rand -base64 64  # session secret
openssl rand -hex 32     # encryption key
```

### Run

Direct:

```bash
./target/release/server
```

Or with pm2:

```bash
pm2 start ./target/release/server --name aetheria-rs
pm2 save
```

Server listens on `127.0.0.1:4310`. Point nginx at it (example config in `deploy/nginx-aetheria.conf.example`).

**Port 4310 already taken**, usually a leftover process squatting on it:

```bash
netstat -tulpn | grep 4310
kill -9 <PID>
```

All database migrations are squashed into a single clean baseline migration `0001_init.sql`.

**Automatic migration reconciliation:** when the server starts, `db/mod.rs` detects
existing databases whose `_sqlx_migrations` table contains stale entries from the
pre-squashed multi-migration setup (versions 1–27 with old checksums). It extracts
the correct SHA-384 checksum from the compile-time-embedded `Migrator` and rewrites
`_sqlx_migrations` to the new baseline, no manual intervention needed.

**Manual fix (if needed):** if the automatic reconciliation fails, check what's
recorded first:

```bash
sqlite3 crates/server/aetheria.sqlite3 "SELECT version, description, hex(checksum) FROM _sqlx_migrations ORDER BY version DESC LIMIT 5;"
```

The checksum is the SHA-384 of the migration SQL. To reset manually:

```bash
CHK=$(sha384sum crates/server/migrations/0001_init.sql | awk '{print $1}')
sqlite3 crates/server/aetheria.sqlite3 "DELETE FROM _sqlx_migrations; INSERT INTO _sqlx_migrations VALUES (1, 'init', CURRENT_TIMESTAMP, X'$CHK', 0);"
```

Since `migrations/` gets embedded at compile time via `sqlx::migrate!`, touch `crates/server/src/db/mod.rs` if a build finishes without picking up migration edits.

### Nginx

Auto-generate `deploy/nginx-aetheria.conf` directly from your `.env` variables (`AETHERIA_BIND`, `MAX_UPLOAD_SIZE_MB`, `AETHERIA_DOMAIN`):

```bash
chmod +x deploy/generate-nginx-config.sh
./deploy/generate-nginx-config.sh
```

This keeps your Nginx proxy target, upload limits, and domain name in sync with your `.env` configuration without requiring separate manual edits.
