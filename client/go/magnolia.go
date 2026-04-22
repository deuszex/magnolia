// Package magnolia provides a client for the Magnolia API.
//
// Session-based usage:
//
//	c := magnolia.NewClient("https://magnolia.example.com")
//	if _, err := c.Login("alice", "hunter2"); err != nil {
//	    log.Fatal(err)
//	}
//	posts, err := c.ListPosts(magnolia.ListPostsParams{Limit: 10})
//
// HMAC proxy usage:
//
//	p := magnolia.NewHMACClient("https://magnolia.example.com", "proxy-id", "64-char-hex-key")
//	conv, err := p.GetOrCreateConversation("", "bob")
package magnolia

import (
	"bytes"
	"crypto/hmac"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"mime"
	"mime/multipart"
	"net/http"
	"net/http/cookiejar"
	"net/textproto"
	"net/url"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"time"

	"github.com/gorilla/websocket"
)

//  Errors 

// APIError is returned for any non-2xx response.
type APIError struct {
	StatusCode int
	Body       string
}

func (e *APIError) Error() string {
	return fmt.Sprintf("HTTP %d: %s", e.StatusCode, e.Body)
}

//  Response types 

type UserResponse struct {
	UserID      string  `json:"user_id"`
	Email       *string `json:"email"`
	Username    string  `json:"username"`
	DisplayName *string `json:"display_name"`
	AvatarURL   *string `json:"avatar_url"`
	Verified    bool    `json:"verified"`
	Admin       bool    `json:"admin"`
}

type LoginResponse struct {
	User UserResponse `json:"user"`
}

type ProfileResponse struct {
	UserID       string  `json:"user_id"`
	Email        *string `json:"email"`
	EmailVisible bool    `json:"email_visible"`
	Username     string  `json:"username"`
	DisplayName  *string `json:"display_name"`
	Bio          *string `json:"bio"`
	AvatarURL    *string `json:"avatar_url"`
	Location     *string `json:"location"`
	Website      *string `json:"website"`
	PublicKey    *string `json:"public_key"`
	CreatedAt    string  `json:"created_at"`
}

type PostContentItem struct {
	ContentID    string  `json:"content_id"`
	ContentType  string  `json:"content_type"`
	DisplayOrder int     `json:"display_order"`
	Content      string  `json:"content"`
	ThumbnailURL *string `json:"thumbnail_url"`
	Filename     *string `json:"filename"`
	MimeType     *string `json:"mime_type"`
	FileSize     *int64  `json:"file_size"`
}

type PostResponse struct {
	PostID          string            `json:"post_id"`
	AuthorID        string            `json:"author_id"`
	AuthorName      *string           `json:"author_name"`
	AuthorAvatarURL *string           `json:"author_avatar_url"`
	Contents        []PostContentItem `json:"contents"`
	Tags            []string          `json:"tags"`
	IsPublished     bool              `json:"is_published"`
	CommentCount    int               `json:"comment_count"`
	CreatedAt       string            `json:"created_at"`
	SourceServer    *string           `json:"source_server"`
}

type PostListResponse struct {
	Posts      []PostResponse `json:"posts"`
	Total      int            `json:"total"`
	HasMore    bool           `json:"has_more"`
	NextCursor *string        `json:"next_cursor"`
}

type CommentResponse struct {
	CommentID       string  `json:"comment_id"`
	PostID          string  `json:"post_id"`
	AuthorID        string  `json:"author_id"`
	AuthorName      string  `json:"author_display_name"`
	AuthorAvatarURL *string `json:"author_avatar_url"`
	ParentCommentID *string `json:"parent_comment_id"`
	ContentType     string  `json:"content_type"`
	Content         string  `json:"content"`
	MediaURL        *string `json:"media_url"`
	MediaID         *string `json:"media_id"`
	Filename        *string `json:"filename"`
	IsDeleted       bool    `json:"is_deleted"`
	ReplyCount      int     `json:"reply_count"`
	CreatedAt       string  `json:"created_at"`
	UpdatedAt       string  `json:"updated_at"`
}

type CommentListResponse struct {
	Comments []CommentResponse `json:"comments"`
	Total    int               `json:"total"`
	HasMore  bool              `json:"has_more"`
}

type MediaUploadResponse struct {
	MediaID      string  `json:"media_id"`
	URL          string  `json:"url"`
	ThumbnailURL *string `json:"thumbnail_url"`
}

type MediaItemResponse struct {
	MediaID      string   `json:"media_id"`
	URL          string   `json:"url"`
	ThumbnailURL *string  `json:"thumbnail_url"`
	Description  *string  `json:"description"`
	Tags         []string `json:"tags"`
}

type BatchDeleteResponse struct {
	SuccessCount int      `json:"success_count"`
	FailedIDs    []string `json:"failed_ids"`
}

type ConversationMember struct {
	UserID      string  `json:"user_id"`
	Role        string  `json:"role"`
	JoinedAt    string  `json:"joined_at"`
	IsProxy     bool    `json:"is_proxy"`
	DisplayName *string `json:"display_name"`
	Username    *string `json:"username"`
}

type ConversationResponse struct {
	ConversationID   string               `json:"conversation_id"`
	ConversationType string               `json:"conversation_type"`
	Name             *string              `json:"name"`
	DisplayName      *string              `json:"display_name"`
	MemberCount      int                  `json:"member_count"`
	LastMessageAt    *string              `json:"last_message_at"`
	UnreadCount      int                  `json:"unread_count"`
	IsFavourite      bool                 `json:"is_favourite"`
	Members          []ConversationMember `json:"members"`
	CreatedAt        string               `json:"created_at"`
	UpdatedAt        string               `json:"updated_at"`
}

