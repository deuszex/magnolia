// Magnolia API client - session and HMAC proxy modes.
//
// Session usage:
//   var client = new MagnoliaClient("https://magnolia.example.com", timeout: TimeSpan.FromSeconds(10));
//   await client.LoginAsync("alice", "hunter2");
//   var posts = await client.ListPostsAsync(new() { Limit = 10 });
//
// HMAC proxy usage:
//   var proxy = new MagnoliaHMACClient("https://...", "proxy-id", "64-char-hex", timeout: TimeSpan.FromSeconds(10));
//   var conv = await proxy.GetOrCreateConversationAsync(targetUsername: "bob");

using System.Net;
using System.Net.WebSockets;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace Magnolia;

//  Errors 

public sealed class APIException(int statusCode, string body)
    : Exception($"HTTP {statusCode}: {body}")
{
    public int StatusCode { get; } = statusCode;
    public string Body { get; } = body;
}

//  Response types 

public record UserResponse(
    [property: JsonPropertyName("user_id")]      string UserId,
    [property: JsonPropertyName("email")]        string? Email,
    [property: JsonPropertyName("username")]     string Username,
    [property: JsonPropertyName("display_name")] string? DisplayName,
    [property: JsonPropertyName("avatar_url")]   string? AvatarUrl,
    [property: JsonPropertyName("verified")]     bool Verified,
    [property: JsonPropertyName("admin")]        bool Admin
);

public record LoginResponse(
    [property: JsonPropertyName("user")] UserResponse User
);

public record ProfileResponse(
    [property: JsonPropertyName("user_id")]       string UserId,
    [property: JsonPropertyName("email")]         string? Email,
    [property: JsonPropertyName("email_visible")] bool EmailVisible,
    [property: JsonPropertyName("username")]      string Username,
    [property: JsonPropertyName("display_name")]  string? DisplayName,
    [property: JsonPropertyName("bio")]           string? Bio,
    [property: JsonPropertyName("avatar_url")]    string? AvatarUrl,
    [property: JsonPropertyName("location")]      string? Location,
    [property: JsonPropertyName("website")]       string? Website,
    [property: JsonPropertyName("public_key")]    string? PublicKey,
    [property: JsonPropertyName("created_at")]    string CreatedAt
);

public record PostContentItem(
    [property: JsonPropertyName("content_id")]    string ContentId,
    [property: JsonPropertyName("content_type")]  string ContentType,
    [property: JsonPropertyName("display_order")] int DisplayOrder,
    [property: JsonPropertyName("content")]       string Content,
    [property: JsonPropertyName("thumbnail_url")] string? ThumbnailUrl,
    [property: JsonPropertyName("filename")]      string? Filename,
    [property: JsonPropertyName("mime_type")]     string? MimeType,
    [property: JsonPropertyName("file_size")]     long? FileSize
);

public record PostResponse(
    [property: JsonPropertyName("post_id")]           string PostId,
    [property: JsonPropertyName("author_id")]         string AuthorId,
    [property: JsonPropertyName("author_name")]       string? AuthorName,
    [property: JsonPropertyName("author_avatar_url")] string? AuthorAvatarUrl,
    [property: JsonPropertyName("contents")]          List<PostContentItem> Contents,
    [property: JsonPropertyName("tags")]              List<string> Tags,
    [property: JsonPropertyName("is_published")]      bool IsPublished,
    [property: JsonPropertyName("comment_count")]     int CommentCount,
    [property: JsonPropertyName("created_at")]        string CreatedAt,
    [property: JsonPropertyName("source_server")]     string? SourceServer
);

public record PostListResponse(
    [property: JsonPropertyName("posts")]       List<PostResponse> Posts,
    [property: JsonPropertyName("total")]       int Total,
    [property: JsonPropertyName("has_more")]    bool HasMore,
    [property: JsonPropertyName("next_cursor")] string? NextCursor
);

public record CommentResponse(
    [property: JsonPropertyName("comment_id")]         string CommentId,
    [property: JsonPropertyName("post_id")]            string PostId,
    [property: JsonPropertyName("author_id")]          string AuthorId,
    [property: JsonPropertyName("author_display_name")]string AuthorDisplayName,
    [property: JsonPropertyName("author_avatar_url")]  string? AuthorAvatarUrl,
    [property: JsonPropertyName("parent_comment_id")]  string? ParentCommentId,
    [property: JsonPropertyName("content_type")]       string ContentType,
    [property: JsonPropertyName("content")]            string Content,
    [property: JsonPropertyName("media_url")]          string? MediaUrl,
    [property: JsonPropertyName("media_id")]           string? MediaId,
    [property: JsonPropertyName("filename")]           string? Filename,
    [property: JsonPropertyName("is_deleted")]         bool IsDeleted,
    [property: JsonPropertyName("reply_count")]        int ReplyCount,
    [property: JsonPropertyName("created_at")]         string CreatedAt,
    [property: JsonPropertyName("updated_at")]         string UpdatedAt
);

public record CommentListResponse(
    [property: JsonPropertyName("comments")] List<CommentResponse> Comments,
    [property: JsonPropertyName("total")]    int Total,
    [property: JsonPropertyName("has_more")] bool HasMore
);

public record MediaUploadResponse(
    [property: JsonPropertyName("media_id")]      string MediaId,
    [property: JsonPropertyName("url")]           string Url,
    [property: JsonPropertyName("thumbnail_url")] string? ThumbnailUrl
);

public record MediaItemResponse(
    [property: JsonPropertyName("media_id")]      string MediaId,
    [property: JsonPropertyName("url")]           string Url,
    [property: JsonPropertyName("thumbnail_url")] string? ThumbnailUrl,
    [property: JsonPropertyName("description")]   string? Description,
    [property: JsonPropertyName("tags")]          List<string> Tags
);

public record BatchDeleteResponse(
    [property: JsonPropertyName("success_count")] int SuccessCount,
    [property: JsonPropertyName("failed_ids")]    List<string> FailedIds
);

