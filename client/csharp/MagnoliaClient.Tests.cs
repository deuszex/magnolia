// Integration tests for MagnoliaClient and MagnoliaHMACClient.
//
// Run against a live server:
//
//   MAGNOLIA_BASE_URL=https://... MAGNOLIA_USERNAME=alice MAGNOLIA_PASSWORD=... \
//       dotnet test
//
// Required env vars:
//   MAGNOLIA_BASE_URL       server root, e.g. https://magnolia.example.com
//   MAGNOLIA_USERNAME       login identifier (username or email)
//   MAGNOLIA_PASSWORD       login password
//
// Optional env vars (related tests are skipped when absent):
//   MAGNOLIA_PROXY_ID        )  both required for HMAC tests
//   MAGNOLIA_HMAC_KEY        )
//   MAGNOLIA_PROXY_USERNAME  )  both required for proxy session tests
//   MAGNOLIA_PROXY_PASSWORD  )
//   MAGNOLIA_MEDIA_FILE      path to any file; enables media upload/download tests
//   MAGNOLIA_TARGET_USER_ID  enables conversation and message tests

using Magnolia;
using Xunit;

namespace Magnolia.Tests;

//  Test fixtures 

/// <summary>
/// Module-scoped fixture: logs in once, shares state across all tests in the class,
/// and cleans up created resources after the run.
/// </summary>
public sealed class SessionFixture : IAsyncLifetime
{
    private static readonly TimeSpan Timeout = TimeSpan.FromSeconds(10);
    private static readonly string Prefix    = $"mgtest_{DateTimeOffset.UtcNow.ToUnixTimeSeconds()}";

    public MagnoliaClient Client { get; } = new(Env("MAGNOLIA_BASE_URL"), Timeout);

    public string UserId    { get; private set; } = "";
    public string PostId    { get; set; } = "";
    public string CommentId { get; set; } = "";
    public string MediaId   { get; set; } = "";
    public string ConvId    { get; set; } = "";
    public string MsgId     { get; set; } = "";

    public static string Prefix_ => Prefix;

    public async Task InitializeAsync()
    {
        RequireEnv("MAGNOLIA_BASE_URL", "MAGNOLIA_USERNAME", "MAGNOLIA_PASSWORD");
        var resp = await Client.LoginAsync(Env("MAGNOLIA_USERNAME"), Env("MAGNOLIA_PASSWORD"));
        UserId = resp.User.UserId;
    }

    public async Task DisposeAsync()
    {
        if (MsgId     != "") try { await Client.DeleteMessageAsync(MsgId);           } catch { }
        if (CommentId != "") try { await Client.DeleteCommentAsync(CommentId);        } catch { }
        if (MediaId   != "") try { await Client.DeleteMediaAsync(MediaId);            } catch { }
        if (ConvId    != "") try { await Client.DeleteConversationAsync(ConvId);      } catch { }
        if (PostId    != "") try { await Client.DeletePostAsync(PostId);              } catch { }
        try { await Client.LogoutAsync(); } catch { }
        Client.Dispose();
    }

    public static string Env(string key) => System.Environment.GetEnvironmentVariable(key) ?? "";

    public static void RequireEnv(params string[] keys)
    {
        var missing = keys.Where(k => string.IsNullOrEmpty(Env(k))).ToArray();
        if (missing.Length > 0)
            throw new SkipException($"env var(s) not set: {string.Join(", ", missing)}");
    }

    public static void SkipIfEmpty(string value, string envKey)
    {
        if (string.IsNullOrEmpty(value))
            throw new SkipException($"env var not set: {envKey}");
    }
}

// xUnit v2 skip support
public sealed class SkipException(string reason) : Exception(reason) { }

public sealed class SkipFactAttribute(string? reason = null) : FactAttribute
{
    public override string? Skip => reason;
}

//  Helpers 

file static class Helpers
{
    public static string MediaTypeFor(string filePath)
    {
        var ext = Path.GetExtension(filePath).ToLowerInvariant();
        return ext switch
        {
            ".jpg" or ".jpeg" or ".png" or ".gif" or ".webp" => "image",
            ".mp4" or ".webm" or ".mov"                      => "video",
            _                                                => "file",
        };
    }
}

//  Session client tests 