type ConversationListResponse struct {
	Conversations []ConversationResponse `json:"conversations"`
}

type MessageAttachment struct {
	MediaID      string  `json:"media_id"`
	MediaType    string  `json:"media_type"`
	Filename     *string `json:"filename"`
	FileSize     int64   `json:"file_size"`
	URL          string  `json:"url"`
	ThumbnailURL *string `json:"thumbnail_url"`
	MimeType     *string `json:"mime_type"`
}

type MessageResponse struct {
	MessageID               string              `json:"message_id"`
	ConversationID          string              `json:"conversation_id"`
	SenderID                string              `json:"sender_id"`
	SenderEmail             *string             `json:"sender_email"`
	SenderName              *string             `json:"sender_name"`
	SenderAvatarURL         *string             `json:"sender_avatar_url"`
	RemoteSenderQualifiedID *string             `json:"remote_sender_qualified_id"`
	EncryptedContent        string              `json:"encrypted_content"`
	Attachments             []MessageAttachment `json:"attachments"`
	CreatedAt               string              `json:"created_at"`
	FederatedStatus         *string             `json:"federated_status"`
}

type MessageListResponse struct {
	Messages []MessageResponse `json:"messages"`
	HasMore  bool              `json:"has_more"`
}

type UnreadCountsResponse struct {
	Counts map[string]int `json:"counts"`
}

type MessagingPreferences struct {
	AcceptMessages bool `json:"accept_messages"`
}

type BlockedUser struct {
	UserID        string `json:"user_id"`
	BlockedUserID string `json:"blocked_user_id"`
	CreatedAt     string `json:"created_at"`
}

type BlocklistResponse struct {
	Blocks []BlockedUser `json:"blocks"`
}

// Proxy types

type ProxyAuthResponse struct {
	ProxyID     string  `json:"proxy_id"`
	Username    string  `json:"username"`
	DisplayName *string `json:"display_name"`
	AvatarURL   *string `json:"avatar_url"`
}

type ProxyUserResponse struct {
	ProxyID            string  `json:"proxy_id"`
	PairedUserID       *string `json:"paired_user_id"`
	Active             bool    `json:"active"`
	DisplayName        *string `json:"display_name"`
	Username           string  `json:"username"`
	Bio                *string `json:"bio"`
	AvatarURL          *string `json:"avatar_url"`
	PublicKey          *string `json:"public_key"`
	HasPassword        bool    `json:"has_password"`
	HasE2EKey          bool    `json:"has_e2e_key"`
	HasHMACKey         bool    `json:"has_hmac_key"`
	HMACKeyFingerprint *string `json:"hmac_key_fingerprint"`
	CreatedAt          string  `json:"created_at"`
	UpdatedAt          string  `json:"updated_at"`
}

type PublicProxyResponse struct {
	ProxyID     string  `json:"proxy_id"`
	Username    string  `json:"username"`
	DisplayName *string `json:"display_name"`
	AvatarURL   *string `json:"avatar_url"`
}

type UpdateProxyParams struct {
	DisplayName   *string `json:"display_name,omitempty"`
	Bio           *string `json:"bio,omitempty"`
	AvatarMediaID *string `json:"avatar_media_id,omitempty"`
	Active        *bool   `json:"active,omitempty"`
}

// HMAC proxy response types

type HMACMessageResponse struct {
	MessageID string `json:"message_id"`
	CreatedAt string `json:"created_at"`
}

type HMACPostResponse struct {
	PostID    string `json:"post_id"`
	CreatedAt string `json:"created_at"`
}

type HMACConversationResponse struct {
	ConversationID string `json:"conversation_id"`
	Created        bool   `json:"created"`
}

//  Request param / input types 

// PostContentRequest is a content item for creating or updating a post.
type PostContentRequest struct {
	ContentType  string  `json:"content_type"`
	DisplayOrder int     `json:"display_order"`
	Content      string  `json:"content"`
	Filename     *string `json:"filename,omitempty"`
	MimeType     *string `json:"mime_type,omitempty"`
	MediaID      *string `json:"media_id,omitempty"`
}

type UpdateProfileParams struct {
	DisplayName   *string `json:"display_name"`
	Bio           *string `json:"bio"`
	AvatarMediaID *string `json:"avatar_media_id"`
	Location      *string `json:"location"`
	Website       *string `json:"website"`
}

type ListPostsParams struct {
	AuthorID      string
	IncludeDrafts bool
	ContentType   string
	Limit         int
	Offset        int
	After         string
}

type SearchPostsParams struct {
	Q         string
	Tags      string
	HasImages bool
	HasVideos bool
	HasFiles  bool
	AuthorID  string
	FromDate  string
	ToDate    string
	Limit     int
	Offset    int
}

type ListCommentsParams struct {
	ParentCommentID string
	IncludeReplies  bool
	Sort            string // "newest" (default) or "oldest"
	Limit           int
	Offset          int
}

type UploadMediaParams struct {
	Filename    string
	MimeType    string
	Description string
	Tags        string
}

type ListConversationsParams struct {
	Limit  int
	Offset int
}

type ListMessagesParams struct {
	Limit  int
	Offset int
}

type ListConversationMediaParams struct {
	MediaType string
	Limit     int
	Offset    int
}

