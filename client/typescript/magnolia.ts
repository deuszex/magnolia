/**
 * Magnolia API client - session and HMAC proxy modes.
 *
 * Uses only the native fetch API (Node 18+) and the `ws` package for WebSocket.
 * No Axios.
 *
 * Session usage:
 *   const client = new MagnoliaClient("https://magnolia.example.com", { timeoutMs: 10_000 });
 *   await client.login("alice", "hunter2");
 *   const posts = await client.listPosts({ limit: 10 });
 *
 * HMAC proxy usage:
 *   const proxy = new MagnoliaHMACClient("https://...", "proxy-id", "64-char-hex", { timeoutMs: 10_000 });
 *   const conv  = await proxy.getOrCreateConversation({ targetUsername: "bob" });
 */

import { createHash, createHmac } from "node:crypto";
import { readFile } from "node:fs/promises";
import { basename, extname } from "node:path";
import type { ClientOptions } from "ws";

//  Errors 

export class APIError extends Error {
  constructor(
    public readonly statusCode: number,
    public readonly body: string,
  ) {
    super(`HTTP ${statusCode}: ${body}`);
    this.name = "APIError";
  }
}

//  Response types 

export interface UserResponse {
  user_id: string;
  email: string | null;
  username: string;
  display_name: string | null;
  avatar_url: string | null;
  verified: boolean;
  admin: boolean;
}

export interface LoginResponse {
  user: UserResponse;
}

export interface ProfileResponse {
  user_id: string;
  email: string | null;
  email_visible: boolean;
  username: string;
  display_name: string | null;
  bio: string | null;
  avatar_url: string | null;
  location: string | null;
  website: string | null;
  public_key: string | null;
  created_at: string;
}

export interface PostContentItem {
  content_id: string;
  content_type: string;
  display_order: number;
  content: string;
  thumbnail_url: string | null;
  filename: string | null;
  mime_type: string | null;
  file_size: number | null;
}

export interface PostResponse {
  post_id: string;
  author_id: string;
  author_name: string | null;
  author_avatar_url: string | null;
  contents: PostContentItem[];
  tags: string[];
  is_published: boolean;
  comment_count: number;
  created_at: string;
  source_server: string | null;
}

export interface PostListResponse {
  posts: PostResponse[];
  total: number;
  has_more: boolean;
  next_cursor: string | null;
}

export interface CommentResponse {
  comment_id: string;
  post_id: string;
  author_id: string;
  author_display_name: string;
  author_avatar_url: string | null;
  parent_comment_id: string | null;
  content_type: string;
  content: string;
  media_url: string | null;
  media_id: string | null;
  filename: string | null;
  is_deleted: boolean;
  reply_count: number;
  created_at: string;
  updated_at: string;
}

export interface CommentListResponse {
  comments: CommentResponse[];
  total: number;
  has_more: boolean;
}

export interface MediaUploadResponse {
  media_id: string;
  url: string;
  thumbnail_url: string | null;
}

export interface MediaItemResponse {
  media_id: string;
  url: string;
  thumbnail_url: string | null;
  description: string | null;
  tags: string[];
}

export interface BatchDeleteResponse {
  success_count: number;
  failed_ids: string[];
}

export interface ConversationMember {
  user_id: string;
  role: string;
  joined_at: string;
  is_proxy: boolean;
  display_name: string | null;
  username: string | null;
}

export interface ConversationResponse {
  conversation_id: string;
  conversation_type: string;
  name: string | null;
  display_name: string | null;
  member_count: number;
  last_message_at: string | null;
  unread_count: number;
  is_favourite: boolean;
  members: ConversationMember[];
  created_at: string;
  updated_at: string;
}

export interface ConversationListResponse {
  conversations: ConversationResponse[];
}

export interface MessageAttachment {
  media_id: string;
  media_type: string;
  filename: string | null;
  file_size: number;
  url: string;
  thumbnail_url: string | null;
  mime_type: string | null;
}