public record ConversationMember(
    [property: JsonPropertyName("user_id")]      string UserId,
    [property: JsonPropertyName("role")]         string Role,
    [property: JsonPropertyName("joined_at")]    string JoinedAt,
    [property: JsonPropertyName("is_proxy")]     bool IsProxy,
    [property: JsonPropertyName("display_name")] string? DisplayName,
    [property: JsonPropertyName("username")]     string? Username
);

public record ConversationResponse(
    [property: JsonPropertyName("conversation_id")]   string ConversationId,
    [property: JsonPropertyName("conversation_type")] string ConversationType,
    [property: JsonPropertyName("name")]              string? Name,
    [property: JsonPropertyName("display_name")]      string? DisplayName,
    [property: JsonPropertyName("member_count")]      int MemberCount,
    [property: JsonPropertyName("last_message_at")]   string? LastMessageAt,
    [property: JsonPropertyName("unread_count")]      int UnreadCount,
    [property: JsonPropertyName("is_favourite")]      bool IsFavourite,
    [property: JsonPropertyName("members")]           List<ConversationMember> Members,
    [property: JsonPropertyName("created_at")]        string CreatedAt,
    [property: JsonPropertyName("updated_at")]        string UpdatedAt
);

public record ConversationListResponse(
    [property: JsonPropertyName("conversations")] List<ConversationResponse> Conversations
);

public record MessageAttachment(
    [property: JsonPropertyName("media_id")]      string MediaId,
    [property: JsonPropertyName("media_type")]    string MediaType,
    [property: JsonPropertyName("filename")]      string? Filename,
    [property: JsonPropertyName("file_size")]     long FileSize,
    [property: JsonPropertyName("url")]           string Url,
    [property: JsonPropertyName("thumbnail_url")] string? ThumbnailUrl,
    [property: JsonPropertyName("mime_type")]     string? MimeType
);

public record MessageResponse(
    [property: JsonPropertyName("message_id")]                  string MessageId,
    [property: JsonPropertyName("conversation_id")]             string ConversationId,
    [property: JsonPropertyName("sender_id")]                   string SenderId,
    [property: JsonPropertyName("sender_email")]                string? SenderEmail,
    [property: JsonPropertyName("sender_name")]                 string? SenderName,
    [property: JsonPropertyName("sender_avatar_url")]           string? SenderAvatarUrl,
    [property: JsonPropertyName("remote_sender_qualified_id")]  string? RemoteSenderQualifiedId,
    [property: JsonPropertyName("encrypted_content")]           string EncryptedContent,
    [property: JsonPropertyName("attachments")]                 List<MessageAttachment> Attachments,
    [property: JsonPropertyName("created_at")]                  string CreatedAt,
    [property: JsonPropertyName("federated_status")]            string? FederatedStatus
);

public record MessageListResponse(
    [property: JsonPropertyName("messages")] List<MessageResponse> Messages,
    [property: JsonPropertyName("has_more")] bool HasMore
);

public record UnreadCountsResponse(
    [property: JsonPropertyName("counts")] Dictionary<string, int> Counts
);

public record MessagingPreferences(
    [property: JsonPropertyName("accept_messages")] bool AcceptMessages
);

public record BlockedUser(
    [property: JsonPropertyName("user_id")]         string UserId,
    [property: JsonPropertyName("blocked_user_id")] string BlockedUserId,
    [property: JsonPropertyName("created_at")]      string CreatedAt
);

public record BlocklistResponse(
    [property: JsonPropertyName("blocks")] List<BlockedUser> Blocks
);

public record HMACMessageResponse(
    [property: JsonPropertyName("message_id")] string MessageId,
    [property: JsonPropertyName("created_at")] string CreatedAt
);

public record HMACPostResponse(
    [property: JsonPropertyName("post_id")]    string PostId,
    [property: JsonPropertyName("created_at")] string CreatedAt
);

public record HMACConversationResponse(
    [property: JsonPropertyName("conversation_id")] string ConversationId,
    [property: JsonPropertyName("created")]         bool Created
);

public record ProxyAuthResponse(
    [property: JsonPropertyName("proxy_id")]      string ProxyId,
    [property: JsonPropertyName("username")]      string Username,
    [property: JsonPropertyName("display_name")]  string? DisplayName,
    [property: JsonPropertyName("avatar_url")]    string? AvatarUrl
);

public record ProxyUserResponse(
    [property: JsonPropertyName("proxy_id")]              string ProxyId,
    [property: JsonPropertyName("paired_user_id")]        string? PairedUserId,
    [property: JsonPropertyName("active")]                bool Active,
    [property: JsonPropertyName("display_name")]          string? DisplayName,
    [property: JsonPropertyName("username")]              string Username,
    [property: JsonPropertyName("bio")]                   string? Bio,
    [property: JsonPropertyName("avatar_url")]            string? AvatarUrl,
    [property: JsonPropertyName("public_key")]            string? PublicKey,
    [property: JsonPropertyName("has_password")]          bool HasPassword,
    [property: JsonPropertyName("has_e2e_key")]           bool HasE2EKey,
    [property: JsonPropertyName("has_hmac_key")]          bool HasHMACKey,
    [property: JsonPropertyName("hmac_key_fingerprint")]  string? HMACKeyFingerprint,
    [property: JsonPropertyName("created_at")]            string CreatedAt,
    [property: JsonPropertyName("updated_at")]            string UpdatedAt
);

public record PublicProxyResponse(
    [property: JsonPropertyName("proxy_id")]     string ProxyId,
    [property: JsonPropertyName("username")]     string Username,
    [property: JsonPropertyName("display_name")] string? DisplayName,
    [property: JsonPropertyName("avatar_url")]   string? AvatarUrl
);

public record UpdateProxyParams(
    string? DisplayName   = null,
    string? Bio           = null,
    string? AvatarMediaId = null,
    string? Location      = null,
    string? Website       = null
);

//  Request / param types 