//  Helpers 

func sha256Hex(data []byte) string {
	sum := sha256.Sum256(data)
	return hex.EncodeToString(sum[:])
}

// hmacSHA256Hex signs message with key. The key is the 64-char hex string used
// as raw UTF-8 bytes - it is NOT decoded to 32 bytes before use.
func hmacSHA256Hex(key, message string) string {
	mac := hmac.New(sha256.New, []byte(key))
	mac.Write([]byte(message))
	return hex.EncodeToString(mac.Sum(nil))
}

func guessMIME(filename string) string {
	if t := mime.TypeByExtension(strings.ToLower(filepath.Ext(filename))); t != "" {
		return t
	}
	return "application/octet-stream"
}

func nowUnix() int64 { return time.Now().Unix() }

//  Options 

// Option configures the underlying HTTP client of a Client or HMACClient.
type Option func(*http.Client)

// WithTimeout sets a per-request timeout. 0 means no timeout.
func WithTimeout(d time.Duration) Option {
	return func(hc *http.Client) { hc.Timeout = d }
}

//  Session client 

// Client is a session-based Magnolia API client.
// The session cookie (session_id) is managed automatically.
type Client struct {
	baseURL string
	http    *http.Client
}

// NewClient creates a new session client for the given server URL.
func NewClient(baseURL string, opts ...Option) *Client {
	jar, _ := cookiejar.New(nil)
	hc := &http.Client{Jar: jar}
	for _, o := range opts {
		o(hc)
	}
	return &Client{
		baseURL: strings.TrimRight(baseURL, "/"),
		http:    hc,
	}
}

//  Internal helpers 

func (c *Client) do(req *http.Request) (*http.Response, error) {
	resp, err := c.http.Do(req)
	if err != nil {
		return nil, err
	}
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		body, _ := io.ReadAll(resp.Body)
		resp.Body.Close()
		return nil, &APIError{resp.StatusCode, string(body)}
	}
	return resp, nil
}

func (c *Client) getInto(path string, params url.Values, v any) error {
	u := c.baseURL + path
	if len(params) > 0 {
		u += "?" + params.Encode()
	}
	req, err := http.NewRequest(http.MethodGet, u, nil)
	if err != nil {
		return err
	}
	resp, err := c.do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	return json.NewDecoder(resp.Body).Decode(v)
}

func (c *Client) getRaw(path string) ([]byte, error) {
	req, err := http.NewRequest(http.MethodGet, c.baseURL+path, nil)
	if err != nil {
		return nil, err
	}
	resp, err := c.do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	return io.ReadAll(resp.Body)
}

func (c *Client) sendJSON(method, path string, body any, v any) error {
	var bodyReader io.Reader
	if body != nil {
		data, err := json.Marshal(body)
		if err != nil {
			return err
		}
		bodyReader = bytes.NewReader(data)
	}
	req, err := http.NewRequest(method, c.baseURL+path, bodyReader)
	if err != nil {
		return err
	}
	if body != nil {
		req.Header.Set("Content-Type", "application/json")
	}
	resp, err := c.do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	if v == nil {
		return nil
	}
	return json.NewDecoder(resp.Body).Decode(v)
}

func (c *Client) postJSON(path string, body any, v any) error {
	return c.sendJSON(http.MethodPost, path, body, v)
}

func (c *Client) putJSON(path string, body any, v any) error {
	return c.sendJSON(http.MethodPut, path, body, v)
}

func (c *Client) patchJSON(path string, body any, v any) error {
	return c.sendJSON(http.MethodPatch, path, body, v)
}

func (c *Client) deleteReq(path string) error {
	req, err := http.NewRequest(http.MethodDelete, c.baseURL+path, nil)
	if err != nil {
		return err
	}
	resp, err := c.do(req)
	if err != nil {
		return err
	}
	resp.Body.Close()
	return nil
}

func (c *Client) postMultipart(path string, fields map[string]string, filename, mimeType string, fileData []byte, v any) error {
	var buf bytes.Buffer
	w := multipart.NewWriter(&buf)

	for k, val := range fields {
		if err := w.WriteField(k, val); err != nil {
			return err
		}
	}

	h := make(textproto.MIMEHeader)
	h.Set("Content-Disposition", fmt.Sprintf(`form-data; name="file"; filename="%s"`, filename))
	h.Set("Content-Type", mimeType)
	part, err := w.CreatePart(h)
	if err != nil {
		return err
	}
	if _, err := part.Write(fileData); err != nil {
		return err
	}
	w.Close()

	req, err := http.NewRequest(http.MethodPost, c.baseURL+path, &buf)
	if err != nil {
		return err
	}
	req.Header.Set("Content-Type", w.FormDataContentType())
	resp, err := c.do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	if v == nil {
		return nil
	}
	return json.NewDecoder(resp.Body).Decode(v)
}

//  Auth 

// Login authenticates and starts a session. The cookie is stored automatically.
func (c *Client) Login(identifier, password string) (*LoginResponse, error) {
	var result LoginResponse
	return &result, c.postJSON("/api/auth/login", map[string]string{
		"identifier": identifier,
		"password":   password,
	}, &result)
}

// Logout ends the current session.
func (c *Client) Logout() error {
	return c.postJSON("/api/auth/logout", nil, nil)
}

// Me returns the currently authenticated user.
// The server returns a flat UserResponse (not wrapped in {"user": ...}).
func (c *Client) Me() (*UserResponse, error) {
	var result UserResponse
	return &result, c.getInto("/api/auth/me", nil, &result)
}

