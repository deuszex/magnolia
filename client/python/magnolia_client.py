"""
Magnolia API client - session and HMAC proxy modes.

Requirements:
    pip install requests

WebSocket support (optional, for real-time events):
    pip install websocket-client
"""

from __future__ import annotations

import hashlib
import hmac as _hmac
import mimetypes
import time
from pathlib import Path
from typing import Any, Optional

import requests
from requests import Response


# --------------------------------------------------------------------------- #
# Shared utilities
# --------------------------------------------------------------------------- #

class MagnoliaError(Exception):
    """Raised when the server returns a non-2xx status."""

    def __init__(self, status_code: int, body: Any) -> None:
        self.status_code = status_code
        self.body = body
        super().__init__(f"HTTP {status_code}: {body}")


def _raise(resp: Response) -> Response:
    if not resp.ok:
        try:
            body = resp.json()
        except Exception:
            body = resp.text
        raise MagnoliaError(resp.status_code, body)
    return resp


def _sha256_hex(data: str | bytes) -> str:
    if isinstance(data, str):
        data = data.encode()
    return hashlib.sha256(data).hexdigest()


def _hmac_sha256_hex(key: str, message: str) -> str:
    """
    HMAC-SHA256 where the key is the 64-char hex string treated as raw key
    material (each character as a byte), NOT decoded to 32 bytes.
    """
    return _hmac.new(key.encode(), message.encode(), hashlib.sha256).hexdigest()


def _guess_mime(filename: str) -> str:
    mime, _ = mimetypes.guess_type(filename)
    return mime or "application/octet-stream"


def _strip_none(d: dict) -> dict:
    return {k: v for k, v in d.items() if v is not None}


class _Session(requests.Session):
    """requests.Session that applies a default timeout to every request."""

    def __init__(self, timeout: Optional[float]) -> None:
        super().__init__()
        self._default_timeout = timeout

    def request(self, method, url, **kwargs):
        kwargs.setdefault("timeout", self._default_timeout)
        return super().request(method, url, **kwargs)


# --------------------------------------------------------------------------- #
# Session client
# --------------------------------------------------------------------------- #