[Collection("Session")]
public sealed class AuthTests(SessionFixture fx) : IClassFixture<SessionFixture>
{
    [Fact]
    public async Task Me_ReturnsLoggedInUser()
    {
        var resp = await fx.Client.MeAsync();
        Assert.Equal(fx.UserId, resp.User.UserId);
    }
}

[Collection("Session")]
public sealed class ProfileTests(SessionFixture fx) : IClassFixture<SessionFixture>
{
    [Fact]
    public async Task GetProfile_ReturnsProfile()
    {
        var resp = await fx.Client.GetProfileAsync(fx.UserId);
        Assert.Equal(fx.UserId, resp.UserId);
    }

    [Fact]
    public async Task UpdateProfile_RoundTripsBio()
    {
        var orig    = await fx.Client.GetProfileAsync(fx.UserId);
        var newBio  = $"{SessionFixture.Prefix_}_bio";
        try
        {
            var updated = await fx.Client.UpdateProfileAsync(bio: newBio);
            Assert.Equal(newBio, updated.Bio);
        }
        finally
        {
            await fx.Client.UpdateProfileAsync(bio: orig.Bio);
        }
    }
}

[Collection("Session")]
public sealed class PostTests(SessionFixture fx) : IClassFixture<SessionFixture>
{
    [Fact]
    public async Task FullLifecycle()
    {
        // Create
        var created = await fx.Client.CreatePostAsync(
            [new("text", 0, $"{SessionFixture.Prefix_} post")],
            publish: false,
            tags: [SessionFixture.Prefix_]);
        Assert.False(string.IsNullOrEmpty(created.PostId));
        Assert.False(created.IsPublished);
        fx.PostId = created.PostId;

        // Get
        var got = await fx.Client.GetPostAsync(fx.PostId);
        Assert.Equal(fx.PostId, got.PostId);

        // Update
        var newContent = $"{SessionFixture.Prefix_} updated";
        var updated = await fx.Client.UpdatePostAsync(fx.PostId,
            contents: [new("text", 0, newContent)]);
        Assert.Equal(newContent, updated.Contents[0].Content);

        // Publish
        var toggled = await fx.Client.PublishPostAsync(fx.PostId);
        Assert.True(toggled.IsPublished);

        // List
        var list = await fx.Client.ListPostsAsync(new(Limit: 5));
        Assert.NotNull(list.Posts);

        // Search
        var search = await fx.Client.SearchPostsAsync(new(Q: SessionFixture.Prefix_));
        Assert.NotNull(search.Posts);
    }
}

[Collection("Session")]
public sealed class CommentTests(SessionFixture fx) : IClassFixture<SessionFixture>
{
    [Fact]
    public async Task FullLifecycle()
    {
        if (string.IsNullOrEmpty(fx.PostId))
            throw new SkipException("PostId not set - PostTests must run first");

        // Create
        var created = await fx.Client.CreateCommentAsync(fx.PostId, $"{SessionFixture.Prefix_} comment");
        Assert.False(string.IsNullOrEmpty(created.CommentId));
        fx.CommentId = created.CommentId;

        // Update
        var edited  = $"{SessionFixture.Prefix_} edited";
        var updated = await fx.Client.UpdateCommentAsync(fx.CommentId, edited);
        Assert.Equal(edited, updated.Content);

        // List
        var list = await fx.Client.ListCommentsAsync(fx.PostId);
        Assert.Contains(list.Comments, c => c.CommentId == fx.CommentId);
    }
}

[Collection("Session")]
public sealed class MediaTests(SessionFixture fx) : IClassFixture<SessionFixture>
{
    [Fact]
    public async Task Upload_Download_Update_Delete()
    {
        SessionFixture.RequireEnv("MAGNOLIA_MEDIA_FILE");
        var file = SessionFixture.Env("MAGNOLIA_MEDIA_FILE");

        var uploaded = await fx.Client.UploadMediaAsync(file);
        Assert.False(string.IsNullOrEmpty(uploaded.MediaId));
        fx.MediaId = uploaded.MediaId;

        var raw = await fx.Client.DownloadMediaAsync(fx.MediaId);
        Assert.True(raw.Length > 0);

        // Thumbnail: 404 is acceptable for non-image/video
        try
        {
            var thumb = await fx.Client.GetThumbnailAsync(fx.MediaId);
            Assert.True(thumb.Length > 0);
        }
        catch (APIException ex) when (ex.StatusCode == 404) { }

        await fx.Client.UpdateMediaAsync(fx.MediaId, description: $"{SessionFixture.Prefix_} media");
    }

