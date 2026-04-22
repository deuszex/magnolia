// Proxy user management page for regular users
var proxyPage = (function () {

    function el(tag, props, children) {
        var e = document.createElement(tag);
        if (props) Object.keys(props).forEach(function (k) {
            if (k === 'class') e.className = props[k];
            else if (k === 'html') e.innerHTML = props[k];
            else if (k === 'text') e.textContent = props[k];
            else e[k] = props[k];
        });
        if (children) children.forEach(function (c) { if (c) e.appendChild(c); });
        return e;
    }

    function escHtml(s) {
        return String(s || '')
            .replace(/&/g, '&amp;')
            .replace(/</g, '&lt;')
            .replace(/>/g, '&gt;')
            .replace(/"/g, '&quot;');
    }

    function showMsg(container, msg, type) {
        var box = container.querySelector('.proxy-feedback');
        if (!box) { box = el('div', { class: 'proxy-feedback' }); container.prepend(box); }
        box.className = 'proxy-feedback ' + (type === 'error' ? 'error-box' : 'success-box');
        box.textContent = msg;
        setTimeout(function () { box.textContent = ''; box.className = 'proxy-feedback'; }, 4000);
    }

    function toggle(labelText, descText, inputId, checked) {
        var row = el('div', { class: 'pref-row' });
        var info = el('div');
        info.appendChild(el('div', { class: 'pref-label', text: labelText }));
        if (descText) info.appendChild(el('div', { class: 'pref-desc', text: descText }));
        row.appendChild(info);
        var label = el('label', { class: 'toggle-wrap' });
        var inp = el('input', { type: 'checkbox', id: inputId });
        inp.checked = !!checked;
        label.appendChild(inp);
        label.appendChild(el('span', { class: 'toggle-track' }));
        row.appendChild(label);
        return row;
    }

    function renderPage(feedArea) {
        feedArea.innerHTML = '';
        var page = el('div', { class: 'settings-page' });
        var section = el('div', { class: 'settings-section', text: 'Loading…' });
        page.appendChild(section);
        feedArea.appendChild(page);

        api.get('/api/proxy').then(function (proxy) {
            renderProxyInfo(section, proxy);
        }).catch(function (err) {
            if (err.status === 404) {
                renderNoProxy(section);
            } else if (err.status === 403) {
                section.innerHTML = '';
                section.appendChild(el('div', { class: 'info-box', text: 'Proxy accounts are not enabled on this server.' }));
            } else {
                section.innerHTML = '<div class="error-box">' + escHtml(err.message || 'Failed to load proxy info') + '</div>';
            }
        });
    }

    function renderNoProxy(section) {
        section.innerHTML = '';
        section.appendChild(el('div', { class: 'settings-section-title', text: 'Proxy Account' }));
        section.appendChild(el('div', { class: 'pref-desc', text: 'You do not have a proxy account. Create one to enable automation.' }));

        var feedback = el('div', { class: 'proxy-feedback' });
        section.appendChild(feedback);

        var usernameGroup = el('div', { class: 'form-group', style: 'margin-top:14px' });
        usernameGroup.appendChild(el('label', { text: 'Proxy Username' }));
        var usernameInput = el('input', { type: 'text', class: 'form-input', id: 'px-new-username', placeholder: 'e.g. my_bot' });
        usernameGroup.appendChild(usernameInput);
        section.appendChild(usernameGroup);

        var createBtn = el('button', { class: 'btn btn-primary', text: 'Create Proxy Account' });
        createBtn.onclick = function () {
            var username = document.getElementById('px-new-username').value.trim();
            if (!username) {
                showMsg(section, 'Enter a username for the proxy.', 'error');
                return;
            }
            createBtn.disabled = true;
            api.post('/api/proxy', { username: username }).then(function (proxy) {
                renderProxyInfo(section, proxy);
            }).catch(function (err) {
                showMsg(section, err.message || 'Failed to create proxy', 'error');
                createBtn.disabled = false;
            });
        };
        section.appendChild(createBtn);
    }

    // Generate a random 32-byte key as a 64-char hex string
    function generateHmacKey() {
        var bytes = new Uint8Array(32);
        crypto.getRandomValues(bytes);
        return Array.from(bytes).map(function (b) { return b.toString(16).padStart(2, '0'); }).join('');
    }

    // Trigger a browser file download of text content
    function downloadText(filename, text) {
        var blob = new Blob([text], { type: 'text/plain' });
        var url = URL.createObjectURL(blob);
        var a = document.createElement('a');
        a.href = url;
        a.download = filename;
        a.click();
        URL.revokeObjectURL(url);
    }

    function renderHmacSection(section, proxy, apiPrefix) {
        var hmacSec = el('div', { class: 'settings-section', style: 'margin-bottom:16px' });
        hmacSec.appendChild(el('div', { class: 'settings-section-title', text: 'HMAC API Key' }));
        hmacSec.appendChild(el('div', { class: 'pref-desc', text: 'Used for one-shot request signing (POST /api/proxy/hmac/send-message and /api/proxy/hmac/create-post). The key is generated here, downloaded by you, and the server stores it to verify signatures. If you lose the key, generate a new one.' }));

        if (proxy.has_hmac_key) {
            hmacSec.appendChild(el('div', {
                class: 'info-box',
                style: 'margin:8px 0;font-family:monospace',
                text: 'Current key fingerprint: ' + proxy.hmac_key_fingerprint + '…'
            }));
        } else {
            hmacSec.appendChild(el('div', { class: 'info-box', style: 'margin:8px 0', text: 'No HMAC key set. Generate one below.' }));
        }

        // In-memory key state for this render
        var pendingKey = null;

        var keyDisplayRow = el('div', { style: 'display:none;margin:10px 0' });
        var keyBox = el('div', { style: 'font-family:monospace;font-size:0.8rem;word-break:break-all;background:var(--t-bg-alt,#1e1e1e);border:1px solid var(--t-border,#333);border-radius:6px;padding:10px;user-select:all' });
        keyDisplayRow.appendChild(keyBox);

        var downloadBtn = el('button', { class: 'btn btn-secondary', text: 'Download Key File', style: 'margin-top:8px' });
        var saveBtn = el('button', { class: 'btn btn-primary', text: 'Save to Server', style: 'margin-top:8px;margin-left:8px;opacity:0.4;pointer-events:none' });

        downloadBtn.onclick = function () {
            if (!pendingKey) return;
            downloadText('proxy-hmac-key-' + proxy.proxy_id.slice(0, 8) + '.txt',
                'HMAC Key for proxy @' + proxy.username + '\n' +
                'Proxy ID: ' + proxy.proxy_id + '\n' +
                'Key (hex): ' + pendingKey + '\n' +
                'Fingerprint: ' + pendingKey.slice(0, 8) + '\n\n' +
                'Keep this file secure. Anyone with this key can sign requests as this proxy.'
            );
            // Unlock save button after download
            saveBtn.style.opacity = '';
            saveBtn.style.pointerEvents = '';
        };

        saveBtn.onclick = function () {
            if (!pendingKey) return;
            saveBtn.disabled = true;
            api.put(apiPrefix + '/hmac-key', { hmac_key: pendingKey }).then(function () {
                showMsg(section, 'HMAC key saved to server.', 'success');
                pendingKey = null;
                keyDisplayRow.style.display = 'none';
                saveBtn.disabled = false;
                // Update fingerprint display inline
                hmacSec.querySelector('.info-box').textContent = 'Current key fingerprint: ' + pendingKey ? pendingKey.slice(0, 8) : '';
                // Re-fetch to update the fingerprint label properly
                api.get(apiPrefix === '/api/proxy' ? '/api/proxy' : null).then(function () {}).catch(function () {});
            }).catch(function (err) {
                showMsg(section, err.message || 'Failed to save key', 'error');
                saveBtn.disabled = false;
            });
        };

        keyDisplayRow.appendChild(downloadBtn);
        keyDisplayRow.appendChild(saveBtn);
        hmacSec.appendChild(keyDisplayRow);

        var genBtn = el('button', {
            class: 'btn ' + (proxy.has_hmac_key ? 'btn-secondary' : 'btn-primary'),
            text: proxy.has_hmac_key ? 'Regenerate HMAC Key' : 'Generate HMAC Key',
            style: 'margin-top:10px'
        });
        genBtn.onclick = function () {
            pendingKey = generateHmacKey();
            keyBox.textContent = pendingKey;
            keyDisplayRow.style.display = '';
            // Reset save button lock  must download first
            saveBtn.style.opacity = '0.4';
            saveBtn.style.pointerEvents = 'none';
        };
        hmacSec.appendChild(genBtn);

        section.appendChild(hmacSec);
    }

    function renderAvatarSection(section, proxy, apiPrefix) {
        var avatarSec = el('div', { class: 'settings-section', style: 'margin-bottom:16px' });
        avatarSec.appendChild(el('div', { class: 'settings-section-title', text: 'Avatar' }));

        var previewRow = el('div', { style: 'display:flex;align-items:center;gap:14px;margin:10px 0' });
        var avatarImg = el('img', { style: 'width:64px;height:64px;border-radius:50%;object-fit:cover;background:var(--t-bg-alt,#222)' });
        if (proxy.avatar_url) {
            avatarImg.src = proxy.avatar_url;
        } else {
            avatarImg.style.display = 'none';
        }
        previewRow.appendChild(avatarImg);

        var fileInput = el('input', { type: 'file', accept: 'image/*', style: 'display:none' });
        var uploadBtn = el('button', { class: 'btn btn-secondary', text: proxy.avatar_url ? 'Change Avatar' : 'Upload Avatar' });
        uploadBtn.onclick = function () { fileInput.click(); };

        fileInput.onchange = function () {
            var file = fileInput.files[0];
            if (!file) return;
            uploadBtn.disabled = true;
            uploadBtn.textContent = 'Uploading…';
            api.upload('/api/media', file).then(function (media) {
                return api.patch(apiPrefix, { avatar_media_id: media.media_id });
            }).then(function (updated) {
                showMsg(section, 'Avatar updated.', 'success');
                if (updated && updated.avatar_url) {
                    avatarImg.src = updated.avatar_url;
                    avatarImg.style.display = '';
                }
                uploadBtn.textContent = 'Change Avatar';
                uploadBtn.disabled = false;
            }).catch(function (err) {
                showMsg(section, err.message || 'Upload failed', 'error');
                uploadBtn.textContent = proxy.avatar_url ? 'Change Avatar' : 'Upload Avatar';
                uploadBtn.disabled = false;
            });
        };

        previewRow.appendChild(uploadBtn);
        previewRow.appendChild(fileInput);
        avatarSec.appendChild(previewRow);
        section.appendChild(avatarSec);
    }

    function renderProxyInfo(section, proxy) {
        section.innerHTML = '';
        section.appendChild(el('div', { class: 'settings-section-title', text: 'Proxy Account' }));
        var feedback = el('div', { class: 'proxy-feedback' });
        section.appendChild(feedback);

        // Info card
        var card = el('div', { class: 'settings-card', style: 'display:flex;align-items:center;gap:14px;margin-bottom:20px' });
        if (proxy.avatar_url) {
            var img = el('img', { style: 'width:56px;height:56px;border-radius:50%;object-fit:cover' });
            img.src = proxy.avatar_url;
            card.appendChild(img);
        }
        var info = el('div');
        info.appendChild(el('div', { style: 'font-weight:600;font-size:1.1rem', text: proxy.display_name || '@' + proxy.username }));
        info.appendChild(el('div', { style: 'color:var(--t-secondary);font-size:0.9rem', text: '@' + proxy.username }));
        if (proxy.bio) info.appendChild(el('div', { style: 'margin-top:4px;font-size:0.85rem', text: proxy.bio }));
        if (proxy.public_key) {
            info.appendChild(el('div', {
                style: 'margin-top:4px;font-size:0.75rem;color:var(--t-muted);font-family:monospace',
                text: 'E2E key: ' + proxy.public_key.slice(0, 16) + '…'
            }));
        }
        info.appendChild(el('span', {
            class: 'status-badge ' + (proxy.active ? 'status-active' : 'status-inactive'),
            text: proxy.active ? 'Active' : 'Disabled',
            style: 'margin-top:6px;display:inline-block'
        }));
        info.appendChild(el('div', {
            style: 'margin-top:6px;font-size:0.72rem;font-family:monospace;color:var(--t-muted);word-break:break-all',
            text: 'ID: ' + proxy.proxy_id
        }));
        card.appendChild(info);
        section.appendChild(card);

        // Avatar
        renderAvatarSection(section, proxy, '/api/proxy');

        // Edit profile section
        var editSec = el('div', { class: 'settings-section', style: 'margin-bottom:16px' });
        editSec.appendChild(el('div', { class: 'settings-section-title', text: 'Edit Profile' }));
        var dnInput = el('input', { type: 'text', class: 'form-input', id: 'px-display-name', value: proxy.display_name || '', placeholder: 'Display name' });
        var dnGroup = el('div', { class: 'form-group' });
        dnGroup.appendChild(el('label', { text: 'Display Name' }));
        dnGroup.appendChild(dnInput);
        editSec.appendChild(dnGroup);

        var bioGroup = el('div', { class: 'form-group' });
        bioGroup.appendChild(el('label', { text: 'Bio' }));
        var bioInput = el('textarea', { class: 'form-input', id: 'px-bio', rows: '3' });
        bioInput.value = proxy.bio || '';
        bioGroup.appendChild(bioInput);
        editSec.appendChild(bioGroup);

        editSec.appendChild(toggle('Active', 'When disabled, the proxy cannot post or message', 'px-active', proxy.active));

        var saveProfileBtn = el('button', { class: 'btn btn-primary', text: 'Save Profile' });
        saveProfileBtn.onclick = function () {
            var body = {
                display_name: document.getElementById('px-display-name').value.trim() || null,
                bio: document.getElementById('px-bio').value.trim() || null,
                active: document.getElementById('px-active').checked
            };
            api.patch('/api/proxy', body).then(function (updated) {
                showMsg(section, 'Profile updated.', 'success');
                renderProxyInfo(section, updated);
            }).catch(function (err) {
                showMsg(section, err.message || 'Failed to save', 'error');
            });
        };
        editSec.appendChild(saveProfileBtn);
        section.appendChild(editSec);

        // HMAC API key
        renderHmacSection(section, proxy, '/api/proxy');

        // Session password section
        var pwSec = el('div', { class: 'settings-section', style: 'margin-bottom:16px' });
        pwSec.appendChild(el('div', { class: 'settings-section-title', text: 'Session Password' }));
        pwSec.appendChild(el('div', { class: 'pref-desc', text: 'Set a password to allow the proxy to authenticate with a session cookie and use the full API. Leave unset to limit the proxy to HMAC one-shot mode.' }));
        if (proxy.has_password) {
            pwSec.appendChild(el('div', { class: 'info-box', style: 'margin:8px 0', text: 'A session password is currently set.' }));
        }
        var pwGroup = el('div', { class: 'form-group', style: 'margin-top:10px' });
        pwGroup.appendChild(el('label', { text: 'New Password' }));
        pwGroup.appendChild(el('input', { type: 'password', class: 'form-input', id: 'px-password', placeholder: 'Min 8 chars' }));
        pwSec.appendChild(pwGroup);
        var setPwBtn = el('button', { class: 'btn btn-primary', text: 'Set Password' });
        setPwBtn.onclick = function () {
            var pw = document.getElementById('px-password').value;
            if (pw.length < 8) { showMsg(section, 'Password must be at least 12 characters, Uppercase+Lowercase, 1 number, 1 symbol (?!$...).', 'error'); return; }
            api.put('/api/proxy/password', { password: pw }).then(function () {
                showMsg(section, 'Password updated.', 'success');
                document.getElementById('px-password').value = '';
            }).catch(function (err) { showMsg(section, err.message || 'Failed', 'error'); });
        };
        pwSec.appendChild(setPwBtn);
        section.appendChild(pwSec);

        // E2E key section
        var e2eSec = el('div', { class: 'settings-section' });
        e2eSec.appendChild(el('div', { class: 'settings-section-title', text: 'End-to-End Encryption Key' }));
        e2eSec.appendChild(el('div', { class: 'pref-desc', text: 'Configure the proxy\'s E2E encryption key. The key blob is encrypted with the proxy\'s passphrase  the server only stores the encrypted blob and public key.' }));
        if (proxy.has_e2e_key) {
            e2eSec.appendChild(el('div', { class: 'info-box', style: 'margin:8px 0', text: 'E2E key is configured.' + (proxy.public_key ? ' Public key: ' + proxy.public_key.slice(0, 16) + '…' : '') }));
        } else {
            e2eSec.appendChild(el('div', { class: 'info-box', style: 'margin:8px 0', text: 'No E2E key. The proxy cannot participate in encrypted conversations.' }));
        }
        var e2eGroup = el('div', { class: 'form-group', style: 'margin-top:10px' });
        e2eGroup.appendChild(el('label', { text: 'Passphrase (encrypts the key locally)' }));
        e2eGroup.appendChild(el('input', { type: 'password', class: 'form-input', id: 'px-e2e-passphrase', placeholder: 'Proxy passphrase' }));
        e2eSec.appendChild(e2eGroup);
        var genE2eBtn = el('button', { class: 'btn btn-primary', text: proxy.has_e2e_key ? 'Regenerate E2E Key' : 'Generate E2E Key' });
        genE2eBtn.onclick = function () {
            var passphrase = document.getElementById('px-e2e-passphrase').value;
            if (!passphrase) { showMsg(section, 'Enter a passphrase.', 'error'); return; }
            if (typeof e2e === 'undefined' || typeof e2e.generateKeyBlobWithPassphrase !== 'function') {
                showMsg(section, 'E2E module not available.', 'error'); return;
            }
            genE2eBtn.disabled = true;
            genE2eBtn.textContent = 'Generating…';
            e2e.generateKeyBlobWithPassphrase(passphrase).then(function (result) {
                return api.put('/api/proxy/e2e-key', { e2e_key_blob: result.blob, public_key: result.publicKey });
            }).then(function () {
                showMsg(section, 'E2E key saved.', 'success');
                document.getElementById('px-e2e-passphrase').value = '';
                genE2eBtn.textContent = 'Regenerate E2E Key';
                genE2eBtn.disabled = false;
            }).catch(function (err) {
                showMsg(section, err.message || 'Failed', 'error');
                genE2eBtn.textContent = proxy.has_e2e_key ? 'Regenerate E2E Key' : 'Generate E2E Key';
                genE2eBtn.disabled = false;
            });
        };
        e2eSec.appendChild(genE2eBtn);
        section.appendChild(e2eSec);
    }

    return {
        renderPage: renderPage
    };
})();