// ChangePassword changes the authenticated user's password.
func (c *Client) ChangePassword(currentPassword, newPassword, newPasswordConfirm string) error {
	return c.postJSON("/api/auth/change-password", map[string]string{
		"current_password":     currentPassword,
		"new_password":         newPassword,
		"new_password_confirm": newPasswordConfirm,
	}, nil)
}

//  Profile 

func (c *Client) GetProfile(userID string) (*ProfileResponse, error) {
	var result ProfileResponse
	return &result, c.getInto("/api/users/"+userID+"/profile", nil, &result)
}

func (c *Client) UpdateProfile(params UpdateProfileParams) (*ProfileResponse, error) {
	var result ProfileResponse
	return &result, c.putJSON("/api/profile", params, &result)
}

//  Posts 

func (c *Client) ListPosts(p ListPostsParams) (*PostListResponse, error) {
	params := url.Values{}
	if p.AuthorID != "" {
		params.Set("author_id", p.AuthorID)
	}
	if p.IncludeDrafts {
		params.Set("include_drafts", "true")
	}
	if p.ContentType != "" {
		params.Set("content_type", p.ContentType)
	}
	if p.Limit > 0 {
		params.Set("limit", strconv.Itoa(p.Limit))
	}
	if p.Offset > 0 {
		params.Set("offset", strconv.Itoa(p.Offset))
	}
	if p.After != "" {
		params.Set("after", p.After)
	}
	var result PostListResponse
	return &result, c.getInto("/api/posts", params, &result)
}

func (c *Client) GetPost(postID string) (*PostResponse, error) {
	var result PostResponse
	return &result, c.getInto("/api/posts/"+postID, nil, &result)
}

func (c *Client) CreatePost(contents []PostContentRequest, publish bool, tags []string) (*PostResponse, error) {
	if tags == nil {
		tags = []string{}
	}
	var result PostResponse
	return &result, c.postJSON("/api/posts", map[string]any{
		"contents": contents,
		"publish":  publish,
		"tags":     tags,
	}, &result)
}

// UpdatePost updates an existing post. Pass nil for fields that should be unchanged.
func (c *Client) UpdatePost(postID string, contents []PostContentRequest, publish *bool, tags []string) (*PostResponse, error) {
	body := map[string]any{}
	if contents != nil {
		body["contents"] = contents
	}
	if publish != nil {
		body["publish"] = *publish
	}
	if tags != nil {
		body["tags"] = tags
	}
	var result PostResponse
	return &result, c.putJSON("/api/posts/"+postID, body, &result)
}

func (c *Client) DeletePost(postID string) error {
	return c.deleteReq("/api/posts/" + postID)
}

// PublishPost toggles the published state of a post.
func (c *Client) PublishPost(postID string) (*PostResponse, error) {
	var result PostResponse
	return &result, c.postJSON("/api/posts/"+postID+"/publish", nil, &result)
}

func (c *Client) SearchPosts(p SearchPostsParams) (*PostListResponse, error) {
	params := url.Values{}
	if p.Q != "" {
		params.Set("q", p.Q)
	}
	if p.Tags != "" {
		params.Set("tags", p.Tags)
	}
	if p.HasImages {
		params.Set("has_images", "true")
	}
	if p.HasVideos {
		params.Set("has_videos", "true")
	}
	if p.HasFiles {
		params.Set("has_files", "true")
	}
	if p.AuthorID != "" {
		params.Set("author_id", p.AuthorID)
	}
	if p.FromDate != "" {
		params.Set("from_date", p.FromDate)
	}
	if p.ToDate != "" {
		params.Set("to_date", p.ToDate)
	}
	if p.Limit > 0 {
		params.Set("limit", strconv.Itoa(p.Limit))
	}
	if p.Offset > 0 {
		params.Set("offset", strconv.Itoa(p.Offset))
	}
	var result PostListResponse
	return &result, c.getInto("/api/posts/search", params, &result)
}

//  Comments 

func (c *Client) ListComments(postID string, p ListCommentsParams) (*CommentListResponse, error) {
	params := url.Values{}
	if p.ParentCommentID != "" {
		params.Set("parent_comment_id", p.ParentCommentID)
	}
	if p.IncludeReplies {
		params.Set("include_replies", "true")
	}
	if p.Sort != "" {
		params.Set("sort", p.Sort)
	}
	if p.Limit > 0 {
		params.Set("limit", strconv.Itoa(p.Limit))
	}
	if p.Offset > 0 {
		params.Set("offset", strconv.Itoa(p.Offset))
	}
	var result CommentListResponse
	return &result, c.getInto("/api/posts/"+postID+"/comments", params, &result)
}

// CreateComment posts a comment. contentType defaults to "text" if empty.
// parentCommentID, filename, and mimeType are optional (pass "" to omit).
func (c *Client) CreateComment(postID, content, contentType, parentCommentID, filename, mimeType string) (*CommentResponse, error) {
	if contentType == "" {
		contentType = "text"
	}
	body := map[string]any{
		"content_type": contentType,
		"content":      content,
	}
	if parentCommentID != "" {
		body["parent_comment_id"] = parentCommentID
	}
	if filename != "" {
		body["filename"] = filename
	}
	if mimeType != "" {
		body["mime_type"] = mimeType
	}
	var result CommentResponse
	return &result, c.postJSON("/api/posts/"+postID+"/comments", body, &result)
}