class MagnoliaClient:
    """
    Session-based Magnolia API client.

    The session cookie (``session_id``) is managed automatically by the
    underlying ``requests.Session``.

    Usage::

        client = MagnoliaClient("https://magnolia.example.com")
        client.login("alice", "hunter2")
        posts = client.list_posts(limit=10)
    """

    def __init__(self, base_url: str, timeout: Optional[float] = None) -> None:
        self.base_url = base_url.rstrip("/")
        self._s = _Session(timeout)

    # -- Internal helpers --------------------------------------------------- #

    def _url(self, path: str) -> str:
        return f"{self.base_url}{path}"

    def _get(self, path: str, **kwargs) -> Any:
        return _raise(self._s.get(self._url(path), **kwargs)).json()

    def _post(self, path: str, **kwargs) -> Response:
        return _raise(self._s.post(self._url(path), **kwargs))

    def _put(self, path: str, **kwargs) -> Any:
        return _raise(self._s.put(self._url(path), **kwargs)).json()

    def _put_empty(self, path: str, **kwargs) -> None:
        _raise(self._s.put(self._url(path), **kwargs))

    def _patch(self, path: str, **kwargs) -> Any:
        return _raise(self._s.patch(self._url(path), **kwargs)).json()

    def _delete(self, path: str, **kwargs) -> None:
        _raise(self._s.delete(self._url(path), **kwargs))

    # -- Auth --------------------------------------------------------------- #

    def login(self, identifier: str, password: str) -> dict:
        """Start a session. Returns ``{"user": {...}}``."""
        return self._post(
            "/api/auth/login",
            json={"identifier": identifier, "password": password},
        ).json()

    def logout(self) -> None:
        self._post("/api/auth/logout")

    def me(self) -> dict:
        """Return the currently authenticated user.
        Returns a flat ``UserResponse`` dict (not wrapped in ``{"user": ...}``).
        """
        return self._get("/api/auth/me")

    def change_password(
        self,
        current_password: str,
        new_password: str,
        new_password_confirm: str,
    ) -> None:
        self._post(
            "/api/auth/change-password",
            json={
                "current_password": current_password,
                "new_password": new_password,
                "new_password_confirm": new_password_confirm,
            },
        )

    # -- Profile ------------------------------------------------------------ #

    def get_profile(self, user_id: str) -> dict:
        return self._get(f"/api/users/{user_id}/profile")

    def update_profile(
        self,
        *,
        display_name: Optional[str] = None,
        bio: Optional[str] = None,
        avatar_media_id: Optional[str] = None,
        location: Optional[str] = None,
        website: Optional[str] = None,
    ) -> dict:
        return self._put(
            "/api/profile",
            json={
                "display_name": display_name,
                "bio": bio,
                "avatar_media_id": avatar_media_id,
                "location": location,
                "website": website,
            },
        )

    # -- Posts -------------------------------------------------------------- #

    def list_posts(
        self,
        *,
        author_id: Optional[str] = None,
        include_drafts: bool = False,
        content_type: Optional[str] = None,
        limit: int = 20,
        offset: int = 0,
        after: Optional[str] = None,
    ) -> dict:
        return self._get(
            "/api/posts",
            params=_strip_none({
                "author_id": author_id,
                "include_drafts": include_drafts or None,
                "content_type": content_type,
                "limit": limit,
                "offset": offset,
                "after": after,
            }),
        )

    def get_post(self, post_id: str) -> dict:
        return self._get(f"/api/posts/{post_id}")

    def create_post(
        self,
        contents: list[dict],
        *,
        publish: bool = False,
        tags: Optional[list[str]] = None,
    ) -> dict:
        return self._post(
            "/api/posts",
            json={"contents": contents, "publish": publish, "tags": tags or []},
        ).json()

    def update_post(
        self,
        post_id: str,
        *,
        contents: Optional[list[dict]] = None,
        publish: Optional[bool] = None,
        tags: Optional[list[str]] = None,
    ) -> dict:
        return self._put(
            f"/api/posts/{post_id}",
            json=_strip_none({"contents": contents, "publish": publish, "tags": tags}),
        )

    def delete_post(self, post_id: str) -> None:
        self._delete(f"/api/posts/{post_id}")

    def publish_post(self, post_id: str) -> dict:
        """Toggle the published state of a post."""
        return self._post(f"/api/posts/{post_id}/publish").json()

    def search_posts(
        self,
        *,
        q: Optional[str] = None,
        tags: Optional[str] = None,
        has_images: Optional[bool] = None,
        has_videos: Optional[bool] = None,
        has_files: Optional[bool] = None,
        author_id: Optional[str] = None,
        from_date: Optional[str] = None,
        to_date: Optional[str] = None,
        limit: int = 20,
        offset: int = 0,
    ) -> dict:
        return self._get(
            "/api/posts/search",
            params=_strip_none({
                "q": q,
                "tags": tags,
                "has_images": has_images,
                "has_videos": has_videos,
                "has_files": has_files,
                "author_id": author_id,
                "from_date": from_date,
                "to_date": to_date,
                "limit": limit,
                "offset": offset,
            }),
        )

    # -- Comments ----------------------------------------------------------- #

    def list_comments(
        self,
        post_id: str,
        *,
        parent_comment_id: Optional[str] = None,
        include_replies: bool = False,
        sort: str = "newest",
        limit: int = 50,
        offset: int = 0,
    ) -> dict:
        return self._get(
            f"/api/posts/{post_id}/comments",
            params=_strip_none({
                "parent_comment_id": parent_comment_id,
                "include_replies": include_replies or None,
                "sort": sort,
                "limit": limit,
                "offset": offset,
            }),
        )

    def create_comment(
        self,
        post_id: str,
        content: str,
        *,
        content_type: str = "text",
        parent_comment_id: Optional[str] = None,
        filename: Optional[str] = None,
        mime_type: Optional[str] = None,
    ) -> dict:
        return self._post(
            f"/api/posts/{post_id}/comments",
            json={
                "content_type": content_type,
                "content": content,
                "parent_comment_id": parent_comment_id,
                "filename": filename,
                "mime_type": mime_type,
            },
        ).json()

    def update_comment(self, comment_id: str, content: str) -> dict:
        return self._put(f"/api/comments/{comment_id}", json={"content": content})

    def delete_comment(self, comment_id: str) -> None:
        self._delete(f"/api/comments/{comment_id}")

    # -- Media -------------------------------------------------------------- #

    def upload_media(
        self,
        file: str | Path | bytes,
        *,
        filename: Optional[str] = None,
        mime_type: Optional[str] = None,
        description: Optional[str] = None,
        tags: Optional[str] = None,
    ) -> dict:
        """
        Upload a file. ``file`` may be a path (str/Path) or raw bytes.

        Returns ``{"media_id": ..., "url": ..., "thumbnail_url": ...}``.
        """
        if isinstance(file, (str, Path)):
            path = Path(file)
            filename = filename or path.name
            raw = path.read_bytes()
        else:
            raw = file

        form: dict = {}
        if description:
            form["description"] = description
        if tags:
            form["tags"] = tags

        return self._post(
            "/api/media",
            files={"file": (filename or "upload", raw, mime_type or _guess_mime(filename or ""))},
            data=form,
        ).json()

    def upload_media_chunked(
        self,
        file: str | Path,
        media_type: str,
        *,
        chunk_size: int = 5 * 1024 * 1024,
    ) -> dict:
        """
        Chunked upload for large files. ``media_type`` must be one of
        ``image``, ``video``, or ``file``.

        Returns the same shape as :meth:`upload_media`.
        """
        path = Path(file)
        total_size = path.stat().st_size

        init = self._post(
            "/api/media/chunked/init",
            json={
                "media_type": media_type,
                "filename": path.name,
                "mime_type": _guess_mime(path.name),
                "total_size": total_size,
                "chunk_size": chunk_size,
            },
        ).json()

        upload_id: str = init["upload_id"]
        server_chunk_size: int = init["chunk_size"]

        with path.open("rb") as fh:
            chunk_number = 0
            while True:
                chunk = fh.read(server_chunk_size)
                if not chunk:
                    break
                _raise(
                    self._s.post(
                        self._url(f"/api/media/chunked/{upload_id}/{chunk_number}"),
                        data=chunk,
                        headers={"Content-Type": "application/octet-stream"},
                    )
                )
                chunk_number += 1

        return self._post(f"/api/media/chunked/{upload_id}/complete").json()

    def download_media(self, media_id: str) -> bytes:
        return _raise(self._s.get(self._url(f"/api/media/{media_id}/file"))).content

    def get_thumbnail(self, media_id: str) -> bytes:
        return _raise(self._s.get(self._url(f"/api/media/{media_id}/thumbnail"))).content

    def update_media(
        self,
        media_id: str,
        *,
        description: Optional[str] = None,
        tags: Optional[list[str]] = None,
    ) -> dict:
        return self._put(
            f"/api/media/{media_id}",
            json={"description": description, "tags": tags},
        )

    def delete_media(self, media_id: str) -> None:
        self._delete(f"/api/media/{media_id}")

    def batch_delete_media(self, media_ids: list[str]) -> dict:
        return self._post("/api/media/batch-delete", json={"media_ids": media_ids}).json()

    # -- Conversations ------------------------------------------------------ #

    def list_conversations(
        self,
        *,
        limit: Optional[int] = None,
        offset: int = 0,
    ) -> dict:
        return self._get(
            "/api/conversations",
            params=_strip_none({"limit": limit, "offset": offset}),
        )

    def create_conversation(
        self,
        conversation_type: str,
        member_ids: list[str],
        *,
        name: Optional[str] = None,
    ) -> dict:
        return self._post(
            "/api/conversations",
            json={
                "conversation_type": conversation_type,
                "name": name,
                "member_ids": member_ids,
            },
        ).json()

    def get_conversation(self, conversation_id: str) -> dict:
        return self._get(f"/api/conversations/{conversation_id}")

    def update_conversation(self, conversation_id: str, name: str) -> dict:
        return self._put(f"/api/conversations/{conversation_id}", json={"name": name})

    def delete_conversation(self, conversation_id: str) -> None:
        self._delete(f"/api/conversations/{conversation_id}")

    def add_conversation_member(self, conversation_id: str, user_id: str) -> dict:
        return self._post(
            f"/api/conversations/{conversation_id}/members",
            json={"user_id": user_id},
        ).json()

    def remove_conversation_member(self, conversation_id: str, user_id: str) -> None:
        self._delete(f"/api/conversations/{conversation_id}/members/{user_id}")

    # -- Messages ----------------------------------------------------------- #

    def send_message(
        self,
        conversation_id: str,
        encrypted_content: str,
        *,
        media_ids: Optional[list[str]] = None,
    ) -> dict:
        return self._post(
            f"/api/conversations/{conversation_id}/messages",
            json={
                "encrypted_content": encrypted_content,
                "media_ids": media_ids or [],
            },
        ).json()

    def list_messages(
        self,
        conversation_id: str,
        *,
        limit: Optional[int] = None,
        offset: int = 0,
    ) -> dict:
        return self._get(
            f"/api/conversations/{conversation_id}/messages",
            params=_strip_none({"limit": limit, "offset": offset}),
        )

    def delete_message(self, message_id: str) -> None:
        self._delete(f"/api/messages/{message_id}")

    def get_unread_counts(self) -> dict:
        return self._get("/api/messaging/unread")

    def list_conversation_media(
        self,
        conversation_id: str,
        *,
        media_type: Optional[str] = None,
        limit: int = 50,
        offset: int = 0,
    ) -> dict:
        return self._get(
            f"/api/conversations/{conversation_id}/media",
            params=_strip_none({"media_type": media_type, "limit": limit, "offset": offset}),
        )

    # -- Messaging preferences / blacklist / favourites / background -------- #

    def get_messaging_preferences(self) -> dict:
        return self._get("/api/messaging/preferences")

    def update_messaging_preferences(self, accept_messages: bool) -> dict:
        return self._put("/api/messaging/preferences", json={"accept_messages": accept_messages})

    def list_blocked_users(self) -> dict:
        return self._get("/api/messaging/blacklist")

    def block_user(self, user_id: str) -> dict:
        return self._post("/api/messaging/blacklist", json={"user_id": user_id}).json()

    def unblock_user(self, user_id: str) -> None:
        self._delete(f"/api/messaging/blacklist/{user_id}")

    def add_favourite(self, conversation_id: str) -> None:
        self._post("/api/messaging/favourites", json={"conversation_id": conversation_id})

    def remove_favourite(self, conversation_id: str) -> None:
        self._delete(f"/api/messaging/favourites/{conversation_id}")

    def get_conversation_background(self, conversation_id: str) -> dict:
        return self._get(f"/api/conversations/{conversation_id}/background")

    def set_conversation_background(self, conversation_id: str, media_id: str) -> dict:
        return self._put(
            f"/api/conversations/{conversation_id}/background",
            json={"media_id": media_id},
        )

    def delete_conversation_background(self, conversation_id: str) -> None:
        self._delete(f"/api/conversations/{conversation_id}/background")

    # -- Proxy management (requires user session) -------------------------- #

    def create_proxy(self, username: str) -> dict:
        """Create a proxy account paired to the authenticated user."""
        return self._post("/api/proxy", json={"username": username}).json()

    def get_my_proxy(self) -> dict:
        """Get the proxy account paired to the authenticated user."""
        return self._get("/api/proxy")

    def update_my_proxy(
        self,
        *,
        display_name: Optional[str] = None,
        bio: Optional[str] = None,
        avatar_media_id: Optional[str] = None,
        active: Optional[bool] = None,
    ) -> dict:
        return self._patch(
            "/api/proxy",
            json=_strip_none(
                {
                    "display_name": display_name,
                    "bio": bio,
                    "avatar_media_id": avatar_media_id,
                    "active": active,
                }
            ),
        )

    def set_proxy_password(self, password: str) -> None:
        """Set or replace the proxy's session login password."""
        self._put_empty("/api/proxy/password", json={"password": password})

    def set_proxy_hmac_key(self, hmac_key: str) -> None:
        """Set the proxy's HMAC signing key (64-char hex string)."""
        self._put_empty("/api/proxy/hmac-key", json={"hmac_key": hmac_key})

    def set_proxy_e2e_key(self, public_key: str, e2e_key_blob: str) -> None:
        """Upload an E2E key blob for the proxy."""
        self._put_empty(
            "/api/proxy/e2e-key",
            json={"public_key": public_key, "e2e_key_blob": e2e_key_blob},
        )

    def list_public_proxies(self) -> list:
        """List all active proxy accounts."""
        return self._get("/api/proxy/list-public")

    # -- WebSocket ---------------------------------------------------------- #

    def connect_websocket(self):
        """
        Open a real-time WebSocket connection. Requires ``websocket-client``.

        Returns a ``websocket.WebSocketApp`` that pushes JSON frames from the
        server. Clients do not send frames. The session cookie is forwarded
        automatically.

        Example::

            import json

            def on_message(ws, raw):
                event = json.loads(raw)
                print(event["type"], event)

            ws = client.connect_websocket()
            ws.on_message = on_message
            ws.run_forever()
        """
        try:
            import websocket  # type: ignore[import]
        except ImportError as exc:
            raise ImportError(
                "websocket-client is required for WebSocket support: "
                "pip install websocket-client"
            ) from exc

        cookie_jar = self._s.cookies
        cookie_header = "; ".join(f"{c.name}={c.value}" for c in cookie_jar)
        ws_url = self._url("/api/ws").replace("http://", "ws://").replace("https://", "wss://")
        return websocket.WebSocketApp(ws_url, header={"Cookie": cookie_header})


