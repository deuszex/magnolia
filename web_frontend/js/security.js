// Account security page - E2E encryption setup + password reset key management
var security = (function () {

    async function renderPage(container) {
        container.innerHTML = '<div class="settings-loading">Loading\u2026</div>';

        var cfg, status, e2eBlob;
        try {
            cfg = await api.get('/api/auth/config');
        } catch (e) {
            container.innerHTML = '<div class="error-box">' + escapeHtml(e.message) + '</div>';
            return;
        }

        var signingEnabled = cfg.password_reset_signing_key_available;

        if (signingEnabled) {
            try {
                status = await api.get('/api/auth/me/password-reset-key/status');
            } catch (e) {
                status = { has_key: false };
            }
        }

        try {
            var ekResp = await api.get('/api/auth/me/e2e-key');
            e2eBlob = ekResp && ekResp.e2e_key_blob ? ekResp.e2e_key_blob : null;
        } catch (_) {
            e2eBlob = null;
        }

        var e2eReady = typeof e2e !== 'undefined' && e2e.isReady();
        var e2eSupported = typeof e2e !== 'undefined' && window.crypto && window.crypto.subtle;

        var html = '<div class="settings-page"><h2 class="settings-title">Account Security</h2>';

        //  E2E Encryption 
        html += '<div class="settings-section">';
        html += '<div class="settings-section-title">End-to-End Encryption</div>';

        if (!e2eSupported) {
            html +=
                '<div class="settings-status status-off">' +
                '<i class="settings-status-icon">&#x1F512;</i>' +
                '<span>Not available &mdash; your browser does not support Web Crypto.</span>' +
                '</div>';
        } else if (e2eReady) {
            html +=
                '<div class="settings-status status-ok">' +
                '<i class="settings-status-icon">&#x1F512;</i>' +
                '<span>Encryption is <strong>active</strong> this session. ' +
                'Messages to other E2E-enabled users are encrypted on your device.</span>' +
                '</div>' +
                '<p class="settings-description">' +
                'Your key is stored on the server wrapped in your passphrase - the server cannot read it. ' +
                'Regenerating creates a new key pair; previously encrypted messages will become unreadable.' +
                '</p>' +
                '<div class="settings-actions">' +
                '<button id="sec-btn-e2e-regenerate" class="btn btn-secondary btn-small">Regenerate Key</button>' +
                '<button id="sec-btn-e2e-remove" class="btn btn-danger btn-small">Remove Key</button>' +
                '</div>';
        } else if (e2eBlob) {
            html +=
                '<div class="settings-status status-warn">' +
                '<i class="settings-status-icon">&#x1F513;</i>' +
                '<span>An E2E key exists but is <strong>not unlocked</strong> this session.</span>' +
                '</div>' +
                '<p class="settings-description">' +
                'Enter your passphrase to decrypt messages in this session, ' +
                'or remove the key entirely.' +
                '</p>' +
                '<div class="settings-actions">' +
                '<button id="sec-btn-e2e-unlock" class="btn btn-primary btn-small">Unlock Now</button>' +
                '<button id="sec-btn-e2e-remove" class="btn btn-danger btn-small">Remove Key</button>' +
                '</div>';
        } else {
            html +=
                '<div class="settings-status status-off">' +
                '<i class="settings-status-icon">&#x1F513;</i>' +
                '<span>Encryption is <strong>not set up</strong>. Messages are sent in plaintext.</span>' +
                '</div>' +
                '<p class="settings-description">' +
                'Set up end-to-end encryption for direct messages. ' +
                'Your private key is encrypted with a passphrase you choose and stored on the server &mdash; ' +
                'only you can unlock it. You will need your passphrase each session.' +
                '</p>' +
                '<div class="settings-actions">' +
                '<button id="sec-btn-e2e-setup" class="btn btn-primary btn-small">Set Up Encryption</button>' +
                '</div>';
        }

        html += '</div>'; // .settings-section

        //  Password Recovery Key 
        html += '<div class="settings-section">';
        html += '<div class="settings-section-title">Password Recovery Key</div>';

        if (!signingEnabled) {
            html +=
                '<div class="settings-status status-off">' +
                '<i class="settings-status-icon">&#x1F5DD;</i>' +
                '<span>Recovery key reset is not enabled on this server.</span>' +
                '</div>';
        } else {
            var hasKey = status && status.has_key;
            html += hasKey
                ? '<div class="settings-status status-ok">' +
                  '<i class="settings-status-icon">&#x1F5DD;</i>' +
                  '<span>A recovery key is stored on this server.</span>' +
                  '</div>'
                : '<div class="settings-status status-off">' +
                  '<i class="settings-status-icon">&#x1F5DD;</i>' +
                  '<span>No recovery key yet.</span>' +
                  '</div>';

            html +=
                '<p class="settings-description">' +
                'A recovery key lets you reset your password without email, even when locked out. ' +
                'Download it and keep it somewhere safe (password manager, USB drive). ' +
                'Each key can only be used <strong>once</strong> &mdash; generating a new key revokes the old one.' +
                '</p>' +
                '<div class="settings-actions">' +
                '<button id="sec-btn-generate" class="btn btn-primary btn-small">' +
                (hasKey ? 'Regenerate Key' : 'Generate Key') +
                '</button>' +
                (hasKey
                    ? '<button id="sec-btn-revoke" class="btn btn-danger btn-small">Revoke Key</button>'
                    : '') +
                '</div>';
        }

        html += '</div>'; // .settings-section (password recovery)

        // Active Sessions section - rendered dynamically after innerHTML set
        html += '<div class="settings-section" id="sec-sessions-section">' +
            '<div class="settings-section-title">Active Sessions</div>' +
            '<div id="sec-sessions-list" class="sec-sessions-list">Loading\u2026</div>' +
            '</div>';

        html += '</div>'; // .settings-page

        container.innerHTML = html;

        // Load sessions asynchronously
        loadSessions();

        // Wire E2E buttons
        var e2eSetup = document.getElementById('sec-btn-e2e-setup');
        if (e2eSetup) e2eSetup.onclick = function () { setupE2E(container); };

        var e2eUnlock = document.getElementById('sec-btn-e2e-unlock');
        if (e2eUnlock) e2eUnlock.onclick = function () { unlockE2E(container); };

        var e2eRegenerate = document.getElementById('sec-btn-e2e-regenerate');
        if (e2eRegenerate) e2eRegenerate.onclick = function () { regenerateE2E(container); };

        var e2eRemove = document.getElementById('sec-btn-e2e-remove');
        if (e2eRemove) e2eRemove.onclick = function () { removeE2E(container); };

        // Wire recovery key buttons
        if (!signingEnabled) return;

        document.getElementById('sec-btn-generate').onclick = function () { generateKey(container); };
        var revokeBtn = document.getElementById('sec-btn-revoke');
        if (revokeBtn) revokeBtn.onclick = function () { revokeKey(container); };
    }

    async function setupE2E(container) {
        var btn = document.getElementById('sec-btn-e2e-setup');
        if (btn) { btn.disabled = true; btn.textContent = 'Setting up\u2026'; }
        try {
            await e2e.setup();
            renderPage(container);
        } catch (err) {
            if (btn) { btn.disabled = false; btn.textContent = 'Set Up Encryption'; }
            if (!err || err.message !== 'cancelled') {
                alert('Failed to set up encryption: ' + (err && err.message || err));
            }
        }
    }

    async function unlockE2E(container) {
        var btn = document.getElementById('sec-btn-e2e-unlock');
        if (btn) { btn.disabled = true; btn.textContent = 'Unlocking\u2026'; }
        try {
            await e2e.init();
            renderPage(container);
        } catch (_) {
            if (btn) { btn.disabled = false; btn.textContent = 'Unlock Now'; }
        }
    }

    async function regenerateE2E(container) {
        if (!confirm('This will generate a new key pair. Your existing encrypted messages may become unreadable. Continue?')) return;
        try { await api.del('/api/auth/me/e2e-key'); } catch (_) {}
        e2e.reset();
        await setupE2E(container);
    }

    async function removeE2E(container) {
        if (!confirm('Remove your E2E encryption key? Encrypted messages will become unreadable.')) return;
        try {
            await api.del('/api/auth/me/e2e-key');
            e2e.reset();
            renderPage(container);
        } catch (e) {
            alert('Failed to remove key: ' + e.message);
        }
    }

    async function generateKey(container) {
        var btn = document.getElementById('sec-btn-generate');
        if (btn) { btn.disabled = true; btn.textContent = 'Generating\u2026'; }
        try {
            var data = await api.post('/api/auth/me/password-reset-key/generate', {});
            downloadKeyFile(data);
            renderPage(container);
        } catch (e) {
            alert('Failed to generate key: ' + e.message);
            if (btn) { btn.disabled = false; btn.textContent = 'Generate Key'; }
        }
    }

    async function revokeKey(container) {
        if (!confirm('Revoke the recovery key? You will not be able to use it to reset your password.')) return;
        try {
            await api.del('/api/auth/me/password-reset-key');
            renderPage(container);
        } catch (e) {
            alert('Failed to revoke key: ' + e.message);
        }
    }

    function loadSessions() {
        var list = document.getElementById('sec-sessions-list');
        if (!list) return;
        api.get('/api/auth/sessions').then(function (sessions) {
            list.innerHTML = '';
            if (!sessions || !sessions.length) {
                list.innerHTML = '<p class="sec-sessions-empty">No active sessions found.</p>';
                return;
            }
            sessions.forEach(function (s) {
                var card = document.createElement('div');
                card.className = 'sec-session-card' + (s.is_current ? ' sec-session-current' : '');

                var displayLabel = formatSession(s);
                var rawUa = s.user_agent || '';
                var ip = s.ip_address || null;
                var loginTime = s.created_at ? new Date(s.created_at).toLocaleString() : '-';
                var expiresTime = s.expires_at ? new Date(s.expires_at).toLocaleString() : '-';

                card.innerHTML =
                    '<div class="sec-session-info">' +
                    '<div class="sec-session-ua">' + escapeHtml(displayLabel) + '</div>' +
                    (rawUa ? '<div class="sec-session-ua-raw">' + escapeHtml(rawUa) + '</div>' : '') +
                    '<div class="sec-session-meta">' +
                    (ip ? '<span>' + escapeHtml(ip) + '</span>' : '') +
                    '<span>Signed in: ' + escapeHtml(loginTime) + '</span>' +
                    '<span>Expires: ' + escapeHtml(expiresTime) + '</span>' +
                    '</div>' +
                    (s.is_current ? '<span class="sec-session-badge">Current session</span>' : '') +
                    '</div>' +
                    '<div class="sec-session-actions"></div>';

                if (!s.is_current) {
                    var revokeBtn = document.createElement('button');
                    revokeBtn.className = 'btn btn-small btn-danger';
                    revokeBtn.textContent = 'Revoke';
                    revokeBtn.onclick = function () {
                        if (!confirm('Revoke this session? That device will be signed out.')) return;
                        revokeBtn.disabled = true;
                        revokeBtn.textContent = 'Revoking\u2026';
                        api.del('/api/auth/sessions/' + encodeURIComponent(s.session_id))
                            .then(function () { loadSessions(); })
                            .catch(function (err) {
                                revokeBtn.disabled = false;
                                revokeBtn.textContent = 'Revoke';
                                alert('Failed: ' + (err.message || err));
                            });
                    };
                    card.querySelector('.sec-session-actions').appendChild(revokeBtn);
                }

                list.appendChild(card);
            });
        }).catch(function (err) {
            if (list) list.innerHTML = '<p class="error-box">' + escapeHtml(err.message || 'Failed to load sessions') + '</p>';
        });
    }

    function parseUserAgent(ua) {
        if (!ua) return { browser: 'Unknown client', os: null };

        var browser = 'Unknown browser';
        var os = null;

        // OS detection
        if (/Windows NT 10/.test(ua)) os = 'Windows 10/11';
        else if (/Windows NT 6\.3/.test(ua)) os = 'Windows 8.1';
        else if (/Windows NT 6\.1/.test(ua)) os = 'Windows 7';
        else if (/Windows/.test(ua)) os = 'Windows';
        else if (/Mac OS X ([\d_]+)/.test(ua)) os = 'macOS ' + ua.match(/Mac OS X ([\d_]+)/)[1].replace(/_/g, '.');
        else if (/Android ([\d.]+)/.test(ua)) os = 'Android ' + ua.match(/Android ([\d.]+)/)[1];
        else if (/iPhone OS ([\d_]+)/.test(ua)) os = 'iOS ' + ua.match(/iPhone OS ([\d_]+)/)[1].replace(/_/g, '.');
        else if (/Linux/.test(ua)) os = 'Linux';

        // Browser detection - order matters (check specific before generic)
        var m;
        if (/Edg\/([\d.]+)/.test(ua)) {
            m = ua.match(/Edg\/([\d]+)/);
            browser = 'Edge ' + (m ? m[1] : '');
        } else if (/OPR\/([\d]+)/.test(ua) || /Opera\/([\d]+)/.test(ua)) {
            m = ua.match(/OPR\/([\d]+)/) || ua.match(/Opera\/([\d]+)/);
            browser = 'Opera ' + (m ? m[1] : '');
        } else if (/Firefox\/([\d]+)/.test(ua)) {
            m = ua.match(/Firefox\/([\d]+)/);
            browser = 'Firefox ' + (m ? m[1] : '');
        } else if (/Chrome\/([\d]+)/.test(ua)) {
            m = ua.match(/Chrome\/([\d]+)/);
            browser = 'Chrome ' + (m ? m[1] : '');
        } else if (/Safari\/([\d]+)/.test(ua) && /Version\/([\d]+)/.test(ua)) {
            m = ua.match(/Version\/([\d]+)/);
            browser = 'Safari ' + (m ? m[1] : '');
        } else if (/curl\/([\d.]+)/.test(ua)) {
            browser = 'curl';
        } else if (/python/i.test(ua)) {
            browser = 'Python HTTP client';
        }

        return { browser: browser, os: os };
    }

    function formatSession(s) {
        var parsed = parseUserAgent(s.user_agent);
        var label = parsed.browser;
        if (parsed.os) label += ' \u2014 ' + parsed.os;
        return label;
    }

    function truncate(str, max) {
        return str.length > max ? str.slice(0, max) + '\u2026' : str;
    }

    function downloadKeyFile(data) {
        var fileContent = JSON.stringify({
            _info: 'Magnolia password recovery key. Keep this file safe.',
            user_id: data.user_id,
            username: data.username,
            key: data.key,
            generated_at: data.generated_at
        }, null, 2);

        var blob = new Blob([fileContent], { type: 'application/json' });
        var url = URL.createObjectURL(blob);
        var a = document.createElement('a');
        a.href = url;
        a.download = 'magnolia-recovery-key-' + (data.username || data.user_id) + '.json';
        document.body.appendChild(a);
        a.click();
        document.body.removeChild(a);
        URL.revokeObjectURL(url);
    }

    return { renderPage: renderPage };
})();