export interface MessageResponse {
  message_id: string;
  conversation_id: string;
  sender_id: string;
  sender_email: string | null;
  sender_name: string | null;
  sender_avatar_url: string | null;
  remote_sender_qualified_id: string | null;
  encrypted_content: string;
  attachments: MessageAttachment[];
  created_at: string;
  federated_status: string | null;
}

export interface MessageListResponse {
  messages: MessageResponse[];
  has_more: boolean;
}

export interface UnreadCountsResponse {
  counts: Record<string, number>;
}

export interface MessagingPreferences {
  accept_messages: boolean;
}

export interface BlockedUser {
  user_id: string;
  blocked_user_id: string;
  created_at: string;
}

export interface BlocklistResponse {
  blocks: BlockedUser[];
}

export interface HMACMessageResponse {
  message_id: string;
  created_at: string;
}

export interface HMACPostResponse {
  post_id: string;
  created_at: string;
}

export interface HMACConversationResponse {
  conversation_id: string;
  created: boolean;
}

export interface ProxyAuthResponse {
  proxy_id: string;
  username: string;
  display_name: string | null;
  avatar_url: string | null;
}

export interface ProxyUserResponse {
  proxy_id: string;
  paired_user_id: string | null;
  active: boolean;
  display_name: string | null;
  username: string;
  bio: string | null;
  avatar_url: string | null;
  public_key: string | null;
  has_password: boolean;
  has_e2e_key: boolean;
  has_hmac_key: boolean;
  hmac_key_fingerprint: string | null;
  created_at: string;
  updated_at: string;
}

export interface PublicProxyResponse {
  proxy_id: string;
  username: string;
  display_name: string | null;
  avatar_url: string | null;
}

export interface UpdateProxyParams {
  display_name?: string | null;
  bio?: string | null;
  avatar_media_id?: string | null;
  active?: boolean;
}

//  Request / param types

export interface PostContentRequest {
  content_type: string;
  display_order: number;
  content: string;
  filename?: string;
  mime_type?: string;
  media_id?: string;
}

export interface UpdateProfileParams {
  display_name?: string | null;
  bio?: string | null;
  avatar_media_id?: string | null;
  location?: string | null;
  website?: string | null;
}

export interface ListPostsParams {
  author_id?: string;
  include_drafts?: boolean;
  content_type?: string;
  limit?: number;
  offset?: number;
  after?: string;
}

export interface SearchPostsParams {
  q?: string;
  tags?: string;
  has_images?: boolean;
  has_videos?: boolean;
  has_files?: boolean;
  author_id?: string;
  from_date?: string;
  to_date?: string;
  limit?: number;
  offset?: number;
}

export interface ListCommentsParams {
  parent_comment_id?: string;
  include_replies?: boolean;
  sort?: "newest" | "oldest";
  limit?: number;
  offset?: number;
}

export interface UploadMediaParams {
  filename?: string;
  mimeType?: string;
  description?: string;
  tags?: string;
}

export interface ListConversationsParams {
  limit?: number;
  offset?: number;
}

export interface ListMessagesParams {
  limit?: number;
  offset?: number;
}

export interface ListConversationMediaParams {
  media_type?: string;
  limit?: number;
  offset?: number;
}

export interface ClientOptions {
  /** Per-request timeout in milliseconds. 0 or omitted means no timeout. */
  timeoutMs?: number;
}

//  Utilities 

function sha256Hex(data: string | Uint8Array): string {
  return createHash("sha256").update(data).digest("hex");
}

/**
 * HMAC-SHA256 where key is the 64-char hex string used as raw UTF-8 bytes,
 * NOT decoded to 32 bytes before use.
 */
function hmacSHA256Hex(key: string, message: string): string {
  return createHmac("sha256", key).update(message).digest("hex");
}

function nowUnix(): number {
  return Math.floor(Date.now() / 1000);
}

