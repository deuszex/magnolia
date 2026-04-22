# Magnolia TypeScript Client

Session-based and HMAC proxy client for the Magnolia API.

## Requirements

- Node.js 22+ *(native `fetch`, `AbortSignal.timeout`, `--experimental-strip-types`)*
- [`ws`](https://github.com/websockets/ws) *(only needed for `connectWebSocket()`)*

> **Note:** This client uses the native `fetch` API. Axios is **not** used.

## Installation

```sh
cd client/typescript
npm install
```

No build step required. Node 22 runs `.ts` files directly via `--experimental-strip-types`.

## Usage

### Session client

```ts
import { MagnoliaClient } from "./magnolia.ts";

const client = new MagnoliaClient("https://magnolia.example.com", { timeoutMs: 10_000 });

await client.login("alice", "hunter2");

const posts = await client.listPosts({ limit: 10 });

const post = await client.createPost(
  [{ content_type: "text", display_order: 0, content: "Hello world" }],
  true,
  ["intro"],
);

await client.logout();
```

### HMAC proxy client

```ts
import { MagnoliaHMACClient } from "./magnolia.ts";

const proxy = new MagnoliaHMACClient(
  "https://magnolia.example.com",
  "<proxy-uuid>",
  "<64-char-hex-key>",
  { timeoutMs: 10_000 },
);

const conv = await proxy.getOrCreateConversation({ targetUsername: "bob" });
await proxy.sendMessage(conv.conversation_id, "encrypted-payload");
```

The `hmacKey` is the 64-character lowercase hex string stored on the proxy account. It is used as raw UTF-8 key material - do **not** hex-decode it before passing.

## Running the tests

The test suite uses Node's built-in `node:test` runner and runs against a live server. All network calls use a 10-second timeout.

### Required environment variables

| Variable | Description |
|---|---|
| `MAGNOLIA_BASE_URL` | Server root, e.g. `https://magnolia.example.com` |
| `MAGNOLIA_USERNAME` | Login identifier (username or email) |
| `MAGNOLIA_PASSWORD` | Login password |

### Optional environment variables

Missing optional variables cause the relevant tests to be **skipped**, not failed.

| Variable | Enables |
|---|---|
| `MAGNOLIA_PROXY_ID` | HMAC proxy tests (required together with `MAGNOLIA_HMAC_KEY`) |
| `MAGNOLIA_HMAC_KEY` | HMAC proxy tests (required together with `MAGNOLIA_PROXY_ID`) |
| `MAGNOLIA_MEDIA_FILE` | Media upload, download, chunked upload, and HMAC media upload tests |
| `MAGNOLIA_TARGET_USER_ID` | Conversation and message tests |

### Run

```sh
cd client/typescript
npm install

export MAGNOLIA_BASE_URL=https://magnolia.example.com
export MAGNOLIA_USERNAME=alice
export MAGNOLIA_PASSWORD=hunter2

# Optional:
export MAGNOLIA_PROXY_ID=<proxy-uuid>
export MAGNOLIA_HMAC_KEY=<64-char-hex>
export MAGNOLIA_MEDIA_FILE=/path/to/file.png
export MAGNOLIA_TARGET_USER_ID=<user-uuid>

npm test
# or directly:
node --experimental-strip-types magnolia.test.ts
```

### Run a single test

`node:test` supports filtering by name pattern:

```sh
node --experimental-strip-types magnolia.test.ts --test-name-pattern "Posts"
```