    [Fact]
    public async Task BatchDelete()
    {
        SessionFixture.RequireEnv("MAGNOLIA_MEDIA_FILE");
        var file  = SessionFixture.Env("MAGNOLIA_MEDIA_FILE");
        var extra = await fx.Client.UploadMediaAsync(file);
        var result = await fx.Client.BatchDeleteMediaAsync([extra.MediaId]);
        Assert.Equal(1, result.SuccessCount);
    }

    [Fact]
    public async Task ChunkedUpload()
    {
        SessionFixture.RequireEnv("MAGNOLIA_MEDIA_FILE");
        var file      = SessionFixture.Env("MAGNOLIA_MEDIA_FILE");
        var mediaType = Helpers.MediaTypeFor(file);
        // 64 KiB chunks - forces multiple chunks even for small test files
        var resp = await fx.Client.UploadMediaChunkedAsync(file, mediaType, 64 * 1024);
        Assert.False(string.IsNullOrEmpty(resp.MediaId));
        await fx.Client.DeleteMediaAsync(resp.MediaId);
    }
}

[Collection("Session")]
public sealed class ConversationTests(SessionFixture fx) : IClassFixture<SessionFixture>
{
    [Fact]
    public async Task FullLifecycle()
    {
        SessionFixture.RequireEnv("MAGNOLIA_TARGET_USER_ID");
        var targetId = SessionFixture.Env("MAGNOLIA_TARGET_USER_ID");

        // Create
        var conv = await fx.Client.CreateConversationAsync("direct", [targetId]);
        Assert.False(string.IsNullOrEmpty(conv.ConversationId));
        fx.ConvId = conv.ConversationId;

        // Get
        var got = await fx.Client.GetConversationAsync(fx.ConvId);
        Assert.Equal(fx.ConvId, got.ConversationId);
        Assert.Contains(got.Members, m => m.UserId == targetId);

        // List
        var list = await fx.Client.ListConversationsAsync(new(Limit: 50));
        Assert.Contains(list.Conversations, c => c.ConversationId == fx.ConvId);

        // Send
        var msg = await fx.Client.SendMessageAsync(fx.ConvId, $"{SessionFixture.Prefix_}_payload");
        Assert.False(string.IsNullOrEmpty(msg.MessageId));
        fx.MsgId = msg.MessageId;

        // List messages
        var msgs = await fx.Client.ListMessagesAsync(fx.ConvId);
        Assert.Contains(msgs.Messages, m => m.MessageId == fx.MsgId);

        // Unread counts
        var counts = await fx.Client.GetUnreadCountsAsync();
        Assert.NotNull(counts.Counts);
    }
}

[Collection("Session")]
public sealed class MessagingPrefsTests(SessionFixture fx) : IClassFixture<SessionFixture>
{
    [Fact]
    public async Task Toggle_RoundTrips()
    {
        var prefs    = await fx.Client.GetMessagingPreferencesAsync();
        var original = prefs.AcceptMessages;
        try
        {
            var toggled = await fx.Client.UpdateMessagingPreferencesAsync(!original);
            Assert.Equal(!original, toggled.AcceptMessages);
        }
        finally
        {
            await fx.Client.UpdateMessagingPreferencesAsync(original);
        }
    }
}

//  Proxy management tests

[Collection("Session")]
public sealed class ProxyManagementTests(SessionFixture fx) : IClassFixture<SessionFixture>
{
    [Fact]
    public async Task GetMyProxy_ReturnsProxyOrSkips()
    {
        try
        {
            var proxy = await fx.Client.GetMyProxyAsync();
            Assert.False(string.IsNullOrEmpty(proxy.ProxyId));
            Assert.False(string.IsNullOrEmpty(proxy.Username));

            // Round-trip bio update
            var originalBio = proxy.Bio;
            var newBio      = $"{SessionFixture.Prefix_}_proxy_bio";
            try
            {
                var updated = await fx.Client.UpdateMyProxyAsync(new(Bio: newBio));
                Assert.Equal(newBio, updated.Bio);
            }
            finally
            {
                await fx.Client.UpdateMyProxyAsync(new(Bio: originalBio));
            }
        }
        catch (APIException ex) when (ex.StatusCode == 404)
        {
            throw new SkipException("No proxy account paired to this user");
        }
    }