function guessMime(filename: string): string {
  const ext = extname(filename).toLowerCase();
  const map: Record<string, string> = {
    ".jpg": "image/jpeg", ".jpeg": "image/jpeg", ".png": "image/png",
    ".gif": "image/gif", ".webp": "image/webp", ".svg": "image/svg+xml",
    ".mp4": "video/mp4", ".webm": "video/webm", ".mov": "video/quicktime",
    ".mp3": "audio/mpeg", ".ogg": "audio/ogg",
    ".pdf": "application/pdf",
    ".zip": "application/zip",
    ".txt": "text/plain", ".md": "text/markdown",
  };
  return map[ext] ?? "application/octet-stream";
}

function buildQuery(params: Record<string, string | number | boolean | undefined>): string {
  const q = new URLSearchParams();
  for (const [k, v] of Object.entries(params)) {
    if (v !== undefined && v !== "" && v !== false) {
      q.set(k, String(v));
    }
  }
  const s = q.toString();
  return s ? `?${s}` : "";
}

/** Creates an AbortSignal that fires after `ms` milliseconds. Returns undefined if ms is falsy. */
function timeoutSignal(ms: number | undefined): AbortSignal | undefined {
  if (!ms) return undefined;
  return AbortSignal.timeout(ms);
}

async function checkResponse(resp: Response): Promise<void> {
  if (!resp.ok) {
    const body = await resp.text();
    throw new APIError(resp.status, body);
  }
}

//  Cookie jar (Node fetch doesn't handle cookies automatically) 

class CookieJar {
  private cookies: Map<string, string> = new Map();

  update(resp: Response): void {
    // Node 18+ fetch exposes Set-Cookie via getSetCookie()
    const setCookieHeaders: string[] =
      typeof (resp.headers as any).getSetCookie === "function"
        ? (resp.headers as any).getSetCookie()
        : [];

    for (const raw of setCookieHeaders) {
      const [pair] = raw.split(";");
      const eq = pair.indexOf("=");
      if (eq !== -1) {
        const name = pair.slice(0, eq).trim();
        const value = pair.slice(eq + 1).trim();
        this.cookies.set(name, value);
      }
    }
  }

  header(): string {
    return Array.from(this.cookies.entries())
      .map(([k, v]) => `${k}=${v}`)
      .join("; ");
  }
}

//  Session client 

export class MagnoliaClient {
  private readonly baseURL: string;
  private readonly timeoutMs: number | undefined;
  private readonly jar = new CookieJar();

  constructor(baseURL: string, opts: ClientOptions = {}) {
    this.baseURL = baseURL.replace(/\/$/, "");
    this.timeoutMs = opts.timeoutMs || undefined;
  }

  //  Internal helpers 

  private fetchInit(extra: RequestInit = {}): RequestInit {
    const cookieHeader = this.jar.header();
    const headers: Record<string, string> = {
      ...(extra.headers as Record<string, string> ?? {}),
    };
    if (cookieHeader) headers["Cookie"] = cookieHeader;

    return {
      ...extra,
      headers,
      signal: timeoutSignal(this.timeoutMs),
    };
  }

  private async request(path: string, init: RequestInit = {}): Promise<Response> {
    const resp = await fetch(this.baseURL + path, this.fetchInit(init));
    this.jar.update(resp);
    await checkResponse(resp);
    return resp;
  }

  private async getJSON<T>(path: string, query = ""): Promise<T> {
    const resp = await this.request(path + query);
    return resp.json() as Promise<T>;
  }

  private async getRaw(path: string): Promise<Uint8Array> {
    const resp = await this.request(path);
    return new Uint8Array(await resp.arrayBuffer());
  }

  private async sendJSON<T>(method: string, path: string, body?: unknown): Promise<T> {
    const resp = await this.request(path, {
      method,
      headers: body !== undefined ? { "Content-Type": "application/json" } : {},
      body: body !== undefined ? JSON.stringify(body) : undefined,
    });
    // 204 / 205 have no body
    if (resp.status === 204 || resp.status === 205) return undefined as T;
    return resp.json() as Promise<T>;
  }

  private async postJSON<T>(path: string, body?: unknown): Promise<T> {
    return this.sendJSON<T>("POST", path, body);
  }