func (c *Client) UpdateComment(commentID, content string) (*CommentResponse, error) {
	var result CommentResponse
	return &result, c.putJSON("/api/comments/"+commentID, map[string]string{"content": content}, &result)
}

func (c *Client) DeleteComment(commentID string) error {
	return c.deleteReq("/api/comments/" + commentID)
}

//  Media 

// UploadMedia uploads a file from disk.
func (c *Client) UploadMedia(filePath string, p UploadMediaParams) (*MediaUploadResponse, error) {
	data, err := os.ReadFile(filePath)
	if err != nil {
		return nil, err
	}
	if p.Filename == "" {
		p.Filename = filepath.Base(filePath)
	}
	return c.UploadMediaBytes(data, p)
}

// UploadMediaBytes uploads raw bytes.
func (c *Client) UploadMediaBytes(data []byte, p UploadMediaParams) (*MediaUploadResponse, error) {
	if p.Filename == "" {
		p.Filename = "upload"
	}
	if p.MimeType == "" {
		p.MimeType = guessMIME(p.Filename)
	}
	fields := map[string]string{}
	if p.Description != "" {
		fields["description"] = p.Description
	}
	if p.Tags != "" {
		fields["tags"] = p.Tags
	}
	var result MediaUploadResponse
	return &result, c.postMultipart("/api/media", fields, p.Filename, p.MimeType, data, &result)
}

// UploadMediaChunked performs a chunked upload of a large file.
// chunkSize of 0 uses 5 MiB.
func (c *Client) UploadMediaChunked(filePath, mediaType string, chunkSize int) (*MediaUploadResponse, error) {
	if chunkSize == 0 {
		chunkSize = 5 * 1024 * 1024
	}
	data, err := os.ReadFile(filePath)
	if err != nil {
		return nil, err
	}
	filename := filepath.Base(filePath)

	var init struct {
		UploadID  string `json:"upload_id"`
		ChunkSize int    `json:"chunk_size"`
	}
	if err := c.postJSON("/api/media/chunked/init", map[string]any{
		"media_type": mediaType,
		"filename":   filename,
		"mime_type":  guessMIME(filename),
		"total_size": len(data),
		"chunk_size": chunkSize,
	}, &init); err != nil {
		return nil, err
	}

	for i := 0; i*init.ChunkSize < len(data); i++ {
		start := i * init.ChunkSize
		end := start + init.ChunkSize
		if end > len(data) {
			end = len(data)
		}
		req, err := http.NewRequest(http.MethodPost,
			fmt.Sprintf("%s/api/media/chunked/%s/%d", c.baseURL, init.UploadID, i),
			bytes.NewReader(data[start:end]))
		if err != nil {
			return nil, err
		}
		req.Header.Set("Content-Type", "application/octet-stream")
		resp, err := c.do(req)
		if err != nil {
			return nil, err
		}
		resp.Body.Close()
	}

	var result MediaUploadResponse
	return &result, c.postJSON("/api/media/chunked/"+init.UploadID+"/complete", nil, &result)
}

// DownloadMedia returns the raw file bytes for a media item.
func (c *Client) DownloadMedia(mediaID string) ([]byte, error) {
	return c.getRaw("/api/media/" + mediaID + "/file")
}

// GetThumbnail returns the thumbnail bytes for a media item.
func (c *Client) GetThumbnail(mediaID string) ([]byte, error) {
	return c.getRaw("/api/media/" + mediaID + "/thumbnail")
}

func (c *Client) UpdateMedia(mediaID string, description *string, tags []string) (*MediaItemResponse, error) {
	var result MediaItemResponse
	return &result, c.putJSON("/api/media/"+mediaID, map[string]any{
		"description": description,
		"tags":        tags,
	}, &result)
}

func (c *Client) DeleteMedia(mediaID string) error {
	return c.deleteReq("/api/media/" + mediaID)
}

func (c *Client) BatchDeleteMedia(mediaIDs []string) (*BatchDeleteResponse, error) {
	var result BatchDeleteResponse
	return &result, c.postJSON("/api/media/batch-delete", map[string]any{"media_ids": mediaIDs}, &result)
}

//  Conversations 

func (c *Client) ListConversations(p ListConversationsParams) (*ConversationListResponse, error) {
	params := url.Values{}
	if p.Limit > 0 {
		params.Set("limit", strconv.Itoa(p.Limit))
	}
	if p.Offset > 0 {
		params.Set("offset", strconv.Itoa(p.Offset))
	}
	var result ConversationListResponse
	return &result, c.getInto("/api/conversations", params, &result)
}

// CreateConversation creates a new conversation. name is only used for groups.
func (c *Client) CreateConversation(convType string, memberIDs []string, name string) (*ConversationResponse, error) {
	body := map[string]any{
		"conversation_type": convType,
		"member_ids":        memberIDs,
	}
	if name != "" {
		body["name"] = name
	}
	var result ConversationResponse
	return &result, c.postJSON("/api/conversations", body, &result)
}

func (c *Client) GetConversation(conversationID string) (*ConversationResponse, error) {
	var result ConversationResponse
	return &result, c.getInto("/api/conversations/"+conversationID, nil, &result)
}

func (c *Client) UpdateConversation(conversationID, name string) (*ConversationResponse, error) {
	var result ConversationResponse
	return &result, c.putJSON("/api/conversations/"+conversationID, map[string]string{"name": name}, &result)
}

func (c *Client) DeleteConversation(conversationID string) error {
	return c.deleteReq("/api/conversations/" + conversationID)
}