public record PostContentRequest(
    [property: JsonPropertyName("content_type")]  string ContentType,
    [property: JsonPropertyName("display_order")] int DisplayOrder,
    [property: JsonPropertyName("content")]       string Content,
    [property: JsonPropertyName("filename")]      string? Filename  = null,
    [property: JsonPropertyName("mime_type")]     string? MimeType  = null,
    [property: JsonPropertyName("media_id")]      string? MediaId   = null
);

public record ListPostsParams(
    string? AuthorId      = null,
    bool    IncludeDrafts = false,
    string? ContentType   = null,
    int     Limit         = 0,
    int     Offset        = 0,
    string? After         = null
);

public record SearchPostsParams(
    string? Q        = null,
    string? Tags     = null,
    bool    HasImages = false,
    bool    HasVideos = false,
    bool    HasFiles  = false,
    string? AuthorId  = null,
    string? FromDate  = null,
    string? ToDate    = null,
    int     Limit     = 0,
    int     Offset    = 0
);

public record ListCommentsParams(
    string? ParentCommentId = null,
    bool    IncludeReplies  = false,
    string  Sort            = "newest",
    int     Limit           = 0,
    int     Offset          = 0
);

public record UploadMediaParams(
    string? Filename    = null,
    string? MimeType    = null,
    string? Description = null,
    string? Tags        = null
);

public record ListConversationsParams(int Limit = 0, int Offset = 0);
public record ListMessagesParams(int Limit = 0, int Offset = 0);

public record ListConversationMediaParams(
    string? MediaType = null,
    int     Limit     = 0,
    int     Offset    = 0
);

//  Utilities 

internal static class Crypto
{
    internal static string Sha256Hex(byte[] data)
    {
        var hash = SHA256.HashData(data);
        return Convert.ToHexString(hash).ToLowerInvariant();
    }

    internal static string Sha256Hex(string text) => Sha256Hex(Encoding.UTF8.GetBytes(text));

    /// <summary>
    /// HMAC-SHA256 where the key is the 64-char hex string used as raw UTF-8 bytes -
    /// NOT decoded to 32 bytes before use.
    /// </summary>
    internal static string HmacSha256Hex(string key, string message)
    {
        var keyBytes = Encoding.UTF8.GetBytes(key);
        var msgBytes = Encoding.UTF8.GetBytes(message);
        var hash = HMACSHA256.HashData(keyBytes, msgBytes);
        return Convert.ToHexString(hash).ToLowerInvariant();
    }
}

internal static class MimeHelper
{
    private static readonly Dictionary<string, string> Map = new(StringComparer.OrdinalIgnoreCase)
    {
        [".jpg"]  = "image/jpeg",   [".jpeg"] = "image/jpeg",
        [".png"]  = "image/png",    [".gif"]  = "image/gif",
        [".webp"] = "image/webp",   [".svg"]  = "image/svg+xml",
        [".mp4"]  = "video/mp4",    [".webm"] = "video/webm",
        [".mov"]  = "video/quicktime",
        [".mp3"]  = "audio/mpeg",   [".ogg"]  = "audio/ogg",
        [".pdf"]  = "application/pdf",
        [".zip"]  = "application/zip",
        [".txt"]  = "text/plain",   [".md"]   = "text/markdown",
    };

    internal static string Guess(string filename)
    {
        var ext = Path.GetExtension(filename);
        return Map.TryGetValue(ext, out var mime) ? mime : "application/octet-stream";
    }

    internal static string MediaType(string filename)
    {
        var ext = Path.GetExtension(filename).ToLowerInvariant();
        if (ext is ".jpg" or ".jpeg" or ".png" or ".gif" or ".webp" or ".svg") return "image";
        if (ext is ".mp4" or ".webm" or ".mov") return "video";
        return "file";
    }
}

internal static class QueryBuilder
{
    internal static string Build(IEnumerable<(string Key, object? Value)> pairs)
    {
        var parts = pairs
            .Where(p => p.Value is not null)
            .Where(p => p.Value is not bool b || b)
            .Where(p => p.Value is not int i || i != 0)
            .Where(p => p.Value is not string s || s.Length > 0)
            .Select(p => $"{Uri.EscapeDataString(p.Key)}={Uri.EscapeDataString(p.Value!.ToString()!)}");
        var q = string.Join("&", parts);
        return q.Length > 0 ? "?" + q : "";
    }
}

//  JSON options 

internal static class Json
{
    internal static readonly JsonSerializerOptions Options = new()
    {
        PropertyNamingPolicy        = JsonNamingPolicy.SnakeCaseLower,
        DefaultIgnoreCondition      = JsonIgnoreCondition.WhenWritingNull,
        NumberHandling              = JsonNumberHandling.AllowReadingFromString,
    };

    internal static string Serialize(object value)   => JsonSerializer.Serialize(value, Options);
    internal static T Deserialize<T>(string json)    => JsonSerializer.Deserialize<T>(json, Options)!;
}

//  Session client 

/// <summary>
/// Session-based Magnolia API client.
/// The session cookie (session_id) is managed automatically via <see cref="CookieContainer"/>.
/// </summary>
public sealed class MagnoliaClient : IDisposable
{
    private readonly string _baseUrl;
    private readonly HttpClient _http;
    private readonly TimeSpan _timeout;

    /// <param name="baseUrl">Server root URL.</param>
    /// <param name="timeout">Per-request timeout. Default is no timeout.</param>
    public MagnoliaClient(string baseUrl, TimeSpan timeout = default)
    {
        _baseUrl = baseUrl.TrimEnd('/');
        _timeout = timeout == default ? Timeout.InfiniteTimeSpan : timeout;

        var handler = new HttpClientHandler
        {
            CookieContainer    = new CookieContainer(),
            UseCookies         = true,
            AllowAutoRedirect  = true,
        };
        _http = new HttpClient(handler) { Timeout = _timeout };
    }

    public void Dispose() => _http.Dispose();

    //  Internal helpers 

    private async Task<T> GetJsonAsync<T>(string path, string query = "")
    {
        var resp = await _http.GetAsync(_baseUrl + path + query).ConfigureAwait(false);
        return await ReadJsonAsync<T>(resp).ConfigureAwait(false);
    }