  private async putJSON<T>(path: string, body: unknown): Promise<T> {
    return this.sendJSON<T>("PUT", path, body);
  }

  private async patchJSON<T>(path: string, body: unknown): Promise<T> {
    return this.sendJSON<T>("PATCH", path, body);
  }

  private async deleteReq(path: string): Promise<void> {
    await this.request(path, { method: "DELETE" });
  }

  private async postMultipart<T>(
    path: string,
    fields: Record<string, string>,
    file: { name: string; mimeType: string; data: Uint8Array },
  ): Promise<T> {
    const form = new FormData();
    for (const [k, v] of Object.entries(fields)) form.set(k, v);
    form.set("file", new Blob([file.data.buffer as ArrayBuffer], { type: file.mimeType }), file.name);
    const resp = await this.request(path, { method: "POST", body: form });
    return resp.json() as Promise<T>;
  }

  //  Auth 

  async login(identifier: string, password: string): Promise<LoginResponse> {
    return this.postJSON("/api/auth/login", { identifier, password });
  }

  async logout(): Promise<void> {
    await this.postJSON("/api/auth/logout");
  }

  // The server returns a flat UserResponse for GET /api/auth/me,
  // not wrapped in {"user": ...} like the login response.
  async me(): Promise<UserResponse> {
    return this.getJSON("/api/auth/me");
  }

  async changePassword(currentPassword: string, newPassword: string, newPasswordConfirm: string): Promise<void> {
    await this.postJSON("/api/auth/change-password", {
      current_password: currentPassword,
      new_password: newPassword,
      new_password_confirm: newPasswordConfirm,
    });
  }

  //  Profile 

  async getProfile(userID: string): Promise<ProfileResponse> {
    return this.getJSON(`/api/users/${userID}/profile`);
  }

  async updateProfile(params: UpdateProfileParams): Promise<ProfileResponse> {
    return this.putJSON("/api/profile", params);
  }

  //  Posts 

  async listPosts(params: ListPostsParams = {}): Promise<PostListResponse> {
    return this.getJSON("/api/posts", buildQuery({
      author_id: params.author_id,
      include_drafts: params.include_drafts,
      content_type: params.content_type,
      limit: params.limit,
      offset: params.offset,
      after: params.after,
    }));
  }

  async getPost(postID: string): Promise<PostResponse> {
    return this.getJSON(`/api/posts/${postID}`);
  }

  async createPost(contents: PostContentRequest[], publish = false, tags: string[] = []): Promise<PostResponse> {
    return this.postJSON("/api/posts", { contents, publish, tags });
  }

  async updatePost(postID: string, fields: {
    contents?: PostContentRequest[];
    publish?: boolean;
    tags?: string[];
  }): Promise<PostResponse> {
    return this.putJSON(`/api/posts/${postID}`, fields);
  }

  async deletePost(postID: string): Promise<void> {
    return this.deleteReq(`/api/posts/${postID}`);
  }

  async publishPost(postID: string): Promise<PostResponse> {
    return this.postJSON(`/api/posts/${postID}/publish`);
  }

  async searchPosts(params: SearchPostsParams = {}): Promise<PostListResponse> {
    return this.getJSON("/api/posts/search", buildQuery({
      q: params.q,
      tags: params.tags,
      has_images: params.has_images,
      has_videos: params.has_videos,
      has_files: params.has_files,
      author_id: params.author_id,
      from_date: params.from_date,
      to_date: params.to_date,
      limit: params.limit,
      offset: params.offset,
    }));
  }

  //  Comments 

  async listComments(postID: string, params: ListCommentsParams = {}): Promise<CommentListResponse> {
    return this.getJSON(`/api/posts/${postID}/comments`, buildQuery({
      parent_comment_id: params.parent_comment_id,
      include_replies: params.include_replies,
      sort: params.sort,
      limit: params.limit,
      offset: params.offset,
    }));
  }

