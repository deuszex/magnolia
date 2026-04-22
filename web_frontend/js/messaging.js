// Messaging UI — sidebar list, chat modal with attachments, preferences, blacklist
var messaging = (function () {
    var pollTimer = null;
    var currentConversationId = null;
    var currentConversationMeta = null; // { conversation_type, members: [{user_id, role}] }
    var chatModalExpanded = false;
    var pendingChatAttachment = null; // { media_id, url, filename, media_type, file_size }
    var renderedMessageIds = []; // track rendered message IDs to avoid destroying DOM on poll

    // Sidebar 

    async function loadSidebar() {
        try {
            var data = await api.get('/api/conversations?limit=100');
            var convs = data.conversations || [];
            renderSidebarList(convs);
        } catch (e) {
            console.error('Failed to load sidebar:', e);
        }
    }

    function renderSidebarList(convs) {
        // Sort by last_message_at descending
        convs.sort(function (a, b) {
            var ta = a.last_message_at || '';
            var tb = b.last_message_at || '';
            return tb.localeCompare(ta);
        });

        var groups = convs.filter(function (c) { return c.conversation_type === 'group'; });
        var directs = convs.filter(function (c) { return c.conversation_type === 'direct'; });

        var groupsFav = groups.filter(function (c) { return c.is_favourite; });
        var groupsRest = groups.filter(function (c) { return !c.is_favourite; });
        var usersFav = directs.filter(function (c) { return c.is_favourite; });
        var usersRest = directs.filter(function (c) { return !c.is_favourite; });

        renderSublist('msg-groups-fav', groupsFav);
        renderSublist('msg-groups-rest', groupsRest);
        renderSublist('msg-users-fav', usersFav);
        renderSublist('msg-users-rest', usersRest);
    }

    function renderSublist(containerId, convs) {
        var container = document.getElementById(containerId);
        if (!container) return;
        container.innerHTML = '';

        convs.forEach(function (c) {
            container.appendChild(buildConvItem(c));
        });
    }

    function buildConvItem(c) {
        var el = document.createElement('div');
        var isActive = c.conversation_id === currentConversationId;
        el.className = 'msg-conv-item' + (isActive ? ' active' : '');
        el.dataset.convId = c.conversation_id;

        var label = c.display_name || c.name || (c.conversation_type === 'group' ? 'Group' : 'DM');
        var unread = c.unread_count || 0;
        var time = c.last_message_at ? formatTime(c.last_message_at) : '';

        el.innerHTML =
            '<div class="msg-conv-info">' +
            '<div class="msg-conv-name">' + escapeHtml(label) + '</div>' +
            '</div>' +
            '<div class="msg-conv-meta">' +
            '<span class="msg-conv-time">' + time + '</span>' +
            (unread > 0 ? '<span class="msg-unread-badge">' + unread + '</span>' : '') +
            '<button class="msg-fav-star' + (c.is_favourite ? ' active' : '') + '" data-conv-id="' + escapeAttr(c.conversation_id) + '" title="Toggle favourite">&#9733;</button>' +
            '</div>';

        el.onclick = function () { openChat(c.conversation_id, label); };
        el.querySelector('.msg-fav-star').onclick = function (e) {
            e.stopPropagation();
            toggleFavourite(c.conversation_id, !c.is_favourite);
        };
        return el;
    }

    async function toggleFavourite(conversationId, add) {
        try {
            if (add) {
                await api.post('/api/messaging/favourites', { conversation_id: conversationId });
            } else {
                await api.del('/api/messaging/favourites/' + conversationId);
            }
            loadSidebar();
        } catch (e) {
            console.error('Failed to toggle favourite:', e);
        }
    }

    // Chat modal

    async function openChat(conversationId, title) {
        currentConversationId = conversationId;
        currentConversationMeta = null;
        app.state.currentConversationId = conversationId;

        // Close mobile sidebar drawer if open
        if (typeof app.closeSidebars === 'function') app.closeSidebars();

        var modal = document.getElementById('chat-modal');
        modal.style.display = '';

        document.getElementById('chat-modal-title').textContent = title || 'Chat';
        clearChatAttachment();
        switchTab('chat');

        // Fetch conversation details to know type + members for E2E
        try {
            var conv = await api.get('/api/conversations/' + conversationId);
            currentConversationMeta = conv;
            // Show lock icon in title only when it's a DM and E2E key is actually loaded
            var titleEl = document.getElementById('chat-modal-title');
            if (titleEl && conv.conversation_type === 'direct' && typeof e2e !== 'undefined' && e2e.isReady()) {
                titleEl.textContent = '\uD83D\uDD12 ' + (title || 'Chat');
            }
        } catch (_) { /* non-fatal */ }

        loadMessages(conversationId);

        // Mark active item and clear its unread badge
        document.querySelectorAll('.msg-conv-item').forEach(function (el) {
            var active = el.dataset.convId === conversationId;
            el.classList.toggle('active', active);
            if (active) {
                var badge = el.querySelector('.msg-unread-badge');
                if (badge) badge.remove();
            }
        });
        startMessagePolling(conversationId);

        // Notify main.js to check for active call
        if (typeof app.onConversationOpened === 'function') {
            app.onConversationOpened(conversationId);
        }
    }

    function closeChat() {
        document.getElementById('chat-modal').style.display = 'none';
        currentConversationId = null;
        currentConversationMeta = null;
        app.state.currentConversationId = null;
        clearChatAttachment();
        stopMessagePolling();

        // Hide join button when chat closes
        var joinBtn = document.getElementById('btn-chat-join-call');
        if (joinBtn) joinBtn.style.display = 'none';
    }

    function renderConversationInfoPanel(panel) {
        var meta = currentConversationMeta;
        if (!meta) {
            panel.innerHTML = '<p class="chat-info-id">Loading\u2026</p>';
            return;
        }

        var createdAt = meta.created_at ? new Date(meta.created_at).toLocaleString() : '';
        var members = meta.members || [];
        var me = app.state.currentUser && app.state.currentUser.user_id;
        var myRole = (members.find(function (m) { return m.user_id === me; }) || {}).role;
        var isGroupAdmin = meta.conversation_type === 'group' && myRole === 'admin';

        var membersHtml = members.map(function (m) {
            var name = m.display_name || m.username || m.user_id;
            var badge = m.is_proxy ? ' <span class="chat-info-proxy-badge">proxy</span>' : '';
            var sub = m.username && m.display_name ? ' <span class="chat-info-member-sub">@' + escapeHtml(m.username) + '</span>' : '';
            return '<li>' + escapeHtml(name) + badge + sub + '</li>';
        }).join('');

        var addMemberHtml = isGroupAdmin ? (
            '<div class="chat-info-add-member">' +
            '<div class="chat-info-add-member-title">Add member</div>' +
            '<div class="chat-info-add-tabs">' +
            '<button class="chat-info-add-tab active" data-src="user">User</button>' +
            '<button class="chat-info-add-tab" data-src="proxy">Proxy</button>' +
            '</div>' +
            '<div id="chat-add-user-section">' +
            '<input id="chat-add-user-input" class="form-input chat-info-add-input" placeholder="Search by username\u2026" autocomplete="off">' +
            '<ul id="chat-add-user-results" class="chat-info-add-results"></ul>' +
            '</div>' +
            '<div id="chat-add-proxy-section" style="display:none">' +
            '<ul id="chat-add-proxy-list" class="chat-info-add-results">Loading\u2026</ul>' +
            '</div>' +
            '</div>'
        ) : '';

        panel.innerHTML =
            '<div class="chat-info-id">ID: <code>' + escapeHtml(meta.conversation_id) + '</code></div>' +
            (createdAt ? '<div class="chat-info-id">Created: ' + escapeHtml(createdAt) + '</div>' : '') +
            '<ul class="chat-info-members">' + membersHtml + '</ul>' +
            addMemberHtml;

        if (isGroupAdmin) {
            initAddMemberUI(panel, meta);
        }
    }

    function initAddMemberUI(panel, meta) {
        var existingIds = (meta.members || []).map(function (m) { return m.user_id; });
        var convId = meta.conversation_id;

        // Tab switching
        panel.querySelectorAll('.chat-info-add-tab').forEach(function (tab) {
            tab.onclick = function () {
                panel.querySelectorAll('.chat-info-add-tab').forEach(function (t) { t.classList.remove('active'); });
                tab.classList.add('active');
                var src = tab.dataset.src;
                panel.querySelector('#chat-add-user-section').style.display = src === 'user' ? '' : 'none';
                panel.querySelector('#chat-add-proxy-section').style.display = src === 'proxy' ? '' : 'none';
                if (src === 'proxy') loadProxyAddList(panel, convId, existingIds);
            };
        });

        // User search
        var userInput = panel.querySelector('#chat-add-user-input');
        var userResults = panel.querySelector('#chat-add-user-results');
        var searchTimer = null;
        userInput.oninput = function () {
            clearTimeout(searchTimer);
            var q = userInput.value.trim();
            if (!q) { userResults.innerHTML = ''; return; }
            searchTimer = setTimeout(async function () {
                try {
                    var res = await api.get('/api/users?search=' + encodeURIComponent(q) + '&limit=10');
                    var users = (res.users || res).filter(function (u) { return existingIds.indexOf(u.user_id) === -1 && u.user_id !== '__proxy__' && u.user_id !== '__fed__'; });
                    userResults.innerHTML = users.length ? users.map(function (u) {
                        var label = escapeHtml(u.display_name || u.username || u.user_id);
                        var sub = u.username ? ' <span class="chat-info-member-sub">@' + escapeHtml(u.username) + '</span>' : '';
                        return '<li class="chat-info-add-item" data-id="' + escapeAttr(u.user_id) + '">' + label + sub + '</li>';
                    }).join('') : '<li class="chat-info-add-empty">No results</li>';
                    userResults.querySelectorAll('.chat-info-add-item').forEach(function (li) {
                        li.onclick = function () { addMemberToConversation(convId, li.dataset.id, panel); };
                    });
                } catch (e) {
                    userResults.innerHTML = '<li class="chat-info-add-empty">Error: ' + escapeHtml(e.message) + '</li>';
                }
            }, 300);
        };
    }

    async function loadProxyAddList(panel, convId, existingIds) {
        var list = panel.querySelector('#chat-add-proxy-list');
        try {
            var res = await api.get('/api/proxy/list-public');
            var proxies = (res.proxies || res).filter(function (p) { return existingIds.indexOf(p.proxy_id) === -1; });
            list.innerHTML = proxies.length ? proxies.map(function (p) {
                var name = escapeHtml(p.display_name || p.proxy_id);
                return '<li class="chat-info-add-item" data-id="' + escapeAttr(p.proxy_id) + '">' +
                    name + ' <span class="chat-info-proxy-badge">proxy</span></li>';
            }).join('') : '<li class="chat-info-add-empty">No proxies available</li>';
            list.querySelectorAll('.chat-info-add-item').forEach(function (li) {
                li.onclick = function () { addMemberToConversation(convId, li.dataset.id, panel); };
            });
        } catch (e) {
            list.innerHTML = '<li class="chat-info-add-empty">Error: ' + escapeHtml(e.message) + '</li>';
        }
    }

    async function addMemberToConversation(convId, userId, panel) {
        try {
            await api.post('/api/conversations/' + convId + '/members', { user_id: userId });
            // Refresh conversation meta and re-render panel
            var updated = await api.get('/api/conversations/' + convId);
            currentConversationMeta = updated;
            renderConversationInfoPanel(panel);
        } catch (e) {
            var err = panel.querySelector('#chat-add-member-error');
            if (!err) {
                err = document.createElement('div');
                err.id = 'chat-add-member-error';
                err.className = 'error-box';
                err.style.marginTop = '6px';
                panel.appendChild(err);
            }
            err.textContent = 'Failed to add member: ' + e.message;
        }
    }

    function toggleChatSize() {
        var modal = document.getElementById('chat-modal');
        chatModalExpanded = !chatModalExpanded;
        // Clear any user-resized inline dimensions so the CSS class takes over cleanly
        modal.style.width = '';
        modal.style.height = '';
        modal.classList.toggle('expanded', chatModalExpanded);
    }

    function initResizeHandle() {
        var modal = document.getElementById('chat-modal');
        var handle = document.getElementById('chat-modal-resize-handle');
        if (!modal || !handle) return;

        var startX, startY, startW, startH;

        // Base (min) and expanded (max) sizes - must match CSS values
        var MIN_W = 360, MIN_H = 520;

        function getMaxW() {
            // match CSS: min(680px, 100vw - right_offset - 16px)
            // On mobile the modal is full-width and handle is hidden, so just cap at 680
            return Math.min(680, window.innerWidth - 228 - 16);
        }
        function getMaxH() {
            return Math.floor(window.innerHeight * 0.8);
        }

        handle.addEventListener('mousedown', function (e) {
            e.preventDefault();
            // Snapshot starting state
            startX = e.clientX;
            startY = e.clientY;
            startW = modal.offsetWidth;
            startH = modal.offsetHeight;

            // Disable CSS transition during drag for snappy feel
            modal.style.transition = 'none';

            function onMove(e) {
                var dw = startX - e.clientX; // dragging left = wider
                var dh = startY - e.clientY; // dragging up   = taller
                var maxW = getMaxW();
                var maxH = getMaxH();

                var newW = Math.max(MIN_W, Math.min(maxW, startW + dw));
                var newH = Math.max(MIN_H, Math.min(maxH, startH + dh));

                modal.style.width = newW + 'px';
                modal.style.height = newH + 'px';

                // Sync expanded flag so toggleChatSize still works sensibly
                var atMax = newW >= maxW && newH >= maxH;
                chatModalExpanded = atMax;
                modal.classList.toggle('expanded', atMax);
            }

            function onUp() {
                // Re-enable transition
                modal.style.transition = '';
                document.removeEventListener('mousemove', onMove);
                document.removeEventListener('mouseup', onUp);
            }

            document.addEventListener('mousemove', onMove);
            document.addEventListener('mouseup', onUp);
        });
    }

    function switchTab(tabName) {
        document.querySelectorAll('.chat-tab').forEach(function (btn) {
            btn.classList.toggle('active', btn.dataset.tab === tabName);
        });
        document.querySelectorAll('.chat-tab-content').forEach(function (el) {
            el.style.display = 'none';
            el.classList.remove('active');
        });
        var activeContent = document.getElementById('chat-tab-' + tabName);
        if (activeContent) {
            activeContent.style.display = '';
            activeContent.classList.add('active');
        }

        // When opening media tab, load the currently active sub-tab
        if (tabName === 'media' && currentConversationId) {
            var activeSub = document.querySelector('.media-subtab.active');
            var mediaType = activeSub ? activeSub.dataset.media : 'image';
            loadConversationMedia(mediaType);
        }
        if (tabName === 'calls' && currentConversationId) loadConversationCallHistory();
    }

    function switchMediaSubtab(mediaType) {
        document.querySelectorAll('.media-subtab').forEach(function (btn) {
            btn.classList.toggle('active', btn.dataset.media === mediaType);
        });
        document.querySelectorAll('.media-sub-content').forEach(function (el) {
            el.style.display = 'none';
        });
        var target = document.getElementById('media-sub-' + mediaType);
        if (target) target.style.display = '';

        if (currentConversationId) loadConversationMedia(mediaType);
    }

    function initChatModal() {
        document.querySelectorAll('.chat-tab').forEach(function (btn) {
            btn.onclick = function () { switchTab(btn.dataset.tab); };
        });

        // Wire media sub-tabs
        document.querySelectorAll('.media-subtab').forEach(function (btn) {
            btn.onclick = function () { switchMediaSubtab(btn.dataset.media); };
        });

        var resizeBtn = document.getElementById('btn-chat-resize');
        if (resizeBtn) resizeBtn.onclick = toggleChatSize;

        initResizeHandle();

        var closeBtn = document.getElementById('btn-chat-close');
        if (closeBtn) closeBtn.onclick = closeChat;

        // Title button toggles the conversation info panel
        var titleBtn = document.getElementById('chat-modal-title');
        if (titleBtn) {
            titleBtn.onclick = function () {
                var panel = document.getElementById('chat-info-panel');
                if (!panel) return;
                if (panel.style.display === 'none') {
                    renderConversationInfoPanel(panel);
                    panel.style.display = '';
                } else {
                    panel.style.display = 'none';
                }
            };
        }

        var sendBtn = document.getElementById('btn-chat-send');
        if (sendBtn) sendBtn.onclick = function () { sendMessage(); };

        var chatInput = document.getElementById('chat-input');
        if (chatInput) {
            chatInput.onkeydown = function (e) {
                if (e.key === 'Enter' && !e.shiftKey) {
                    e.preventDefault();
                    sendMessage();
                }
            };
        }

        // Attach button
        var attachBtn = document.getElementById('btn-chat-attach');
        var fileInput = document.getElementById('chat-file-input');
        if (attachBtn && fileInput) {
            attachBtn.onclick = function () { fileInput.click(); };
            fileInput.onchange = function () {
                if (fileInput.files.length > 0) {
                    uploadChatAttachment(fileInput.files[0]);
                }
                fileInput.value = '';
            };
        }
    }

    // Chat attachment 

    async function uploadChatAttachment(file) {
        var previewEl = document.getElementById('chat-attachment-preview');
        if (!previewEl) return;

        var mediaType = 'file';
        if (file.type.startsWith('image/')) mediaType = 'image';
        else if (file.type.startsWith('video/')) mediaType = 'video';

        previewEl.style.display = '';
        previewEl.innerHTML = '<span class="file-name">' + escapeHtml(file.name) + '</span><span class="attachment-uploading">Uploading...</span>';

        try {
            var result = await api.upload('/api/media', file, { media_type: mediaType });
            pendingChatAttachment = {
                media_id: result.media_id,
                url: result.url,
                thumbnail_url: result.thumbnail_url,
                filename: file.name,
                media_type: mediaType,
                file_size: file.size
            };

            var html = '';
            if (mediaType === 'image' && result.thumbnail_url) {
                html += '<img src="' + escapeAttr(result.thumbnail_url) + '" alt="">';
            }
            html += '<span class="file-name">' + escapeHtml(file.name) + '</span>';
            html += '<button class="attachment-remove" title="Remove">&times;</button>';
            previewEl.innerHTML = html;

            previewEl.querySelector('.attachment-remove').onclick = function () {
                clearChatAttachment();
            };
        } catch (e) {
            previewEl.innerHTML = '<span style="color:#ef4444">Upload failed</span>';
            setTimeout(function () { clearChatAttachment(); }, 2000);
        }
    }

    function clearChatAttachment() {
        pendingChatAttachment = null;
        var previewEl = document.getElementById('chat-attachment-preview');
        if (previewEl) {
            previewEl.style.display = 'none';
            previewEl.innerHTML = '';
        }
    }

    // Messages 

    async function loadMessages(conversationId) {
        var container = document.getElementById('chat-messages');
        if (!container) return;
        container.innerHTML = '<div class="loading-indicator">Loading...</div>';

        try {
            var data = await api.get('/api/conversations/' + conversationId + '/messages?limit=100');
            renderMessages(data.messages || []);
            container.scrollTop = container.scrollHeight;
        } catch (e) {
            container.innerHTML = '<div class="empty-state">Failed to load messages</div>';
        }
    }

    function createMessageBubble(m) {
        var isOwn = app.state.currentUser && m.sender_id === app.state.currentUser.user_id;
        var bubble = document.createElement('div');
        bubble.className = 'message-bubble ' + (isOwn ? 'own' : 'other');
        bubble.dataset.messageId = m.message_id;

        var html = '';
        if (!isOwn) {
            var senderName = m.remote_sender_qualified_id
                || m.sender_name
                || (m.sender_email && m.sender_email.indexOf('system.internal') === -1 ? m.sender_email : null)
                || (m.sender_id === '__fed__' ? 'Remote user' : m.sender_id.substring(0, 12));
            html += '<div class="message-sender">' + escapeHtml(senderName) + '</div>';
        }

        // Text content (may need async decryption, rendered inline first, then updated)
        if (m.encrypted_content) {
            var displayText = m.encrypted_content;
            if (typeof e2e !== 'undefined' && e2e.isEncrypted(displayText)) {
                displayText = '\u{1F512} [decrypting\u2026]';
            }
            html += '<div class="message-text">' + linkPreview.linkify(displayText) + '</div>';
        }

        // Media attachments
        if (m.attachments && m.attachments.length > 0) {
            m.attachments.forEach(function (att) {
                var mediaUrl = att.url || ('/api/media/' + att.media_id + '/file');
                if (att.media_type === 'image') {
                    html += '<div class="message-media"><img src="' + escapeAttr(mediaUrl) + '" alt=""></div>';
                } else if (att.media_type === 'video') {
                    html += '<div class="message-media"><video src="' + escapeAttr(mediaUrl) + '" controls preload="metadata"></video></div>';
                } else {
                    html += '<a class="message-file-link" href="' + escapeAttr(mediaUrl) + '" download="' + escapeAttr(att.filename || 'file') + '">&#128196; ' + escapeHtml(att.filename || 'File') + (att.file_size ? ' (' + formatSize(att.file_size) + ')' : '') + '</a>';
                }
            });
        }

        var timeHtml = '<div class="message-time">' + formatTime(m.created_at);
        if (isOwn && m.federated_status === 'pending') {
            timeHtml += ' <span class="msg-undelivered" title="Not yet delivered to remote server">\u23F3</span>';
        }
        timeHtml += '</div>';
        html += timeHtml;
        bubble.innerHTML = html;

        // Async-decrypt E2E messages and update the bubble in place
        if (m.encrypted_content && typeof e2e !== 'undefined' && e2e.isEncrypted(m.encrypted_content)) {
            // ECDH shared key is symmetric: encrypt(my_priv, their_pub) == decrypt(their_priv, my_pub).
            // So both sender and receiver must derive the key using the OTHER party's public key.
            // Using m.sender_id works for received messages, but for own sent messages it would
            // try ECDH(my_priv, my_pub), a completely different key, causing decryption failure.
            var meta = currentConversationMeta;
            (async function () {
                var keyPartnerId = m.sender_id; // correct for messages received from others
                if (isOwn && meta && meta.members) {
                    var other = meta.members.find(function (mb) {
                        return mb.user_id !== (app.state.currentUser && app.state.currentUser.user_id);
                    });
                    if (other) keyPartnerId = other.user_id;
                }
                var plain = await e2e.decrypt(m.encrypted_content, keyPartnerId);
                var textEl = bubble.querySelector('.message-text');
                if (textEl) textEl.innerHTML = linkPreview.linkify(plain);
                linkPreview.attachPreview(bubble, plain);
            })();
        } else if (m.encrypted_content) {
            // Attach link preview for text messages
            linkPreview.attachPreview(bubble, m.encrypted_content);
        }

        // Click message images to enlarge
        bubble.querySelectorAll('.message-media img').forEach(function (img) {
            img.onclick = function () {
                if (typeof posts !== 'undefined' && posts.openLightbox) {
                    posts.openLightbox([img.src], 0);
                }
            };
        });

        return bubble;
    }

    function renderMessages(messages) {
        var container = document.getElementById('chat-messages');
        if (!container) return;
        container.innerHTML = '';
        renderedMessageIds = [];

        if (messages.length === 0) {
            container.innerHTML = '<div class="empty-state" style="padding:20px 0;font-size:13px">No messages yet</div>';
            return;
        }

        messages.forEach(function (m) {
            container.appendChild(createMessageBubble(m));
            renderedMessageIds.push(m.message_id);
        });
    }

    // Patch federated_status badges in-place without touching the rest of the DOM.
    function patchFederatedStatus(messages) {
        var container = document.getElementById('chat-messages');
        if (!container) return;
        var myId = app.state.currentUser && app.state.currentUser.user_id;
        messages.forEach(function (m) {
            if (m.sender_id !== myId) return;
            var bubble = container.querySelector('[data-message-id="' + m.message_id + '"]');
            if (!bubble) return;
            var timeEl = bubble.querySelector('.message-time');
            if (!timeEl) return;
            var existing = timeEl.querySelector('.msg-undelivered');
            if (m.federated_status === 'pending') {
                if (!existing) {
                    var span = document.createElement('span');
                    span.className = 'msg-undelivered';
                    span.title = 'Not yet delivered to remote server';
                    span.textContent = '\u23F3';
                    timeEl.appendChild(span);
                }
            } else {
                if (existing) existing.remove();
            }
        });
    }

    // Differential update: skip if identical, append-only for new messages,
    // full re-render only when messages were deleted or reordered.
    function updateMessages(messages) {
        var container = document.getElementById('chat-messages');
        if (!container) return false; // returns whether we scrolled

        var newIds = messages.map(function (m) { return m.message_id; });

        // Identical IDs, only patch federated_status badges, no DOM rebuild.
        if (newIds.length === renderedMessageIds.length &&
            newIds.every(function (id, i) { return id === renderedMessageIds[i]; })) {
            patchFederatedStatus(messages);
            return false;
        }

        // Check if it's an append-only change (all existing messages still present at same positions)
        var isAppendOnly = renderedMessageIds.length > 0 &&
            newIds.length > renderedMessageIds.length &&
            renderedMessageIds.every(function (id, i) { return id === newIds[i]; });

        if (isAppendOnly) {
            // Only append the new messages, preserves existing DOM (videos keep playing)
            var wasAtBottom = container.scrollHeight - container.scrollTop - container.clientHeight < 50;
            for (var i = renderedMessageIds.length; i < messages.length; i++) {
                container.appendChild(createMessageBubble(messages[i]));
            }
            renderedMessageIds = newIds;
            if (wasAtBottom) container.scrollTop = container.scrollHeight;
            return wasAtBottom;
        }

        // Structural change (deletion, reorder), full re-render required
        var wasAtBottom = container.scrollHeight - container.scrollTop - container.clientHeight < 50;
        renderMessages(messages);
        if (wasAtBottom) container.scrollTop = container.scrollHeight;
        return wasAtBottom;
    }

    async function sendMessage() {
        if (!currentConversationId) return;
        var input = document.getElementById('chat-input');
        var text = input.value.trim();

        if (!text && !pendingChatAttachment) return;
        input.value = '';

        var body = {};
        var rawText = text;

        try {
            // Encrypt for direct messages only when E2E key is loaded and ready
            if (text && typeof e2e !== 'undefined' && e2e.isReady() && currentConversationMeta &&
                currentConversationMeta.conversation_type === 'direct') {
                var me = app.state.currentUser && app.state.currentUser.user_id;
                var other = (currentConversationMeta.members || []).find(function (m) { return m.user_id !== me; });
                if (other) {
                    rawText = await e2e.encrypt(text, other.user_id);
                }
            }

            if (rawText) body.encrypted_content = rawText;
            if (pendingChatAttachment) {
                body.media_ids = [pendingChatAttachment.media_id];
                if (!rawText) {
                    body.encrypted_content = '[' + pendingChatAttachment.media_type + ': ' + pendingChatAttachment.filename + ']';
                }
            }

            clearChatAttachment();

            var sent = await api.post('/api/conversations/' + currentConversationId + '/messages', body);
            if (sent && sent.message_id && renderedMessageIds.indexOf(sent.message_id) === -1) {
                var chatContainer = document.getElementById('chat-messages');
                if (chatContainer) {
                    chatContainer.appendChild(createMessageBubble(sent));
                    renderedMessageIds.push(sent.message_id);
                    chatContainer.scrollTop = chatContainer.scrollHeight;
                }
            } else {
                loadMessages(currentConversationId);
            }
        } catch (e) {
            alert('Failed to send: ' + e.message);
            input.value = text; // restore text so user can retry
        }
    }

    function startMessagePolling(conversationId) {
        stopMessagePolling();
        pollTimer = setInterval(async function () {
            if (currentConversationId !== conversationId) { stopMessagePolling(); return; }
            try {
                var data = await api.get('/api/conversations/' + conversationId + '/messages?limit=100');
                updateMessages(data.messages || []);
            } catch (_) { /* ignore */ }
        }, 5000);
    }

    function stopMessagePolling() {
        if (pollTimer) { clearInterval(pollTimer); pollTimer = null; }
    }

    // Conversation media (gallery/files/videos) 

    async function loadConversationMedia(type) {
        var containerId;
        if (type === 'image') containerId = 'chat-gallery';
        else if (type === 'file') containerId = 'chat-files';
        else if (type === 'video') containerId = 'chat-videos';
        else return;

        var container = document.getElementById(containerId);
        if (!container || !currentConversationId) return;
        container.innerHTML = '<div class="loading-indicator">Loading...</div>';

        try {
            var data = await api.get('/api/conversations/' + currentConversationId + '/media?media_type=' + type + '&limit=50');
            container.innerHTML = '';

            if (!data || data.length === 0) {
                container.innerHTML = '<div class="empty-state" style="padding:20px 0;font-size:13px">No ' + type + 's shared</div>';
                return;
            }

            // Collect all image URLs for lightbox navigation
            var allImageUrls = [];
            if (type === 'image') {
                allImageUrls = data.map(function (item) { return item.url; });
            }

            data.forEach(function (item, itemIndex) {
                if (type === 'image') {
                    var img = document.createElement('div');
                    img.className = 'gallery-item';
                    img.innerHTML = '<img src="' + escapeAttr(item.url) + '" alt="' + escapeAttr(item.filename || '') + '">';
                    img.querySelector('img').onclick = function () {
                        if (typeof posts !== 'undefined' && posts.openLightbox) {
                            posts.openLightbox(allImageUrls, itemIndex);
                        }
                    };
                    container.appendChild(img);
                } else if (type === 'video') {
                    var vid = document.createElement('div');
                    vid.className = 'gallery-item video-item';
                    vid.innerHTML = '<video src="' + escapeAttr(item.url) + '" controls preload="metadata"></video>' +
                        '<div class="gallery-label">' + escapeHtml(item.filename || 'Video') + '</div>';
                    container.appendChild(vid);
                } else {
                    var file = document.createElement('a');
                    file.className = 'file-item';
                    file.href = item.url;
                    file.target = '_blank';
                    file.innerHTML = '<span class="file-name">' + escapeHtml(item.filename || 'File') + '</span>' +
                        '<span class="file-size">' + formatSize(item.file_size) + '</span>';
                    container.appendChild(file);
                }
            });
        } catch (e) {
            container.innerHTML = '<div class="empty-state">Failed to load media</div>';
        }
    }

    // New conversation dialog 

    function renderNewConversationDialog() {
        var html =
            '<h3>New Conversation</h3>' +
            '<div class="form-group"><label>Type</label>' +
            '<select id="new-conv-type" class="form-input">' +
            '<option value="direct">Direct Message</option>' +
            '<option value="group">Group Chat</option></select></div>' +
            '<div id="new-conv-name-group" class="form-group" style="display:none">' +
            '<label>Group Name</label>' +
            '<input id="new-conv-name" class="form-input" placeholder="My group"></div>' +
            '<div class="form-group">' +
            '<label>Members</label>' +
            '<div id="new-conv-source-tabs" class="conv-source-tabs">' +
            '<button class="conv-source-tab active" data-source="local">Local</button>' +
            '<button class="conv-source-tab" data-source="remote">Remote</button>' +
            '</div>' +
            '<input id="new-conv-user-search" class="form-input" placeholder="Search users\u2026" autocomplete="off">' +
            '<div id="new-conv-chips" class="conv-chips"></div>' +
            '<div id="new-conv-user-list" class="conv-user-picker"></div>' +
            '</div>' +
            '<div id="new-conv-error" class="error-box" style="display:none"></div>' +
            '<div class="dialog-actions">' +
            '<button class="btn btn-small btn-secondary" id="btn-cancel-new">Cancel</button>' +
            '<button class="btn btn-small btn-primary" id="btn-create-conv">Create</button></div>';

        showDialog(html);

        // selectedUsers: { userId -> displayLabel }
        var selectedUsers = {};
        var allUsers = [];
        var serverMap = {}; // server_connection_id -> display label
        var searchTimer = null;
        var pickerSource = 'local'; // 'local' | 'remote'

        function renderChips() {
            var chipsEl = document.getElementById('new-conv-chips');
            if (!chipsEl) return;
            chipsEl.innerHTML = '';
            Object.keys(selectedUsers).forEach(function (uid) {
                var chip = document.createElement('span');
                chip.className = 'conv-chip';
                chip.innerHTML = escapeHtml(selectedUsers[uid]) +
                    ' <button class="conv-chip-remove" data-uid="' + escapeAttr(uid) + '">&times;</button>';
                chipsEl.appendChild(chip);
            });
            chipsEl.querySelectorAll('.conv-chip-remove').forEach(function (btn) {
                btn.onclick = function (e) {
                    e.stopPropagation();
                    delete selectedUsers[btn.dataset.uid];
                    renderChips();
                    renderPickerList();
                };
            });
        }

        function appendUserItem(listEl, u) {
            var label = u.display_name || u.email || u.user_id;
            var isSelected = !!selectedUsers[u.user_id];
            var item = document.createElement('div');
            item.className = 'conv-picker-item' + (isSelected ? ' selected' : '');
            var avatarHtml = (typeof profile !== 'undefined') ? profile.avatarHtml(u.avatar_url, label, 'sm') : '';
            item.innerHTML =
                avatarHtml +
                '<span class="conv-picker-name">' + escapeHtml(label) + '</span>' +
                (isSelected ? '<span class="conv-picker-check">&#10003;</span>' : '');
            item.onclick = function () {
                var type = document.getElementById('new-conv-type').value;
                if (type === 'direct') { selectedUsers = {}; }
                if (selectedUsers[u.user_id]) {
                    delete selectedUsers[u.user_id];
                } else {
                    selectedUsers[u.user_id] = label;
                }
                renderChips();
                renderPickerList();
            };
            listEl.appendChild(item);
        }

        function renderPickerList() {
            var listEl = document.getElementById('new-conv-user-list');
            if (!listEl) return;
            listEl.innerHTML = '';
            var myId = app.state.currentUser && app.state.currentUser.user_id;
            var visible = allUsers.filter(function (u) { return u.user_id !== myId; });

            if (visible.length === 0) {
                listEl.innerHTML = '<div class="conv-picker-empty">No users found</div>';
                return;
            }

            if (pickerSource === 'remote') {
                // Group by server_connection_id.
                var groups = {};
                var groupOrder = [];
                visible.forEach(function (u) {
                    var sid = u.server_connection_id || '';
                    if (!groups[sid]) { groups[sid] = []; groupOrder.push(sid); }
                    groups[sid].push(u);
                });
                groupOrder.forEach(function (sid) {
                    var serverLabel = serverMap[sid] || sid;
                    var header = document.createElement('div');
                    header.className = 'conv-picker-server-header';
                    header.textContent = serverLabel;
                    listEl.appendChild(header);
                    groups[sid].forEach(function (u) { appendUserItem(listEl, u); });
                });
            } else {
                visible.forEach(function (u) { appendUserItem(listEl, u); });
            }
        }

        function loadPickerUsers(q) {
            if (pickerSource === 'remote') {
                api.get('/api/federation/users?limit=200' + (q ? '&q=' + encodeURIComponent(q) : ''))
                    .then(function (data) {
                        allUsers = (Array.isArray(data) ? data : []).map(function (u) {
                            return {
                                user_id: u.server_connection_id + ':' + u.remote_user_id,
                                server_connection_id: u.server_connection_id,
                                remote_user_id: u.remote_user_id,
                                display_name: u.display_name || u.username || null,
                                email: u.qualified_name,
                                avatar_url: u.avatar_url || null,
                            };
                        });
                        renderPickerList();
                    }).catch(function () { });
            } else {
                api.get('/api/users?limit=80' + (q ? '&q=' + encodeURIComponent(q) : ''))
                    .then(function (data) {
                        allUsers = (data.users || []).filter(function (u) {
                            return u.user_id !== '__proxy__' && u.user_id !== '__fed__';
                        });
                        renderPickerList();
                    }).catch(function () { });
            }
        }

        function switchToSource(src) {
            pickerSource = src;
            document.querySelectorAll('.conv-source-tab').forEach(function (t) {
                t.classList.toggle('active', t.dataset.source === src);
            });
            document.getElementById('new-conv-user-search').value = '';
            allUsers = [];
            loadPickerUsers('');
        }

        // Pre-load server list for grouping labels.
        api.get('/api/federation/servers').then(function (servers) {
            (Array.isArray(servers) ? servers : []).forEach(function (s) {
                var label = s.display_name || s.address
                    .replace(/^https?:\/\//, '')
                    .replace(/\/$/, '');
                serverMap[s.id] = label;
            });
        }).catch(function () { });

        loadPickerUsers('');

        document.getElementById('new-conv-type').onchange = function () {
            document.getElementById('new-conv-name-group').style.display = this.value === 'group' ? '' : 'none';
            selectedUsers = {};
            allUsers = [];
            renderChips();
            switchToSource('local');
        };

        document.getElementById('new-conv-source-tabs').addEventListener('click', function (e) {
            var tab = e.target.closest('.conv-source-tab');
            if (!tab) return;
            switchToSource(tab.dataset.source);
        });

        document.getElementById('new-conv-user-search').oninput = function () {
            var q = this.value.trim();
            clearTimeout(searchTimer);
            searchTimer = setTimeout(function () { loadPickerUsers(q); }, 250);
        };

        document.getElementById('btn-cancel-new').onclick = hideDialog;
        document.getElementById('btn-create-conv').onclick = async function () {
            var type = document.getElementById('new-conv-type').value;
            var name = document.getElementById('new-conv-name').value.trim();
            var errorEl = document.getElementById('new-conv-error');
            var memberIds = Object.keys(selectedUsers);

            if (memberIds.length === 0) {
                errorEl.textContent = 'Select at least one user';
                errorEl.style.display = '';
                return;
            }

            try {
                var body = { conversation_type: type, member_ids: memberIds };
                if (type === 'group' && name) body.name = name;
                var conv = await api.post('/api/conversations', body);
                hideDialog();
                loadSidebar();
                openChat(conv.conversation_id, conv.display_name || conv.name || (type === 'group' ? 'Group' : 'DM'));
            } catch (e) {
                errorEl.textContent = e.message;
                errorEl.style.display = '';
            }
        };
    }

    // Preferences 

    async function renderPreferences(container) {
        container.innerHTML = '<div class="settings-page"><h2>Messaging Preferences</h2><div>Loading...</div></div>';

        try {
            var prefs = await api.get('/api/messaging/preferences');
            var page = container.querySelector('.settings-page');
            page.innerHTML =
                '<h2>Messaging Preferences</h2>' +
                '<div class="settings-row">' +
                '<div><div class="settings-row-label">Accept messages</div>' +
                '<div class="settings-row-desc">Allow other users to send you direct messages</div></div>' +
                '<div class="toggle-switch' + (prefs.accept_messages ? ' active' : '') + '" id="toggle-accept"></div>' +
                '</div>';

            document.getElementById('toggle-accept').onclick = async function () {
                var isActive = this.classList.contains('active');
                try {
                    await api.put('/api/messaging/preferences', { accept_messages: !isActive });
                    this.classList.toggle('active');
                } catch (e) { alert(e.message); }
            };
        } catch (e) {
            container.innerHTML = '<div class="settings-page"><h2>Messaging Preferences</h2><div class="error-box">' + escapeHtml(e.message) + '</div></div>';
        }
    }

    // Blacklist 

    async function renderBlacklist(container) {
        container.innerHTML = '<div class="settings-page"><h2>Blocked Users</h2><div>Loading...</div></div>';

        try {
            var data = await api.get('/api/messaging/blacklist');
            var blocks = data.blocks || [];
            var page = container.querySelector('.settings-page');

            page.innerHTML =
                '<h2>Blocked Users</h2>' +
                '<div class="form-group" style="margin-bottom:24px">' +
                '<label>Block a user</label>' +
                '<div style="display:flex;gap:8px">' +
                '<input id="block-user-id" class="form-input" placeholder="User ID to block">' +
                '<button class="btn btn-small btn-danger" id="btn-block-user">Block</button></div></div>' +
                '<div id="block-error" class="error-box" style="display:none"></div>' +
                '<div id="blocks-list"></div>';

            renderBlocksList(blocks, container);

            document.getElementById('btn-block-user').onclick = async function () {
                var userId = document.getElementById('block-user-id').value.trim();
                var errorEl = document.getElementById('block-error');
                if (!userId) { errorEl.textContent = 'Enter a user ID'; errorEl.style.display = ''; return; }
                try {
                    await api.post('/api/messaging/blacklist', { user_id: userId });
                    document.getElementById('block-user-id').value = '';
                    errorEl.style.display = 'none';
                    renderBlacklist(container);
                } catch (e) {
                    errorEl.textContent = e.message;
                    errorEl.style.display = '';
                }
            };
        } catch (e) {
            container.innerHTML = '<div class="settings-page"><h2>Blocked Users</h2><div class="error-box">' + escapeHtml(e.message) + '</div></div>';
        }
    }

    function renderBlocksList(blocks, parentContainer) {
        var container = document.getElementById('blocks-list');
        if (!container) return;
        if (blocks.length === 0) {
            container.innerHTML = '<div style="color:#9ca3af;padding:16px 0">No blocked users</div>';
            return;
        }
        container.innerHTML = '';
        blocks.forEach(function (b) {
            var row = document.createElement('div');
            row.className = 'blocked-user-item';
            row.innerHTML =
                '<span class="blocked-user-id">' + escapeHtml(b.blocked_user_id) + '</span>' +
                '<button class="btn btn-small btn-secondary" data-unblock="' + escapeAttr(b.blocked_user_id) + '">Unblock</button>';
            container.appendChild(row);
        });
        container.querySelectorAll('[data-unblock]').forEach(function (btn) {
            btn.onclick = async function () {
                try {
                    await api.del('/api/messaging/blacklist/' + btn.dataset.unblock);
                    renderBlacklist(parentContainer);
                } catch (e) { alert(e.message); }
            };
        });
    }

    // Dialog helpers 

    function showDialog(html) {
        document.getElementById('dialog').innerHTML = html;
        document.getElementById('dialog-overlay').style.display = '';
    }

    function hideDialog() {
        document.getElementById('dialog-overlay').style.display = 'none';
        document.getElementById('dialog').innerHTML = '';
    }

    // Call History (per-conversation tab) 

    async function loadConversationCallHistory() {
        var container = document.getElementById('chat-call-history');
        if (!container || !currentConversationId) return;
        container.innerHTML = '<div class="loading-indicator">Loading...</div>';

        try {
            var data = await api.get('/api/conversations/' + currentConversationId + '/calls?limit=50');
            var calls = data.calls || [];
            container.innerHTML = '';

            if (calls.length === 0) {
                container.innerHTML = '<div class="empty-state" style="padding:20px 0;font-size:13px">No calls yet</div>';
                return;
            }

            calls.forEach(function (c) { container.appendChild(renderCallHistoryItem(c)); });
        } catch (e) {
            container.innerHTML = '<div class="empty-state">Failed to load call history</div>';
        }
    }

    // Call History (global page in feed area) 

    async function renderCallHistory(feedAreaContainer) {
        feedAreaContainer.innerHTML =
            '<div class="settings-page">' +
            '<h2>Call History</h2>' +
            '<div id="global-call-history"><div class="loading-indicator">Loading...</div></div>' +
            '</div>';

        try {
            var data = await api.get('/api/calls/history?limit=50');
            var calls = data.calls || [];
            var container = document.getElementById('global-call-history');
            container.innerHTML = '';

            if (calls.length === 0) {
                container.innerHTML = '<div style="color:#9ca3af;padding:16px 0">No call history</div>';
                return;
            }

            calls.forEach(function (c) { container.appendChild(renderCallHistoryItem(c)); });
        } catch (e) {
            var el = document.getElementById('global-call-history');
            if (el) el.innerHTML = '<div class="error-box">' + escapeHtml(e.message) + '</div>';
        }
    }

    function renderCallHistoryItem(call) {
        var el = document.createElement('div');
        el.className = 'call-history-item';

        var icon = call.call_type === 'video' ? '&#127909;' : '&#128222;';
        var statusLabel = call.status || 'ended';
        if (statusLabel === 'ended') statusLabel = 'Ended';
        else if (statusLabel === 'missed') statusLabel = 'Missed';
        else if (statusLabel === 'rejected') statusLabel = 'Rejected';
        else if (statusLabel === 'active') statusLabel = 'Active';
        else if (statusLabel === 'ringing') statusLabel = 'Ringing';
        else statusLabel = statusLabel.charAt(0).toUpperCase() + statusLabel.slice(1);

        var statusClass = 'call-status-' + (call.status || 'ended');

        var durationStr = '';
        if (call.duration_seconds) {
            durationStr = formatDuration(call.duration_seconds);
        }

        var nameLabel = call.conversation_name || call.initiator_name || call.initiated_by || '';
        var timeStr = call.created_at ? formatTime(call.created_at) : '';

        el.innerHTML =
            '<span class="call-history-icon">' + icon + '</span>' +
            '<div class="call-history-info">' +
            '<span class="call-history-name">' + escapeHtml(nameLabel) + '</span>' +
            '<span class="call-history-meta">' +
            '<span class="' + statusClass + '">' + statusLabel + '</span>' +
            (durationStr ? ' &middot; ' + durationStr : '') +
            (timeStr ? ' &middot; ' + timeStr : '') +
            '</span>' +
            '</div>';

        return el;
    }

    function formatDuration(seconds) {
        if (!seconds || seconds < 0) return '';
        var m = Math.floor(seconds / 60);
        var s = seconds % 60;
        if (m > 0) return m + 'm ' + s + 's';
        return s + 's';
    }

    // Federation Settings 

    async function renderFederationSettings(container) {
        container.innerHTML =
            '<div class="settings-page">' +
            '<h2>Federation Settings</h2>' +
            '<div id="fed-settings-body"><div class="settings-loading">Loading\u2026</div></div>' +
            '</div>';

        var page = container.querySelector('.settings-page');

        try {
            var results = await Promise.all([
                api.get('/api/users/federation-settings'),
                api.get('/api/federation/servers'),
                api.get('/api/users/federation-rules'),
            ]);
            var data = results[0];
            var servers = results[1].servers || [];
            var rules = results[2].rules || [];

            var body = document.getElementById('fed-settings-body');

            var modeOptions = [
                { value: 'off', label: 'Off \u2014 not visible to federated servers' },
                { value: 'blacklist', label: 'All servers (except blocked)' },
                { value: 'whitelist', label: 'Only servers I specify' }
            ];

            function modeSelect(id, value) {
                var sel = document.createElement('select');
                sel.id = id;
                sel.className = 'form-input';
                modeOptions.forEach(function (o) {
                    var opt = document.createElement('option');
                    opt.value = o.value;
                    opt.textContent = o.label;
                    if (o.value === value) opt.selected = true;
                    sel.appendChild(opt);
                });
                return sel;
            }

            var profileSel = modeSelect('fed-profile-mode', data.sharing_mode);
            var postSel = modeSelect('fed-post-mode', data.post_sharing_mode);

            body.innerHTML =
                '<div class="settings-row">' +
                '<div><div class="settings-row-label">Profile visibility</div>' +
                '<div class="settings-row-desc">Who can discover your profile via federated search</div></div>' +
                '</div>' +
                '<div class="form-group" id="fed-profile-wrap" style="margin-bottom:8px"></div>' +
                '<div id="fed-profile-whitelist" style="margin-bottom:4px"></div>' +
                '<div id="fed-profile-blacklist" style="margin-bottom:20px"></div>' +
                '<div class="settings-row">' +
                '<div><div class="settings-row-label">Post sharing</div>' +
                '<div class="settings-row-desc">Whether your posts are shared with federated servers</div></div>' +
                '</div>' +
                '<div class="form-group" id="fed-post-wrap" style="margin-bottom:8px"></div>' +
                '<div id="fed-post-whitelist" style="margin-bottom:4px"></div>' +
                '<div id="fed-post-blacklist" style="margin-bottom:24px"></div>' +
                '<button class="btn btn-primary" id="fed-save-btn">Save</button>' +
                '<div id="fed-save-msg" style="margin-top:8px;font-size:13px"></div>' +
                '<hr style="margin:28px 0">' +
                '<h3>Blocked Servers</h3>' +
                '<p style="font-size:13px;color:var(--t-secondary);margin-bottom:12px">Block an entire server, their users will not appear in search and their content will not be shared with you.</p>' +
                '<div id="fed-server-bans-section"><div class="settings-loading">Loading\u2026</div></div>' +
                '<hr style="margin:28px 0">' +
                '<h3>Federated Users</h3>' +
                '<p style="font-size:13px;color:var(--t-secondary);margin-bottom:12px">Users shared by connected servers. Search by name or <code>username@server</code>.</p>' +
                '<div class="form-group" style="margin-bottom:12px">' +
                '<input id="fed-user-search" class="form-input" placeholder="Search federated users\u2026" autocomplete="off">' +
                '</div>' +
                '<div id="fed-user-results" style="min-height:40px"></div>' +
                '<hr style="margin:28px 0">' +
                '<h3>Blocked Remote Users</h3>' +
                '<div id="fed-bans-section"><div class="settings-loading">Loading\u2026</div></div>';

            document.getElementById('fed-profile-wrap').appendChild(profileSel);
            document.getElementById('fed-post-wrap').appendChild(postSel);

            // Build server label: prefer display_name, fall back to address
            function serverLabel(s) {
                return s.display_name || s.address;
            }

            // Render a whitelist server-toggle panel beneath a mode select.
            // ruleType: 'sharing' or 'post_sharing'
            function renderWhitelistPanel(panelId, ruleType) {
                var panel = document.getElementById(panelId);
                if (!panel) return;
                var sel = ruleType === 'sharing' ? profileSel : postSel;
                if (sel.value !== 'whitelist') { panel.innerHTML = ''; return; }

                if (!servers.length) {
                    panel.innerHTML = '<div style="font-size:13px;color:var(--t-muted);margin-bottom:4px">No active federated servers connected.</div>';
                    return;
                }

                var allowed = new Set(
                    rules.filter(function (r) { return r.rule_type === ruleType && r.effect === 'allow'; })
                        .map(function (r) { return r.server_connection_id; })
                );

                var html = '<div style="font-size:12px;color:var(--t-muted);margin-bottom:8px">Toggle which servers are allowed:</div>' +
                    '<div class="fed-whitelist-grid">';
                servers.forEach(function (s) {
                    var checked = allowed.has(s.id) ? 'checked' : '';
                    html +=
                        '<label class="fed-server-toggle">' +
                        '<input type="checkbox" class="fed-server-cb" ' +
                        'data-server="' + escapeAttr(s.id) + '" ' +
                        'data-rule-type="' + escapeAttr(ruleType) + '" ' + checked + '>' +
                        '<span class="fed-server-name">' + escapeHtml(serverLabel(s)) + '</span>' +
                        '</label>';
                });
                html += '</div>';
                panel.innerHTML = html;

                panel.querySelectorAll('.fed-server-cb').forEach(function (cb) {
                    cb.onchange = async function () {
                        var sid = cb.dataset.server;
                        var rt = cb.dataset.ruleType;
                        try {
                            if (cb.checked) {
                                await api.post('/api/users/federation-rules', {
                                    server_connection_id: sid,
                                    rule_type: rt,
                                    effect: 'allow'
                                });
                                // Sync local rules array
                                rules = rules.filter(function (r) {
                                    return !(r.server_connection_id === sid && r.rule_type === rt);
                                });
                                rules.push({ server_connection_id: sid, rule_type: rt, effect: 'allow' });
                            } else {
                                await api.del('/api/users/federation-rules/' +
                                    encodeURIComponent(sid) + '/' + encodeURIComponent(rt));
                                rules = rules.filter(function (r) {
                                    return !(r.server_connection_id === sid && r.rule_type === rt);
                                });
                            }
                        } catch (e) {
                            alert('Error: ' + e.message);
                            cb.checked = !cb.checked; // revert
                        }
                    };
                });
            }

            // Render a blacklist server-toggle panel beneath a mode select.
            // ruleType: 'sharing' or 'post_sharing'
            function renderBlacklistPanel(panelId, ruleType) {
                var panel = document.getElementById(panelId);
                if (!panel) return;
                var sel = ruleType === 'sharing' ? profileSel : postSel;
                if (sel.value !== 'blacklist') { panel.innerHTML = ''; return; }

                if (!servers.length) {
                    panel.innerHTML = '<div style="font-size:13px;color:var(--t-muted);margin-bottom:4px">No active federated servers connected.</div>';
                    return;
                }

                var blocked = new Set(
                    rules.filter(function (r) { return r.rule_type === ruleType && r.effect === 'deny'; })
                        .map(function (r) { return r.server_connection_id; })
                );

                var html = '<div style="font-size:12px;color:var(--t-muted);margin-bottom:8px">Check servers to block:</div>' +
                    '<div class="fed-whitelist-grid">';
                servers.forEach(function (s) {
                    var checked = blocked.has(s.id) ? 'checked' : '';
                    html +=
                        '<label class="fed-server-toggle">' +
                        '<input type="checkbox" class="fed-server-deny-cb" ' +
                        'data-server="' + escapeAttr(s.id) + '" ' +
                        'data-rule-type="' + escapeAttr(ruleType) + '" ' + checked + '>' +
                        '<span class="fed-server-name">' + escapeHtml(serverLabel(s)) + '</span>' +
                        '</label>';
                });
                html += '</div>';
                panel.innerHTML = html;

                panel.querySelectorAll('.fed-server-deny-cb').forEach(function (cb) {
                    cb.onchange = async function () {
                        var sid = cb.dataset.server;
                        var rt = cb.dataset.ruleType;
                        try {
                            if (cb.checked) {
                                await api.post('/api/users/federation-rules', {
                                    server_connection_id: sid,
                                    rule_type: rt,
                                    effect: 'deny'
                                });
                                rules = rules.filter(function (r) {
                                    return !(r.server_connection_id === sid && r.rule_type === rt);
                                });
                                rules.push({ server_connection_id: sid, rule_type: rt, effect: 'deny' });
                            } else {
                                await api.del('/api/users/federation-rules/' +
                                    encodeURIComponent(sid) + '/' + encodeURIComponent(rt));
                                rules = rules.filter(function (r) {
                                    return !(r.server_connection_id === sid && r.rule_type === rt);
                                });
                            }
                            loadServerBans(servers, rules);
                        } catch (e) {
                            alert('Error: ' + e.message);
                            cb.checked = !cb.checked;
                        }
                    };
                });
            }

            renderWhitelistPanel('fed-profile-whitelist', 'sharing');
            renderWhitelistPanel('fed-post-whitelist', 'post_sharing');
            renderBlacklistPanel('fed-profile-blacklist', 'sharing');
            renderBlacklistPanel('fed-post-blacklist', 'post_sharing');

            profileSel.onchange = function () {
                renderWhitelistPanel('fed-profile-whitelist', 'sharing');
                renderBlacklistPanel('fed-profile-blacklist', 'sharing');
            };
            postSel.onchange = function () {
                renderWhitelistPanel('fed-post-whitelist', 'post_sharing');
                renderBlacklistPanel('fed-post-blacklist', 'post_sharing');
            };

            document.getElementById('fed-save-btn').onclick = async function () {
                var msg = document.getElementById('fed-save-msg');
                try {
                    await api.put('/api/users/federation-settings', {
                        sharing_mode: profileSel.value,
                        post_sharing_mode: postSel.value
                    });
                    // Re-render rule panels in case mode changed
                    renderWhitelistPanel('fed-profile-whitelist', 'sharing');
                    renderWhitelistPanel('fed-post-whitelist', 'post_sharing');
                    renderBlacklistPanel('fed-profile-blacklist', 'sharing');
                    renderBlacklistPanel('fed-post-blacklist', 'post_sharing');
                    msg.style.color = '#4ade80';
                    msg.textContent = 'Saved.';
                    setTimeout(function () { msg.textContent = ''; }, 3000);
                } catch (e) {
                    msg.style.color = '#f87171';
                    msg.textContent = 'Error: ' + escapeHtml(e.message);
                }
            };

            loadServerBans(servers, rules);
            loadFedBans(servers);
            initFedSearch(servers);
        } catch (e) {
            page.innerHTML =
                '<h2>Federation Settings</h2>' +
                '<div class="error-box">' + escapeHtml(e.message) + '</div>';
        }

        function serverLabel(s) {
            return s.display_name || s.address;
        }

        // Federated user search 

        function initFedSearch(servers) {
            var input = document.getElementById('fed-user-search');
            var results = document.getElementById('fed-user-results');
            if (!input || !results) return;

            var timer = null;

            // Build server filter dropdown above results if multiple servers exist
            var serverFilterId = null;
            if (servers && servers.length > 1) {
                var wrap = document.createElement('div');
                wrap.style.cssText = 'display:flex;align-items:center;gap:8px;margin-bottom:12px';
                var lbl = document.createElement('label');
                lbl.style.cssText = 'font-size:12px;color:var(--t-muted);white-space:nowrap';
                lbl.textContent = 'Server:';
                var sel = document.createElement('select');
                sel.className = 'form-input';
                sel.style.cssText = 'flex:1;max-width:260px';
                serverFilterId = 'fed-server-filter';
                sel.id = serverFilterId;
                var allOpt = document.createElement('option');
                allOpt.value = '';
                allOpt.textContent = 'All servers';
                sel.appendChild(allOpt);
                servers.forEach(function (s) {
                    var o = document.createElement('option');
                    o.value = s.id;
                    o.textContent = serverLabel(s);
                    sel.appendChild(o);
                });
                wrap.appendChild(lbl);
                wrap.appendChild(sel);
                results.parentNode.insertBefore(wrap, results);
                sel.onchange = function () {
                    clearTimeout(timer);
                    doSearch(input.value.trim(), sel.value);
                };
            }

            function renderFedUsers(users) {
                results.innerHTML = '';
                if (!users.length) {
                    results.innerHTML = '<div style="color:var(--t-muted);font-size:13px;padding:8px 0">No federated users found.</div>';
                    return;
                }
                users.forEach(function (u) {
                    var row = document.createElement('div');
                    row.className = 'fed-user-row';
                    row.style.cssText = 'display:flex;align-items:center;gap:10px;padding:8px 0;border-bottom:1px solid var(--glass-border)';

                    var avatarHtml = (typeof profile !== 'undefined')
                        ? profile.avatarHtml(u.avatar_url || null, u.display_name || u.username, 'sm')
                        : '';

                    var info = document.createElement('div');
                    info.style.flex = '1';
                    info.innerHTML =
                        '<div style="font-size:13px;color:var(--t-primary);font-weight:500">' + escapeHtml(u.display_name || u.username) + '</div>' +
                        '<div style="font-size:11px;color:var(--t-muted)">' + escapeHtml(u.qualified_name) + '</div>';

                    var blockBtn = document.createElement('button');
                    blockBtn.className = 'btn btn-small btn-danger';
                    blockBtn.textContent = 'Block';
                    blockBtn.onclick = async function () {
                        try {
                            await api.post('/api/users/federation-bans', {
                                server_connection_id: u.server_connection_id,
                                remote_user_id: u.remote_user_id
                            });
                            blockBtn.textContent = 'Blocked';
                            blockBtn.disabled = true;
                            loadFedBans(servers);
                        } catch (e) { alert(e.message); }
                    };

                    var avatarWrap = document.createElement('div');
                    avatarWrap.innerHTML = avatarHtml;

                    row.appendChild(avatarWrap);
                    row.appendChild(info);
                    row.appendChild(blockBtn);
                    results.appendChild(row);
                });
            }

            function doSearch(q, sid) {
                var url = '/api/federation/users?limit=40';
                if (q) url += '&q=' + encodeURIComponent(q);
                if (sid) url += '&server_id=' + encodeURIComponent(sid);
                api.get(url).then(function (data) {
                    renderFedUsers(Array.isArray(data) ? data : (data.users || []));
                }).catch(function (e) {
                    results.innerHTML = '<div class="error-box">' + escapeHtml(e.message) + '</div>';
                });
            }

            doSearch('', '');

            input.oninput = function () {
                clearTimeout(timer);
                var q = input.value.trim();
                var sid = serverFilterId ? (document.getElementById(serverFilterId) || {}).value || '' : '';
                timer = setTimeout(function () { doSearch(q, sid); }, 250);
            };
        }

        // Blocked servers 

        function loadServerBans(servers, currentRules) {
            var sec = document.getElementById('fed-server-bans-section');
            if (!sec) return;
            sec.innerHTML = '';

            if (!servers.length) {
                sec.innerHTML = '<div style="color:var(--t-muted);font-size:13px;padding:8px 0">No active federated servers connected.</div>';
                return;
            }

            // A server is "blocked" if it has deny rules for BOTH sharing and post_sharing.
            // Partially blocked (only one rule type) is shown as partial.
            var denySharing = new Set(currentRules.filter(function (r) { return r.rule_type === 'sharing' && r.effect === 'deny'; }).map(function (r) { return r.server_connection_id; }));
            var denyPostSharing = new Set(currentRules.filter(function (r) { return r.rule_type === 'post_sharing' && r.effect === 'deny'; }).map(function (r) { return r.server_connection_id; }));

            var listDiv = document.createElement('div');
            listDiv.style.marginBottom = '12px';

            servers.forEach(function (s) {
                var blockedProfile = denySharing.has(s.id);
                var blockedPosts = denyPostSharing.has(s.id);
                var fullyBlocked = blockedProfile && blockedPosts;

                var row = document.createElement('div');
                row.style.cssText = 'display:flex;align-items:center;gap:10px;padding:10px 0;border-bottom:1px solid var(--glass-border)';

                var nameDiv = document.createElement('div');
                nameDiv.style.flex = '1';
                var statusText = fullyBlocked ? 'Blocked'
                    : (blockedProfile || blockedPosts)
                        ? 'Partially blocked (' + (blockedProfile ? 'profile' : '') + (blockedProfile && blockedPosts ? ', ' : '') + (blockedPosts ? 'posts' : '') + ')'
                        : '';
                nameDiv.innerHTML =
                    '<div style="font-size:13px;color:var(--t-primary);font-weight:500">' + escapeHtml(serverLabel(s)) + '</div>' +
                    (statusText ? '<div style="font-size:11px;color:var(--t-muted);margin-top:2px">' + escapeHtml(statusText) + '</div>' : '');

                var blockBtn = document.createElement('button');
                if (fullyBlocked) {
                    blockBtn.className = 'btn btn-small btn-secondary';
                    blockBtn.textContent = 'Unblock';
                    blockBtn.onclick = async function () {
                        try {
                            await Promise.all([
                                denySharing.has(s.id) ? api.del('/api/users/federation-rules/' + encodeURIComponent(s.id) + '/sharing') : Promise.resolve(),
                                denyPostSharing.has(s.id) ? api.del('/api/users/federation-rules/' + encodeURIComponent(s.id) + '/post_sharing') : Promise.resolve(),
                            ]);
                            rules = rules.filter(function (r) {
                                return !(r.server_connection_id === s.id && r.effect === 'deny');
                            });
                            renderBlacklistPanel('fed-profile-blacklist', 'sharing');
                            renderBlacklistPanel('fed-post-blacklist', 'post_sharing');
                            loadServerBans(servers, rules);
                        } catch (e) { alert(e.message); }
                    };
                } else {
                    blockBtn.className = 'btn btn-small btn-danger';
                    blockBtn.textContent = 'Block';
                    blockBtn.onclick = async function () {
                        try {
                            var toAdd = [];
                            if (!blockedProfile) toAdd.push(api.post('/api/users/federation-rules', { server_connection_id: s.id, rule_type: 'sharing', effect: 'deny' }));
                            if (!blockedPosts) toAdd.push(api.post('/api/users/federation-rules', { server_connection_id: s.id, rule_type: 'post_sharing', effect: 'deny' }));
                            await Promise.all(toAdd);
                            rules = rules.filter(function (r) {
                                return !(r.server_connection_id === s.id && r.effect === 'deny');
                            });
                            rules.push({ server_connection_id: s.id, rule_type: 'sharing', effect: 'deny' });
                            rules.push({ server_connection_id: s.id, rule_type: 'post_sharing', effect: 'deny' });
                            renderBlacklistPanel('fed-profile-blacklist', 'sharing');
                            renderBlacklistPanel('fed-post-blacklist', 'post_sharing');
                            loadServerBans(servers, rules);
                        } catch (e) { alert(e.message); }
                    };
                }

                row.appendChild(nameDiv);
                row.appendChild(blockBtn);
                listDiv.appendChild(row);
            });

            sec.appendChild(listDiv);
        }

        // Blocked remote users 

        async function loadFedBans(servers) {
            var sec = document.getElementById('fed-bans-section');
            if (!sec) return;
            try {
                var data = await api.get('/api/users/federation-bans');
                var bans = data.bans || [];

                // Build server name map
                var serverNames = {};
                (servers || []).forEach(function (s) {
                    serverNames[s.id] = s.display_name || s.address;
                });

                sec.innerHTML = '';

                // Existing bans list
                if (!bans.length) {
                    var emptyDiv = document.createElement('div');
                    emptyDiv.style.cssText = 'color:var(--t-muted);font-size:13px;padding:8px 0';
                    emptyDiv.textContent = 'No blocked remote users.';
                    sec.appendChild(emptyDiv);
                    return;
                }

                var listDiv = document.createElement('div');
                listDiv.id = 'fed-bans-list';
                bans.forEach(function (b) {
                    var displayName = b.user_display_name || b.username || b.remote_user_id;
                    var serverName = serverNames[b.server_connection_id]
                        || b.server_display_name
                        || b.server_address
                        || b.server_connection_id;

                    var item = document.createElement('div');
                    item.className = 'blocked-user-item';
                    item.style.cssText = 'display:flex;align-items:center;gap:8px;padding:10px 0;border-bottom:1px solid var(--glass-border)';
                    item.innerHTML =
                        '<div style="flex:1">' +
                        '<div style="font-size:13px;color:var(--t-primary);font-weight:500">' + escapeHtml(displayName) + '</div>' +
                        '<div style="font-size:11px;color:var(--t-muted);margin-top:2px">via ' + escapeHtml(serverName) + '</div>' +
                        '</div>' +
                        '<button class="btn btn-small btn-secondary" ' +
                        'data-server="' + escapeAttr(b.server_connection_id) + '" ' +
                        'data-user="' + escapeAttr(b.remote_user_id) + '">Unblock</button>';

                    item.querySelector('[data-server]').onclick = async function (e) {
                        var btn = e.currentTarget;
                        var sid = btn.dataset.server;
                        var uid = btn.dataset.user;
                        try {
                            await api.del('/api/users/federation-bans/' + encodeURIComponent(sid) + '/' + encodeURIComponent(uid));
                            loadFedBans(servers);
                        } catch (err) { alert(err.message); }
                    };

                    listDiv.appendChild(item);
                });
                sec.appendChild(listDiv);

            } catch (e) {
                if (sec) sec.innerHTML = '<div class="error-box">' + escapeHtml(e.message) + '</div>';
            }
        }
    }

    // Stop all polling 

    function stopPolling() {
        stopMessagePolling();
    }

    function formatTime(isoStr) {
        if (!isoStr) return '';
        try {
            var d = new Date(isoStr);
            var now = new Date();
            if (d.toDateString() === now.toDateString()) {
                return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
            }
            return d.toLocaleDateString([], { month: 'short', day: 'numeric' });
        } catch (_) { return ''; }
    }

    function formatSize(bytes) {
        if (!bytes) return '';
        if (bytes < 1024) return bytes + ' B';
        if (bytes < 1048576) return (bytes / 1024).toFixed(1) + ' KB';
        return (bytes / 1048576).toFixed(1) + ' MB';
    }

    // Users sidebar (left) 

    var userSearchTimer = null;

    async function loadUsers(query) {
        var container = document.getElementById('users-list');
        if (!container) return;

        var url = '/api/users?limit=100';
        if (query) url += '&q=' + encodeURIComponent(query);

        try {
            var data = await api.get(url);
            var users = data.users || [];
            container.innerHTML = '';

            if (users.length === 0) {
                container.innerHTML = '<div class="empty-state" style="padding:12px;font-size:13px">No users found</div>';
                return;
            }

            users.forEach(function (u) {
                // Don't show self or sentinel system accounts
                if (app.state.currentUser && u.user_id === app.state.currentUser.user_id) return;
                if (u.user_id === '__proxy__' || u.user_id === '__fed__') return;

                var el = document.createElement('div');
                el.className = 'user-item';
                el.setAttribute('data-user-id', u.user_id);
                var userLabel = u.display_name || u.email;
                var userAvatarHtml = (typeof profile !== 'undefined') ? profile.avatarHtml(u.avatar_url, userLabel, 'md') : '';
                el.innerHTML =
                    '<div class="user-item-main">' +
                    userAvatarHtml +
                    '<span class="user-item-name">' + escapeHtml(userLabel) + '</span>' +
                    '</div>' +
                    '<a class="user-item-profile-btn" href="#profile/' + escapeAttr(u.user_id) + '" title="View profile">&#128100;</a>';
                el.querySelector('.user-item-main').onclick = function () {
                    startDirectMessage(u.user_id, userLabel);
                };
                el.querySelector('.user-item-profile-btn').onclick = function (e) {
                    e.stopPropagation();
                };
                container.appendChild(el);
            });
        } catch (e) {
            console.error('Failed to load users:', e);
        }
    }

    function initUsersSidebar() {
        var searchInput = document.getElementById('users-search-input');
        if (searchInput) {
            searchInput.oninput = function () {
                clearTimeout(userSearchTimer);
                userSearchTimer = setTimeout(function () {
                    loadUsers(searchInput.value.trim());
                }, 300);
            };
        }
    }

    async function startDirectMessage(userId, userEmail) {
        // Create or open existing DM with this user
        try {
            var conv = await api.post('/api/conversations', {
                conversation_type: 'direct',
                member_ids: [userId]
            });
            loadSidebar();
            openChat(conv.conversation_id, userEmail || conv.name || 'DM');
        } catch (e) {
            // Conversation might already exist, check error
            if (e.message && e.message.toLowerCase().indexOf('already exists') !== -1) {
                // Try to find existing conversation in sidebar
                try {
                    var data = await api.get('/api/conversations?limit=100');
                    var convs = data.conversations || [];
                    var existing = convs.find(function (c) {
                        return c.conversation_type === 'direct' &&
                            (c.display_name === userEmail || c.name === userEmail);
                    });
                    if (existing) {
                        openChat(existing.conversation_id, userEmail || existing.display_name || existing.name || 'DM');
                    } else {
                        alert('Failed to open conversation: ' + e.message);
                    }
                } catch (_) {
                    alert('Failed to open conversation: ' + e.message);
                }
            } else {
                alert('Failed to start conversation: ' + e.message);
            }
        }
    }

    return {
        renderFederationSettings: renderFederationSettings,
        loadSidebar: loadSidebar,
        loadUsers: loadUsers,
        initUsersSidebar: initUsersSidebar,
        openChat: openChat,
        closeChat: closeChat,
        initChatModal: initChatModal,
        renderNewConversationDialog: renderNewConversationDialog,
        renderPreferences: renderPreferences,
        renderBlacklist: renderBlacklist,
        stopPolling: stopPolling,
        hideDialog: hideDialog,
        renderCallHistory: renderCallHistory,
        startDirectMessage: startDirectMessage,
        openConversation: function (id, title) { openChat(id, title || null); },
        selectConversation: function (id) { openChat(id, null); },
        onNewMessageSignal: function (data) {
            // Refresh the open chat if it's the affected conversation.
            if (data.conversation_id && data.conversation_id === currentConversationId) {
                if (data.message && data.message.message_id) {
                    if (renderedMessageIds.indexOf(data.message.message_id) === -1) {
                        var container = document.getElementById('chat-messages');
                        if (container) {
                            var wasAtBottom = container.scrollHeight - container.scrollTop - container.clientHeight < 50;
                            container.appendChild(createMessageBubble(data.message));
                            renderedMessageIds.push(data.message.message_id);
                            if (wasAtBottom) container.scrollTop = container.scrollHeight;
                        }
                    }
                } else {
                    loadMessages(currentConversationId);
                }
            }

            // Update the sidebar item in-place: bump timestamp and unread badge.
            if (data.conversation_id) {
                var convId = data.conversation_id;
                var item = document.querySelector('.msg-conv-item[data-conv-id="' + convId + '"]');
                if (item) {
                    // Bump timestamp
                    var timeEl = item.querySelector('.msg-conv-time');
                    if (timeEl && data.message && data.message.created_at) {
                        timeEl.textContent = formatTime(data.message.created_at);
                    }
                    // Increment unread badge (only if this isn't the open conversation)
                    if (convId !== currentConversationId) {
                        var badge = item.querySelector('.msg-unread-badge');
                        if (badge) {
                            badge.textContent = (parseInt(badge.textContent, 10) || 0) + 1;
                        } else {
                            var meta = item.querySelector('.msg-conv-meta');
                            if (meta) {
                                var newBadge = document.createElement('span');
                                newBadge.className = 'msg-unread-badge';
                                newBadge.textContent = '1';
                                // Insert before the star button
                                var star = meta.querySelector('.msg-fav-star');
                                meta.insertBefore(newBadge, star);
                            }
                        }
                        // Move item to the top of its list
                        if (item.parentNode) {
                            item.parentNode.insertBefore(item, item.parentNode.firstChild);
                        }
                    }
                } else {
                    // Conversation not in sidebar yet (new DM from someone) - reload
                    loadSidebar();
                }
            }
        }
    };
})();
