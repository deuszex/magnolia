// User Directory, local + federated users, grouped by server, lazy-loaded
var directory = (function () {
    var PAGE_SIZE = 24;
    var searchTimer = null;
    var currentQuery = '';
    var activeFilter = 'all'; // 'all' | 'local' | server_connection_id
    var currentUserId = null; // set at render time, used to skip self

    // Sentinel user IDs are internal system identifiers, not real accounts
    function isSentinel(userId) {
        return userId === '__proxy__' || userId === '__fed__';
    }

    // Per-section pagination state: { offset, total, loading }
    var sectionState = {};

    // Entry point 

    async function render(container) {
        // Fetch current user once so we can hide the self-entry from the local list.
        api.get('/api/auth/me').then(function (me) {
            if (me && me.user_id) currentUserId = me.user_id;
        }).catch(function () { });

        container.innerHTML =
            '<div class="dir-page">' +
            '<div class="dir-header">' +
            '<div class="dir-search-wrap">' +
            '<input id="dir-search" class="form-input dir-search-input" ' +
            'placeholder="Search users\u2026" autocomplete="off">' +
            '</div>' +
            '<div id="dir-filter-tabs" class="dir-filter-tabs"></div>' +
            '</div>' +
            '<div id="dir-body" class="dir-body"><div class="settings-loading">Loading\u2026</div></div>' +
            '</div>';

        currentQuery = '';
        activeFilter = 'all';
        sectionState = {};

        var searchInput = document.getElementById('dir-search');
        searchInput.oninput = function () {
            clearTimeout(searchTimer);
            var q = searchInput.value.trim();
            searchTimer = setTimeout(function () {
                currentQuery = q;
                reloadAll();
            }, 300);
        };

        try {
            var serversData = await api.get('/api/federation/servers');
            var servers = serversData.servers || [];
            buildFilterTabs(servers);
            await buildSections(servers);
        } catch (e) {
            document.getElementById('dir-body').innerHTML =
                '<div class="error-box">' + escapeHtml(e.message) + '</div>';
        }
    }

    // Filter tabs 

    function buildFilterTabs(servers) {
        var bar = document.getElementById('dir-filter-tabs');
        if (!bar) return;
        bar.innerHTML = '';

        var tabs = [{ id: 'all', label: 'All' }, { id: 'local', label: 'This Server' }];
        servers.forEach(function (s) {
            tabs.push({ id: s.id, label: s.display_name || s.address });
        });

        tabs.forEach(function (t) {
            var btn = document.createElement('button');
            btn.className = 'dir-filter-btn' + (t.id === activeFilter ? ' active' : '');
            btn.textContent = t.label;
            btn.title = t.label;
            btn.onclick = function () {
                activeFilter = t.id;
                bar.querySelectorAll('.dir-filter-btn').forEach(function (b) {
                    b.classList.toggle('active', b === btn);
                });
                applyFilter();
            };
            bar.appendChild(btn);
        });
    }

    function applyFilter() {
        var body = document.getElementById('dir-body');
        if (!body) return;
        body.querySelectorAll('.dir-section').forEach(function (sec) {
            var sid = sec.dataset.sectionId;
            var visible = activeFilter === 'all' ||
                (activeFilter === 'local' && sid === 'local') ||
                (activeFilter !== 'local' && sid === activeFilter);
            sec.style.display = visible ? '' : 'none';
        });
    }

    // Build sections 

    async function buildSections(servers) {
        var body = document.getElementById('dir-body');
        body.innerHTML = '';

        // Local section
        var localSec = createSection('local', 'This Server');
        body.appendChild(localSec);
        await loadSection('local', null);

        // Federated server sections
        for (var i = 0; i < servers.length; i++) {
            var s = servers[i];
            var label = (s.display_name || s.address);
            var sec = createSection(s.id, label);
            body.appendChild(sec);
            await loadSection(s.id, s.id);
        }
    }

    function createSection(sectionId, title) {
        var sec = document.createElement('div');
        sec.className = 'dir-section';
        sec.dataset.sectionId = sectionId;

        var hdr = document.createElement('div');
        hdr.className = 'dir-section-header';
        hdr.innerHTML =
            '<span class="dir-section-title">' + escapeHtml(title) + '</span>' +
            '<span class="dir-section-count" id="dir-count-' + safeId(sectionId) + '"></span>' +
            '<button class="dir-section-toggle" title="Toggle">' +
            '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="16" height="16">' +
            '<polyline points="6 9 12 15 18 9"/></svg>' +
            '</button>';

        var body = document.createElement('div');
        body.className = 'dir-section-body';
        body.id = 'dir-section-' + sectionId;

        var grid = document.createElement('div');
        grid.className = 'dir-user-grid';
        grid.id = 'dir-grid-' + sectionId;

        var footer = document.createElement('div');
        footer.className = 'dir-section-footer';
        footer.id = 'dir-footer-' + sectionId;

        body.appendChild(grid);
        body.appendChild(footer);

        hdr.querySelector('.dir-section-toggle').onclick = function () {
            var collapsed = body.classList.toggle('dir-section-collapsed');
            hdr.querySelector('.dir-section-toggle').classList.toggle('rotated', collapsed);
        };

        sec.appendChild(hdr);
        sec.appendChild(body);
        return sec;
    }

    // Load / reload 

    async function loadSection(sectionId, serverConnectionId) {
        if (!sectionState[sectionId]) {
            sectionState[sectionId] = { offset: 0, total: 0, loading: false };
        }
        var state = sectionState[sectionId];
        if (state.loading) return;
        state.loading = true;

        var grid = document.getElementById('dir-grid-' + sectionId);
        var footer = document.getElementById('dir-footer-' + sectionId);
        var countEl = document.getElementById('dir-count-' + sectionId);

        if (state.offset === 0 && grid) {
            grid.innerHTML = '<div class="dir-loading">Loading\u2026</div>';
        }

        try {
            var users, total;

            if (sectionId === 'local') {
                var url = '/api/users?limit=' + PAGE_SIZE + '&offset=' + state.offset;
                if (currentQuery) url += '&q=' + encodeURIComponent(currentQuery);
                var data = await api.get(url);
                var raw = data.users || [];
                var selfFiltered = raw.some(function (u) { return u.user_id === currentUserId; });
                users = raw
                    .filter(function (u) { return u.user_id !== currentUserId && !isSentinel(u.user_id); })
                    .map(function (u) { return { _type: 'local', user_id: u.user_id, display_name: u.display_name, handle: u.email, avatar_url: u.avatar_url }; });
                var sentinelCount = raw.filter(function (u) { return isSentinel(u.user_id); }).length;
                total = Math.max(0, (data.total || 0) - (selfFiltered ? 1 : 0) - sentinelCount);
            } else {
                var url = '/api/federation/users?limit=' + PAGE_SIZE + '&offset=' + state.offset +
                    '&server_id=' + encodeURIComponent(serverConnectionId);
                if (currentQuery) url += '&q=' + encodeURIComponent(currentQuery);
                var data = await api.get(url);
                var raw = Array.isArray(data) ? data : (data.users || []);
                users = raw.map(function (u) { return { _type: 'federated', server_connection_id: u.server_connection_id, remote_user_id: u.remote_user_id, display_name: u.display_name || u.username, handle: u.qualified_name, avatar_url: u.avatar_url }; });
                total = raw.length; // federation endpoint doesn't return total, approximate
            }

            state.total = sectionId === 'local' ? total : (state.offset === 0 ? users.length : state.total + users.length);

            if (state.offset === 0 && grid) grid.innerHTML = '';
            if (countEl) countEl.textContent = state.total > 0 ? '(' + state.total + ')' : '';

            if (!users.length && state.offset === 0) {
                if (grid) grid.innerHTML = '<div class="dir-empty">No users found.</div>';
            } else {
                users.forEach(function (u) {
                    if (grid) grid.appendChild(buildUserCard(u));
                });
            }

            // Update footer: "Load more" or nothing
            if (footer) {
                footer.innerHTML = '';
                var canLoadMore = sectionId === 'local'
                    ? (state.offset + PAGE_SIZE < total)
                    : (users.length === PAGE_SIZE);

                if (canLoadMore) {
                    var moreBtn = document.createElement('button');
                    moreBtn.className = 'btn btn-secondary btn-small dir-load-more';
                    moreBtn.textContent = 'Load more';
                    moreBtn.onclick = (function (sid, scid) {
                        return function () {
                            sectionState[sid].offset += PAGE_SIZE;
                            loadSection(sid, scid);
                        };
                    })(sectionId, serverConnectionId);
                    footer.appendChild(moreBtn);
                }
            }

        } catch (e) {
            if (grid) grid.innerHTML = '<div class="error-box">' + escapeHtml(e.message) + '</div>';
        } finally {
            state.loading = false;
        }
    }

    async function reloadAll() {
        var body = document.getElementById('dir-body');
        if (!body) return;
        // Reset all section offsets and reload visible ones
        body.querySelectorAll('.dir-section').forEach(function (sec) {
            var sid = sec.dataset.sectionId;
            if (sectionState[sid]) {
                sectionState[sid].offset = 0;
                sectionState[sid].total = 0;
            }
        });
        // Find all section IDs from DOM
        body.querySelectorAll('.dir-section').forEach(function (sec) {
            var sid = sec.dataset.sectionId;
            var scid = sid === 'local' ? null : sid;
            loadSection(sid, scid);
        });
    }

    // User card 

    function buildUserCard(u) {
        var card = document.createElement('div');
        card.className = 'dir-user-card';

        var avatarHtml = (typeof profile !== 'undefined')
            ? profile.avatarHtml(u.avatar_url || null, u.display_name || u.handle, 'md')
            : '<div class="dir-avatar-placeholder">' + escapeHtml((u.display_name || u.handle || '?').charAt(0).toUpperCase()) + '</div>';

        var serverBadge = u._type === 'federated' && u.handle
            ? '<div class="dir-user-server">' + escapeHtml(u.handle) + '</div>'
            : '';

        card.innerHTML =
            '<div class="dir-user-avatar">' + avatarHtml + '</div>' +
            '<div class="dir-user-info">' +
            '<div class="dir-user-name">' + escapeHtml(u.display_name || u.handle || 'Unknown') + '</div>' +
            serverBadge +
            '</div>' +
            '<div class="dir-user-actions"></div>';

        var actions = card.querySelector('.dir-user-actions');

        if (u._type === 'local') {
            // View Profile
            var profBtn = document.createElement('a');
            profBtn.className = 'btn btn-small btn-secondary dir-action-btn';
            profBtn.href = '#profile/' + u.user_id;
            profBtn.textContent = 'Profile';
            actions.appendChild(profBtn);

            // Message
            var msgBtn = document.createElement('button');
            msgBtn.className = 'btn btn-small btn-primary dir-action-btn';
            msgBtn.textContent = 'Message';
            msgBtn.onclick = function () {
                if (typeof messaging !== 'undefined' && typeof messaging.startDirectMessage === 'function') {
                    messaging.startDirectMessage(u.user_id, u.display_name || u.handle);
                }
            };
            actions.appendChild(msgBtn);

            // Block
            var blockBtn = document.createElement('button');
            blockBtn.className = 'btn btn-small btn-danger dir-action-btn';
            blockBtn.textContent = 'Block';
            blockBtn.onclick = async function () {
                if (!confirm('Block ' + (u.display_name || u.handle) + '?')) return;
                try {
                    await api.post('/api/messaging/blacklist', { user_id: u.user_id });
                    blockBtn.textContent = 'Blocked';
                    blockBtn.disabled = true;
                } catch (e) { alert(e.message); }
            };
            actions.appendChild(blockBtn);
        } else {
            // Federated user
            // Message
            var msgBtn = document.createElement('button');
            msgBtn.className = 'btn btn-small btn-primary dir-action-btn';
            msgBtn.textContent = 'Message';
            msgBtn.onclick = async function () {
                try {
                    var conv = await api.post('/api/federation/dm', {
                        server_connection_id: u.server_connection_id,
                        remote_user_id: u.remote_user_id
                    });
                    if (typeof messaging !== 'undefined' && conv.conversation_id) {
                        messaging.openConversation(conv.conversation_id, u.display_name || u.handle);
                    }
                } catch (e) { alert(e.message); }
            };
            actions.appendChild(msgBtn);

            // Block
            var blockBtn = document.createElement('button');
            blockBtn.className = 'btn btn-small btn-danger dir-action-btn';
            blockBtn.textContent = 'Block';
            blockBtn.onclick = async function () {
                if (!confirm('Block ' + (u.display_name || u.handle) + '?')) return;
                try {
                    await api.post('/api/users/federation-bans', {
                        server_connection_id: u.server_connection_id,
                        remote_user_id: u.remote_user_id
                    });
                    blockBtn.textContent = 'Blocked';
                    blockBtn.disabled = true;
                } catch (e) { alert(e.message); }
            };
            actions.appendChild(blockBtn);
        }

        return card;
    }

    // Sanitise a string for use as an element ID (not for HTML attributes).
    function safeId(str) {
        return (str || '').replace(/[^a-zA-Z0-9_-]/g, '_');
    }

    return {
        render: render,
    };
})();