func (c *Client) AddConversationMember(conversationID, userID string) (*ConversationResponse, error) {
	var result ConversationResponse
	return &result, c.postJSON("/api/conversations/"+conversationID+"/members",
		map[string]string{"user_id": userID}, &result)
}

func (c *Client) RemoveConversationMember(conversationID, userID string) error {
	return c.deleteReq("/api/conversations/" + conversationID + "/members/" + userID)
}

//  Messages 

func (c *Client) SendMessage(conversationID, encryptedContent string, mediaIDs []string) (*MessageResponse, error) {
	if mediaIDs == nil {
		mediaIDs = []string{}
	}
	var result MessageResponse
	return &result, c.postJSON("/api/conversations/"+conversationID+"/messages", map[string]any{
		"encrypted_content": encryptedContent,
		"media_ids":         mediaIDs,
	}, &result)
}

func (c *Client) ListMessages(conversationID string, p ListMessagesParams) (*MessageListResponse, error) {
	params := url.Values{}
	if p.Limit > 0 {
		params.Set("limit", strconv.Itoa(p.Limit))
	}
	if p.Offset > 0 {
		params.Set("offset", strconv.Itoa(p.Offset))
	}
	var result MessageListResponse
	return &result, c.getInto("/api/conversations/"+conversationID+"/messages", params, &result)
}

func (c *Client) DeleteMessage(messageID string) error {
	return c.deleteReq("/api/messages/" + messageID)
}

func (c *Client) GetUnreadCounts() (*UnreadCountsResponse, error) {
	var result UnreadCountsResponse
	return &result, c.getInto("/api/messaging/unread", nil, &result)
}

func (c *Client) ListConversationMedia(conversationID string, p ListConversationMediaParams) ([]MessageAttachment, error) {
	params := url.Values{}
	if p.MediaType != "" {
		params.Set("media_type", p.MediaType)
	}
	if p.Limit > 0 {
		params.Set("limit", strconv.Itoa(p.Limit))
	}
	if p.Offset > 0 {
		params.Set("offset", strconv.Itoa(p.Offset))
	}
	var result []MessageAttachment
	return result, c.getInto("/api/conversations/"+conversationID+"/media", params, &result)
}

//  Messaging preferences / blacklist / favourites / background 

func (c *Client) GetMessagingPreferences() (*MessagingPreferences, error) {
	var result MessagingPreferences
	return &result, c.getInto("/api/messaging/preferences", nil, &result)
}

func (c *Client) UpdateMessagingPreferences(acceptMessages bool) (*MessagingPreferences, error) {
	var result MessagingPreferences
	return &result, c.putJSON("/api/messaging/preferences",
		map[string]bool{"accept_messages": acceptMessages}, &result)
}

func (c *Client) ListBlockedUsers() (*BlocklistResponse, error) {
	var result BlocklistResponse
	return &result, c.getInto("/api/messaging/blacklist", nil, &result)
}

func (c *Client) BlockUser(userID string) (*BlockedUser, error) {
	var result BlockedUser
	return &result, c.postJSON("/api/messaging/blacklist", map[string]string{"user_id": userID}, &result)
}

func (c *Client) UnblockUser(userID string) error {
	return c.deleteReq("/api/messaging/blacklist/" + userID)
}

func (c *Client) AddFavourite(conversationID string) error {
	return c.postJSON("/api/messaging/favourites",
		map[string]string{"conversation_id": conversationID}, nil)
}

func (c *Client) RemoveFavourite(conversationID string) error {
	return c.deleteReq("/api/messaging/favourites/" + conversationID)
}

func (c *Client) GetConversationBackground(conversationID string) (string, error) {
	var result struct {
		MediaID string `json:"media_id"`
	}
	return result.MediaID, c.getInto("/api/conversations/"+conversationID+"/background", nil, &result)
}

func (c *Client) SetConversationBackground(conversationID, mediaID string) error {
	return c.putJSON("/api/conversations/"+conversationID+"/background",
		map[string]string{"media_id": mediaID}, nil)
}

func (c *Client) DeleteConversationBackground(conversationID string) error {
	return c.deleteReq("/api/conversations/" + conversationID + "/background")
}

// -- Proxy management (user session) --

// CreateProxy creates a proxy account paired to the authenticated user.
func (c *Client) CreateProxy(username string) (*ProxyUserResponse, error) {
	var out ProxyUserResponse
	return &out, c.postJSON("/api/proxy", map[string]string{"username": username}, &out)
}

// GetMyProxy returns the proxy account paired to the authenticated user.
func (c *Client) GetMyProxy() (*ProxyUserResponse, error) {
	var out ProxyUserResponse
	return &out, c.getInto("/api/proxy", nil, &out)
}

// UpdateMyProxy updates profile fields of the paired proxy.
func (c *Client) UpdateMyProxy(params UpdateProxyParams) (*ProxyUserResponse, error) {
	var out ProxyUserResponse
	return &out, c.patchJSON("/api/proxy", params, &out)
}

// SetProxyPassword sets or replaces the proxy's session login password.
func (c *Client) SetProxyPassword(password string) error {
	return c.putJSON("/api/proxy/password", map[string]string{"password": password}, nil)
}

// SetProxyHMACKey sets the proxy's HMAC signing key (64-char hex string).
func (c *Client) SetProxyHMACKey(hmacKey string) error {
	return c.putJSON("/api/proxy/hmac-key", map[string]string{"hmac_key": hmacKey}, nil)
}