  async createComment(
    postID: string,
    content: string,
    opts: { contentType?: string; parentCommentID?: string; filename?: string; mimeType?: string } = {},
  ): Promise<CommentResponse> {
    return this.postJSON(`/api/posts/${postID}/comments`, {
      content_type: opts.contentType ?? "text",
      content,
      parent_comment_id: opts.parentCommentID ?? null,
      filename: opts.filename ?? null,
      mime_type: opts.mimeType ?? null,
    });
  }

  async updateComment(commentID: string, content: string): Promise<CommentResponse> {
    return this.putJSON(`/api/comments/${commentID}`, { content });
  }

  async deleteComment(commentID: string): Promise<void> {
    return this.deleteReq(`/api/comments/${commentID}`);
  }

  //  Media 

  /** Upload a file from disk. */
  async uploadMediaFile(filePath: string, params: UploadMediaParams = {}): Promise<MediaUploadResponse> {
    const data = await readFile(filePath);
    const filename = params.filename ?? basename(filePath);
    return this.uploadMediaBytes(new Uint8Array(data), { ...params, filename });
  }

  /** Upload raw bytes. */
  async uploadMediaBytes(data: Uint8Array, params: UploadMediaParams = {}): Promise<MediaUploadResponse> {
    const filename = params.filename ?? "upload";
    const mimeType = params.mimeType ?? guessMime(filename);
    const fields: Record<string, string> = {};
    if (params.description) fields["description"] = params.description;
    if (params.tags) fields["tags"] = params.tags;
    return this.postMultipart("/api/media", fields, { name: filename, mimeType, data });
  }

  /**
   * Chunked upload for large files.
   * @param chunkSize Bytes per chunk; defaults to 5 MiB.
   */
  async uploadMediaChunked(filePath: string, mediaType: string, chunkSize = 5 * 1024 * 1024): Promise<MediaUploadResponse> {
    const data = await readFile(filePath);
    const filename = basename(filePath);

    const init = await this.postJSON<{ upload_id: string; chunk_size: number; total_chunks: number }>(
      "/api/media/chunked/init",
      { media_type: mediaType, filename, mime_type: guessMime(filename), total_size: data.byteLength, chunk_size: chunkSize },
    );

    const serverChunk = init.chunk_size;
    for (let i = 0; i * serverChunk < data.byteLength; i++) {
      const slice = data.subarray(i * serverChunk, Math.min((i + 1) * serverChunk, data.byteLength));
      const resp = await fetch(
        `${this.baseURL}/api/media/chunked/${init.upload_id}/${i}`,
        this.fetchInit({
          method: "POST",
          headers: { "Content-Type": "application/octet-stream" },
          body: slice,
        }),
      );
      this.jar.update(resp);
      await checkResponse(resp);
    }

    return this.postJSON(`/api/media/chunked/${init.upload_id}/complete`);
  }

  async downloadMedia(mediaID: string): Promise<Uint8Array> {
    return this.getRaw(`/api/media/${mediaID}/file`);
  }

  async getThumbnail(mediaID: string): Promise<Uint8Array> {
    return this.getRaw(`/api/media/${mediaID}/thumbnail`);
  }

  async updateMedia(mediaID: string, fields: { description?: string | null; tags?: string[] }): Promise<MediaItemResponse> {
    return this.putJSON(`/api/media/${mediaID}`, fields);
  }

  async deleteMedia(mediaID: string): Promise<void> {
    return this.deleteReq(`/api/media/${mediaID}`);
  }

  async batchDeleteMedia(mediaIDs: string[]): Promise<BatchDeleteResponse> {
    return this.postJSON("/api/media/batch-delete", { media_ids: mediaIDs });
  }

  //  Conversations 

  async listConversations(params: ListConversationsParams = {}): Promise<ConversationListResponse> {
    return this.getJSON("/api/conversations", buildQuery({ limit: params.limit, offset: params.offset }));
  }

  async createConversation(type: "direct" | "group", memberIDs: string[], name?: string): Promise<ConversationResponse> {
    return this.postJSON("/api/conversations", { conversation_type: type, member_ids: memberIDs, name: name ?? null });
  }

