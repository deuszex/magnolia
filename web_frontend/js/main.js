// App entry point — auth check, routing, global state
var app = (function () {
    var state = {
        currentUser: null,
        currentConversationId: null
    };

    function loadScript(src, onload) {
        var s = document.createElement('script');
        s.src = src;
        s.onload = onload || null;
        s.onerror = function () {
            console.warn('Failed to load script:', src);
            if (onload) onload();
        };
        document.head.appendChild(s);
    }

    function init() {
        // Theme init happens after admin.js is loaded (admin users only).
        // Non-admin and unauthenticated users use the default CSS theme.

        // If this is a call tab (#call/...), load calling.js first then skip normal boot
        if (window.location.hash.indexOf('#call/') === 0) {
            loadScript('/js/calling.js', function () {
                api.isAuthenticated().then(function (user) {
                    if (user) {
                        state.currentUser = user;
                        calling.initCallTab();
                    } else {
                        showAuth();
                    }
                });
            });
            return;
        }

        api.isAuthenticated().then(function (user) {
            if (user) {
                state.currentUser = user;
                onAuthenticated();
            } else {
                showAuth();
            }
        });
    }

    function onAuthenticated() {
        // Reload user info if not yet loaded
        if (!state.currentUser) {
            api.isAuthenticated().then(function (user) {
                if (user) {
                    state.currentUser = user;
                    bootApp();
                }
            });
        } else {
            bootApp();
        }
    }

    function bootApp() {
        document.getElementById('app-auth').style.display = 'none';
        document.getElementById('app-main').style.display = '';

        // All app scripts loaded dynamically after auth — never served to unauthenticated users.
        var appScripts = [
            '/js/theme.js',
            '/js/e2e.js',
            '/js/linkpreview.js',
            '/js/posts.js',
            '/js/search.js',
            '/js/messaging.js',
            '/js/calling.js',
            '/js/global-call.js',
            '/js/profile.js',
            '/js/directory.js',
            '/js/events.js'
        ];

        // Admin module only for admin users — never exposed to regular users.
        if (state.currentUser && state.currentUser.admin) {
            appScripts.push('/js/admin.js');
        }

        var remaining = appScripts.length;
        function onScriptLoaded() {
            remaining--;
            if (remaining === 0) {
                if (typeof theme !== 'undefined') {
                    theme.initTheme();
                }
                finishBoot();
            }
        }

        appScripts.forEach(function (src) {
            loadScript(src, onScriptLoaded);
        });
    }

    function finishBoot() {
        // Initialise E2E key pair and upload public key to server
        if (typeof e2e !== 'undefined') {
            e2e.init().catch(function (err) {
                console.warn('E2E init failed:', err);
            });
        }

        // Set user display name + avatar in header
        if (state.currentUser) {
            profile.updateHeaderUser();
        }

        // Wire user menu
        var menuBtn = document.getElementById('btn-user-menu');
        if (menuBtn) {
            menuBtn.onclick = function (e) {
                e.stopPropagation();
                var menu = document.getElementById('user-menu');
                menu.style.display = menu.style.display === 'none' ? '' : 'none';
            };
        }

        // Close user menu on outside click
        document.addEventListener('click', function () {
            var menu = document.getElementById('user-menu');
            if (menu) menu.style.display = 'none';
        });

        // Show admin menu item for admin users
        var adminPanelBtn = document.getElementById('btn-admin-panel');
        if (adminPanelBtn && state.currentUser && state.currentUser.admin) {
            adminPanelBtn.style.display = '';
        }

        // Wire "My Profile" menu item
        var profileBtn = document.getElementById('btn-my-profile');
        if (profileBtn) {
            profileBtn.onclick = function (e) {
                e.preventDefault();
                if (state.currentUser) {
                    window.location.hash = 'profile/' + state.currentUser.user_id;
                }
            };
        }

        // Wire logout
        var logoutBtn = document.getElementById('btn-logout');
        if (logoutBtn) {
            logoutBtn.onclick = function (e) {
                e.preventDefault();
                doLogout();
            };
        }

        // Wire new conversation button
        var newBtn = document.getElementById('btn-new-conversation');
        if (newBtn) {
            newBtn.onclick = function () {
                messaging.renderNewConversationDialog();
            };
        }

        // Init sub-modules
        search.init();
        posts.initCreatePost();
        posts.initInfiniteScroll();
        messaging.initChatModal();
        if (typeof events !== 'undefined') events.init();

        // Wire mobile sidebar drawers
        var sidebarUsersBtn = document.getElementById('btn-sidebar-users');
        var sidebarMsgsBtn = document.getElementById('btn-sidebar-msgs');
        var sidebarBackdrop = document.getElementById('sidebar-backdrop');

        function closeSidebars() {
            document.getElementById('users-sidebar').classList.remove('sidebar-open');
            document.getElementById('msg-sidebar').classList.remove('sidebar-open');
            if (sidebarBackdrop) sidebarBackdrop.style.display = 'none';
        }
        function openSidebar(id) {
            closeSidebars();
            document.getElementById(id).classList.add('sidebar-open');
            if (sidebarBackdrop) sidebarBackdrop.style.display = 'block';
        }

        if (sidebarUsersBtn) sidebarUsersBtn.onclick = function () { openSidebar('users-sidebar'); };
        if (sidebarMsgsBtn) sidebarMsgsBtn.onclick = function () { openSidebar('msg-sidebar'); };
        if (sidebarBackdrop) sidebarBackdrop.onclick = closeSidebars;

        // Expose for messaging.js to close drawers on chat open
        app.closeSidebars = closeSidebars;

        // Connect WebSocket for calling/signaling
        if (typeof calling !== 'undefined') {
            calling.connectWs();
        }

        // Initialise global always-on voice call
        if (typeof globalCall !== 'undefined') {
            globalCall.init();
        }

        // Wire call buttons in chat modal header
        var voiceCallBtn = document.getElementById('btn-chat-voice-call');
        if (voiceCallBtn) {
            voiceCallBtn.onclick = function () {
                var convId = state.currentConversationId;
                if (convId && typeof calling !== 'undefined') {
                    calling.startCall(convId, 'voice');
                }
            };
        }
        var videoCallBtn = document.getElementById('btn-chat-video-call');
        if (videoCallBtn) {
            videoCallBtn.onclick = function () {
                var convId = state.currentConversationId;
                if (convId && typeof calling !== 'undefined') {
                    calling.startCall(convId, 'video');
                }
            };
        }

        // Wire ongoing-call-lane join button
        var laneJoinBtn = document.getElementById('btn-ongoing-call-join');
        if (laneJoinBtn) {
            laneJoinBtn.onclick = function () {
                var lane = document.getElementById('ongoing-call-lane');
                var callId = lane && lane.dataset.callId;
                var callType = lane && lane.dataset.callType;
                var convId = state.currentConversationId;
                if (callId && convId && typeof calling !== 'undefined') {
                    calling.joinCall(callId, callType || 'voice', convId);
                }
            };
        }

        // Callback: check for active call when a conversation opens
        app.onConversationOpened = function (conversationId) {
            checkActiveCall(conversationId);
        };

        // React to real-time call-state changes dispatched by calling.js
        window.addEventListener('magnolia:check-active-call', function () {
            if (state.currentConversationId) {
                checkActiveCall(state.currentConversationId);
            }
        });

        // Listen for hash changes
        window.addEventListener('hashchange', handleRoute);

        // Initial route
        handleRoute();

        // Load sidebars
        messaging.loadSidebar();
        messaging.startSidebarPolling();
        messaging.initUsersSidebar();
        messaging.loadUsers();
    }

    function showAuth() {
        document.getElementById('app-auth').style.display = 'flex';
        document.getElementById('app-main').style.display = 'none';
        // Handle special auth hashes before defaulting to login
        if (auth.checkResetHash() || auth.checkRegisterHash()) {
            return;
        }
        // Check if initial server setup is needed
        api.get('/api/setup/status').then(function (res) {
            if (res && res.setup_required) {
                auth.renderSetup();
            } else {
                auth.renderLogin();
            }
        }).catch(function () {
            auth.renderLogin();
        });
    }

    function checkActiveCall(conversationId) {
        var lane = document.getElementById('ongoing-call-lane');
        if (!lane) return;

        lane.style.display = 'none';

        api.get('/api/conversations/' + conversationId + '/active-call').then(function (data) {
            if (state.currentConversationId !== conversationId) return;

            if (!data || !data.call_id) return;

            // Don't show the lane if the current user is already in the call
            var alreadyJoined = false;
            var joined = [];
            if (data.participants && state.currentUser) {
                data.participants.forEach(function (p) {
                    if (p.status === 'joined') {
                        if (p.user_id === state.currentUser.user_id) {
                            alreadyJoined = true;
                        } else {
                            joined.push(p.display_name || p.user_id.substring(0, 8));
                        }
                    }
                });
            }
            if (alreadyJoined) return;

            // Build participant label: "A, B" / "A and 2 others" / "1 participant"
            var label = '';
            if (joined.length === 0) {
                label = 'No one yet';
            } else if (joined.length <= 2) {
                label = joined.join(', ');
            } else {
                label = joined[0] + ' and ' + (joined.length - 1) + ' others';
            }

            var participantsEl = document.getElementById('ongoing-call-participants');
            if (participantsEl) participantsEl.textContent = '· ' + label;

            lane.dataset.callId = data.call_id;
            lane.dataset.callType = data.call_type;
            lane.style.display = '';
        }).catch(function () { /* silently ignore */ });
    }

    function renderAboutPage(container) {
        if (!container) return;
        container.innerHTML =
            '<div class="settings-page about-page">' +
            '<h2>About Magnolia</h2>' +
            '<section class="be-nice-section">' +
            '<p>Users are responsible for their own data, content, and security.</p>' +
            '<p>You are advised to keep in contact with your server host, and discuss rules before publishing content,<br>' +
            'or initiating conversations.</p>' +
            '</section>' +
            '<section class="about-section">' +
            '<h3>License</h3>' +
            '<p>This work is licensed under ' +
            '<a href="https://creativecommons.org/licenses/by-nc-sa/4.0/" target="_blank" rel="noopener">Creative Commons Attribution-NonCommercial-ShareAlike 4.0 International</a>' +
            '<img src="https://mirrors.creativecommons.org/presskit/icons/cc.svg" alt="CC" class="cc-icon">' +
            '<img src="https://mirrors.creativecommons.org/presskit/icons/by.svg" alt="BY" class="cc-icon">' +
            '<img src="https://mirrors.creativecommons.org/presskit/icons/nc.svg" alt="NC" class="cc-icon">' +
            '<img src="https://mirrors.creativecommons.org/presskit/icons/sa.svg" alt="SA" class="cc-icon">' +
            '</p>' +
            '</section>' +

            '<section class="about-section about-warning">' +
            '<h3>Important notice</h3>' +
            '<p>Magnolia is released under a <strong>non-commercial</strong> license (CC BY-NC-SA 4.0). ' +
            'It may never be sold, bundled into a paid product, or offered as a paid service.</p>' +
            '<p><strong>If you paid for this software, you were scammed.</strong> ' +
            'Selling it is a violation of the license and may constitute fraud. ' +
            'Depending on your country, you may be entitled to a refund and could have grounds for legal action against the seller. ' +
            'Consider contacting a consumer protection authority or legal advisor in your jurisdiction.</p>' +
            '</section>' +

            '</div>';
    }

    function handleRoute() {
        var hash = window.location.hash.replace('#', '') || 'feed';
        var feedArea = document.getElementById('feed-area');

        // Call routes are handled by the call tab, not main app
        if (hash.indexOf('call/') === 0) return;

        // Profile route: #profile/{user_id}
        if (hash.indexOf('profile/') === 0) {
            var userId = hash.substring('profile/'.length);
            if (userId) {
                profile.renderProfile(userId);
                return;
            }
        }

        // Invite/register route: #register/TOKEN — already registered users land here
        // when testing an invite link. Show a notice rather than the feed.
        if (hash.indexOf('register/') === 0) {
            if (feedArea) {
                feedArea.innerHTML =
                    '<div style="padding:2rem;text-align:center;color:var(--t-secondary)">' +
                    '<p>You are already logged in.</p>' +
                    '<p style="font-size:0.875rem">This invite link is for new users. ' +
                    'Share it with the person you invited.</p>' +
                    '</div>';
            }
            return;
        }

        // Admin routes: #admin/users, #admin/invites, #admin/site-config, #admin/theme
        if (hash.indexOf('admin/') === 0 || hash === 'admin') {
            if (typeof admin !== 'undefined' && state.currentUser && state.currentUser.admin) {
                var sub = hash === 'admin' ? 'users' : hash.substring('admin/'.length);
                switch (sub) {
                    case 'users': admin.renderUsers(feedArea); break;
                    case 'invites': admin.renderInvites(feedArea); break;
                    case 'applications': admin.renderApplications(feedArea); break;
                    case 'site-config': admin.renderSiteConfig(feedArea); break;
                    case 'email': admin.renderEmailSettings(feedArea); break;
                    case 'theme': admin.renderTheme(feedArea); break;
                    case 'stun': admin.renderStunServers(feedArea); break;
                    case 'federation': admin.renderFederation(feedArea); break;
                    default: admin.renderUsers(feedArea); break;
                }
            }
            return;
        }

        // Events / notifications page: #events
        if (hash === 'events') {
            if (typeof events !== 'undefined') {
                events.renderPage(feedArea);
            }
            return;
        }

        // User federation settings: #federation
        if (hash === 'federation') {
            if (typeof messaging !== 'undefined') {
                messaging.renderFederationSettings(feedArea);
            }
            return;
        }

        // User directory: #users
        if (hash === 'users') {
            if (typeof directory !== 'undefined') {
                directory.render(feedArea);
            }
            return;
        }

        // User theme preferences: #theme
        if (hash === 'theme') {
            if (typeof theme !== 'undefined') {
                theme.renderUserTheme(feedArea);
            }
            return;
        }

        switch (hash) {
            case 'preferences':
                messaging.renderPreferences(feedArea);
                break;
            case 'blacklist':
                messaging.renderBlacklist(feedArea);
                break;
            case 'calls':
                messaging.renderCallHistory(feedArea);
                break;
            case 'about':
                renderAboutPage(feedArea);
                break;
            case 'feed':
            default:
                // Restore feed UI
                restoreFeedUI();
                posts.loadFeed(true);
                break;
        }
    }

    function restoreFeedUI() {
        var feedArea = document.getElementById('feed-area');
        if (!feedArea) return;

        // Rebuild everything if create-post was removed (navigated away from feed)
        if (!document.getElementById('create-post')) {
            feedArea.innerHTML =
                '<div id="feed-tab-bar" class="feed-tab-bar">' +
                '<button class="feed-tab-btn active" data-feed-tab="local">Home</button>' +
                '<button class="feed-tab-btn" data-feed-tab="federated">Federated</button>' +
                '</div>' +
                '<div id="create-post" class="create-post-card">' +
                '<textarea id="post-text" placeholder="What\'s on your mind?" rows="3"></textarea>' +
                '<div id="post-attachment-previews" class="attachment-previews"></div>' +
                '<div class="create-post-media-bar">' +
                '<button id="btn-attach-image" class="btn-attach" title="Add image">' +
                '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="3" width="18" height="18" rx="2"/><circle cx="8.5" cy="8.5" r="1.5"/><path d="M21 15l-5-5L5 21"/></svg> Photo</button>' +
                '<button id="btn-attach-video" class="btn-attach" title="Add video">' +
                '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polygon points="23 7 16 12 23 17 23 7"/><rect x="1" y="5" width="15" height="14" rx="2"/></svg> Video</button>' +
                '<button id="btn-attach-file" class="btn-attach" title="Add file">' +
                '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/></svg> File</button>' +
                '<input type="file" id="post-file-input" style="display:none" multiple>' +
                '</div>' +
                '<div class="create-post-footer">' +
                '<div class="create-post-tags">' +
                '<div id="post-tag-chips" class="tag-chips"></div>' +
                '<input type="text" id="post-tag-input" placeholder="Add tags..." class="tag-input-small">' +
                '</div>' +
                '<div class="create-post-actions">' +
                '<label class="publish-toggle"><input type="checkbox" id="post-publish" checked> Publish</label>' +
                '<button id="btn-create-post" class="btn btn-primary btn-small">Post</button>' +
                '</div>' +
                '</div>' +
                '</div>' +
                '<div id="post-feed"></div>' +
                '<div id="feed-loading" class="loading-indicator" style="display:none">Loading...</div>' +
                '<div id="feed-empty" class="empty-state" style="display:none"><p>No posts yet. Be the first to share something!</p></div>';
            posts.initCreatePost();
        }
        // Always ensure feed tabs are wired up (covers first load where tab bar is in base.html)
        if (document.getElementById('feed-tab-bar')) {
            posts.initFeedTabs();
        }
    }

    function doLogout() {
        messaging.stopPolling();
        if (typeof calling !== 'undefined') {
            calling.disconnectWs();
        }
        api.post('/api/auth/logout').then(function () {
            state.currentUser = null;
            state.currentConversationId = null;
            window.location.hash = '';
            showAuth();
        }).catch(function () {
            state.currentUser = null;
            state.currentConversationId = null;
            window.location.hash = '';
            showAuth();
        });
    }

    // Bootstrap
    document.addEventListener('DOMContentLoaded', init);

    return {
        state: state,
        onAuthenticated: onAuthenticated
    };
})();