    private async Task<byte[]> GetBytesAsync(string path)
    {
        var resp = await _http.GetAsync(_baseUrl + path).ConfigureAwait(false);
        await EnsureSuccessAsync(resp).ConfigureAwait(false);
        return await resp.Content.ReadAsByteArrayAsync().ConfigureAwait(false);
    }

    private async Task<T> PostJsonAsync<T>(string path, object? body = null)
    {
        var content = body is null
            ? null
            : new StringContent(Json.Serialize(body), Encoding.UTF8, "application/json");
        var resp = await _http.PostAsync(_baseUrl + path, content).ConfigureAwait(false);
        return await ReadJsonAsync<T>(resp).ConfigureAwait(false);
    }

    private async Task PostJsonVoidAsync(string path, object? body = null)
    {
        var content = body is null
            ? null
            : new StringContent(Json.Serialize(body), Encoding.UTF8, "application/json");
        var resp = await _http.PostAsync(_baseUrl + path, content).ConfigureAwait(false);
        await EnsureSuccessAsync(resp).ConfigureAwait(false);
    }

    private async Task<T> PutJsonAsync<T>(string path, object body)
    {
        var content = new StringContent(Json.Serialize(body), Encoding.UTF8, "application/json");
        var resp = await _http.PutAsync(_baseUrl + path, content).ConfigureAwait(false);
        return await ReadJsonAsync<T>(resp).ConfigureAwait(false);
    }

    private async Task PutJsonVoidAsync(string path, object body)
    {
        var content = new StringContent(Json.Serialize(body), Encoding.UTF8, "application/json");
        var resp = await _http.PutAsync(_baseUrl + path, content).ConfigureAwait(false);
        await EnsureSuccessAsync(resp).ConfigureAwait(false);
    }

    private async Task<T> PatchJsonAsync<T>(string path, object body)
    {
        var content = new StringContent(Json.Serialize(body), Encoding.UTF8, "application/json");
        var req     = new HttpRequestMessage(HttpMethod.Patch, _baseUrl + path) { Content = content };
        var resp    = await _http.SendAsync(req).ConfigureAwait(false);
        return await ReadJsonAsync<T>(resp).ConfigureAwait(false);
    }

    private async Task DeleteAsync(string path)
    {
        var resp = await _http.DeleteAsync(_baseUrl + path).ConfigureAwait(false);
        await EnsureSuccessAsync(resp).ConfigureAwait(false);
    }

    private async Task<T> PostMultipartAsync<T>(string path, Dictionary<string, string> fields,
        string filename, string mimeType, byte[] data)
    {
        using var form = new MultipartFormDataContent();
        foreach (var (k, v) in fields)
            form.Add(new StringContent(v), k);
        var fileContent = new ByteArrayContent(data);
        fileContent.Headers.ContentType = new(mimeType);
        form.Add(fileContent, "file", filename);
        var resp = await _http.PostAsync(_baseUrl + path, form).ConfigureAwait(false);
        return await ReadJsonAsync<T>(resp).ConfigureAwait(false);
    }

    private static async Task<T> ReadJsonAsync<T>(HttpResponseMessage resp)
    {
        await EnsureSuccessAsync(resp).ConfigureAwait(false);
        var json = await resp.Content.ReadAsStringAsync().ConfigureAwait(false);
        return Json.Deserialize<T>(json);
    }

    private static async Task EnsureSuccessAsync(HttpResponseMessage resp)
    {
        if (!resp.IsSuccessStatusCode)
        {
            var body = await resp.Content.ReadAsStringAsync().ConfigureAwait(false);
            throw new APIException((int)resp.StatusCode, body);
        }
    }

    //  Auth 

    public Task<LoginResponse> LoginAsync(string identifier, string password)
        => PostJsonAsync<LoginResponse>("/api/auth/login", new { identifier, password });

    public Task LogoutAsync()
        => PostJsonVoidAsync("/api/auth/logout");

    public Task<LoginResponse> MeAsync()
        => GetJsonAsync<LoginResponse>("/api/auth/me");

    public Task ChangePasswordAsync(string currentPassword, string newPassword, string newPasswordConfirm)
        => PostJsonVoidAsync("/api/auth/change-password", new
        {
            current_password      = currentPassword,
            new_password          = newPassword,
            new_password_confirm  = newPasswordConfirm,
        });

    //  Profile 

    public Task<ProfileResponse> GetProfileAsync(string userId)
        => GetJsonAsync<ProfileResponse>($"/api/users/{userId}/profile");

    public Task<ProfileResponse> UpdateProfileAsync(
        string? displayName   = null,
        string? bio           = null,
        string? avatarMediaId = null,
        string? location      = null,
        string? website       = null)
        => PutJsonAsync<ProfileResponse>("/api/profile", new
        {
            display_name    = displayName,
            bio,
            avatar_media_id = avatarMediaId,
            location,
            website,
        });

    //  Posts 

    public Task<PostListResponse> ListPostsAsync(ListPostsParams? p = null)
    {
        p ??= new();
        var q = QueryBuilder.Build([
            ("author_id",      p.AuthorId),
            ("include_drafts", p.IncludeDrafts ? (object)"true" : null),
            ("content_type",   p.ContentType),
            ("limit",          p.Limit   > 0 ? p.Limit   : (int?)null),
            ("offset",         p.Offset  > 0 ? p.Offset  : (int?)null),
            ("after",          p.After),
        ]);
        return GetJsonAsync<PostListResponse>("/api/posts", q);
    }

    public Task<PostResponse> GetPostAsync(string postId)
        => GetJsonAsync<PostResponse>($"/api/posts/{postId}");

    public Task<PostResponse> CreatePostAsync(
        IEnumerable<PostContentRequest> contents,
        bool publish = false,
        IEnumerable<string>? tags = null)
        => PostJsonAsync<PostResponse>("/api/posts", new
        {
            contents,
            publish,
            tags = tags?.ToArray() ?? [],
        });