  async getConversation(conversationID: string): Promise<ConversationResponse> {
    return this.getJSON(`/api/conversations/${conversationID}`);
  }

  async updateConversation(conversationID: string, name: string): Promise<ConversationResponse> {
    return this.putJSON(`/api/conversations/${conversationID}`, { name });
  }

  async deleteConversation(conversationID: string): Promise<void> {
    return this.deleteReq(`/api/conversations/${conversationID}`);
  }

  async addConversationMember(conversationID: string, userID: string): Promise<ConversationResponse> {
    return this.postJSON(`/api/conversations/${conversationID}/members`, { user_id: userID });
  }

  async removeConversationMember(conversationID: string, userID: string): Promise<void> {
    return this.deleteReq(`/api/conversations/${conversationID}/members/${userID}`);
  }

  //  Messages 

  async sendMessage(conversationID: string, encryptedContent: string, mediaIDs: string[] = []): Promise<MessageResponse> {
    return this.postJSON(`/api/conversations/${conversationID}/messages`, {
      encrypted_content: encryptedContent,
      media_ids: mediaIDs,
    });
  }

  async listMessages(conversationID: string, params: ListMessagesParams = {}): Promise<MessageListResponse> {
    return this.getJSON(
      `/api/conversations/${conversationID}/messages`,
      buildQuery({ limit: params.limit, offset: params.offset }),
    );
  }

  async deleteMessage(messageID: string): Promise<void> {
    return this.deleteReq(`/api/messages/${messageID}`);
  }

  async getUnreadCounts(): Promise<UnreadCountsResponse> {
    return this.getJSON("/api/messaging/unread");
  }

  async listConversationMedia(conversationID: string, params: ListConversationMediaParams = {}): Promise<MessageAttachment[]> {
    return this.getJSON(
      `/api/conversations/${conversationID}/media`,
      buildQuery({ media_type: params.media_type, limit: params.limit, offset: params.offset }),
    );
  }

  //  Messaging prefs / blacklist / favourites / background 

  async getMessagingPreferences(): Promise<MessagingPreferences> {
    return this.getJSON("/api/messaging/preferences");
  }

  async updateMessagingPreferences(acceptMessages: boolean): Promise<MessagingPreferences> {
    return this.putJSON("/api/messaging/preferences", { accept_messages: acceptMessages });
  }

  async listBlockedUsers(): Promise<BlocklistResponse> {
    return this.getJSON("/api/messaging/blacklist");
  }

  async blockUser(userID: string): Promise<BlockedUser> {
    return this.postJSON("/api/messaging/blacklist", { user_id: userID });
  }

  async unblockUser(userID: string): Promise<void> {
    return this.deleteReq(`/api/messaging/blacklist/${userID}`);
  }

  async addFavourite(conversationID: string): Promise<void> {
    await this.postJSON("/api/messaging/favourites", { conversation_id: conversationID });
  }

  async removeFavourite(conversationID: string): Promise<void> {
    return this.deleteReq(`/api/messaging/favourites/${conversationID}`);
  }

  async getConversationBackground(conversationID: string): Promise<string> {
    const r = await this.getJSON<{ media_id: string }>(`/api/conversations/${conversationID}/background`);
    return r.media_id;
  }

  async setConversationBackground(conversationID: string, mediaID: string): Promise<void> {
    await this.putJSON(`/api/conversations/${conversationID}/background`, { media_id: mediaID });
  }

  async deleteConversationBackground(conversationID: string): Promise<void> {
    return this.deleteReq(`/api/conversations/${conversationID}/background`);
  }

  //  Proxy management (user session)

  async createProxy(username: string): Promise<ProxyUserResponse> {
    return this.postJSON("/api/proxy", { username });
  }

  async getMyProxy(): Promise<ProxyUserResponse> {
    return this.getJSON("/api/proxy");
  }

  async updateMyProxy(params: UpdateProxyParams): Promise<ProxyUserResponse> {
    return this.patchJSON("/api/proxy", params);
  }