// SetProxyE2EKey uploads an E2E key blob for the proxy.
func (c *Client) SetProxyE2EKey(publicKey, e2eKeyBlob string) error {
	return c.putJSON("/api/proxy/e2e-key", map[string]string{"public_key": publicKey, "e2e_key_blob": e2eKeyBlob}, nil)
}

// ListPublicProxies lists all active proxy accounts.
func (c *Client) ListPublicProxies() ([]PublicProxyResponse, error) {
	var out []PublicProxyResponse
	return out, c.getInto("/api/proxy/list-public", nil, &out)
}

//  WebSocket

// ConnectWebSocket opens a real-time WebSocket connection using the current
// session cookie. The server pushes JSON frames; clients do not send frames.
//
// Requires: github.com/gorilla/websocket
//
// Example:
//
//	conn, err := client.ConnectWebSocket()
//	for {
//	    _, msg, err := conn.ReadMessage()
//	    // msg is a JSON frame: {"type": "new_message", ...}
//	}
func (c *Client) ConnectWebSocket() (*websocket.Conn, error) {
	wsURL := strings.TrimRight(c.baseURL, "/") + "/api/ws"
	wsURL = strings.Replace(wsURL, "http://", "ws://", 1)
	wsURL = strings.Replace(wsURL, "https://", "wss://", 1)

	u, err := url.Parse(wsURL)
	if err != nil {
		return nil, err
	}
	cookies := c.http.Jar.Cookies(u)
	parts := make([]string, 0, len(cookies))
	for _, ck := range cookies {
		parts = append(parts, ck.Name+"="+ck.Value)
	}
	header := http.Header{}
	if len(parts) > 0 {
		header.Set("Cookie", strings.Join(parts, "; "))
	}

	conn, _, err := websocket.DefaultDialer.Dial(wsURL, header)
	return conn, err
}

//  HMAC proxy client 

// HMACClient authenticates each request via HMAC-SHA256 request signing.
// No session cookie is required.
type HMACClient struct {
	baseURL string
	proxyID string
	hmacKey string
	http    *http.Client
}

// NewHMACClient creates a new HMAC proxy client.
// hmacKey must be the 64-character lowercase hex string (used as raw key material).
func NewHMACClient(baseURL, proxyID, hmacKey string, opts ...Option) *HMACClient {
	hc := &http.Client{}
	for _, o := range opts {
		o(hc)
	}
	return &HMACClient{
		baseURL: strings.TrimRight(baseURL, "/"),
		proxyID: proxyID,
		hmacKey: hmacKey,
		http:    hc,
	}
}

func (h *HMACClient) sign(message string) string {
	return hmacSHA256Hex(h.hmacKey, message)
}

func (h *HMACClient) postJSON(path string, body any, v any) error {
	data, err := json.Marshal(body)
	if err != nil {
		return err
	}
	req, err := http.NewRequest(http.MethodPost, h.baseURL+path, bytes.NewReader(data))
	if err != nil {
		return err
	}
	req.Header.Set("Content-Type", "application/json")
	resp, err := h.http.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		body, _ := io.ReadAll(resp.Body)
		return &APIError{resp.StatusCode, string(body)}
	}
	if v == nil {
		return nil
	}
	return json.NewDecoder(resp.Body).Decode(v)
}

// SendMessage sends a message to a conversation the proxy is already a member of.
func (h *HMACClient) SendMessage(conversationID, encryptedContent string, mediaIDs []string) (*HMACMessageResponse, error) {
	if mediaIDs == nil {
		mediaIDs = []string{}
	}
	ts := nowUnix()
	contentHash := sha256Hex([]byte(encryptedContent))
	signed := fmt.Sprintf("%s:%s:%s:%d", h.proxyID, conversationID, contentHash, ts)

	var result HMACMessageResponse
	return &result, h.postJSON("/api/proxy/hmac/send-message", map[string]any{
		"proxy_id":          h.proxyID,
		"conversation_id":   conversationID,
		"encrypted_content": encryptedContent,
		"media_ids":         mediaIDs,
		"signature":         h.sign(signed),
		"timestamp":         ts,
	}, &result)
}

// CreatePost creates a post as the proxy. The proxy must be paired to a user account.
func (h *HMACClient) CreatePost(contents []PostContentRequest, publish bool, tags []string) (*HMACPostResponse, error) {
	if tags == nil {
		tags = []string{}
	}
	ts := nowUnix()
	publishBit := "0"
	if publish {
		publishBit = "1"
	}

	sorted := make([]PostContentRequest, len(contents))
	copy(sorted, contents)
	sort.Slice(sorted, func(i, j int) bool {
		return sorted[i].DisplayOrder < sorted[j].DisplayOrder
	})
	lines := make([]string, len(sorted))
	for i, item := range sorted {
		lines[i] = fmt.Sprintf("%d|%s|%s", item.DisplayOrder, item.ContentType, item.Content)
	}
	canonical := strings.Join(lines, "\n")

	sortedTags := make([]string, len(tags))
	copy(sortedTags, tags)
	sort.Strings(sortedTags)
	canonical += "\ntags:" + strings.Join(sortedTags, ",")
	canonical += "\npublish:" + publishBit

	bodyHash := sha256Hex([]byte(canonical))
	signed := fmt.Sprintf("%s:%s:%s:%d", h.proxyID, bodyHash, publishBit, ts)

	var result HMACPostResponse
	return &result, h.postJSON("/api/proxy/hmac/create-post", map[string]any{
		"proxy_id":  h.proxyID,
		"contents":  contents,
		"publish":   publish,
		"tags":      tags,
		"signature": h.sign(signed),
		"timestamp": ts,
	}, &result)
}

