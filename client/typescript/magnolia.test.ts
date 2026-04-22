/**
 * Integration tests for MagnoliaClient and MagnoliaHMACClient.
 *
 * Run against a live server:
 *
 *   MAGNOLIA_BASE_URL=https://... MAGNOLIA_USERNAME=alice MAGNOLIA_PASSWORD=... \
 *       node --experimental-strip-types magnolia.test.ts
 *
 * Required env vars:
 *   MAGNOLIA_BASE_URL      server root, e.g. https://magnolia.example.com
 *   MAGNOLIA_USERNAME      login identifier (username or email)
 *   MAGNOLIA_PASSWORD      login password
 *
 * Optional env vars (related tests are skipped when absent):
 *   MAGNOLIA_PROXY_ID        )  both required for HMAC tests
 *   MAGNOLIA_HMAC_KEY        )
 *   MAGNOLIA_PROXY_USERNAME  )  both required for proxy session tests
 *   MAGNOLIA_PROXY_PASSWORD  )
 *   MAGNOLIA_MEDIA_FILE      path to any file; enables media upload/download tests
 *   MAGNOLIA_TARGET_USER_ID  enables conversation and message tests
 */

import { describe, it, before, after } from "node:test";
import assert from "node:assert/strict";
import { extname } from "node:path";
import { MagnoliaClient, MagnoliaHMACClient, MagnoliaProxySessionClient, APIError, type PostContentRequest } from "./magnolia.ts";

const TIMEOUT_MS = 10_000;
const PREFIX = `mgtest_${Math.floor(Date.now() / 1000)}`;

//  Helpers 

function env(key: string): string {
  return process.env[key] ?? "";
}

function skip(key: string): never {
  // node:test has no native skip-from-within; throw a recognisable sentinel.
  throw Object.assign(new Error(`SKIP: env var not set: ${key}`), { skip: true });
}

function requireEnv(...keys: string[]): void {
  for (const k of keys) if (!env(k)) skip(k);
}

function mediaTypeFor(filePath: string): string {
  const ext = extname(filePath).toLowerCase();
  if ([".jpg", ".jpeg", ".png", ".gif", ".webp"].includes(ext)) return "image";
  if ([".mp4", ".webm", ".mov"].includes(ext)) return "video";
  return "file";
}

/** Wraps a test body so that skip-sentinels are reported as skips, not failures. */
async function run(t: { skip: (msg?: string) => void }, fn: () => Promise<void>): Promise<void> {
  try {
    await fn();
  } catch (err: any) {
    if (err?.skip) {
      t.skip(err.message);
    } else {
      throw err;
    }
  }
}

//  State shared across tests 

let client: MagnoliaClient;
let userID = "";
let postID = "";
let commentID = "";
let mediaID = "";
let convID = "";
let msgID = "";

//  Session client tests 