    public Task<PostResponse> UpdatePostAsync(string postId,
        IEnumerable<PostContentRequest>? contents = null,
        bool? publish = null,
        IEnumerable<string>? tags = null)
    {
        var body = new Dictionary<string, object?>();
        if (contents is not null) body["contents"] = contents;
        if (publish  is not null) body["publish"]  = publish;
        if (tags     is not null) body["tags"]     = tags.ToArray();
        return PutJsonAsync<PostResponse>($"/api/posts/{postId}", body);
    }

    public Task DeletePostAsync(string postId)
        => DeleteAsync($"/api/posts/{postId}");

    public Task<PostResponse> PublishPostAsync(string postId)
        => PostJsonAsync<PostResponse>($"/api/posts/{postId}/publish");

    public Task<PostListResponse> SearchPostsAsync(SearchPostsParams? p = null)
    {
        p ??= new();
        var q = QueryBuilder.Build([
            ("q",          p.Q),
            ("tags",       p.Tags),
            ("has_images", p.HasImages ? (object)"true" : null),
            ("has_videos", p.HasVideos ? (object)"true" : null),
            ("has_files",  p.HasFiles  ? (object)"true" : null),
            ("author_id",  p.AuthorId),
            ("from_date",  p.FromDate),
            ("to_date",    p.ToDate),
            ("limit",      p.Limit  > 0 ? p.Limit  : (int?)null),
            ("offset",     p.Offset > 0 ? p.Offset : (int?)null),
        ]);
        return GetJsonAsync<PostListResponse>("/api/posts/search", q);
    }

    //  Comments 

    public Task<CommentListResponse> ListCommentsAsync(string postId, ListCommentsParams? p = null)
    {
        p ??= new();
        var q = QueryBuilder.Build([
            ("parent_comment_id", p.ParentCommentId),
            ("include_replies",   p.IncludeReplies ? (object)"true" : null),
            ("sort",              p.Sort),
            ("limit",             p.Limit  > 0 ? p.Limit  : (int?)null),
            ("offset",            p.Offset > 0 ? p.Offset : (int?)null),
        ]);
        return GetJsonAsync<CommentListResponse>($"/api/posts/{postId}/comments", q);
    }

    public Task<CommentResponse> CreateCommentAsync(
        string postId,
        string content,
        string contentType      = "text",
        string? parentCommentId = null,
        string? filename        = null,
        string? mimeType        = null)
        => PostJsonAsync<CommentResponse>($"/api/posts/{postId}/comments", new
        {
            content_type       = contentType,
            content,
            parent_comment_id  = parentCommentId,
            filename,
            mime_type          = mimeType,
        });

    public Task<CommentResponse> UpdateCommentAsync(string commentId, string content)
        => PutJsonAsync<CommentResponse>($"/api/comments/{commentId}", new { content });

    public Task DeleteCommentAsync(string commentId)
        => DeleteAsync($"/api/comments/{commentId}");

    //  Media 

    /// <summary>Upload a file from disk.</summary>
    public async Task<MediaUploadResponse> UploadMediaAsync(string filePath, UploadMediaParams? p = null)
    {
        p ??= new();
        var data     = await File.ReadAllBytesAsync(filePath).ConfigureAwait(false);
        var filename = p.Filename ?? Path.GetFileName(filePath);
        return await UploadMediaBytesAsync(data, new(p.Filename ?? filename, p.MimeType, p.Description, p.Tags))
            .ConfigureAwait(false);
    }

    /// <summary>Upload raw bytes.</summary>
    public Task<MediaUploadResponse> UploadMediaBytesAsync(byte[] data, UploadMediaParams? p = null)
    {
        p ??= new();
        var filename = p.Filename ?? "upload";
        var mimeType = p.MimeType ?? MimeHelper.Guess(filename);
        var fields   = new Dictionary<string, string>();
        if (p.Description is { } d) fields["description"] = d;
        if (p.Tags is { } t)        fields["tags"]        = t;
        return PostMultipartAsync<MediaUploadResponse>("/api/media", fields, filename, mimeType, data);
    }

    /// <summary>Chunked upload for large files. chunkSize defaults to 5 MiB.</summary>
    public async Task<MediaUploadResponse> UploadMediaChunkedAsync(
        string filePath, string mediaType, int chunkSize = 5 * 1024 * 1024)
    {
        var data     = await File.ReadAllBytesAsync(filePath).ConfigureAwait(false);
        var filename = Path.GetFileName(filePath);

        var init = await PostJsonAsync<dynamic>("/api/media/chunked/init", new
        {
            media_type  = mediaType,
            filename,
            mime_type   = MimeHelper.Guess(filename),
            total_size  = data.Length,
            chunk_size  = chunkSize,
        }).ConfigureAwait(false);

        string uploadId       = init.GetProperty("upload_id").GetString()!;
        int serverChunkSize   = init.GetProperty("chunk_size").GetInt32();

        for (int i = 0; i * serverChunkSize < data.Length; i++)
        {
            var start  = i * serverChunkSize;
            var length = Math.Min(serverChunkSize, data.Length - start);
            var chunk  = new ReadOnlyMemory<byte>(data, start, length);

            using var content = new ByteArrayContent(chunk.ToArray());
            content.Headers.ContentType = new("application/octet-stream");
            var resp = await _http
                .PostAsync($"{_baseUrl}/api/media/chunked/{uploadId}/{i}", content)
                .ConfigureAwait(false);
            await EnsureSuccessAsync(resp).ConfigureAwait(false);
        }

        return await PostJsonAsync<MediaUploadResponse>($"/api/media/chunked/{uploadId}/complete")
            .ConfigureAwait(false);
    }

    public Task<byte[]> DownloadMediaAsync(string mediaId)
        => GetBytesAsync($"/api/media/{mediaId}/file");

    public Task<byte[]> GetThumbnailAsync(string mediaId)
        => GetBytesAsync($"/api/media/{mediaId}/thumbnail");

