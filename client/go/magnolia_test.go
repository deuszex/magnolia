// Integration tests for Client and HMACClient.
//
// Run against a live server:
//
//	MAGNOLIA_BASE_URL=https://... MAGNOLIA_USERNAME=alice MAGNOLIA_PASSWORD=... \
//	    go test -v -run TestIntegration ./...
//
// Required env vars:
//
//	MAGNOLIA_BASE_URL       server root, e.g. https://magnolia.example.com
//	MAGNOLIA_USERNAME       login identifier (username or email)
//	MAGNOLIA_PASSWORD       login password
//
// Optional env vars (related subtests are skipped when absent):
//
//	MAGNOLIA_PROXY_ID        )  both required for HMAC subtests
//	MAGNOLIA_HMAC_KEY        )

//	MAGNOLIA_PROXY_USERNAME  )  both required for proxy session subtests
//	MAGNOLIA_PROXY_PASSWORD  )

//	MAGNOLIA_MEDIA_FILE      path to any file; enables media upload/download subtests
//	MAGNOLIA_TARGET_USER_ID  enables conversation and message subtests
package magnolia_test

import (
	"errors"
	"fmt"
	"mime"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	magnolia "magnolia/client"
)

const testTimeout = 10 * time.Second

// mustEnv skips the test if the env var is unset and returns its value.
func mustEnv(t *testing.T, key string) string {
	t.Helper()
	if v := os.Getenv(key); v != "" {
		return v
	}
	t.Skipf("env var not set: %s", key)
	return ""
}

// optEnv returns the env var value or "".
func optEnv(key string) string { return os.Getenv(key) }

// check fatals the test if err is non-nil.
func check(t *testing.T, label string, err error) {
	t.Helper()
	if err != nil {
		t.Fatalf("%s: %v", label, err)
	}
}

// mediaTypeFor guesses the Magnolia media_type string from a file path.
func mediaTypeFor(path string) string {
	mt := mime.TypeByExtension(strings.ToLower(filepath.Ext(path)))
	switch {
	case strings.HasPrefix(mt, "image/"):
		return "image"
	case strings.HasPrefix(mt, "video/"):
		return "video"
	default:
		return "file"
	}
}