describe("Session client", () => {
  before(async () => {
    requireEnv("MAGNOLIA_BASE_URL", "MAGNOLIA_USERNAME", "MAGNOLIA_PASSWORD");
    client = new MagnoliaClient(env("MAGNOLIA_BASE_URL"), { timeoutMs: TIMEOUT_MS });
    const resp = await client.login(env("MAGNOLIA_USERNAME"), env("MAGNOLIA_PASSWORD"));
    assert.ok(resp.user.user_id, "login returned empty user_id");
    userID = resp.user.user_id;
  });

  after(async () => {
    // Clean up in dependency order.
    if (msgID)     { try { await client.deleteMessage(msgID);       } catch {} }
    if (commentID) { try { await client.deleteComment(commentID);   } catch {} }
    if (mediaID)   { try { await client.deleteMedia(mediaID);       } catch {} }
    if (convID)    { try { await client.deleteConversation(convID); } catch {} }
    if (postID)    { try { await client.deletePost(postID);         } catch {} }
    try { await client.logout(); } catch {}
  });

  //  Auth 

  it("Auth/Me returns the logged-in user", async () => {
    const resp = await client.me();
    assert.equal(resp.user.user_id, userID);
  });

  //  Profile 

  it("Profile/Get returns the user's profile", async () => {
    const resp = await client.getProfile(userID);
    assert.equal(resp.user_id, userID);
  });

  it("Profile/Update round-trips bio and restores it", async () => {
    const orig = await client.getProfile(userID);
    const newBio = `${PREFIX}_bio`;
    try {
      const updated = await client.updateProfile({ bio: newBio });
      assert.equal(updated.bio, newBio);
    } finally {
      await client.updateProfile({ bio: orig.bio });
    }
  });

  //  Posts 

  it("Posts/Create saves a draft", async () => {
    const resp = await client.createPost(
      [{ content_type: "text", display_order: 0, content: `${PREFIX} post` }],
      false,
      [PREFIX],
    );
    assert.ok(resp.post_id, "empty post_id");
    assert.equal(resp.is_published, false);
    postID = resp.post_id;
  });

  it("Posts/Get retrieves the post", async () => {
    assert.ok(postID, "no postID - previous test skipped?");
    const resp = await client.getPost(postID);
    assert.equal(resp.post_id, postID);
  });

  it("Posts/Update changes content", async () => {
    assert.ok(postID);
    const newContent = `${PREFIX} updated`;
    const resp = await client.updatePost(postID, {
      contents: [{ content_type: "text", display_order: 0, content: newContent }],
    });
    assert.equal(resp.contents[0]!.content, newContent);
  });

  it("Posts/Publish toggles is_published", async () => {
    assert.ok(postID);
    const resp = await client.publishPost(postID);
    assert.equal(resp.is_published, true);
  });

  it("Posts/List returns an array", async () => {
    const resp = await client.listPosts({ limit: 5 });
    assert.ok(Array.isArray(resp.posts));
  });

  it("Posts/Search returns an array", async () => {
    const resp = await client.searchPosts({ q: PREFIX });
    assert.ok(Array.isArray(resp.posts));
  });

  //  Comments 

  it("Comments/Create posts a comment", async () => {
    assert.ok(postID);
    const resp = await client.createComment(postID, `${PREFIX} comment`);
    assert.ok(resp.comment_id, "empty comment_id");
    assert.equal(resp.content, `${PREFIX} comment`);
    commentID = resp.comment_id;
  });

  it("Comments/Update changes content", async () => {
    assert.ok(commentID);
    const edited = `${PREFIX} edited`;
    const resp = await client.updateComment(commentID, edited);
    assert.equal(resp.content, edited);
  });

  it("Comments/List includes the created comment", async () => {
    assert.ok(postID && commentID);
    const resp = await client.listComments(postID);
    assert.ok(resp.comments.some(c => c.comment_id === commentID));
  });

  //  Media 

  it("Media/Upload stores a file", async (t) => {
    await run(t, async () => {
      requireEnv("MAGNOLIA_MEDIA_FILE");
      const resp = await client.uploadMediaFile(env("MAGNOLIA_MEDIA_FILE"));
      assert.ok(resp.media_id, "empty media_id");
      assert.ok(resp.url);
      mediaID = resp.media_id;
    });
  });

  it("Media/Download returns bytes", async (t) => {
    await run(t, async () => {
      if (!mediaID) skip("MAGNOLIA_MEDIA_FILE");
      const data = await client.downloadMedia(mediaID);
      assert.ok(data.byteLength > 0);
    });
  });

  it("Media/Thumbnail returns bytes or 404 for non-image/video", async (t) => {
    await run(t, async () => {
      if (!mediaID) skip("MAGNOLIA_MEDIA_FILE");
      try {
        const data = await client.getThumbnail(mediaID);
        assert.ok(data.byteLength > 0);
      } catch (err) {
        if (err instanceof APIError && err.statusCode === 404) return;
        throw err;
      }
    });
  });

  it("Media/Update sets description", async (t) => {
    await run(t, async () => {
      if (!mediaID) skip("MAGNOLIA_MEDIA_FILE");
      await client.updateMedia(mediaID, { description: `${PREFIX} media` });
    });
  });

  it("Media/BatchDelete removes a second upload", async (t) => {
    await run(t, async () => {
      requireEnv("MAGNOLIA_MEDIA_FILE");
      const extra = await client.uploadMediaFile(env("MAGNOLIA_MEDIA_FILE"));
      const result = await client.batchDeleteMedia([extra.media_id]);
      assert.equal(result.success_count, 1);
    });
  });

  it("Media/UploadChunked stores a file via chunked protocol", async (t) => {
    await run(t, async () => {
      requireEnv("MAGNOLIA_MEDIA_FILE");
      const filePath = env("MAGNOLIA_MEDIA_FILE");
      // 64 KiB chunks - forces multiple chunks even on small files
      const resp = await client.uploadMediaChunked(filePath, mediaTypeFor(filePath), 64 * 1024);
      assert.ok(resp.media_id, "empty media_id");
      await client.deleteMedia(resp.media_id);
    });
  });

  //  Conversations + Messages 

  it("Conversations/Create opens a direct conversation", async (t) => {
    await run(t, async () => {
      requireEnv("MAGNOLIA_TARGET_USER_ID");
      const resp = await client.createConversation("direct", [env("MAGNOLIA_TARGET_USER_ID")]);
      assert.ok(resp.conversation_id);
      convID = resp.conversation_id;
    });
  });

  it("Conversations/Get returns members list", async (t) => {
    await run(t, async () => {
      if (!convID) skip("MAGNOLIA_TARGET_USER_ID");
      const resp = await client.getConversation(convID);
      assert.equal(resp.conversation_id, convID);
      assert.ok(resp.members.some(m => m.user_id === env("MAGNOLIA_TARGET_USER_ID")));
    });
  });

  it("Conversations/List includes created conversation", async (t) => {
    await run(t, async () => {
      if (!convID) skip("MAGNOLIA_TARGET_USER_ID");
      const resp = await client.listConversations({ limit: 50 });
      assert.ok(resp.conversations.some(c => c.conversation_id === convID));
    });
  });

  it("Messages/Send delivers a message", async (t) => {
    await run(t, async () => {
      if (!convID) skip("MAGNOLIA_TARGET_USER_ID");
      const resp = await client.sendMessage(convID, `${PREFIX}_payload`);
      assert.ok(resp.message_id);
      msgID = resp.message_id;
    });
  });

  it("Messages/List includes the sent message", async (t) => {
    await run(t, async () => {
      if (!convID || !msgID) skip("MAGNOLIA_TARGET_USER_ID");
      const resp = await client.listMessages(convID);
      assert.ok(resp.messages.some(m => m.message_id === msgID));
    });
  });

  it("Messages/UnreadCounts returns counts map", async () => {
    const resp = await client.getUnreadCounts();
    assert.ok(typeof resp.counts === "object");
  });

  //  Proxy management (user session)

  it("Proxy/GetMyProxy returns proxy or skips if none paired", async (t) => {
    await run(t, async () => {
      try {
        const resp = await client.getMyProxy();
        assert.ok(resp.proxy_id, "empty proxy_id");
      } catch (err) {
        if (err instanceof APIError && err.statusCode === 404) {
          skip("no proxy account paired to this user");
        }
        throw err;
      }
    });
  });

  it("Proxy/ListPublicProxies returns an array", async () => {
    const resp = await client.listPublicProxies();
    assert.ok(Array.isArray(resp));
  });

  //  Messaging preferences

  it("MessagingPrefs/Toggle round-trips and restores", async () => {
    const prefs = await client.getMessagingPreferences();
    const original = prefs.accept_messages;
    try {
      const toggled = await client.updateMessagingPreferences(!original);
      assert.equal(toggled.accept_messages, !original);
    } finally {
      await client.updateMessagingPreferences(original);
    }
  });
});