  async setProxyPassword(password: string): Promise<void> {
    await this.putJSON("/api/proxy/password", { password });
  }

  async setProxyHMACKey(hmacKey: string): Promise<void> {
    await this.putJSON("/api/proxy/hmac-key", { hmac_key: hmacKey });
  }

  async setProxyE2EKey(publicKey: string, e2eKeyBlob: string): Promise<void> {
    await this.putJSON("/api/proxy/e2e-key", { public_key: publicKey, e2e_key_blob: e2eKeyBlob });
  }

  async listPublicProxies(): Promise<PublicProxyResponse[]> {
    return this.getJSON("/api/proxy/list-public");
  }

  //  WebSocket

  /**
   * Open a real-time WebSocket connection using the current session cookie.
   * The server pushes JSON frames; clients do not send frames.
   *
   * Requires the `ws` package.
   *
   * @example
   * const ws = await client.connectWebSocket();
   * ws.on("message", (raw) => {
   *   const event = JSON.parse(raw.toString());
   *   console.log(event.type, event);
   * });
   */
  async connectWebSocket(): Promise<import("ws").WebSocket> {
    const { WebSocket } = await import("ws");
    const wsURL = (this.baseURL + "/api/ws")
      .replace(/^http:\/\//, "ws://")
      .replace(/^https:\/\//, "wss://");
    const cookieHeader = this.jar.header();
    const wsOpts: ClientOptions = cookieHeader ? { headers: { Cookie: cookieHeader } } : {};
    return new WebSocket(wsURL, wsOpts);
  }
}

//  Proxy session client

/**
 * Session client for a proxy account.
 * Uses the `proxy_session_id` cookie, separate from the user `session_id`.
 */
export class MagnoliaProxySessionClient {
  private readonly baseURL: string;
  private readonly timeoutMs: number | undefined;
  private readonly jar = new CookieJar();

  constructor(baseURL: string, opts: ClientOptions = {}) {
    this.baseURL = baseURL.replace(/\/$/, "");
    this.timeoutMs = opts.timeoutMs || undefined;
  }

  private async request(path: string, init: RequestInit = {}): Promise<Response> {
    const cookieHeader = this.jar.header();
    const headers: Record<string, string> = {
      ...(init.headers as Record<string, string> ?? {}),
    };
    if (cookieHeader) headers["Cookie"] = cookieHeader;
    const resp = await fetch(this.baseURL + path, {
      ...init,
      headers,
      signal: timeoutSignal(this.timeoutMs),
    });
    this.jar.update(resp);
    await checkResponse(resp);
    return resp;
  }

  private async sendJSON<T>(method: string, path: string, body?: unknown): Promise<T> {
    const resp = await this.request(path, {
      method,
      headers: body !== undefined ? { "Content-Type": "application/json" } : {},
      body: body !== undefined ? JSON.stringify(body) : undefined,
    });
    if (resp.status === 204 || resp.status === 205) return undefined as T;
    return resp.json() as Promise<T>;
  }

  async login(username: string, password: string): Promise<ProxyAuthResponse> {
    return this.sendJSON("POST", "/api/proxy/login", { username, password });
  }

  async logout(): Promise<void> {
    await this.sendJSON("POST", "/api/proxy/logout");
  }

  async me(): Promise<ProxyUserResponse> {
    const resp = await this.request("/api/proxy/me");
    return resp.json() as Promise<ProxyUserResponse>;
  }
}

//  HMAC proxy client

export class MagnoliaHMACClient {
  private readonly baseURL: string;
  private readonly proxyID: string;
  private readonly hmacKey: string;
  private readonly timeoutMs: number | undefined;

  constructor(baseURL: string, proxyID: string, hmacKey: string, opts: ClientOptions = {}) {
    this.baseURL = baseURL.replace(/\/$/, "");
    this.proxyID = proxyID;
    this.hmacKey = hmacKey;
    this.timeoutMs = opts.timeoutMs || undefined;
  }

  private sign(message: string): string {
    return hmacSHA256Hex(this.hmacKey, message);
  }

  private async postJSON<T>(path: string, body: unknown): Promise<T> {
    const resp = await fetch(this.baseURL + path, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
      signal: timeoutSignal(this.timeoutMs),
    });
    await checkResponse(resp);
    return resp.json() as Promise<T>;
  }

  //  HMAC endpoints 

  /** Send a message to a conversation the proxy is already a member of. */
  async sendMessage(conversationID: string, encryptedContent: string, mediaIDs: string[] = []): Promise<HMACMessageResponse> {
    const ts = nowUnix();
    const contentHash = sha256Hex(encryptedContent);
    const signed = `${this.proxyID}:${conversationID}:${contentHash}:${ts}`;
    return this.postJSON("/api/proxy/hmac/send-message", {
      proxy_id: this.proxyID,
      conversation_id: conversationID,
      encrypted_content: encryptedContent,
      media_ids: mediaIDs,
      signature: this.sign(signed),
      timestamp: ts,
    });
  }

  /**
   * Create a post as the proxy. The proxy must be paired to a user account.
   */
  async createPost(contents: PostContentRequest[], publish = false, tags: string[] = []): Promise<HMACPostResponse> {
    const ts = nowUnix();
    const publishBit = publish ? "1" : "0";

    const sorted = [...contents].sort((a, b) => a.display_order - b.display_order);
    const lines = sorted.map(c => `${c.display_order}|${c.content_type}|${c.content}`);
    let canonical = lines.join("\n");
    canonical += "\ntags:" + [...tags].sort().join(",");
    canonical += `\npublish:${publishBit}`;

    const bodyHash = sha256Hex(canonical);
    const signed = `${this.proxyID}:${bodyHash}:${publishBit}:${ts}`;

    return this.postJSON("/api/proxy/hmac/create-post", {
      proxy_id: this.proxyID,
      contents,
      publish,
      tags,
      signature: this.sign(signed),
      timestamp: ts,
    });
  }

  /**
   * Get or create a direct conversation between the proxy and a target user.
   * Provide exactly one of targetUserID or targetUsername.
   */
  async getOrCreateConversation(opts: { targetUserID?: string; targetUsername?: string }): Promise<HMACConversationResponse> {
    const { targetUserID, targetUsername } = opts;
    if (!!targetUserID === !!targetUsername) {
      throw new Error("Provide exactly one of targetUserID or targetUsername");
    }
    const ts = nowUnix();
    const signed = `${this.proxyID}:${ts}`;
    return this.postJSON("/api/proxy/hmac/get-or-create-conversation", {
      proxy_id: this.proxyID,
      target_user_id: targetUserID ?? null,
      target_username: targetUsername ?? null,
      signature: this.sign(signed),
      timestamp: ts,
    });
  }

  /** Upload a file from disk as the proxy. */
  async uploadMediaFile(filePath: string): Promise<MediaUploadResponse> {
    const data = await readFile(filePath);
    return this.uploadMediaBytes(new Uint8Array(data), basename(filePath));
  }

  /**
   * Upload raw bytes as the proxy.
   * The file hash is computed over the raw bytes directly.
   */
  async uploadMediaBytes(data: Uint8Array, filename: string, mimeType?: string): Promise<MediaUploadResponse> {
    const mime = mimeType ?? guessMime(filename);
    const ts = nowUnix();
    const fileHash = sha256Hex(data);
    const signed = `${this.proxyID}:${fileHash}:${ts}`;
    const sig = this.sign(signed);

    const form = new FormData();
    form.set("proxy_id", this.proxyID);
    form.set("signature", sig);
    form.set("timestamp", String(ts));
    form.set("file", new Blob([data.buffer as ArrayBuffer], { type: mime }), filename);

    const resp = await fetch(this.baseURL + "/api/proxy/hmac/upload-media", {
      method: "POST",
      body: form,
      signal: timeoutSignal(this.timeoutMs),
    });
    await checkResponse(resp);
    return resp.json() as Promise<MediaUploadResponse>;
  }
}
