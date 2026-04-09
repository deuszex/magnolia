// Events / notifications module — bell badge, events page, toast notifications
var events = (function () {

    var CATEGORIES = ['system', 'federation', 'admin', 'message', 'post', 'call'];
    var PRIORITY_ORDER = { critical: 0, warning: 1, info: 2 };

    // Badge 

    function updateBadge(count) {
        var badge = document.getElementById('events-badge');
        if (!badge) return;
        if (count > 0) {
            badge.textContent = count > 99 ? '99+' : String(count);
            badge.style.display = '';
        } else {
            badge.style.display = 'none';
        }
    }

    function refreshBadge() {
        api.get('/api/events/count').then(function (r) {
            updateBadge(r ? r.count : 0);
        }).catch(function () { });
    }

    // Toast 

    function showToast(event) {
        var container = document.getElementById('toast-container');
        if (!container) return;

        var toast = document.createElement('div');
        toast.className = 'toast toast-' + (event.priority || 'info');

        var priorityIcon = { critical: '🚨', warning: '⚠️', info: 'ℹ️' }[event.priority] || 'ℹ️';

        toast.innerHTML =
            '<div class="toast-header">' +
            '<span class="toast-icon">' + escHtml(priorityIcon) + '</span>' +
            '<span class="toast-title">' + escHtml(event.title) + '</span>' +
            '<button class="toast-close" aria-label="Dismiss">&times;</button>' +
            '</div>' +
            (event.body ? '<div class="toast-body">' + escHtml(event.body) + '</div>' : '') +
            '<div class="toast-actions">' +
            '<a href="#events" class="toast-link">View all</a>' +
            '</div>';

        toast.querySelector('.toast-close').onclick = function () {
            dismissToast(toast);
        };

        container.appendChild(toast);

        // Auto-dismiss after 7s (critical stays until dismissed)
        if (event.priority !== 'critical') {
            setTimeout(function () { dismissToast(toast); }, 7000);
        }

        return toast;
    }

    function dismissToast(toast) {
        toast.classList.add('toast-out');
        setTimeout(function () {
            if (toast.parentNode) toast.parentNode.removeChild(toast);
        }, 300);
    }

    function escHtml(s) {
        return String(s || '')
            .replace(/&/g, '&amp;')
            .replace(/</g, '&lt;')
            .replace(/>/g, '&gt;')
            .replace(/"/g, '&quot;');
    }

    // WS handler — called from messaging.js / main.js 

    function handleWsEvent(event) {
        // event is the inner "event" object from {"type":"event","event":{...}}
        refreshBadge();
        showToast(event);
        // Refresh the events list if the page is currently open
        if (window.location.hash === '#events') {
            loadEvents(true);
        }
    }

    // Events page 

    var currentFilter = { viewed: 'unviewed', category: null };
    var currentPrefs = { disabled_categories: [] };

    function renderPage(container) {
        container.innerHTML =
            '<div class="events-page">' +
            '<div class="events-header">' +
            '<h2 class="events-title">Notifications</h2>' +
            '<div class="events-actions">' +
            '<button id="evt-mark-all" class="btn btn-small">Mark all read</button>' +
            '<button id="evt-prefs-btn" class="btn btn-small">Preferences</button>' +
            '</div>' +
            '</div>' +
            '<div class="events-filter-bar" id="evt-filter-bar">' +
            '<button class="evt-filter-btn active" data-viewed="unviewed">Unread</button>' +
            '<button class="evt-filter-btn" data-viewed="viewed">Read</button>' +
            '<button class="evt-filter-btn" data-viewed="all">All</button>' +
            '<span class="evt-filter-sep">|</span>' +
            '<button class="evt-filter-btn evt-cat-btn active" data-category="">All categories</button>' +
            CATEGORIES.map(function (c) {
                return '<button class="evt-filter-btn evt-cat-btn" data-category="' + c + '">' + capFirst(c) + '</button>';
            }).join('') +
            '</div>' +
            '<div id="evt-prefs-panel" class="evt-prefs-panel" style="display:none"></div>' +
            '<div id="evt-list" class="evt-list"></div>' +
            '<div id="evt-load-more" style="text-align:center;margin:1rem;display:none">' +
            '<button class="btn btn-small" id="evt-more-btn">Load more</button>' +
            '</div>' +
            '</div>';

        // Filter bar — viewed tabs
        container.querySelectorAll('.evt-filter-btn[data-viewed]').forEach(function (btn) {
            btn.onclick = function () {
                container.querySelectorAll('.evt-filter-btn[data-viewed]').forEach(function (b) { b.classList.remove('active'); });
                btn.classList.add('active');
                currentFilter.viewed = btn.dataset.viewed;
                loadEvents(true);
            };
        });

        // Filter bar — category tabs
        container.querySelectorAll('.evt-cat-btn').forEach(function (btn) {
            btn.onclick = function () {
                container.querySelectorAll('.evt-cat-btn').forEach(function (b) { b.classList.remove('active'); });
                btn.classList.add('active');
                currentFilter.category = btn.dataset.category || null;
                loadEvents(true);
            };
        });

        document.getElementById('evt-mark-all').onclick = function () {
            var params = currentFilter.category ? '?category=' + currentFilter.category : '';
            api.put('/api/events/viewed-all' + params, {}).then(function () {
                refreshBadge();
                loadEvents(true);
            }).catch(function () { });
        };

        document.getElementById('evt-prefs-btn').onclick = togglePrefsPanel;

        loadPrefs();
        loadEvents(true);
    }

    var evtOffset = 0;
    var evtTotal = 0;
    var EVT_LIMIT = 20;

    function loadEvents(reset) {
        if (reset) evtOffset = 0;
        var params = '?viewed=' + currentFilter.viewed +
            '&limit=' + EVT_LIMIT + '&offset=' + evtOffset;
        if (currentFilter.category) params += '&category=' + currentFilter.category;

        api.get('/api/events' + params).then(function (r) {
            if (!r) return;
            evtTotal = r.total;
            updateBadge(r.unread);
            var list = document.getElementById('evt-list');
            if (!list) return;
            if (reset) list.innerHTML = '';
            if (r.events.length === 0 && reset) {
                list.innerHTML = '<p class="empty-state">No notifications.</p>';
            } else {
                r.events.forEach(function (ev) {
                    list.appendChild(buildEventCard(ev));
                });
            }
            evtOffset += r.events.length;
            var moreBtn = document.getElementById('evt-load-more');
            if (moreBtn) moreBtn.style.display = evtOffset < evtTotal ? '' : 'none';
        }).catch(function () { });
    }

    function buildEventCard(ev) {
        var card = document.createElement('div');
        card.className = 'evt-card evt-priority-' + (ev.priority || 'info') + (ev.viewed ? ' evt-viewed' : '');
        card.dataset.eventId = ev.id;

        var ts = ev.created_at ? new Date(ev.created_at).toLocaleString() : '';
        card.innerHTML =
            '<div class="evt-card-header">' +
            '<span class="evt-cat-badge evt-cat-' + escHtml(ev.category) + '">' + escHtml(capFirst(ev.category)) + '</span>' +
            '<span class="evt-priority-dot" title="' + escHtml(ev.priority) + '"></span>' +
            '<span class="evt-title">' + escHtml(ev.title) + '</span>' +
            '<span class="evt-ts">' + escHtml(ts) + '</span>' +
            '<div class="evt-card-actions">' +
            (!ev.viewed ? '<button class="btn-icon evt-btn-viewed" title="Mark read">&#10003;</button>' : '') +
            '<button class="btn-icon evt-btn-delete" title="Delete">&#128465;</button>' +
            '</div>' +
            '</div>' +
            (ev.body ? '<div class="evt-body">' + escHtml(ev.body) + '</div>' : '');

        var markBtn = card.querySelector('.evt-btn-viewed');
        if (markBtn) {
            markBtn.onclick = function (e) {
                e.stopPropagation();
                api.put('/api/events/' + ev.id + '/viewed', {}).then(function () {
                    card.classList.add('evt-viewed');
                    if (markBtn.parentNode) markBtn.parentNode.removeChild(markBtn);
                    refreshBadge();
                }).catch(function () { });
            };
        }

        card.querySelector('.evt-btn-delete').onclick = function (e) {
            e.stopPropagation();
            api.del('/api/events/' + ev.id).then(function () {
                if (card.parentNode) card.parentNode.removeChild(card);
                refreshBadge();
            }).catch(function () { });
        };

        return card;
    }

    function loadMoreEvents() {
        loadEvents(false);
    }

    // Preferences panel 

    function togglePrefsPanel() {
        var panel = document.getElementById('evt-prefs-panel');
        if (!panel) return;
        if (panel.style.display === 'none') {
            renderPrefsPanel(panel);
            panel.style.display = '';
        } else {
            panel.style.display = 'none';
        }
    }

    function renderPrefsPanel(panel) {
        panel.innerHTML =
            '<div class="evt-prefs-inner">' +
            '<strong>Disabled categories (no notifications):</strong>' +
            '<div class="evt-prefs-cats">' +
            CATEGORIES.map(function (c) {
                var disabled = currentPrefs.disabled_categories.indexOf(c) !== -1;
                return '<label class="evt-pref-row">' +
                    '<input type="checkbox" data-cat="' + c + '" ' + (disabled ? 'checked' : '') + '> ' +
                    capFirst(c) + '</label>';
            }).join('') +
            '</div>' +
            '<button class="btn btn-primary btn-small" id="evt-save-prefs">Save</button>' +
            '</div>';

        document.getElementById('evt-save-prefs').onclick = savePrefs;
    }

    function loadPrefs() {
        api.get('/api/events/prefs').then(function (r) {
            if (r) currentPrefs = r;
        }).catch(function () { });
    }

    function savePrefs() {
        var disabled = [];
        document.querySelectorAll('.evt-pref-row input[data-cat]:checked').forEach(function (cb) {
            disabled.push(cb.dataset.cat);
        });
        api.put('/api/events/prefs', { disabled_categories: disabled }).then(function () {
            currentPrefs.disabled_categories = disabled;
            var panel = document.getElementById('evt-prefs-panel');
            if (panel) panel.style.display = 'none';
        }).catch(function () { });
    }

    // Helpers 

    function capFirst(s) {
        return s ? s.charAt(0).toUpperCase() + s.slice(1) : s;
    }

    // Public API 

    var badgeTimer = null;

    return {
        init: function () {
            refreshBadge();
            // Poll badge every 30s as fallback for WS push
            if (badgeTimer) clearInterval(badgeTimer);
            badgeTimer = setInterval(refreshBadge, 30000);
            // Load more button
            document.addEventListener('click', function (e) {
                if (e.target && e.target.id === 'evt-more-btn') {
                    loadMoreEvents();
                }
            });
        },
        refreshBadge: refreshBadge,
        updateBadge: updateBadge,
        handleWsEvent: handleWsEvent,
        renderPage: renderPage,
    };
})();