    [Fact]
    public async Task ListPublicProxies_ReturnsList()
    {
        var result = await fx.Client.ListPublicProxiesAsync();
        Assert.NotNull(result);
    }
}

//  Proxy session client tests

public sealed class ProxySessionTests
{
    private static readonly TimeSpan Timeout = TimeSpan.FromSeconds(10);

    private static string Env(string key) => System.Environment.GetEnvironmentVariable(key) ?? "";

    private static void RequireEnv(params string[] keys)
    {
        var missing = keys.Where(k => string.IsNullOrEmpty(Env(k))).ToArray();
        if (missing.Length > 0)
            throw new SkipException($"env var(s) not set: {string.Join(", ", missing)}");
    }

    [Fact]
    public async Task LoginMeLogout_RoundTrip()
    {
        RequireEnv("MAGNOLIA_BASE_URL", "MAGNOLIA_PROXY_USERNAME", "MAGNOLIA_PROXY_PASSWORD");
        using var proxy = new MagnoliaProxySessionClient(Env("MAGNOLIA_BASE_URL"), Timeout);

        var loginResp = await proxy.LoginAsync(
            Env("MAGNOLIA_PROXY_USERNAME"), Env("MAGNOLIA_PROXY_PASSWORD"));
        Assert.False(string.IsNullOrEmpty(loginResp.ProxyId));

        try
        {
            var me = await proxy.MeAsync();
            Assert.Equal(loginResp.ProxyId, me.ProxyId);
            Assert.Equal(loginResp.Username, me.Username);
        }
        finally
        {
            await proxy.LogoutAsync();
        }
    }
}

//  HMAC proxy client tests

public sealed class HMACTests
{
    private static readonly TimeSpan Timeout = TimeSpan.FromSeconds(10);
    private static readonly string Prefix    = $"mgtest_{DateTimeOffset.UtcNow.ToUnixTimeSeconds()}";

    private static string Env(string key) => System.Environment.GetEnvironmentVariable(key) ?? "";

    private static void RequireEnv(params string[] keys)
    {
        var missing = keys.Where(k => string.IsNullOrEmpty(Env(k))).ToArray();
        if (missing.Length > 0)
            throw new SkipException($"env var(s) not set: {string.Join(", ", missing)}");
    }

    private MagnoliaHMACClient BuildClient()
    {
        RequireEnv("MAGNOLIA_BASE_URL", "MAGNOLIA_PROXY_ID", "MAGNOLIA_HMAC_KEY");
        return new MagnoliaHMACClient(
            Env("MAGNOLIA_BASE_URL"),
            Env("MAGNOLIA_PROXY_ID"),
            Env("MAGNOLIA_HMAC_KEY"),
            Timeout);
    }

    [Fact]
    public async Task GetOrCreateConversation_ReturnsConversationId()
    {
        RequireEnv("MAGNOLIA_TARGET_USER_ID");
        using var client = BuildClient();
        var resp = await client.GetOrCreateConversationAsync(targetUserId: Env("MAGNOLIA_TARGET_USER_ID"));
        Assert.False(string.IsNullOrEmpty(resp.ConversationId));
    }

    [Fact]
    public async Task SendMessage_ReturnsMessageId()
    {
        RequireEnv("MAGNOLIA_TARGET_USER_ID");
        using var client = BuildClient();
        var conv = await client.GetOrCreateConversationAsync(targetUserId: Env("MAGNOLIA_TARGET_USER_ID"));
        var resp = await client.SendMessageAsync(conv.ConversationId, $"{Prefix}_hmac_payload");
        Assert.False(string.IsNullOrEmpty(resp.MessageId));
    }

    [Fact]
    public async Task CreatePost_ReturnsDraftPostId()
    {
        using var client = BuildClient();
        var resp = await client.CreatePostAsync(
            [new("text", 0, $"{Prefix} hmac post")],
            publish: false,
            tags: [Prefix]);
        Assert.False(string.IsNullOrEmpty(resp.PostId));
    }

    [Fact]
    public async Task UploadMedia_ReturnsMediaId()
    {
        RequireEnv("MAGNOLIA_MEDIA_FILE");
        using var client = BuildClient();
        var resp = await client.UploadMediaAsync(Env("MAGNOLIA_MEDIA_FILE"));
        Assert.False(string.IsNullOrEmpty(resp.MediaId));
        Assert.False(string.IsNullOrEmpty(resp.Url));
    }
}