    public Task<MediaItemResponse> UpdateMediaAsync(string mediaId,
        string? description = null, IEnumerable<string>? tags = null)
        => PutJsonAsync<MediaItemResponse>($"/api/media/{mediaId}", new { description, tags });

    public Task DeleteMediaAsync(string mediaId)
        => DeleteAsync($"/api/media/{mediaId}");

    public Task<BatchDeleteResponse> BatchDeleteMediaAsync(IEnumerable<string> mediaIds)
        => PostJsonAsync<BatchDeleteResponse>("/api/media/batch-delete", new { media_ids = mediaIds.ToArray() });

    //  Conversations 

    public Task<ConversationListResponse> ListConversationsAsync(ListConversationsParams? p = null)
    {
        p ??= new();
        var q = QueryBuilder.Build([
            ("limit",  p.Limit  > 0 ? p.Limit  : (int?)null),
            ("offset", p.Offset > 0 ? p.Offset : (int?)null),
        ]);
        return GetJsonAsync<ConversationListResponse>("/api/conversations", q);
    }

    public Task<ConversationResponse> CreateConversationAsync(
        string type, IEnumerable<string> memberIds, string? name = null)
        => PostJsonAsync<ConversationResponse>("/api/conversations", new
        {
            conversation_type = type,
            member_ids        = memberIds.ToArray(),
            name,
        });

    public Task<ConversationResponse> GetConversationAsync(string conversationId)
        => GetJsonAsync<ConversationResponse>($"/api/conversations/{conversationId}");

    public Task<ConversationResponse> UpdateConversationAsync(string conversationId, string name)
        => PutJsonAsync<ConversationResponse>($"/api/conversations/{conversationId}", new { name });

    public Task DeleteConversationAsync(string conversationId)
        => DeleteAsync($"/api/conversations/{conversationId}");

    public Task<ConversationResponse> AddConversationMemberAsync(string conversationId, string userId)
        => PostJsonAsync<ConversationResponse>(
            $"/api/conversations/{conversationId}/members", new { user_id = userId });

    public Task RemoveConversationMemberAsync(string conversationId, string userId)
        => DeleteAsync($"/api/conversations/{conversationId}/members/{userId}");

    //  Messages 

    public Task<MessageResponse> SendMessageAsync(
        string conversationId, string encryptedContent, IEnumerable<string>? mediaIds = null)
        => PostJsonAsync<MessageResponse>($"/api/conversations/{conversationId}/messages", new
        {
            encrypted_content = encryptedContent,
            media_ids         = mediaIds?.ToArray() ?? [],
        });

    public Task<MessageListResponse> ListMessagesAsync(string conversationId, ListMessagesParams? p = null)
    {
        p ??= new();
        var q = QueryBuilder.Build([
            ("limit",  p.Limit  > 0 ? p.Limit  : (int?)null),
            ("offset", p.Offset > 0 ? p.Offset : (int?)null),
        ]);
        return GetJsonAsync<MessageListResponse>($"/api/conversations/{conversationId}/messages", q);
    }

    public Task DeleteMessageAsync(string messageId)
        => DeleteAsync($"/api/messages/{messageId}");

    public Task<UnreadCountsResponse> GetUnreadCountsAsync()
        => GetJsonAsync<UnreadCountsResponse>("/api/messaging/unread");

    public Task<List<MessageAttachment>> ListConversationMediaAsync(
        string conversationId, ListConversationMediaParams? p = null)
    {
        p ??= new();
        var q = QueryBuilder.Build([
            ("media_type", p.MediaType),
            ("limit",      p.Limit  > 0 ? p.Limit  : (int?)null),
            ("offset",     p.Offset > 0 ? p.Offset : (int?)null),
        ]);
        return GetJsonAsync<List<MessageAttachment>>($"/api/conversations/{conversationId}/media", q);
    }

    //  Messaging prefs / blacklist / favourites / background 

    public Task<MessagingPreferences> GetMessagingPreferencesAsync()
        => GetJsonAsync<MessagingPreferences>("/api/messaging/preferences");

    public Task<MessagingPreferences> UpdateMessagingPreferencesAsync(bool acceptMessages)
        => PutJsonAsync<MessagingPreferences>("/api/messaging/preferences",
            new { accept_messages = acceptMessages });

    public Task<BlocklistResponse> ListBlockedUsersAsync()
        => GetJsonAsync<BlocklistResponse>("/api/messaging/blacklist");

    public Task<BlockedUser> BlockUserAsync(string userId)
        => PostJsonAsync<BlockedUser>("/api/messaging/blacklist", new { user_id = userId });

    public Task UnblockUserAsync(string userId)
        => DeleteAsync($"/api/messaging/blacklist/{userId}");

    public Task AddFavouriteAsync(string conversationId)
        => PostJsonVoidAsync("/api/messaging/favourites", new { conversation_id = conversationId });

    public Task RemoveFavouriteAsync(string conversationId)
        => DeleteAsync($"/api/messaging/favourites/{conversationId}");

    public async Task<string> GetConversationBackgroundAsync(string conversationId)
    {
        var r = await GetJsonAsync<Dictionary<string, string>>(
            $"/api/conversations/{conversationId}/background").ConfigureAwait(false);
        return r["media_id"];
    }

    public Task SetConversationBackgroundAsync(string conversationId, string mediaId)
        => PutJsonVoidAsync($"/api/conversations/{conversationId}/background",
            new { media_id = mediaId });

    public Task DeleteConversationBackgroundAsync(string conversationId)
        => DeleteAsync($"/api/conversations/{conversationId}/background");

    //  Proxy management (user session)

    public Task<ProxyUserResponse> CreateProxyAsync(string username)
        => PostJsonAsync<ProxyUserResponse>("/api/proxy", new { username });

    public Task<ProxyUserResponse> GetMyProxyAsync()
        => GetJsonAsync<ProxyUserResponse>("/api/proxy");

    public Task<ProxyUserResponse> UpdateMyProxyAsync(UpdateProxyParams p)
        => PatchJsonAsync<ProxyUserResponse>("/api/proxy", new
        {
            display_name    = p.DisplayName,
            bio             = p.Bio,
            avatar_media_id = p.AvatarMediaId,
            location        = p.Location,
            website         = p.Website,
        });

