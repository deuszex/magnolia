"""
Integration tests for MagnoliaClient and MagnoliaHMACClient.

Run against a live server:
    MAGNOLIA_BASE_URL=https://... MAGNOLIA_USERNAME=alice MAGNOLIA_PASSWORD=... \\
        pytest test_magnolia.py -v

Required env vars:
    MAGNOLIA_BASE_URL      server root, e.g. https://magnolia.example.com
    MAGNOLIA_USERNAME      login identifier (username or email)
    MAGNOLIA_PASSWORD      login password

Optional env vars (related tests are skipped when absent):
    MAGNOLIA_PROXY_ID      )  both required for HMAC tests
    MAGNOLIA_HMAC_KEY      )
    MAGNOLIA_PROXY_USERNAME  )  both required for proxy session tests
    MAGNOLIA_PROXY_PASSWORD  )
    MAGNOLIA_MEDIA_FILE    path to any file; enables media upload/download tests
    MAGNOLIA_TARGET_USER_ID  enables conversation and message tests
"""

import mimetypes
import os
import time

import pytest

from magnolia_client import MagnoliaClient, MagnoliaError, MagnoliaHMACClient, MagnoliaProxySessionClient

TIMEOUT = 10.0  # seconds per request
PREFIX = f"mgtest_{int(time.time())}"  # collision-free prefix for all created resources


#  Helpers 

def env(key: str) -> str:
    return os.environ.get(key, "")


def require(*keys: str) -> None:
    """Skip the calling test if any env var is missing."""
    missing = [k for k in keys if not env(k)]
    if missing:
        pytest.skip(f"env var(s) not set: {', '.join(missing)}")


def _media_type(path: str) -> str:
    mime, _ = mimetypes.guess_type(path)
    if mime:
        if mime.startswith("image/"):
            return "image"
        if mime.startswith("video/"):
            return "video"
    return "file"


#  Fixtures 

@pytest.fixture(scope="module")
def client():
    require("MAGNOLIA_BASE_URL", "MAGNOLIA_USERNAME", "MAGNOLIA_PASSWORD")
    c = MagnoliaClient(env("MAGNOLIA_BASE_URL"), timeout=TIMEOUT)
    resp = c.login(env("MAGNOLIA_USERNAME"), env("MAGNOLIA_PASSWORD"))
    assert "user" in resp, f"unexpected login response: {resp}"
    yield c
    try:
        c.logout()
    except Exception:
        pass


@pytest.fixture(scope="module")
def hmac_client():
    require("MAGNOLIA_BASE_URL", "MAGNOLIA_PROXY_ID", "MAGNOLIA_HMAC_KEY")
    return MagnoliaHMACClient(
        env("MAGNOLIA_BASE_URL"),
        env("MAGNOLIA_PROXY_ID"),
        env("MAGNOLIA_HMAC_KEY"),
        timeout=TIMEOUT,
    )


#  Auth 

def test_auth_me(client):
    # GET /api/auth/me returns a flat UserResponse (no {"user": ...} wrapper).
    resp = client.me()
    assert resp["user_id"], "me() returned empty user_id"


#  Profile 

def test_profile_get(client):
    user_id = client.me()["user_id"]
    profile = client.get_profile(user_id)
    assert profile["user_id"] == user_id


def test_profile_update(client):
    user_id = client.me()["user_id"]
    original = client.get_profile(user_id).get("bio")
    new_bio = f"{PREFIX}_bio"
    try:
        updated = client.update_profile(bio=new_bio)
        assert updated["bio"] == new_bio
    finally:
        client.update_profile(bio=original)


#  Posts 

def test_posts_full_lifecycle(client):
    # Create as published - GET /api/posts/{id} is a public route whose
    # OptionalAuth extractor cannot resolve the session (no auth middleware on
    # public routes), so draft posts always return 404 even for the author.
    post = client.create_post(
        [{"content_type": "text", "display_order": 0, "content": f"{PREFIX} post"}],
        publish=True,
        tags=[PREFIX],
    )
    post_id = post["post_id"]
    try:
        assert post["is_published"] is True

        # Get
        got = client.get_post(post_id)
        assert got["post_id"] == post_id

        # Update content
        new_content = f"{PREFIX} updated"
        updated = client.update_post(
            post_id,
            contents=[{"content_type": "text", "display_order": 0, "content": new_content}],
        )
        assert updated["contents"][0]["content"] == new_content

        # Publish toggle - we created published, so toggling gives unpublished
        toggled = client.publish_post(post_id)
        assert toggled["is_published"] is False

        # List
        listing = client.list_posts(limit=5)
        assert "posts" in listing

        # Search
        results = client.search_posts(q=PREFIX)
        assert "posts" in results
    finally:
        client.delete_post(post_id)


#  Comments 

def test_comments_full_lifecycle(client):
    post = client.create_post(
        [{"content_type": "text", "display_order": 0, "content": f"{PREFIX} comment-target"}],
        publish=True,
    )
    post_id = post["post_id"]
    comment_id = None
    try:
        comment = client.create_comment(post_id, f"{PREFIX} comment")
        comment_id = comment["comment_id"]
        assert comment["content"] == f"{PREFIX} comment"

        edited = f"{PREFIX} edited"
        updated = client.update_comment(comment_id, edited)
        assert updated["content"] == edited

        listing = client.list_comments(post_id)
        assert any(c["comment_id"] == comment_id for c in listing["comments"])
    finally:
        if comment_id:
            try:
                client.delete_comment(comment_id)
            except Exception:
                pass
        client.delete_post(post_id)


#  Media 