# --------------------------------------------------------------------------- #
# Proxy session client
# --------------------------------------------------------------------------- #

class MagnoliaProxySessionClient:
    """
    Session client for a proxy account.

    Proxy accounts have their own session cookie (``proxy_session_id``) and
    login endpoint, separate from user sessions.  This client covers the
    proxy session lifecycle and the session-authenticated proxy endpoints.

    Usage::

        proxy = MagnoliaProxySessionClient("https://magnolia.example.com")
        proxy.login("my-bot", "password")
        me = proxy.me()
    """

    def __init__(self, base_url: str, timeout: Optional[float] = None) -> None:
        self.base_url = base_url.rstrip("/")
        self._s = _Session(timeout)

    def _url(self, path: str) -> str:
        return f"{self.base_url}{path}"

    def _get(self, path: str, **kwargs) -> Any:
        return _raise(self._s.get(self._url(path), **kwargs)).json()

    def _post(self, path: str, **kwargs) -> Response:
        return _raise(self._s.post(self._url(path), **kwargs))

    def _patch(self, path: str, **kwargs) -> Any:
        return _raise(self._s.patch(self._url(path), **kwargs)).json()

    def login(self, username: str, password: str) -> dict:
        """Start a proxy session. Returns ``ProxyAuthResponse``."""
        return self._post(
            "/api/proxy/login",
            json={"username": username, "password": password},
        ).json()

    def logout(self) -> None:
        """End the proxy session."""
        self._post("/api/proxy/logout")

    def me(self) -> dict:
        """Return the currently authenticated proxy account."""
        return self._get("/api/proxy/me")