    public Task SetProxyPasswordAsync(string password)
        => PutJsonVoidAsync("/api/proxy/password", new { password });

    public Task SetProxyHMACKeyAsync(string hmacKey)
        => PutJsonVoidAsync("/api/proxy/hmac-key", new { hmac_key = hmacKey });

    public Task SetProxyE2EKeyAsync(string publicKey, string encryptedPrivateKey)
        => PutJsonVoidAsync("/api/proxy/e2e-key", new
        {
            public_key             = publicKey,
            encrypted_private_key  = encryptedPrivateKey,
        });

    public Task<List<PublicProxyResponse>> ListPublicProxiesAsync()
        => GetJsonAsync<List<PublicProxyResponse>>("/api/proxy/list-public");

    //  WebSocket

    /// <summary>
    /// Opens a real-time WebSocket connection using the current session cookie.
    /// The server pushes JSON frames; clients do not send frames.
    /// </summary>
    /// <example>
    /// var ws = await client.ConnectWebSocketAsync();
    /// var buf = new byte[64 * 1024];
    /// while (true) {
    ///     var result = await ws.ReceiveAsync(buf, CancellationToken.None);
    ///     var json = Encoding.UTF8.GetString(buf, 0, result.Count);
    ///     Console.WriteLine(json);
    /// }
    /// </example>
    public async Task<ClientWebSocket> ConnectWebSocketAsync(CancellationToken cancellationToken = default)
    {
        var wsUrl = (_baseUrl + "/api/ws")
            .Replace("http://", "ws://", StringComparison.Ordinal)
            .Replace("https://", "wss://", StringComparison.Ordinal);

        // Extract session cookie from HttpClientHandler's CookieContainer.
        var handler = (HttpClientHandler)((HttpMessageInvoker)_http).GetType()
            .GetProperty("Handler", System.Reflection.BindingFlags.NonPublic | System.Reflection.BindingFlags.Instance)
            ?.GetValue(_http)!;

        var ws = new ClientWebSocket();

        // Forward cookies manually - ClientWebSocket doesn't share HttpClient's jar.
        var uri = new Uri(_baseUrl);
        if (_http.DefaultRequestHeaders.TryGetValues("Cookie", out var existing))
            ws.Options.SetRequestHeader("Cookie", string.Join("; ", existing));

        await ws.ConnectAsync(new Uri(wsUrl), cancellationToken).ConfigureAwait(false);
        return ws;
    }
}

//  Proxy session client

/// <summary>
/// Session-based client for the proxy account endpoints.
/// Uses the <c>proxy_session_id</c> cookie, separate from the user <c>session_id</c>.
/// </summary>
public sealed class MagnoliaProxySessionClient : IDisposable
{
    private readonly string _baseUrl;
    private readonly HttpClient _http;

    public MagnoliaProxySessionClient(string baseUrl, TimeSpan timeout = default)
    {
        _baseUrl = baseUrl.TrimEnd('/');
        var handler = new HttpClientHandler
        {
            CookieContainer   = new CookieContainer(),
            UseCookies        = true,
            AllowAutoRedirect = true,
        };
        _http = new HttpClient(handler)
        {
            Timeout = timeout == default ? Timeout.InfiniteTimeSpan : timeout,
        };
    }

    public void Dispose() => _http.Dispose();

    private async Task<T> PostJsonAsync<T>(string path, object? body = null)
    {
        var content = body is null
            ? null
            : new StringContent(Json.Serialize(body), Encoding.UTF8, "application/json");
        var resp = await _http.PostAsync(_baseUrl + path, content).ConfigureAwait(false);
        return await ReadJsonAsync<T>(resp).ConfigureAwait(false);
    }

    private async Task PostJsonVoidAsync(string path, object? body = null)
    {
        var content = body is null
            ? null
            : new StringContent(Json.Serialize(body), Encoding.UTF8, "application/json");
        var resp = await _http.PostAsync(_baseUrl + path, content).ConfigureAwait(false);
        await EnsureSuccessAsync(resp).ConfigureAwait(false);
    }

    private async Task<T> GetJsonAsync<T>(string path)
    {
        var resp = await _http.GetAsync(_baseUrl + path).ConfigureAwait(false);
        return await ReadJsonAsync<T>(resp).ConfigureAwait(false);
    }

    private static async Task<T> ReadJsonAsync<T>(HttpResponseMessage resp)
    {
        await EnsureSuccessAsync(resp).ConfigureAwait(false);
        var json = await resp.Content.ReadAsStringAsync().ConfigureAwait(false);
        return Json.Deserialize<T>(json);
    }

    private static async Task EnsureSuccessAsync(HttpResponseMessage resp)
    {
        if (!resp.IsSuccessStatusCode)
        {
            var body = await resp.Content.ReadAsStringAsync().ConfigureAwait(false);
            throw new APIException((int)resp.StatusCode, body);
        }
    }

    public Task<ProxyAuthResponse> LoginAsync(string username, string password)
        => PostJsonAsync<ProxyAuthResponse>("/api/proxy/login", new { username, password });

    public Task LogoutAsync()
        => PostJsonVoidAsync("/api/proxy/logout");

    public Task<ProxyUserResponse> MeAsync()
        => GetJsonAsync<ProxyUserResponse>("/api/proxy/me");
}

//  HMAC proxy client

/// <summary>
/// Proxy client authenticated via per-request HMAC-SHA256 signatures.
/// No session cookie - each call is independently signed.
/// </summary>
public sealed class MagnoliaHMACClient : IDisposable
{
    private readonly string _baseUrl;
    private readonly string _proxyId;
    private readonly string _hmacKey;
    private readonly HttpClient _http;