//  Proxy session client tests

describe("Proxy session client", () => {
  it("ProxySession/Login-Me-Logout round-trip", async (t) => {
    await run(t, async () => {
      requireEnv("MAGNOLIA_BASE_URL", "MAGNOLIA_PROXY_USERNAME", "MAGNOLIA_PROXY_PASSWORD");
      const proxy = new MagnoliaProxySessionClient(env("MAGNOLIA_BASE_URL"), { timeoutMs: TIMEOUT_MS });
      const resp = await proxy.login(env("MAGNOLIA_PROXY_USERNAME"), env("MAGNOLIA_PROXY_PASSWORD"));
      assert.ok(resp.proxy_id, "empty proxy_id after login");
      try {
        const me = await proxy.me();
        assert.equal(me.proxy_id, resp.proxy_id);
        assert.equal(me.username, resp.username);
      } finally {
        await proxy.logout();
      }
    });
  });
});

//  HMAC proxy client tests

describe("HMAC proxy client", () => {
  let hmac: MagnoliaHMACClient;

  before(async (t) => {
    if (!env("MAGNOLIA_BASE_URL") || !env("MAGNOLIA_PROXY_ID") || !env("MAGNOLIA_HMAC_KEY")) {
      t.skip("MAGNOLIA_PROXY_ID / MAGNOLIA_HMAC_KEY not set");
      return;
    }
    hmac = new MagnoliaHMACClient(
      env("MAGNOLIA_BASE_URL"),
      env("MAGNOLIA_PROXY_ID"),
      env("MAGNOLIA_HMAC_KEY"),
      { timeoutMs: TIMEOUT_MS },
    );
  });

  it("HMAC/GetOrCreateConversation returns a conversation_id", async (t) => {
    await run(t, async () => {
      requireEnv("MAGNOLIA_TARGET_USER_ID");
      const resp = await hmac.getOrCreateConversation({ targetUserID: env("MAGNOLIA_TARGET_USER_ID") });
      assert.ok(resp.conversation_id);
    });
  });

  it("HMAC/SendMessage delivers a message", async (t) => {
    await run(t, async () => {
      requireEnv("MAGNOLIA_TARGET_USER_ID");
      const conv = await hmac.getOrCreateConversation({ targetUserID: env("MAGNOLIA_TARGET_USER_ID") });
      const resp = await hmac.sendMessage(conv.conversation_id, `${PREFIX}_hmac_payload`);
      assert.ok(resp.message_id);
    });
  });

  it("HMAC/CreatePost creates a draft post", async (t) => {
    await run(t, async () => {
      const contents: PostContentRequest[] = [
        { content_type: "text", display_order: 0, content: `${PREFIX} hmac post` },
      ];
      const resp = await hmac.createPost(contents, false, [PREFIX]);
      assert.ok(resp.post_id);
    });
  });

  it("HMAC/UploadMedia stores a file", async (t) => {
    await run(t, async () => {
      requireEnv("MAGNOLIA_MEDIA_FILE");
      const resp = await hmac.uploadMediaFile(env("MAGNOLIA_MEDIA_FILE"));
      assert.ok(resp.media_id);
      assert.ok(resp.url);
    });
  });
});
