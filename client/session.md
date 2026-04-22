# Session-Based API

Authentication uses an HTTP-only session cookie (`session_id`) issued on login. All protected endpoints require this cookie. Credentials are never sent again after login.

Rate limits are noted where they differ from the default (120 requests/60 s for public endpoints, 30/60 s for search). 

A number of endpoints are not documented here, and are not in the client codebase. [List here](#endpoints-not-documented-above)

---

## Authentication

### POST /api/auth/login

Start a session. Rate-limited: **5 requests / 60 s**.

```json
{
  "identifier": "alice",
  "password": "hunter2"
}
```

`identifier` accepts either a username or an email address.

**Response** `200 OK` - sets `session_id` cookie (HTTP-only).

```json
{
  "user": {
    "user_id": "string",
    "email": "string | null",
    "username": "string",
    "display_name": "string | null",
    "avatar_url": "string | null",
    "verified": true,
    "admin": false
  }
}
```

---

### POST /api/auth/logout

End the current session. Cookie is cleared server-side.

No request body required.

**Response** `200 OK`

---

### GET /api/auth/me

Return the currently authenticated user.

**Response** `200 OK` - same `{ user: UserResponse }` shape as login.

---

### POST /api/auth/change-password

Change password while authenticated. Rate-limited: **5 requests / 60 s**.

```json
{
  "current_password": "string",
  "new_password": "string",
  "new_password_confirm": "string"
}
```

**Response** `200 OK`

---

## Profile

### GET /api/users/{user_id}/profile

Get a user's public profile.

**Response** `200 OK`

```json
{
  "user_id": "string",
  "email": "string | null",
  "email_visible": false,
  "username": "string",
  "display_name": "string | null",
  "bio": "string | null",
  "avatar_url": "string | null",
  "location": "string | null",
  "website": "string | null",
  "public_key": "string | null",
  "created_at": "string"
}
```

`email` is only present if the user has enabled email visibility or you are viewing your own profile.

---

### PUT /api/profile

Update the authenticated user's own profile.

```json
{
  "display_name": "string | null",
  "bio": "string | null",
  "avatar_media_id": "string | null",
  "location": "string | null",
  "website": "string | null"
}
```

All fields are optional. `avatar_media_id` must be a `media_id` from a previously uploaded image.

**Response** `200 OK` - updated `ProfileResponse`.

---

## Posts

### GET /api/posts

List published posts.

| Query param | Default | Description |
|---|---|---|
| `author_id` | - | Filter by author |
| `include_drafts` | `false` | Include own unpublished posts (ignored for others) |
| `content_type` | - | Filter by content type (`text`, `image`, `video`, `file`) |
| `limit` | `20` | Max results |
| `offset` | `0` | Pagination offset |
| `after` | - | Cursor: `post_id` to paginate from |

**Response** `200 OK`

```json
{
  "posts": [
    {
      "post_id": "string",
      "author_id": "string",
      "author_name": "string | null",
      "author_avatar_url": "string | null",
      "contents": [ { "content_id": "string", "content_type": "text", "display_order": 0, "content": "string", "thumbnail_url": null, "filename": null, "mime_type": null, "file_size": null } ],
      "tags": ["string"],
      "is_published": true,
      "comment_count": 0,
      "created_at": "string",
      "source_server": "string | null"
    }
  ],
  "total": 0,
  "has_more": false,
  "next_cursor": "string | null"
}
```

---

### GET /api/posts/{post_id}

Get a single post with full content.

**Response** `200 OK` - `PostResponse` (same contents shape, no `author_name`/`author_avatar_url`).

---

### POST /api/posts

Create a post. Requires authentication.

```json
{
  "publish": false,
  "tags": ["string"],
  "contents": [
    {
      "content_type": "text",
      "display_order": 0,
      "content": "Hello world",
      "filename": null,
      "mime_type": null,
      "media_id": null
    }
  ]
}
```

| Field | Required | Notes |
|---|---|---|
| `contents` | yes | 1–20 items |
| `contents[].content_type` | yes | `text`, `image`, `video`, `file` |
| `contents[].display_order` | yes | 0-indexed render order |
| `contents[].content` | yes | Text content for `text`; ignored when `media_id` is set |
| `contents[].media_id` | no | Reference to an already-uploaded media item |
| `contents[].filename` | no | Original filename (inline uploads) |
| `contents[].mime_type` | no | MIME type (inline uploads) |
| `publish` | no | `true` to publish immediately; `false` (default) saves as draft |
| `tags` | no | String list |

**Response** `201 Created` - `PostResponse`

---

### PUT /api/posts/{post_id}

Update an existing post (own posts only). All fields are optional; omitted fields are unchanged.

```json
{
  "publish": true,
  "tags": ["updated"],
  "contents": [ { "content_type": "text", "display_order": 0, "content": "Edited" } ]
}
```

`contents`, when provided, **replaces** all existing content items.

**Response** `200 OK` - `PostResponse`

---

### DELETE /api/posts/{post_id}

Delete a post (own posts; admin can delete any).

**Response** `204 No Content`

---

### POST /api/posts/{post_id}/publish

Toggle the published state of a post.

No request body required.

**Response** `200 OK` - `PostResponse` with updated `is_published`.

---

### GET /api/posts/search

Search published posts. Rate-limited: **30 requests / 60 s**.

| Query param | Description |
|---|---|
| `q` | Full-text search string |
| `tags` | Comma-separated tag names |
| `has_images` | `true` to require image content |
| `has_videos` | `true` to require video content |
| `has_files` | `true` to require file content |
| `author_id` | Filter by author |
| `from_date` | RFC 3339 start date |
| `to_date` | RFC 3339 end date |
| `limit` | Max results (default `20`) |
| `offset` | Pagination offset |

**Response** `200 OK` - `PostListResponse`

---

## Comments

### GET /api/posts/{post_id}/comments

List comments on a post.

| Query param | Default | Description |
|---|---|---|
| `parent_comment_id` | - | Filter to replies of a specific comment |
| `include_replies` | `false` | Nest replies inside each top-level comment |
| `sort` | `newest` | `newest` or `oldest` |
| `limit` | `50` | Max results |
| `offset` | `0` | Pagination offset |

**Response** `200 OK`

```json
{
  "comments": [
    {
      "comment_id": "string",
      "post_id": "string",
      "author_id": "string",
      "author_display_name": "string",
      "author_avatar_url": "string | null",
      "parent_comment_id": "string | null",
      "content_type": "text",
      "content": "string",
      "media_url": "string | null",
      "media_id": "string | null",
      "filename": "string | null",
      "is_deleted": false,
      "reply_count": 0,
      "created_at": "string",
      "updated_at": "string"
    }
  ],
  "total": 0,
  "has_more": false
}
```

---

### POST /api/posts/{post_id}/comments

Post a comment. Requires authentication.

```json
{
  "content_type": "text",
  "content": "string",
  "parent_comment_id": null,
  "filename": null,
  "mime_type": null
}
```

`content_type` may be `text`, `image`, or `gif`. For media comments, `content` holds base64 data or a URL.

**Response** `201 Created` - `CommentResponse`

---

### PUT /api/comments/{comment_id}

Edit a comment (own comments only; text content only).

```json
{ "content": "string" }
```

**Response** `200 OK` - `CommentResponse`

---

### DELETE /api/comments/{comment_id}

Delete a comment (own comments; admin can delete any). The record is soft-deleted (`is_deleted: true`).

**Response** `204 No Content`

---

## Media

Upload limit: **20 requests / 60 s**. Maximum body: **50 MB**.

### POST /api/media

Upload a file. Send as `multipart/form-data` - the file part should be named `file`.

The server derives `media_type` from the MIME type of the uploaded part. Optional metadata fields can be included as additional form fields:

| Form field | Description |
|---|---|
| `file` | The file to upload (required) |
| `description` | Caption / description (max 1000 chars) |
| `tags` | Comma-separated tags |

**Response** `201 Created`

```json
{
  "media_id": "string",
  "url": "string",
  "thumbnail_url": "string | null"
}
```

---

### POST /api/media/chunked/init

Initialise a chunked upload for large files.

```json
{
  "media_type": "video",
  "filename": "clip.mp4",
  "mime_type": "video/mp4",
  "total_size": 104857600,
  "chunk_size": 5242880
}
```

**Response** `200 OK`

```json
{
  "upload_id": "string",
  "chunk_size": 5242880,
  "total_chunks": 20
}
```

---

### POST /api/media/chunked/{upload_id}/{chunk_number}

Upload a single chunk. Send raw binary body. `chunk_number` is 0-indexed.

**Response** `200 OK`

---

### POST /api/media/chunked/{upload_id}/complete

Finalise a chunked upload after all chunks are delivered.

No request body required.

**Response** `201 Created` - same shape as `POST /api/media`.

---

### GET /api/media/{media_id}/file

Download the raw media file. Returns the file with appropriate `Content-Type`.

---

### GET /api/media/{media_id}/thumbnail

Fetch the thumbnail. `404` if none exists for the media type.

---

### PUT /api/media/{media_id}

Update media metadata.

```json
{
  "description": "string | null",
  "tags": ["string"]
}
```

**Response** `200 OK` - `MediaItemResponse`

---

### DELETE /api/media/{media_id}

Delete a single media item.

**Response** `204 No Content`

---

### POST /api/media/batch-delete

Delete multiple items in one request.

```json
{ "media_ids": ["string"] }
```

Up to 100 IDs per call.

**Response** `200 OK`

```json
{
  "success_count": 5,
  "failed_ids": []
}
```

---

## Messaging

### GET /api/conversations

List all conversations the authenticated user is a member of.

| Query param | Default | Description |
|---|---|---|
| `limit` | - | Max results |
| `offset` | `0` | Pagination offset |

**Response** `200 OK`

```json
{
  "conversations": [
    {
      "conversation_id": "string",
      "conversation_type": "direct",
      "name": "string | null",
      "display_name": "string | null",
      "member_count": 2,
      "last_message_at": "string | null",
      "unread_count": 0,
      "is_favourite": false
    }
  ]
}
```

---

### POST /api/conversations

Create a new conversation.

```json
{
  "conversation_type": "direct",
  "name": null,
  "member_ids": ["user_id_b"]
}
```

| Field | Notes |
|---|---|
| `conversation_type` | `direct` or `group` |
| `name` | Group name (max 200 chars, only for groups) |
| `member_ids` | For direct: exactly 1 other user ID. For group: at least 1. |

**Response** `201 Created` - `ConversationResponse`

```json
{
  "conversation_id": "string",
  "conversation_type": "string",
  "name": "string | null",
  "members": [
    {
      "user_id": "string",
      "role": "admin",
      "joined_at": "string",
      "is_proxy": false,
      "display_name": "string | null",
      "username": "string | null"
    }
  ],
  "created_at": "string",
  "updated_at": "string"
}
```

---

### GET /api/conversations/{id}

Get full conversation details including member list.

**Response** `200 OK` - `ConversationResponse`

---

### PUT /api/conversations/{id}

Update conversation metadata (group admin only).

```json
{ "name": "New Group Name" }
```

**Response** `200 OK` - `ConversationResponse`

---

### DELETE /api/conversations/{id}

Delete a conversation (group admin or DM participant).

**Response** `204 No Content`

---

### POST /api/conversations/{id}/members

Add a member to a group conversation (group admin only).

```json
{ "user_id": "string" }
```

**Response** `200 OK` - `ConversationResponse`

---

### DELETE /api/conversations/{id}/members/{user_id}

Remove a member from a group conversation (group admin only).

**Response** `204 No Content`

---

### POST /api/conversations/{id}/messages

Send a message to a conversation.

```json
{
  "encrypted_content": "string",
  "media_ids": ["string"]
}
```

`encrypted_content` is the E2E-encrypted ciphertext (Base64, max 65536 chars). `media_ids` references media already uploaded via `POST /api/media`.

**Response** `201 Created`

```json
{
  "message_id": "string",
  "conversation_id": "string",
  "sender_id": "string",
  "sender_email": "string | null",
  "sender_name": "string | null",
  "sender_avatar_url": "string | null",
  "remote_sender_qualified_id": "string | null",
  "encrypted_content": "string",
  "attachments": [
    {
      "media_id": "string",
      "media_type": "string",
      "filename": "string | null",
      "file_size": 0,
      "url": "string",
      "thumbnail_url": "string | null",
      "mime_type": "string | null"
    }
  ],
  "created_at": "string",
  "federated_status": "string | null"
}
```

---

### GET /api/conversations/{id}/messages

List messages in a conversation (newest first).

| Query param | Default | Description |
|---|---|---|
| `limit` | - | Max results |
| `offset` | `0` | Pagination offset |

**Response** `200 OK`

```json
{
  "messages": [ "...ChatMessageResponse..." ],
  "has_more": false
}
```

---

### DELETE /api/messages/{id}

Delete a message (own messages only).

**Response** `204 No Content`

---

### GET /api/messaging/unread

Get unread message counts keyed by `conversation_id`.

**Response** `200 OK`

```json
{
  "counts": {
    "conversation_id_a": 3,
    "conversation_id_b": 0
  }
}
```

---

### GET /api/conversations/{id}/media

List media shared in a conversation.

| Query param | Default | Description |
|---|---|---|
| `media_type` | - | Filter by type (`image`, `video`, `file`) |
| `limit` | `50` | Max results |
| `offset` | `0` | Pagination offset |

**Response** `200 OK` - list of `MessageAttachmentResponse`

---

### GET /api/messaging/preferences

Get messaging preferences.

**Response** `200 OK`

```json
{ "accept_messages": true }
```

---

### PUT /api/messaging/preferences

Update messaging preferences.

```json
{ "accept_messages": false }
```

**Response** `200 OK`

---

### GET /api/messaging/blacklist

List blocked users.

**Response** `200 OK`

```json
{
  "blocks": [
    { "user_id": "string", "blocked_user_id": "string", "created_at": "string" }
  ]
}
```

---

### POST /api/messaging/blacklist

Block a user.

```json
{ "user_id": "string" }
```

**Response** `201 Created` - `BlockedUserResponse`

---

### DELETE /api/messaging/blacklist/{user_id}

Unblock a user.

**Response** `204 No Content`

---

### POST /api/messaging/favourites

Mark a conversation as a favourite.

```json
{ "conversation_id": "string" }
```

**Response** `201 Created`

---

### DELETE /api/messaging/favourites/{conversation_id}

Remove a conversation from favourites.

**Response** `204 No Content`

---

### GET /api/conversations/{id}/background

Get the background image set for a conversation.

**Response** `200 OK`

```json
{ "media_id": "string" }
```

---

### PUT /api/conversations/{id}/background

Set a conversation background image.

```json
{ "media_id": "string" }
```

**Response** `200 OK`

---

### DELETE /api/conversations/{id}/background

Remove the conversation background image.

**Response** `204 No Content`

---

## WebSocket

### GET /api/ws

Upgrade to a WebSocket connection. Requires session cookie. Used for real-time push of new messages, events, and call signals. Rate-limited: **10 connections / 60 s**.

The server pushes JSON frames with a `type` field. Clients do not send frames.

---

## Proxy Management

These endpoints are authenticated with the **user** session cookie (`session_id`), not the proxy session. They let a logged-in user manage their sole paired proxy account.

### POST /api/proxy

Create a proxy account paired to the authenticated user. Each user may have at most one proxy.

```json
{ "username": "string" }
```

`username` must be 2–30 characters and unique across all proxy accounts.

**Response** `201 Created` - `ProxyUserResponse`

```json
{
  "proxy_id": "string",
  "paired_user_id": "string | null",
  "active": false,
  "display_name": "string | null",
  "username": "string",
  "bio": "string | null",
  "avatar_url": "string | null",
  "public_key": "string | null",
  "has_password": false,
  "has_e2e_key": false,
  "has_hmac_key": false,
  "hmac_key_fingerprint": "string | null",
  "created_at": "string",
  "updated_at": "string"
}
```

---

### GET /api/proxy

Get the proxy account paired to the authenticated user. `404` if none exists.

**Response** `200 OK` - `ProxyUserResponse`

---

### PATCH /api/proxy

Update the paired proxy's profile. All fields are optional.

```json
{
  "display_name": "string | null",
  "bio": "string | null",
  "avatar_media_id": "string | null",
  "active": true
}
```

**Response** `200 OK` - `ProxyUserResponse`

---

### PUT /api/proxy/password

Set or replace the proxy's session login password (min 8, max 128 chars).

```json
{ "password": "string" }
```

**Response** `204 No Content`

---

### PUT /api/proxy/hmac-key

Set the HMAC signing key used for one-shot HMAC requests. The key must be exactly 64 lowercase hex characters (32 bytes, client-generated).

```json
{ "hmac_key": "string" }
```

**Response** `204 No Content`

---

### PUT /api/proxy/e2e-key

Upload an E2E key blob for the proxy (opaque ciphertext; server never decrypts it).

```json
{
  "public_key": "string",
  "e2e_key_blob": "string"
}
```

**Response** `204 No Content`

---

### GET /api/proxy/list-public

List all active proxy accounts. Requires user session. Used to look up proxy IDs for group membership.

**Response** `200 OK`

```json
[
  {
    "proxy_id": "string",
    "username": "string",
    "display_name": "string | null",
    "avatar_url": "string | null"
  }
]
```

---

## Proxy Session

Proxy accounts have their own session cookie (`proxy_session_id`) separate from the user session. These endpoints let a proxy authenticate and identify itself.

### POST /api/proxy/login

Start a proxy session. Rate-limited: **5 requests / 60 s**.

```json
{
  "username": "string",
  "password": "string"
}
```

**Response** `200 OK` - sets `proxy_session_id` cookie (HTTP-only).

```json
{
  "proxy_id": "string",
  "username": "string",
  "display_name": "string | null",
  "avatar_url": "string | null"
}
```

---

### POST /api/proxy/logout

End the current proxy session. Cookie is cleared server-side.

No request body required.

**Response** `204 No Content`

---

### GET /api/proxy/me

Return the currently authenticated proxy account. Requires `proxy_session_id` cookie.

**Response** `200 OK` - `ProxyUserResponse`

---

## Error responses

| Status | Meaning |
|---|---|
| `400 Bad Request` | Validation failure |
| `401 Unauthorized` | No session or session expired |
| `403 Forbidden` | Authenticated but not permitted (e.g. not a group admin) |
| `404 Not Found` | Resource does not exist |
| `429 Too Many Requests` | Rate limit exceeded |

---

## Endpoints not documented above

The following session-authenticated endpoints exist but are not covered in this document:

- `GET /api/users` - user directory search
- `PUT /api/auth/me/public-key` - upload E2E public key
- `GET /api/auth/me/e2e-key` / `PUT /api/auth/me/e2e-key` - encrypted key blob storage
- `GET /api/auth/me/password-reset-key/status` / `POST …/generate` / `DELETE …` - recovery key management
- `GET /api/media`, `GET /api/media/images`, `GET /api/media/videos`, `GET /api/media/files` - media gallery listing
- `GET /api/media/storage` - storage usage statistics
- `GET /api/media/{media_id}` - media item metadata
- `GET /api/events`, `GET /api/events/count`, `PUT /api/events/viewed-all`, `PUT /api/events/{id}/viewed`, `DELETE /api/events/{id}` - notification events
- `GET /api/events/prefs` / `PUT /api/events/prefs` - notification preferences
- `PUT /api/profile/email-visible` - toggle email visibility
- `GET /api/link-preview` - server-side link preview fetch
- `GET /api/posts/{post_id}/comments/count` - comment count
- `GET /api/comments/{comment_id}` - single comment fetch
- `GET /api/comments/{comment_id}/replies` - replies list
- `GET /api/calls/ice-config` - STUN/TURN credentials
- `GET /api/calls/history` - call history
- `GET /api/conversations/{id}/calls` / `POST …` - per-conversation call management
- `GET /api/conversations/{id}/active-call` - current active call
- `GET /api/global-call`, `POST /api/global-call/join`, `POST /api/global-call/leave` - server-wide call room
- Proxy management and proxy session endpoints are documented in the Proxy Management and Proxy Session sections above
- All `/api/admin/*` routes - admin-only management endpoints
- All `/api/federation/*` routes - server-to-server federation endpoints