    /// <param name="baseUrl">  Server root URL.</param>
    /// <param name="proxyId">  The proxy account's ID.</param>
    /// <param name="hmacKey">  64-character lowercase hex string (used as raw key material).</param>
    /// <param name="timeout">  Per-request timeout.</param>
    public MagnoliaHMACClient(string baseUrl, string proxyId, string hmacKey,
        TimeSpan timeout = default)
    {
        _baseUrl  = baseUrl.TrimEnd('/');
        _proxyId  = proxyId;
        _hmacKey  = hmacKey;
        _http     = new HttpClient
        {
            Timeout = timeout == default ? Timeout.InfiniteTimeSpan : timeout,
        };
    }

    public void Dispose() => _http.Dispose();

    private static long NowUnix() => DateTimeOffset.UtcNow.ToUnixTimeSeconds();

    private string Sign(string message) => Crypto.HmacSha256Hex(_hmacKey, message);

    private async Task<T> PostJsonAsync<T>(string path, object body)
    {
        var content = new StringContent(Json.Serialize(body), Encoding.UTF8, "application/json");
        var resp    = await _http.PostAsync(_baseUrl + path, content).ConfigureAwait(false);
        if (!resp.IsSuccessStatusCode)
        {
            var err = await resp.Content.ReadAsStringAsync().ConfigureAwait(false);
            throw new APIException((int)resp.StatusCode, err);
        }
        var json = await resp.Content.ReadAsStringAsync().ConfigureAwait(false);
        return Json.Deserialize<T>(json);
    }

    //  HMAC endpoints 

    /// <summary>Send a message to a conversation the proxy is already a member of.</summary>
    public Task<HMACMessageResponse> SendMessageAsync(
        string conversationId, string encryptedContent, IEnumerable<string>? mediaIds = null)
    {
        var ts          = NowUnix();
        var contentHash = Crypto.Sha256Hex(encryptedContent);
        var signed      = $"{_proxyId}:{conversationId}:{contentHash}:{ts}";
        return PostJsonAsync<HMACMessageResponse>("/api/proxy/hmac/send-message", new
        {
            proxy_id          = _proxyId,
            conversation_id   = conversationId,
            encrypted_content = encryptedContent,
            media_ids         = mediaIds?.ToArray() ?? [],
            signature         = Sign(signed),
            timestamp         = ts,
        });
    }

    /// <summary>
    /// Create a post as the proxy. The proxy must be paired to a user account.
    /// </summary>
    public Task<HMACPostResponse> CreatePostAsync(
        IEnumerable<PostContentRequest> contents,
        bool publish = false,
        IEnumerable<string>? tags = null)
    {
        var ts         = NowUnix();
        var publishBit = publish ? "1" : "0";
        var tagList    = tags?.OrderBy(t => t).ToList() ?? [];
        var sorted     = contents.OrderBy(c => c.DisplayOrder).ToList();

        var lines     = sorted.Select(c => $"{c.DisplayOrder}|{c.ContentType}|{c.Content}");
        var canonical = string.Join("\n", lines)
                      + $"\ntags:{string.Join(",", tagList)}"
                      + $"\npublish:{publishBit}";

        var bodyHash = Crypto.Sha256Hex(canonical);
        var signed   = $"{_proxyId}:{bodyHash}:{publishBit}:{ts}";

        return PostJsonAsync<HMACPostResponse>("/api/proxy/hmac/create-post", new
        {
            proxy_id  = _proxyId,
            contents  = sorted,
            publish,
            tags      = tagList,
            signature = Sign(signed),
            timestamp = ts,
        });
    }

    /// <summary>
    /// Get or create a direct conversation between the proxy and a target user.
    /// Provide exactly one of <paramref name="targetUserId"/> or <paramref name="targetUsername"/>.
    /// </summary>
    public Task<HMACConversationResponse> GetOrCreateConversationAsync(
        string? targetUserId = null, string? targetUsername = null)
    {
        if (string.IsNullOrEmpty(targetUserId) == string.IsNullOrEmpty(targetUsername))
            throw new ArgumentException("Provide exactly one of targetUserId or targetUsername.");

        var ts     = NowUnix();
        var signed = $"{_proxyId}:{ts}";
        return PostJsonAsync<HMACConversationResponse>(
            "/api/proxy/hmac/get-or-create-conversation", new
            {
                proxy_id         = _proxyId,
                target_user_id   = targetUserId,
                target_username  = targetUsername,
                signature        = Sign(signed),
                timestamp        = ts,
            });
    }

    /// <summary>Upload a file from disk as the proxy.</summary>
    public async Task<MediaUploadResponse> UploadMediaAsync(string filePath)
    {
        var data = await File.ReadAllBytesAsync(filePath).ConfigureAwait(false);
        return await UploadMediaBytesAsync(data, Path.GetFileName(filePath)).ConfigureAwait(false);
    }

    /// <summary>
    /// Upload raw bytes as the proxy.
    /// The file hash is computed over raw bytes directly.
    /// </summary>
    public async Task<MediaUploadResponse> UploadMediaBytesAsync(
        byte[] data, string filename, string? mimeType = null)
    {
        var mime = mimeType ?? MimeHelper.Guess(filename);
        var ts   = NowUnix();
        var fileHash = Crypto.Sha256Hex(data);
        var signed   = $"{_proxyId}:{fileHash}:{ts}";

        using var form = new MultipartFormDataContent();
        form.Add(new StringContent(_proxyId),     "proxy_id");
        form.Add(new StringContent(Sign(signed)), "signature");
        form.Add(new StringContent(ts.ToString()), "timestamp");
        var fileContent = new ByteArrayContent(data);
        fileContent.Headers.ContentType = new(mime);
        form.Add(fileContent, "file", filename);

        var resp = await _http.PostAsync(_baseUrl + "/api/proxy/hmac/upload-media", form)
            .ConfigureAwait(false);
        if (!resp.IsSuccessStatusCode)
        {
            var err = await resp.Content.ReadAsStringAsync().ConfigureAwait(false);
            throw new APIException((int)resp.StatusCode, err);
        }
        var json = await resp.Content.ReadAsStringAsync().ConfigureAwait(false);
        return Json.Deserialize<MediaUploadResponse>(json);
    }
}