# --------------------------------------------------------------------------- #
# HMAC proxy client
# --------------------------------------------------------------------------- #

class MagnoliaHMACClient:
    """
    Proxy client authenticated via per-request HMAC-SHA256 signatures.
    No session cookie - each call is independently signed.

    Usage::

        proxy = MagnoliaHMACClient(
            base_url="https://magnolia.example.com",
            proxy_id="proxy-uuid",
            hmac_key="64-char-hex-string",
        )
        result = proxy.get_or_create_conversation(target_username="bob")
        proxy.send_message(result["conversation_id"], encrypted_content="...")
    """

    def __init__(self, base_url: str, proxy_id: str, hmac_key: str, timeout: Optional[float] = None) -> None:
        """
        :param base_url:  Server root URL.
        :param proxy_id:  The proxy account's user ID.
        :param hmac_key:  64-character lowercase hex string used as raw key material.
        :param timeout:   Per-request timeout in seconds, or None for no timeout.
        """
        self.base_url = base_url.rstrip("/")
        self.proxy_id = proxy_id
        self.hmac_key = hmac_key
        self._s = _Session(timeout)

    def _url(self, path: str) -> str:
        return f"{self.base_url}{path}"

    @staticmethod
    def _now() -> int:
        return int(time.time())

    def _sign(self, message: str) -> str:
        return _hmac_sha256_hex(self.hmac_key, message)

    def send_message(
        self,
        conversation_id: str,
        encrypted_content: str,
        *,
        media_ids: Optional[list[str]] = None,
    ) -> dict:
        """Send a message to a conversation the proxy is already a member of."""
        ts = self._now()
        signed = f"{self.proxy_id}:{conversation_id}:{_sha256_hex(encrypted_content)}:{ts}"
        return _raise(
            self._s.post(
                self._url("/api/proxy/hmac/send-message"),
                json={
                    "proxy_id": self.proxy_id,
                    "conversation_id": conversation_id,
                    "encrypted_content": encrypted_content,
                    "media_ids": media_ids or [],
                    "signature": self._sign(signed),
                    "timestamp": ts,
                },
            )
        ).json()

    def create_post(
        self,
        contents: list[dict],
        *,
        publish: bool = False,
        tags: Optional[list[str]] = None,
    ) -> dict:
        """
        Create a post as the proxy. The proxy must be paired to a user account.

        ``contents`` items must have ``display_order``, ``content_type``, and
        ``content`` keys (plus optional ``filename``, ``mime_type``, ``media_id``).
        """
        tags = tags or []
        ts = self._now()
        publish_bit = "1" if publish else "0"

        sorted_contents = sorted(contents, key=lambda c: c["display_order"])
        lines = [
            f"{c['display_order']}|{c['content_type']}|{c['content']}"
            for c in sorted_contents
        ]
        canonical = "\n".join(lines)
        canonical += "\ntags:" + ",".join(sorted(tags))
        canonical += f"\npublish:{publish_bit}"

        body_hash = _sha256_hex(canonical)
        signed = f"{self.proxy_id}:{body_hash}:{publish_bit}:{ts}"

        return _raise(
            self._s.post(
                self._url("/api/proxy/hmac/create-post"),
                json={
                    "proxy_id": self.proxy_id,
                    "contents": contents,
                    "publish": publish,
                    "tags": tags,
                    "signature": self._sign(signed),
                    "timestamp": ts,
                },
            )
        ).json()

    def get_or_create_conversation(
        self,
        *,
        target_user_id: Optional[str] = None,
        target_username: Optional[str] = None,
    ) -> dict:
        """
        Get or create a direct conversation between the proxy and a target user.
        Provide exactly one of ``target_user_id`` or ``target_username``.

        Returns ``{"conversation_id": ..., "created": bool}``.
        """
        if (target_user_id is None) == (target_username is None):
            raise ValueError("Provide exactly one of target_user_id or target_username.")

        ts = self._now()
        signed = f"{self.proxy_id}:{ts}"

        return _raise(
            self._s.post(
                self._url("/api/proxy/hmac/get-or-create-conversation"),
                json={
                    "proxy_id": self.proxy_id,
                    "target_user_id": target_user_id,
                    "target_username": target_username,
                    "signature": self._sign(signed),
                    "timestamp": ts,
                },
            )
        ).json()

    def upload_media(
        self,
        file: str | Path | bytes,
        *,
        filename: Optional[str] = None,
        mime_type: Optional[str] = None,
    ) -> dict:
        """
        Upload a media file as the proxy.

        Returns ``{"media_id": ..., "url": ..., "thumbnail_url": ...}``.

        Note: this endpoint is rate-limited per proxy. Exceeding it disables
        the proxy automatically.
        """
        if isinstance(file, (str, Path)):
            path = Path(file)
            filename = filename or path.name
            raw = path.read_bytes()
        else:
            raw = file

        ts = self._now()
        file_hash = _sha256_hex(raw)
        signed = f"{self.proxy_id}:{file_hash}:{ts}"

        return _raise(
            self._s.post(
                self._url("/api/proxy/hmac/upload-media"),
                data={
                    "proxy_id": self.proxy_id,
                    "signature": self._sign(signed),
                    "timestamp": str(ts),
                },
                files={
                    "file": (
                        filename or "upload",
                        raw,
                        mime_type or _guess_mime(filename or ""),
                    )
                },
            )
        ).json()