def test_media_upload_download_delete(client):
    require("MAGNOLIA_MEDIA_FILE")
    media_file = env("MAGNOLIA_MEDIA_FILE")
    media_id = None
    try:
        resp = client.upload_media(media_file)
        media_id = resp["media_id"]
        assert resp["url"]

        raw = client.download_media(media_id)
        assert len(raw) > 0

        # Thumbnail is only generated for images/videos; 404 is acceptable otherwise
        try:
            thumb = client.get_thumbnail(media_id)
            assert len(thumb) > 0
        except MagnoliaError as exc:
            assert exc.status_code == 404, f"unexpected thumbnail error: {exc}"

        client.update_media(media_id, description=f"{PREFIX} media")

        # Batch delete exercises a different endpoint; upload a second file first
        resp2 = client.upload_media(media_file)
        client.batch_delete_media([resp2["media_id"]])
    finally:
        if media_id:
            try:
                client.delete_media(media_id)
            except Exception:
                pass


def test_media_chunked_upload(client):
    require("MAGNOLIA_MEDIA_FILE")
    media_file = env("MAGNOLIA_MEDIA_FILE")
    media_id = None
    try:
        # Use a small chunk size so at least two chunks are sent even for tiny files
        resp = client.upload_media_chunked(media_file, _media_type(media_file), chunk_size=64 * 1024)
        media_id = resp["media_id"]
        assert resp["url"]
    finally:
        if media_id:
            try:
                client.delete_media(media_id)
            except Exception:
                pass


#  Conversations + Messages 

def test_conversations_and_messages(client):
    require("MAGNOLIA_TARGET_USER_ID")
    target_id = env("MAGNOLIA_TARGET_USER_ID")

    conv = client.create_conversation("direct", [target_id])
    conv_id = conv["conversation_id"]
    msg_id = None
    try:
        got = client.get_conversation(conv_id)
        assert got["conversation_id"] == conv_id
        assert any(m["user_id"] == target_id for m in got["members"])

        msg = client.send_message(conv_id, f"{PREFIX}_encrypted_payload")
        msg_id = msg["message_id"]
        assert msg["conversation_id"] == conv_id

        listing = client.list_messages(conv_id)
        assert any(m["message_id"] == msg_id for m in listing["messages"])

        counts = client.get_unread_counts()
        assert "counts" in counts

        convs = client.list_conversations()
        assert any(c["conversation_id"] == conv_id for c in convs["conversations"])
    finally:
        if msg_id:
            try:
                print("f")
                client.delete_message(msg_id)
            except Exception:
                pass
        client.delete_conversation(conv_id)


#  Messaging preferences 

def test_messaging_preferences(client):
    prefs = client.get_messaging_preferences()
    original = prefs["accept_messages"]
    try:
        updated = client.update_messaging_preferences(not original)
        assert updated["accept_messages"] is not original
    finally:
        client.update_messaging_preferences(original)


#  Proxy management (user session)

def test_proxy_management(client):
    # If this user has no proxy paired, the test is skipped.
    try:
        proxy = client.get_my_proxy()
    except MagnoliaError as exc:
        if exc.status_code == 404:
            pytest.skip("No proxy account paired to this user")
        raise

    assert proxy["proxy_id"], "proxy_id empty"
    assert proxy["username"], "username empty"

    original_bio = proxy.get("bio")
    new_bio = f"{PREFIX}_proxy_bio"
    try:
        updated = client.update_my_proxy(bio=new_bio)
        assert updated["bio"] == new_bio
    finally:
        client.update_my_proxy(bio=original_bio)


def test_list_public_proxies(client):
    result = client.list_public_proxies()
    assert isinstance(result, list)


#  Proxy session

def test_proxy_session_login_me_logout():
    require("MAGNOLIA_BASE_URL", "MAGNOLIA_PROXY_USERNAME", "MAGNOLIA_PROXY_PASSWORD")
    proxy = MagnoliaProxySessionClient(env("MAGNOLIA_BASE_URL"), timeout=TIMEOUT)
    resp = proxy.login(env("MAGNOLIA_PROXY_USERNAME"), env("MAGNOLIA_PROXY_PASSWORD"))
    assert resp["proxy_id"], "proxy_id empty after login"
    try:
        me = proxy.me()
        assert me["proxy_id"] == resp["proxy_id"]
        assert me["username"] == resp["username"]
    finally:
        proxy.logout()


#  HMAC

def test_hmac_get_or_create_conversation(hmac_client):
    require("MAGNOLIA_TARGET_USER_ID")
    resp = hmac_client.get_or_create_conversation(target_user_id=env("MAGNOLIA_TARGET_USER_ID"))
    assert resp["conversation_id"]


def test_hmac_send_message(hmac_client):
    require("MAGNOLIA_TARGET_USER_ID")
    conv = hmac_client.get_or_create_conversation(target_user_id=env("MAGNOLIA_TARGET_USER_ID"))
    resp = hmac_client.send_message(conv["conversation_id"], f"{PREFIX}_hmac_payload")
    assert resp["message_id"]


def test_hmac_create_post(hmac_client):
    resp = hmac_client.create_post(
        [{"content_type": "text", "display_order": 0, "content": f"{PREFIX} hmac post"}],
        publish=True,
        tags=[PREFIX],
    )
    assert resp["post_id"]


def test_hmac_upload_media(hmac_client):
    require("MAGNOLIA_MEDIA_FILE")
    resp = hmac_client.upload_media(env("MAGNOLIA_MEDIA_FILE"))
    assert resp["media_id"]
    assert resp["url"]