func TestIntegration(t *testing.T) {
	baseURL := mustEnv(t, "MAGNOLIA_BASE_URL")
	username := mustEnv(t, "MAGNOLIA_USERNAME")
	password := mustEnv(t, "MAGNOLIA_PASSWORD")

	c := magnolia.NewClient(baseURL, magnolia.WithTimeout(testTimeout))
	prefix := fmt.Sprintf("mgtest_%d", time.Now().Unix())

	// IDs accumulated across subtests; cleaned up in a single deferred block.
	var (
		userID    string
		postID    string
		commentID string
		mediaID   string
		convID    string
		msgID     string
	)

	t.Cleanup(func() {
		// Delete in dependency order so FK constraints are satisfied.
		if msgID != "" {
			_ = c.DeleteMessage(msgID)
		}
		if commentID != "" {
			_ = c.DeleteComment(commentID)
		}
		if mediaID != "" {
			_ = c.DeleteMedia(mediaID)
		}
		if convID != "" {
			_ = c.DeleteConversation(convID)
		}
		if postID != "" {
			_ = c.DeletePost(postID)
		}
		_ = c.Logout()
	})

	//  Auth 

	t.Run("Auth/Login", func(t *testing.T) {
		resp, err := c.Login(username, password)
		check(t, "Login", err)
		if resp.User.UserID == "" {
			t.Fatal("empty user_id in login response")
		}
		userID = resp.User.UserID
	})

	t.Run("Auth/Me", func(t *testing.T) {
		if userID == "" {
			t.Skip("login did not complete")
		}
		resp, err := c.Me()
		check(t, "Me", err)
		if resp.User.UserID != userID {
			t.Fatalf("Me user_id %q != login user_id %q", resp.User.UserID, userID)
		}
	})

	//  Profile 

	t.Run("Profile/Get", func(t *testing.T) {
		if userID == "" {
			t.Skip()
		}
		resp, err := c.GetProfile(userID)
		check(t, "GetProfile", err)
		if resp.UserID != userID {
			t.Fatalf("got user_id %q, want %q", resp.UserID, userID)
		}
	})

	t.Run("Profile/Update", func(t *testing.T) {
		if userID == "" {
			t.Skip()
		}
		orig, err := c.GetProfile(userID)
		check(t, "GetProfile", err)

		newBio := prefix + "_bio"
		updated, err := c.UpdateProfile(magnolia.UpdateProfileParams{Bio: &newBio})
		check(t, "UpdateProfile", err)
		if updated.Bio == nil || *updated.Bio != newBio {
			t.Fatalf("bio not updated, got %v", updated.Bio)
		}
		// Restore original bio regardless of test outcome.
		_, _ = c.UpdateProfile(magnolia.UpdateProfileParams{Bio: orig.Bio})
	})

	//  Posts 

	t.Run("Posts/Create", func(t *testing.T) {
		if userID == "" {
			t.Skip()
		}
		resp, err := c.CreatePost([]magnolia.PostContentRequest{
			{ContentType: "text", DisplayOrder: 0, Content: prefix + " post"},
		}, false, []string{prefix})
		check(t, "CreatePost", err)
		if resp.PostID == "" {
			t.Fatal("empty post_id")
		}
		postID = resp.PostID
	})

	t.Run("Posts/Get", func(t *testing.T) {
		if postID == "" {
			t.Skip()
		}
		resp, err := c.GetPost(postID)
		check(t, "GetPost", err)
		if resp.PostID != postID {
			t.Fatalf("got %q, want %q", resp.PostID, postID)
		}
	})

	t.Run("Posts/Update", func(t *testing.T) {
		if postID == "" {
			t.Skip()
		}
		newContent := prefix + " updated"
		resp, err := c.UpdatePost(postID, []magnolia.PostContentRequest{
			{ContentType: "text", DisplayOrder: 0, Content: newContent},
		}, nil, nil)
		check(t, "UpdatePost", err)
		if len(resp.Contents) == 0 || resp.Contents[0].Content != newContent {
			t.Fatalf("content not updated: %+v", resp.Contents)
		}
	})

	t.Run("Posts/Publish", func(t *testing.T) {
		if postID == "" {
			t.Skip()
		}
		resp, err := c.PublishPost(postID)
		check(t, "PublishPost", err)
		if !resp.IsPublished {
			t.Fatal("expected is_published=true after toggle")
		}
	})

	t.Run("Posts/List", func(t *testing.T) {
		if userID == "" {
			t.Skip()
		}
		resp, err := c.ListPosts(magnolia.ListPostsParams{Limit: 5})
		check(t, "ListPosts", err)
		if resp.Posts == nil {
			t.Fatal("nil posts slice")
		}
	})

	t.Run("Posts/Search", func(t *testing.T) {
		if postID == "" {
			t.Skip()
		}
		resp, err := c.SearchPosts(magnolia.SearchPostsParams{Q: prefix})
		check(t, "SearchPosts", err)
		if resp.Posts == nil {
			t.Fatal("nil posts slice")
		}
	})

	//  Comments 

	t.Run("Comments/Create", func(t *testing.T) {
		if postID == "" {
			t.Skip()
		}
		resp, err := c.CreateComment(postID, prefix+" comment", "", "", "", "")
		check(t, "CreateComment", err)
		if resp.CommentID == "" {
			t.Fatal("empty comment_id")
		}
		commentID = resp.CommentID
	})

	t.Run("Comments/Update", func(t *testing.T) {
		if commentID == "" {
			t.Skip()
		}
		edited := prefix + " edited"
		resp, err := c.UpdateComment(commentID, edited)
		check(t, "UpdateComment", err)
		if resp.Content != edited {
			t.Fatalf("got %q, want %q", resp.Content, edited)
		}
	})

	t.Run("Comments/List", func(t *testing.T) {
		if postID == "" || commentID == "" {
			t.Skip()
		}
		resp, err := c.ListComments(postID, magnolia.ListCommentsParams{})
		check(t, "ListComments", err)
		for _, cm := range resp.Comments {
			if cm.CommentID == commentID {
				return
			}
		}
		t.Fatalf("comment %q not found in listing", commentID)
	})

	//  Media 

	mediaFile := optEnv("MAGNOLIA_MEDIA_FILE")

	t.Run("Media/Upload", func(t *testing.T) {
		if mediaFile == "" {
			t.Skip("MAGNOLIA_MEDIA_FILE not set")
		}
		resp, err := c.UploadMedia(mediaFile, magnolia.UploadMediaParams{})
		check(t, "UploadMedia", err)
		if resp.MediaID == "" {
			t.Fatal("empty media_id")
		}
		mediaID = resp.MediaID
	})

	t.Run("Media/Download", func(t *testing.T) {
		if mediaID == "" {
			t.Skip()
		}
		data, err := c.DownloadMedia(mediaID)
		check(t, "DownloadMedia", err)
		if len(data) == 0 {
			t.Fatal("empty download response")
		}
	})

	t.Run("Media/Thumbnail", func(t *testing.T) {
		if mediaID == "" {
			t.Skip()
		}
		_, err := c.GetThumbnail(mediaID)
		if err != nil {
			var apiErr *magnolia.APIError
			if errors.As(err, &apiErr) && apiErr.StatusCode == 404 {
				t.Log("no thumbnail for this media type - acceptable")
				return
			}
			t.Fatalf("GetThumbnail: %v", err)
		}
	})

	t.Run("Media/Update", func(t *testing.T) {
		if mediaID == "" {
			t.Skip()
		}
		desc := prefix + " media"
		_, err := c.UpdateMedia(mediaID, &desc, nil)
		check(t, "UpdateMedia", err)
	})

	t.Run("Media/BatchDelete", func(t *testing.T) {
		if mediaFile == "" {
			t.Skip("MAGNOLIA_MEDIA_FILE not set")
		}
		// Upload a throwaway file specifically to batch-delete it.
		resp, err := c.UploadMedia(mediaFile, magnolia.UploadMediaParams{})
		check(t, "UploadMedia (batch target)", err)
		result, err := c.BatchDeleteMedia([]string{resp.MediaID})
		check(t, "BatchDeleteMedia", err)
		if result.SuccessCount != 1 {
			t.Fatalf("expected success_count=1, got %d", result.SuccessCount)
		}
	})

	t.Run("Media/UploadChunked", func(t *testing.T) {
		if mediaFile == "" {
			t.Skip("MAGNOLIA_MEDIA_FILE not set")
		}
		// 64 KiB chunks so multiple chunks are sent even for small files.
		resp, err := c.UploadMediaChunked(mediaFile, mediaTypeFor(mediaFile), 64*1024)
		check(t, "UploadMediaChunked", err)
		if resp.MediaID == "" {
			t.Fatal("empty media_id")
		}
		// Clean up immediately; not tracked in the outer deferred block.
		t.Cleanup(func() { _ = c.DeleteMedia(resp.MediaID) })
	})

	//  Conversations + Messages 

	targetUserID := optEnv("MAGNOLIA_TARGET_USER_ID")

	t.Run("Conversations/Create", func(t *testing.T) {
		if targetUserID == "" {
			t.Skip("MAGNOLIA_TARGET_USER_ID not set")
		}
		resp, err := c.CreateConversation("direct", []string{targetUserID}, "")
		check(t, "CreateConversation", err)
		if resp.ConversationID == "" {
			t.Fatal("empty conversation_id")
		}
		convID = resp.ConversationID
	})

	t.Run("Conversations/Get", func(t *testing.T) {
		if convID == "" {
			t.Skip()
		}
		resp, err := c.GetConversation(convID)
		check(t, "GetConversation", err)
		if resp.ConversationID != convID {
			t.Fatalf("got %q, want %q", resp.ConversationID, convID)
		}
		found := false
		for _, m := range resp.Members {
			if m.UserID == targetUserID {
				found = true
				break
			}
		}
		if !found {
			t.Fatalf("target user %q not in members list", targetUserID)
		}
	})

	t.Run("Conversations/List", func(t *testing.T) {
		resp, err := c.ListConversations(magnolia.ListConversationsParams{Limit: 5})
		check(t, "ListConversations", err)
		if resp.Conversations == nil {
			t.Fatal("nil conversations slice")
		}
	})

	t.Run("Messages/Send", func(t *testing.T) {
		if convID == "" {
			t.Skip()
		}
		resp, err := c.SendMessage(convID, prefix+"_payload", nil)
		check(t, "SendMessage", err)
		if resp.MessageID == "" {
			t.Fatal("empty message_id")
		}
		msgID = resp.MessageID
	})

	t.Run("Messages/List", func(t *testing.T) {
		if convID == "" || msgID == "" {
			t.Skip()
		}
		resp, err := c.ListMessages(convID, magnolia.ListMessagesParams{})
		check(t, "ListMessages", err)
		for _, m := range resp.Messages {
			if m.MessageID == msgID {
				return
			}
		}
		t.Fatalf("message %q not found in listing", msgID)
	})

	t.Run("Messages/UnreadCounts", func(t *testing.T) {
		resp, err := c.GetUnreadCounts()
		check(t, "GetUnreadCounts", err)
		if resp.Counts == nil {
			t.Fatal("nil counts map")
		}
	})

	//  Messaging preferences 

	t.Run("MessagingPrefs/GetAndToggle", func(t *testing.T) {
		if userID == "" {
			t.Skip()
		}
		prefs, err := c.GetMessagingPreferences()
		check(t, "GetMessagingPreferences", err)
		original := prefs.AcceptMessages

		toggled, err := c.UpdateMessagingPreferences(!original)
		check(t, "UpdateMessagingPreferences", err)
		if toggled.AcceptMessages == original {
			t.Fatal("preference was not toggled")
		}
		// Restore.
		_, _ = c.UpdateMessagingPreferences(original)
	})

	//  Proxy management (user session)

	t.Run("Proxy/GetMyProxy", func(t *testing.T) {
		if userID == "" {
			t.Skip()
		}
		resp, err := c.GetMyProxy()
		if err != nil {
			var apiErr *magnolia.APIError
			if errors.As(err, &apiErr) && apiErr.StatusCode == 404 {
				t.Skip("no proxy account paired to this user")
			}
			t.Fatalf("GetMyProxy: %v", err)
		}
		if resp.ProxyID == "" {
			t.Fatal("empty proxy_id")
		}
	})

	t.Run("Proxy/ListPublicProxies", func(t *testing.T) {
		if userID == "" {
			t.Skip()
		}
		proxies, err := c.ListPublicProxies()
		check(t, "ListPublicProxies", err)
		_ = proxies // may be empty; just verify no error
	})

	//  Proxy session

	proxyUsername := optEnv("MAGNOLIA_PROXY_USERNAME")
	proxyPassword := optEnv("MAGNOLIA_PROXY_PASSWORD")

	t.Run("ProxySession/LoginMeLogout", func(t *testing.T) {
		if proxyUsername == "" || proxyPassword == "" {
			t.Skip("MAGNOLIA_PROXY_USERNAME / MAGNOLIA_PROXY_PASSWORD not set")
		}
		p := magnolia.NewProxySessionClient(baseURL, magnolia.WithTimeout(testTimeout))
		loginResp, err := p.Login(proxyUsername, proxyPassword)
		check(t, "Login", err)
		if loginResp.ProxyID == "" {
			t.Fatal("empty proxy_id after login")
		}
		me, err := p.Me()
		check(t, "Me", err)
		if me.ProxyID != loginResp.ProxyID {
			t.Fatalf("Me proxy_id %q != login proxy_id %q", me.ProxyID, loginResp.ProxyID)
		}
		check(t, "Logout", p.Logout())
	})

	//  HMAC

	proxyID := optEnv("MAGNOLIA_PROXY_ID")
	hmacKey := optEnv("MAGNOLIA_HMAC_KEY")

	if proxyID == "" || hmacKey == "" {
		t.Log("MAGNOLIA_PROXY_ID / MAGNOLIA_HMAC_KEY not set - skipping HMAC subtests")
		return
	}

	h := magnolia.NewHMACClient(baseURL, proxyID, hmacKey, magnolia.WithTimeout(testTimeout))

	t.Run("HMAC/GetOrCreateConversation", func(t *testing.T) {
		if targetUserID == "" {
			t.Skip("MAGNOLIA_TARGET_USER_ID not set")
		}
		resp, err := h.GetOrCreateConversation(targetUserID, "")
		check(t, "GetOrCreateConversation", err)
		if resp.ConversationID == "" {
			t.Fatal("empty conversation_id")
		}
	})

	t.Run("HMAC/SendMessage", func(t *testing.T) {
		if targetUserID == "" {
			t.Skip("MAGNOLIA_TARGET_USER_ID not set")
		}
		conv, err := h.GetOrCreateConversation(targetUserID, "")
		check(t, "GetOrCreateConversation", err)
		resp, err := h.SendMessage(conv.ConversationID, prefix+"_hmac_payload", nil)
		check(t, "SendMessage", err)
		if resp.MessageID == "" {
			t.Fatal("empty message_id")
		}
	})

	t.Run("HMAC/CreatePost", func(t *testing.T) {
		resp, err := h.CreatePost([]magnolia.PostContentRequest{
			{ContentType: "text", DisplayOrder: 0, Content: prefix + " hmac post"},
		}, false, []string{prefix})
		check(t, "CreatePost", err)
		if resp.PostID == "" {
			t.Fatal("empty post_id")
		}
	})

	t.Run("HMAC/UploadMedia", func(t *testing.T) {
		if mediaFile == "" {
			t.Skip("MAGNOLIA_MEDIA_FILE not set")
		}
		resp, err := h.UploadMedia(mediaFile)
		check(t, "UploadMedia", err)
		if resp.MediaID == "" {
			t.Fatal("empty media_id")
		}
	})
}