// GetOrCreateConversation finds or creates a direct conversation between the proxy
// and a target user. Exactly one of targetUserID or targetUsername must be non-empty.
func (h *HMACClient) GetOrCreateConversation(targetUserID, targetUsername string) (*HMACConversationResponse, error) {
	if (targetUserID == "") == (targetUsername == "") {
		return nil, fmt.Errorf("magnolia: provide exactly one of targetUserID or targetUsername")
	}
	ts := nowUnix()
	signed := fmt.Sprintf("%s:%d", h.proxyID, ts)

	body := map[string]any{
		"proxy_id":        h.proxyID,
		"target_user_id":  nil,
		"target_username": nil,
		"signature":       h.sign(signed),
		"timestamp":       ts,
	}
	if targetUserID != "" {
		body["target_user_id"] = targetUserID
	} else {
		body["target_username"] = targetUsername
	}

	var result HMACConversationResponse
	return &result, h.postJSON("/api/proxy/hmac/get-or-create-conversation", body, &result)
}

// UploadMedia uploads a file from disk as the proxy.
func (h *HMACClient) UploadMedia(filePath string) (*MediaUploadResponse, error) {
	data, err := os.ReadFile(filePath)
	if err != nil {
		return nil, err
	}
	return h.UploadMediaBytes(data, filepath.Base(filePath), "")
}

// UploadMediaBytes uploads raw bytes as the proxy.
// mimeType may be empty; it will be guessed from filename.
// The file hash is computed over the raw bytes (no encoding conversion).
func (h *HMACClient) UploadMediaBytes(data []byte, filename, mimeType string) (*MediaUploadResponse, error) {
	if filename == "" {
		filename = "upload"
	}
	if mimeType == "" {
		mimeType = guessMIME(filename)
	}
	ts := nowUnix()
	fileHash := sha256Hex(data)
	signed := fmt.Sprintf("%s:%s:%d", h.proxyID, fileHash, ts)
	sig := h.sign(signed)

	var buf bytes.Buffer
	w := multipart.NewWriter(&buf)
	_ = w.WriteField("proxy_id", h.proxyID)
	_ = w.WriteField("signature", sig)
	_ = w.WriteField("timestamp", strconv.FormatInt(ts, 10))

	h2 := make(textproto.MIMEHeader)
	h2.Set("Content-Disposition", fmt.Sprintf(`form-data; name="file"; filename="%s"`, filename))
	h2.Set("Content-Type", mimeType)
	part, err := w.CreatePart(h2)
	if err != nil {
		return nil, err
	}
	if _, err := part.Write(data); err != nil {
		return nil, err
	}
	w.Close()

	req, err := http.NewRequest(http.MethodPost, h.baseURL+"/api/proxy/hmac/upload-media", &buf)
	if err != nil {
		return nil, err
	}
	req.Header.Set("Content-Type", w.FormDataContentType())
	resp, err := h.http.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		body, _ := io.ReadAll(resp.Body)
		return nil, &APIError{resp.StatusCode, string(body)}
	}
	var result MediaUploadResponse
	return &result, json.NewDecoder(resp.Body).Decode(&result)
}

//  Proxy session client

// ProxySessionClient authenticates as a proxy account using a session cookie
// (proxy_session_id), separate from the user session.
type ProxySessionClient struct {
	baseURL string
	http    *http.Client
}

// NewProxySessionClient creates a new proxy session client for the given server URL.
func NewProxySessionClient(baseURL string, opts ...Option) *ProxySessionClient {
	jar, _ := cookiejar.New(nil)
	hc := &http.Client{Jar: jar}
	for _, o := range opts {
		o(hc)
	}
	return &ProxySessionClient{baseURL: strings.TrimRight(baseURL, "/"), http: hc}
}

func (p *ProxySessionClient) doJSON(method, path string, body, v any) error {
	var bodyReader io.Reader
	if body != nil {
		data, err := json.Marshal(body)
		if err != nil {
			return err
		}
		bodyReader = bytes.NewReader(data)
	}
	req, err := http.NewRequest(method, p.baseURL+path, bodyReader)
	if err != nil {
		return err
	}
	if body != nil {
		req.Header.Set("Content-Type", "application/json")
	}
	resp, err := p.http.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		b, _ := io.ReadAll(resp.Body)
		return &APIError{resp.StatusCode, string(b)}
	}
	if v == nil {
		return nil
	}
	return json.NewDecoder(resp.Body).Decode(v)
}

// Login starts a proxy session. Sets the proxy_session_id cookie.
func (p *ProxySessionClient) Login(username, password string) (*ProxyAuthResponse, error) {
	var out ProxyAuthResponse
	return &out, p.doJSON("POST", "/api/proxy/login", map[string]string{
		"username": username,
		"password": password,
	}, &out)
}

// Logout ends the current proxy session.
func (p *ProxySessionClient) Logout() error {
	return p.doJSON("POST", "/api/proxy/logout", nil, nil)
}

// Me returns the currently authenticated proxy account.
func (p *ProxySessionClient) Me() (*ProxyUserResponse, error) {
	var out ProxyUserResponse
	return &out, p.doJSON("GET", "/api/proxy/me", nil, &out)
}
