# Proxy HMAC Endpoints

These endpoints allow a proxy account to send messages and create posts without a session cookie. Authentication is via HMAC-SHA256 request signing using a shared key.

## Prerequisites

- The proxy account must be active.
- A HMAC key must be set on the proxy (via the management UI or API). The key is 32 random bytes, hex-encoded (64 characters). The client holds the key; the server stores it.
- The proxy system must be enabled in site config.

## General signing rules

- The timestamp is a Unix timestamp in **seconds**.
- The server rejects requests where `|server_time - timestamp| > 300` (5 minutes). Keep your clock in sync.
- The HMAC algorithm is **HMAC-SHA256**. The key is the raw bytes of the hex-encoded key string (i.e. treat the 64-character hex string as the key material directly, not decoded to 32 bytes).
- The signature is the lowercase hex encoding of the HMAC output.
- Where a SHA-256 hash of content is part of the signed string, it is also lowercase hex.

---

## POST /api/proxy/hmac/send-message

Send a message to a conversation as the proxy.

The proxy must already be a member of the target conversation.

### Request body

```json
{
  "proxy_id": "string",
  "conversation_id": "string",
  "encrypted_content": "string",
  "media_ids": ["string"],
  "signature": "string",
  "timestamp": 1234567890
}
```

| Field | Required | Description |
|---|---|---|
| `proxy_id` | yes | The proxy's ID |
| `conversation_id` | yes | Target conversation |
| `encrypted_content` | yes | Opaque ciphertext (stored as-is by the server) |
| `media_ids` | no | IDs of media already uploaded via `POST /api/media` |
| `signature` | yes | See below |
| `timestamp` | yes | Unix timestamp (seconds) |

### Signing

```
signed_string = proxy_id + ":" + conversation_id + ":" + sha256(encrypted_content) + ":" + timestamp
signature = hmac_sha256_hex(key, signed_string)
```

### Response

`201 Created`

```json
{
  "message_id": "string",
  "created_at": "string"
}
```

---

## POST /api/proxy/hmac/create-post

Create a post as the proxy. The proxy must be paired to a user account (the paired user becomes the post author).

### Request body

```json
{
  "proxy_id": "string",
  "contents": [
    {
      "content_type": "text",
      "display_order": 0,
      "content": "string",
      "filename": null,
      "mime_type": null,
      "media_id": null
    }
  ],
  "publish": false,
  "tags": ["string"],
  "signature": "string",
  "timestamp": 1234567890
}
```

| Field | Required | Description |
|---|---|---|
| `proxy_id` | yes | The proxy's ID |
| `contents` | yes | 1–20 content items |
| `contents[].content_type` | yes | One of: `text`, `image`, `video`, `file` |
| `contents[].display_order` | yes | Integer, 0-indexed display position |
| `contents[].content` | yes | For text: plaintext. For media: base64 data or storage key |
| `contents[].filename` | no | Original filename (media uploads) |
| `contents[].mime_type` | no | MIME type (media uploads) |
| `contents[].media_id` | no | Reference to an already-uploaded media item |
| `publish` | no | `true` to publish immediately; `false` (default) saves as draft |
| `tags` | no | List of tags |
| `signature` | yes | See below |
| `timestamp` | yes | Unix timestamp (seconds) |

### Signing

Construct the canonical body as follows:

1. Sort `contents` by `display_order` ascending.
2. For each item, produce the string: `display_order|content_type|content`
3. Join all items with `\n`.
4. Append `\ntags:` followed by the tags sorted alphabetically and joined with `,`. If there are no tags, append `\ntags:`.
5. Append `\npublish:` followed by `1` if publishing, `0` if draft.

Then:

```
body_hash = sha256(canonical_body)
signed_string = proxy_id + ":" + body_hash + ":" + publish_bit + ":" + timestamp
signature = hmac_sha256_hex(key, signed_string)
```

Where `publish_bit` is `1` or `0`, matching the `publish` field.

### Example canonical body

For a request with two content items, tags `["rust", "code"]`, and `publish: true`:

```
0|text|Hello world
1|image|data:image/png;base64,...
tags:code,rust
publish:1
```

### Response

`201 Created`

```json
{
  "post_id": "string",
  "created_at": "string"
}
```

---

## POST /api/proxy/hmac/get-or-create-conversation

Get the existing direct conversation between the proxy and a target user, or create one if it does not exist. Only one-on-one conversations are supported - groups cannot be created through this endpoint.

When a new conversation is created, the server fires a notification to the paired user.

### Request body

```json
{
  "proxy_id": "string",
  "target_user_id": "string",
  "target_username": null,
  "signature": "string",
  "timestamp": 1234567890
}
```

| Field | Required | Description |
|---|---|---|
| `proxy_id` | yes | The proxy's ID |
| `target_user_id` | one of | Target user by internal ID |
| `target_username` | one of | Target user by username. Mutually exclusive with `target_user_id` |
| `signature` | yes | See below |
| `timestamp` | yes | Unix timestamp (seconds) |

Exactly one of `target_user_id` or `target_username` must be provided.

### Signing

```
signed_string = proxy_id + ":" + timestamp
signature = hmac_sha256_hex(key, signed_string)
```

### Response

`200 OK` (conversation already existed) or `201 Created` (new conversation):

```json
{
  "conversation_id": "string",
  "created": true
}
```

---

## POST /api/proxy/hmac/upload-media

Upload a media file as the proxy. The proxy owns the uploaded media and can reference it in subsequent `send-message` or `create-post` calls via the returned `media_id`.

This endpoint is **rate-limited per proxy**. The effective limits are the lower of the server-wide defaults (configured in site config) and any per-proxy override. The default limits are 1 file/minute and 12 MB/minute. On a breach, the proxy is automatically disabled and security events are sent to the paired user and all admins.

### Request

`Content-Type: multipart/form-data`

| Field | Type | Required | Description |
|---|---|---|---|
| `proxy_id` | text | yes | The proxy's ID |
| `signature` | text | yes | See below |
| `timestamp` | text | yes | Unix timestamp (seconds), sent as a text field |
| `file` | file | yes | The file to upload. The filename and MIME type are taken from the part headers. |

### Signing

```
file_hash = sha256(raw_file_bytes)
signed_string = proxy_id + ":" + file_hash + ":" + timestamp
signature = hmac_sha256_hex(key, signed_string)
```

`sha256` is computed over the **raw file bytes** as-is - do not apply any encoding or text conversion before hashing.

### Response

`201 Created`

```json
{
  "media_id": "string",
  "url": "string",
  "thumbnail_url": "string | null"
}
```

`thumbnail_url` is populated for image and video uploads.

---

## Error responses

| Status | Meaning |
|---|---|
| `401 Unauthorized` | Proxy not found, no HMAC key set, timestamp out of range, or signature mismatch |
| `403 Forbidden` | Proxy is inactive, not a member of the target conversation, or rate limit breached (proxy is also disabled on breach) |
| `400 Bad Request` | Validation failure (e.g. invalid content_type, no paired user for create-post, missing multipart fields) |
| `404 Not Found` | Target user not found (get-or-create-conversation) |
