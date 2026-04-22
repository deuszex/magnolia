// Admin panel module, user management, invites, site config, theme
var admin = (function () {

    // Helpers 

    function el(tag, props, children) {
        var e = document.createElement(tag);
        if (props) Object.keys(props).forEach(function (k) {
            if (k === 'class') e.className = props[k];
            else if (k === 'html') e.innerHTML = props[k]; // only pass hardcoded literals here, never user data
            else if (k === 'text') e.textContent = props[k];
            else e[k] = props[k];
        });
        if (children) children.forEach(function (c) { if (c) e.appendChild(c); });
        return e;
    }

    function badge(label, cls) {
        return '<span class="status-badge ' + cls + '">' + label + '</span>';
    }

    function fmtDate(s) {
        if (!s) return '—';
        try { return new Date(s).toLocaleString(undefined, { dateStyle: 'short', timeStyle: 'short' }); }
        catch (e) { return s; }
    }

    function showMsg(container, msg, type) {
        var box = container.querySelector('.admin-feedback');
        if (!box) { box = el('div', { class: 'admin-feedback' }); container.prepend(box); }
        box.className = 'admin-feedback ' + (type === 'error' ? 'error-box' : 'success-box');
        box.textContent = msg;
        setTimeout(function () { box.textContent = ''; box.className = 'admin-feedback'; }, 4000);
    }

    // Sub-nav 

    function renderSubNav(feedArea, active) {
        var pages = [
            { id: 'users', label: 'Users' },
            { id: 'invites', label: 'Invites' },
            { id: 'applications', label: 'Applications' },
            { id: 'site-config', label: 'Site Config' },
            { id: 'email', label: 'Email' },
            { id: 'theme', label: 'Theme' },
            { id: 'stun', label: 'STUN/TURN' },
            { id: 'federation', label: 'Federation' },
            { id: 'proxies', label: 'Proxy Accounts' }
        ];
        var nav = el('div', { class: 'admin-subnav' });
        pages.forEach(function (p) {
            var btn = el('button', {
                class: 'admin-subnav-btn' + (p.id === active ? ' active' : ''),
                text: p.label
            });
            btn.onclick = function () { window.location.hash = 'admin/' + p.id; };
            nav.appendChild(btn);
        });
        return nav;
    }

    // Users Page 

    function renderUsers(feedArea) {
        feedArea.innerHTML = '';
        var page = el('div', { class: 'admin-page' });
        page.appendChild(renderSubNav(feedArea, 'users'));

        var section = el('div', { class: 'admin-section' });
        var titleRow = el('div', { class: 'admin-section-title' });
        titleRow.appendChild(el('span', { text: 'Users' }));

        var createBtn = el('button', { class: 'btn btn-primary btn-small', text: '+ Add User' });
        createBtn.onclick = function () { showCreateUserDialog(section, function () { renderUsers(feedArea); }); };
        titleRow.appendChild(createBtn);

        section.appendChild(titleRow);

        var feedback = el('div', { class: 'admin-feedback' });
        section.appendChild(feedback);

        var tableWrap = el('div', { class: 'admin-table-wrap', text: 'Loading…' });
        section.appendChild(tableWrap);
        page.appendChild(section);
        feedArea.appendChild(page);

        api.get('/api/admin/users?limit=200').then(function (data) {
            tableWrap.innerHTML = '';
            if (!data.users || !data.users.length) {
                tableWrap.innerHTML = '<div class="empty-state"><p>No users found.</p></div>';
                return;
            }

            var table = el('table', { class: 'admin-table' });
            table.innerHTML =
                '<thead><tr>' +
                '<th>Username</th><th>Email</th><th>Name</th><th>Status</th><th>Roles</th><th>Joined</th><th>Actions</th>' +
                '</tr></thead>';
            var tbody = el('tbody');

            data.users.forEach(function (u) {
                var tr = el('tr');
                tr.innerHTML =
                    '<td class="primary">' + escHtml(u.username) + '<br><span class="admin-uuid">' + escHtml(u.user_id) + '</span></td>' +
                    '<td>' + escHtml(u.email || '-') + '</td>' +
                    '<td>' + escHtml(u.display_name || '-') + '</td>' +
                    '<td>' + (u.active ? badge('Active', 'status-active') : badge('Inactive', 'status-inactive')) + '</td>' +
                    '<td>' + (u.admin ? badge('Admin', 'status-admin') : '') + (u.verified ? '' : badge('Unverified', 'status-inactive')) + '</td>' +
                    '<td>' + fmtDate(u.created_at) + '</td>';

                var actions = el('td');
                var actionsDiv = el('div', { class: 'admin-actions' });

                // Toggle active
                var toggleBtn = el('button', {
                    class: 'btn btn-small' + (u.active ? ' btn-ghost' : ''),
                    text: u.active ? 'Deactivate' : 'Activate'
                });
                toggleBtn.onclick = function () {
                    api.patch('/api/admin/users/' + u.user_id, { active: !u.active }).then(function () {
                        renderUsers(feedArea);
                    }).catch(function (err) { showMsg(section, err.message || 'Error', 'error'); });
                };
                actionsDiv.appendChild(toggleBtn);

                // Toggle admin
                var adminBtn = el('button', {
                    class: 'btn btn-small' + (u.admin ? ' btn-danger' : ''),
                    text: u.admin ? 'Revoke Admin' : 'Make Admin'
                });
                adminBtn.onclick = function () {
                    api.patch('/api/admin/users/' + u.user_id, { admin: !u.admin }).then(function () {
                        renderUsers(feedArea);
                    }).catch(function (err) { showMsg(section, err.message || 'Error', 'error'); });
                };
                actionsDiv.appendChild(adminBtn);

                // Delete
                var delBtn = el('button', { class: 'btn btn-small btn-danger', text: 'Delete' });
                delBtn.onclick = function () {
                    if (!confirm('Delete user ' + u.email + '? This cannot be undone.')) return;
                    api.del('/api/admin/users/' + u.user_id).then(function () {
                        renderUsers(feedArea);
                    }).catch(function (err) { showMsg(section, err.message || 'Error', 'error'); });
                };
                actionsDiv.appendChild(delBtn);

                actions.appendChild(actionsDiv);
                tr.appendChild(actions);
                tbody.appendChild(tr);
            });

            table.appendChild(tbody);
            tableWrap.appendChild(table);
        }).catch(function (err) {
            tableWrap.innerHTML = '<div class="error-box">' + escHtml(err.message || 'Failed to load users') + '</div>';
        });
    }

    function showCreateUserDialog(container, onSuccess) {
        var overlay = document.getElementById('dialog-overlay');
        var dialog = document.getElementById('dialog');
        overlay.style.display = 'flex';
        dialog.innerHTML =
            '<h3>Add User</h3>' +
            '<div class="form-group"><label>Username <span style="color:var(--t-danger)">*</span></label><input type="text" id="cu-username" placeholder="3–30 characters" autocomplete="off"></div>' +
            '<div class="form-group"><label>Email <span style="color:var(--t-muted);font-weight:normal">(optional)</span></label><input type="email" id="cu-email" placeholder="user@example.com"></div>' +
            '<div class="form-group"><label>Password <span style="color:var(--t-danger)">*</span></label><input type="password" id="cu-pw" placeholder="12 characters, Uppercase+Lowercase, 1 number, 1 symbol (?!$...)"></div>' +
            '<div class="checkbox-row" style="margin-bottom:14px">' +
            '<label><input type="checkbox" id="cu-admin"> Admin</label>' +
            '<label><input type="checkbox" id="cu-verified" checked> Pre-verified</label>' +
            '</div>' +
            '<div id="cu-error"></div>' +
            '<div class="dialog-actions">' +
            '<button class="btn" id="cu-cancel">Cancel</button>' +
            '<button class="btn btn-primary" id="cu-submit">Create User</button>' +
            '</div>';

        document.getElementById('cu-cancel').onclick = function () { overlay.style.display = 'none'; };
        document.getElementById('cu-submit').onclick = function () {
            var username = document.getElementById('cu-username').value.trim();
            var email = document.getElementById('cu-email').value.trim();
            var body = {
                username: username,
                password: document.getElementById('cu-pw').value,
                admin: document.getElementById('cu-admin').checked,
                verified: document.getElementById('cu-verified').checked
            };
            if (email) body.email = email;
            if (!username || username.length < 3) {
                document.getElementById('cu-error').innerHTML = '<div class="error-box">Username must be at least 3 characters.</div>';
                return;
            }
            if (!body.password) {
                document.getElementById('cu-error').innerHTML = '<div class="error-box">Password is required.</div>';
                return;
            }
            api.post('/api/admin/users', body).then(function () {
                overlay.style.display = 'none';
                onSuccess();
            }).catch(function (err) {
                document.getElementById('cu-error').innerHTML = '<div class="error-box">' + escHtml(err.message || 'Error') + '</div>';
            });
        };
    }

    // Invites Page 

    function inviteLink(token) {
        return window.location.origin + '/#register/' + token;
    }

    function renderInvites(feedArea) {
        feedArea.innerHTML = '';
        var page = el('div', { class: 'admin-page' });
        page.appendChild(renderSubNav(feedArea, 'invites'));

        var section = el('div', { class: 'admin-section' });
        var titleRow = el('div', { class: 'admin-section-title' });
        titleRow.appendChild(el('span', { text: 'Invites' }));
        var createBtn = el('button', { class: 'btn btn-primary btn-small', text: '+ Create Invite' });
        createBtn.onclick = function () { showCreateInviteDialog(section, function () { renderInvites(feedArea); }); };
        titleRow.appendChild(createBtn);
        section.appendChild(titleRow);

        var feedback = el('div', { class: 'admin-feedback' });
        section.appendChild(feedback);

        var info = el('div', {
            class: 'info-box',
            html: 'Share the <strong>invite link</strong> with recipients. They click it to open the registration form with the token pre-filled.'
        });
        section.appendChild(info);

        var tableWrap = el('div', { class: 'admin-table-wrap', text: 'Loading…' });
        section.appendChild(tableWrap);

        // Email invite section
        var emailSection = el('div', { class: 'admin-section', style: 'margin-top:20px' });
        var emailTitle = el('div', { class: 'admin-section-title', text: 'Send Email Invites' });
        emailSection.appendChild(emailTitle);
        emailSection.appendChild(el('div', {
            class: 'info-box',
            text: 'Enter one email address per line. Each address will receive an invite link by email (requires SMTP to be configured).'
        }));

        var emailFeedback = el('div', { class: 'admin-feedback' });
        emailSection.appendChild(emailFeedback);

        var emailTextarea = el('textarea', { class: 'form-input', rows: '4', placeholder: 'alice@example.com\nbob@example.com', style: 'width:100%;font-family:monospace;font-size:12px' });

        var msgGroup = el('div', { class: 'form-group', style: 'margin-top:10px' });
        msgGroup.appendChild(el('label', { text: 'Personal message (optional)', style: 'display:block;margin-bottom:4px' }));
        msgGroup.appendChild(el('div', { style: 'font-size:0.78rem;color:var(--t-muted);margin-bottom:6px', text: 'Included in the email so the recipient knows who invited them and why.' }));
        var msgTextarea = el('textarea', { class: 'form-input', rows: '3', placeholder: 'Hey Alice, I\'ve set up a private space for our team, looking forward to having you on board!', style: 'width:100%;resize:vertical' });
        msgGroup.appendChild(msgTextarea);

        var expiryRow = el('div', { class: 'form-group', style: 'display:flex;gap:12px;align-items:center;margin-top:8px' });
        expiryRow.appendChild(el('label', { text: 'Expiry (hours):', style: 'white-space:nowrap' }));
        var expiryInput = el('input', { type: 'number', value: '168', min: '1', max: '8760', style: 'width:100px' });
        expiryRow.appendChild(expiryInput);
        var sendBtn = el('button', { class: 'btn btn-primary btn-small', text: 'Send Invites' });
        sendBtn.onclick = function () {
            var lines = emailTextarea.value.split('\n').map(function (s) { return s.trim(); }).filter(Boolean);
            if (!lines.length) { showMsg(emailSection, 'Enter at least one email address.', 'error'); return; }
            sendBtn.disabled = true;
            var body = {
                emails: lines,
                expires_hours: parseInt(expiryInput.value, 10) || 168,
                message: msgTextarea.value.trim() || null
            };
            api.post('/api/admin/invites/email', body)
                .then(function (res) {
                    sendBtn.disabled = false;
                    var msg = 'Sent: ' + res.sent.length;
                    if (res.failed.length) msg += ' | Failed: ' + res.failed.join(', ');
                    showMsg(emailSection, msg, res.failed.length ? 'error' : 'success');
                    emailTextarea.value = '';
                    renderInvites(feedArea); // refresh table
                }).catch(function (err) {
                    sendBtn.disabled = false;
                    showMsg(emailSection, err.message || 'Failed to send invites', 'error');
                });
        };
        expiryRow.appendChild(sendBtn);
        emailSection.appendChild(emailTextarea);
        emailSection.appendChild(msgGroup);
        emailSection.appendChild(expiryRow);

        page.appendChild(section);
        page.appendChild(emailSection);
        feedArea.appendChild(page);

        api.get('/api/admin/invites?limit=200').then(function (data) {
            tableWrap.innerHTML = '';
            if (!data.invites || !data.invites.length) {
                tableWrap.innerHTML = '<div class="empty-state"><p>No invites yet.</p></div>';
                return;
            }

            var table = el('table', { class: 'admin-table' });
            table.innerHTML =
                '<thead><tr>' +
                '<th>Invite Link</th><th>Email</th><th>Expires</th><th>Status</th><th>Actions</th>' +
                '</tr></thead>';
            var tbody = el('tbody');

            data.invites.forEach(function (inv) {
                var expired = new Date(inv.expires_at) < new Date();
                var used = !!inv.used_at;
                var statusLabel = used ? badge('Used', 'status-used') : (expired ? badge('Expired', 'status-inactive') : badge('Active', 'status-active'));
                var link = inviteLink(inv.token);

                var tr = el('tr');
                tr.innerHTML =
                    '<td class="primary"><a href="' + escHtml(link) + '" style="font-size:11px;word-break:break-all">' + escHtml(link) + '</a></td>' +
                    '<td>' + escHtml(inv.email || '-') + '</td>' +
                    '<td>' + fmtDate(inv.expires_at) + '</td>' +
                    '<td>' + statusLabel + '</td>';

                var actions = el('td');
                var actDiv = el('div', { class: 'admin-actions' });

                var copyLinkBtn = el('button', { class: 'btn btn-small', text: 'Copy Link' });
                copyLinkBtn.onclick = function () {
                    navigator.clipboard.writeText(link).then(function () {
                        copyLinkBtn.textContent = 'Copied!';
                        setTimeout(function () { copyLinkBtn.textContent = 'Copy Link'; }, 1500);
                    });
                };
                actDiv.appendChild(copyLinkBtn);

                if (!used) {
                    var revokeBtn = el('button', { class: 'btn btn-small btn-danger', text: 'Revoke' });
                    revokeBtn.onclick = function () {
                        if (!confirm('Revoke this invite?')) return;
                        api.del('/api/admin/invites/' + inv.invite_id).then(function () {
                            renderInvites(feedArea);
                        }).catch(function (err) { showMsg(section, err.message || 'Error', 'error'); });
                    };
                    actDiv.appendChild(revokeBtn);
                }

                actions.appendChild(actDiv);
                tr.appendChild(actions);
                tbody.appendChild(tr);
            });

            table.appendChild(tbody);
            tableWrap.appendChild(table);
        }).catch(function (err) {
            tableWrap.innerHTML = '<div class="error-box">' + escHtml(err.message || 'Failed to load invites') + '</div>';
        });
    }

    function showCreateInviteDialog(container, onSuccess) {
        var overlay = document.getElementById('dialog-overlay');
        var dialog = document.getElementById('dialog');
        overlay.style.display = 'flex';
        dialog.innerHTML =
            '<h3>Create Invite</h3>' +
            '<div class="form-group"><label>Email (optional, binds invite to a specific address)</label><input type="email" id="ci-email" placeholder="Leave blank for open invite"></div>' +
            '<div class="form-group"><label>Expiry (hours, default 168 = 7 days)</label><input type="number" id="ci-hours" value="168" min="1" max="8760"></div>' +
            '<div id="ci-error"></div>' +
            '<div class="dialog-actions">' +
            '<button class="btn" id="ci-cancel">Cancel</button>' +
            '<button class="btn btn-primary" id="ci-submit">Create</button>' +
            '</div>';

        document.getElementById('ci-cancel').onclick = function () { overlay.style.display = 'none'; };
        document.getElementById('ci-submit').onclick = function () {
            var body = {
                email: document.getElementById('ci-email').value.trim() || null,
                expires_hours: parseInt(document.getElementById('ci-hours').value, 10) || 168
            };
            api.post('/api/admin/invites', body).then(function () {
                overlay.style.display = 'none';
                onSuccess();
            }).catch(function (err) {
                document.getElementById('ci-error').innerHTML = '<div class="error-box">' + escHtml(err.message || 'Error') + '</div>';
            });
        };
    }

    // Applications Page 

    function renderApplications(feedArea) {
        renderApplicationsFiltered(feedArea, 'pending');
    }

    function renderApplicationsFiltered(feedArea, statusFilter) {
        feedArea.innerHTML = '';
        var page = el('div', { class: 'admin-page' });
        page.appendChild(renderSubNav(feedArea, 'applications'));

        var section = el('div', { class: 'admin-section' });
        section.appendChild(el('div', { class: 'admin-section-title', text: 'Registration Applications' }));

        // Filter tabs
        var filterNav = el('div', { class: 'admin-subnav', style: 'margin-bottom:12px' });
        ['pending', 'approved', 'denied', 'all'].forEach(function (s) {
            var btn = el('button', {
                class: 'admin-subnav-btn' + (s === statusFilter ? ' active' : ''),
                text: s.charAt(0).toUpperCase() + s.slice(1)
            });
            btn.onclick = function () { renderApplicationsFiltered(feedArea, s); };
            filterNav.appendChild(btn);
        });
        section.appendChild(filterNav);

        var feedback = el('div', { class: 'admin-feedback' });
        section.appendChild(feedback);

        var tableWrap = el('div', { class: 'admin-table-wrap', text: 'Loading…' });
        section.appendChild(tableWrap);
        page.appendChild(section);
        feedArea.appendChild(page);

        var url = '/api/admin/applications?limit=200' + (statusFilter !== 'all' ? '&status=' + statusFilter : '');
        api.get(url).then(function (data) {
            tableWrap.innerHTML = '';
            if (!data.applications || !data.applications.length) {
                tableWrap.innerHTML = '<div class="empty-state"><p>No applications in this category.</p></div>';
                return;
            }

            var table = el('table', { class: 'admin-table' });
            table.innerHTML =
                '<thead><tr>' +
                '<th>Username</th><th>Email</th><th>Name</th><th>Message</th><th>Status</th><th>Submitted</th><th>Actions</th>' +
                '</tr></thead>';
            var tbody = el('tbody');

            data.applications.forEach(function (a) {
                var expired = a.is_expired || new Date(a.expires_at) < new Date();
                var statusCls = a.status === 'approved' ? 'status-active'
                    : a.status === 'denied' || a.status === 'expired' ? 'status-inactive'
                        : 'status-pending';

                var tr = el('tr');
                tr.innerHTML =
                    '<td class="primary">' + escHtml(a.username || '-') + '</td>' +
                    '<td>' + escHtml(a.email || '-') + '</td>' +
                    '<td>' + escHtml(a.display_name || '-') + '</td>' +
                    '<td style="max-width:160px;white-space:normal">' + escHtml(a.message ? a.message.substring(0, 80) + (a.message.length > 80 ? '…' : '') : '-') + '</td>' +
                    '<td>' + badge(a.status, statusCls) + (expired && a.status === 'pending' ? badge('Expired', 'status-inactive') : '') + '</td>' +
                    '<td>' + fmtDate(a.created_at) + '</td>';

                var actions = el('td');
                var actDiv = el('div', { class: 'admin-actions' });

                if (a.status === 'pending') {
                    var approveBtn = el('button', { class: 'btn btn-small', text: 'Approve' });
                    approveBtn.onclick = function () {
                        approveBtn.disabled = true;
                        api.post('/api/admin/applications/' + a.application_id + '/approve').then(function (res) {
                            if (res.setup_link) {
                                showSetupLinkDialog(res.email || a.username, res.setup_link);
                            } else {
                                showMsg(section, 'Approved. Setup email sent to ' + (res.email || a.username), 'success');
                            }
                            renderApplicationsFiltered(feedArea, statusFilter);
                        }).catch(function (err) {
                            approveBtn.disabled = false;
                            showMsg(section, err.message || 'Failed to approve', 'error');
                        });
                    };
                    actDiv.appendChild(approveBtn);

                    var denyBtn = el('button', { class: 'btn btn-small btn-danger', text: 'Deny' });
                    denyBtn.onclick = function () {
                        if (!confirm('Deny application from ' + (a.username || a.email || 'this applicant') + '?')) return;
                        api.post('/api/admin/applications/' + a.application_id + '/deny').then(function () {
                            renderApplicationsFiltered(feedArea, statusFilter);
                        }).catch(function (err) { showMsg(section, err.message || 'Error', 'error'); });
                    };
                    actDiv.appendChild(denyBtn);
                }

                var delBtn = el('button', { class: 'btn btn-small btn-ghost', text: 'Delete' });
                delBtn.onclick = function () {
                    if (!confirm('Delete this application record?')) return;
                    api.del('/api/admin/applications/' + a.application_id).then(function () {
                        renderApplicationsFiltered(feedArea, statusFilter);
                    }).catch(function (err) { showMsg(section, err.message || 'Error', 'error'); });
                };
                actDiv.appendChild(delBtn);

                actions.appendChild(actDiv);
                tr.appendChild(actions);
                tbody.appendChild(tr);
            });

            table.appendChild(tbody);
            tableWrap.appendChild(table);
        }).catch(function (err) {
            tableWrap.innerHTML = '<div class="error-box">' + escHtml(err.message || 'Failed to load applications') + '</div>';
        });
    }

    function showSetupLinkDialog(email, setupLink) {
        var overlay = document.getElementById('dialog-overlay');
        var dialog = document.getElementById('dialog');
        overlay.style.display = 'flex';
        dialog.innerHTML =
            '<h3>Account Approved</h3>' +
            '<p style="margin-bottom:10px">SMTP is not configured. Share this password setup link with <strong>' + escHtml(email) + '</strong> manually:</p>' +
            '<div style="background:rgba(0,0,0,0.3);border-radius:8px;padding:10px;word-break:break-all;font-family:monospace;font-size:12px;margin-bottom:14px">' +
            escHtml(setupLink) + '</div>' +
            '<div id="sl-copied" style="color:var(--c-success);display:none;margin-bottom:8px">Copied to clipboard!</div>' +
            '<div class="dialog-actions">' +
            '<button class="btn" id="sl-copy">Copy Link</button>' +
            '<button class="btn btn-primary" id="sl-close">Done</button>' +
            '</div>';

        document.getElementById('sl-copy').onclick = function () {
            navigator.clipboard.writeText(setupLink).then(function () {
                document.getElementById('sl-copied').style.display = '';
            });
        };
        document.getElementById('sl-close').onclick = function () { overlay.style.display = 'none'; };
    }

    // Site Config Page 

    function renderSiteConfig(feedArea) {
        feedArea.innerHTML = '';
        var page = el('div', { class: 'admin-page' });
        page.appendChild(renderSubNav(feedArea, 'site-config'));

        var section = el('div', { class: 'admin-section', text: 'Loading…' });
        page.appendChild(section);
        feedArea.appendChild(page);

        api.get('/api/admin/site-config').then(function (cfg) {
            section.innerHTML = '';
            var title = el('div', { class: 'admin-section-title', text: 'Site Configuration' });
            section.appendChild(title);

            var feedback = el('div', { class: 'admin-feedback' });
            section.appendChild(feedback);

            function field(labelText, inputEl) {
                var g = el('div', { class: 'form-group' });
                g.appendChild(el('label', { text: labelText }));
                g.appendChild(inputEl);
                return g;
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
                var track = el('span', { class: 'toggle-track' });
                label.appendChild(inp);
                label.appendChild(track);
                row.appendChild(label);
                return row;
            }

            var mediaPath = el('input', { type: 'text', id: 'sc-media-path', value: cfg.media_storage_path || '' });

            section.appendChild(field('Media Storage Path', mediaPath));

            var togglesSection = el('div', { class: 'pref-section', style: 'background:none;padding:0;box-shadow:none;border:none;margin-bottom:14px' });
            togglesSection.appendChild(toggle('Text Posts', 'Allow text-only posts', 'sc-text', cfg.allow_text_posts));
            togglesSection.appendChild(toggle('Image Posts', 'Allow posts with images', 'sc-image', cfg.allow_image_posts));
            togglesSection.appendChild(toggle('Video Posts', 'Allow posts with videos', 'sc-video', cfg.allow_video_posts));
            togglesSection.appendChild(toggle('File Posts', 'Allow posts with file attachments', 'sc-file', cfg.allow_file_posts));
            togglesSection.appendChild(toggle('Message Auto-Delete', 'Automatically delete messages after a delay', 'sc-autodel', cfg.message_auto_delete_enabled));
            section.appendChild(togglesSection);

            var delayInput = el('input', { type: 'number', id: 'sc-del-hours', value: cfg.message_auto_delete_delay_hours || 168, min: '1' });
            section.appendChild(field('Auto-Delete Delay (hours)', delayInput));

            if (cfg.encryption_key_configured) {
                section.appendChild(toggle('Encryption at Rest', 'Encrypt stored media files', 'sc-enc', cfg.encryption_at_rest_enabled));
            } else {
                section.appendChild(el('div', { class: 'info-box', text: 'Encryption at rest is unavailable, set ENCRYPTION_AT_REST_KEY environment variable to enable it.' }));
            }

            // Registration mode
            var regModeSection = el('div', { style: 'margin-top:20px;margin-bottom:14px' });
            regModeSection.appendChild(el('div', { class: 'pref-section-title', text: 'Registration Mode', style: 'font-size:0.875rem;color:var(--t-secondary);margin-bottom:8px' }));

            var modeOpts = [
                { value: 'open', label: 'Open', desc: 'Anyone can register' },
                { value: 'invite_only', label: 'Invite Only', desc: 'Registration requires an invite token or link' },
                { value: 'application', label: 'Application', desc: 'Users submit an application that an admin must approve' }
            ];
            modeOpts.forEach(function (opt) {
                var row = el('label', { style: 'display:flex;align-items:flex-start;gap:10px;margin-bottom:8px;cursor:pointer' });
                var radio = el('input', { type: 'radio', name: 'reg-mode', id: 'rm-' + opt.value, value: opt.value, style: 'margin-top:3px' });
                radio.checked = cfg.registration_mode === opt.value;
                radio.onchange = function () {
                    var timeoutRow = document.getElementById('sc-timeout-row');
                    if (timeoutRow) timeoutRow.style.display = this.value === 'application' ? '' : 'none';
                };
                var textWrap = el('div');
                textWrap.appendChild(el('div', { text: opt.label, style: 'font-weight:500' }));
                textWrap.appendChild(el('div', { text: opt.desc, style: 'font-size:0.8rem;color:var(--t-muted)' }));
                row.appendChild(radio);
                row.appendChild(textWrap);
                regModeSection.appendChild(row);
            });
            section.appendChild(regModeSection);

            var timeoutRow = el('div', { id: 'sc-timeout-row' });
            timeoutRow.style.display = cfg.registration_mode === 'application' ? '' : 'none';
            var timeoutInput = el('input', { type: 'number', id: 'sc-app-timeout', value: cfg.application_timeout_hours || 48, min: '1', max: '8760' });
            timeoutRow.appendChild(field('Application Timeout (hours)', timeoutInput));
            section.appendChild(timeoutRow);

            var inviteSection = el('div', { style: 'margin-top:20px;margin-bottom:14px' });
            inviteSection.appendChild(el('div', { class: 'pref-section-title', text: 'Invite Security', style: 'font-size:0.875rem;color:var(--t-secondary);margin-bottom:8px' }));
            inviteSection.appendChild(toggle(
                'Enforce Invite Email Address',
                'When enabled, registering with an invite link requires using the exact email address the invite was sent to.',
                'sc-enforce-invite-email',
                cfg.enforce_invite_email
            ));
            section.appendChild(inviteSection);

            var resetSection = el('div', { style: 'margin-top:20px;margin-bottom:14px' });
            resetSection.appendChild(el('div', { class: 'pref-section-title', text: 'Password Reset Methods', style: 'font-size:0.875rem;color:var(--t-secondary);margin-bottom:8px' }));
            resetSection.appendChild(toggle(
                'Email Reset',
                'Allow users to receive a password reset link by email. Requires SMTP to be configured.',
                'sc-reset-email',
                cfg.password_reset_email_enabled
            ));
            resetSection.appendChild(toggle(
                'Recovery Key Reset',
                'Allow users to reset their password using a cryptographic recovery key they download from their account settings.',
                'sc-reset-key',
                cfg.password_reset_signing_key_enabled
            ));
            section.appendChild(resetSection);

            var proxySection = el('div', { style: 'margin-top:20px;margin-bottom:14px' });
            proxySection.appendChild(el('div', { class: 'pref-section-title', text: 'Proxy Accounts', style: 'font-size:0.875rem;color:var(--t-secondary);margin-bottom:8px' }));
            proxySection.appendChild(toggle(
                'Enable Proxy User System',
                'Allow automation proxy accounts that can post and message on behalf of users or independently.',
                'sc-proxy-system',
                cfg.proxy_user_system
            ));
            var proxyPiecesInput = el('input', { type: 'number', id: 'sc-proxy-pieces', value: cfg.proxy_rate_limit_pieces || 1, min: '1' });
            proxySection.appendChild(field('Media Upload Rate Limit (files/minute)', proxyPiecesInput));
            var proxyBytesInput = el('input', { type: 'number', id: 'sc-proxy-bytes', value: Math.round((cfg.proxy_rate_limit_bytes || 12582912) / 1048576), min: '1' });
            proxySection.appendChild(field('Media Upload Rate Limit (MB/minute)', proxyBytesInput));
            section.appendChild(proxySection);

            var saveBtn = el('button', { class: 'btn btn-primary', text: 'Save Changes' });
            saveBtn.onclick = function () {
                var selectedMode = document.querySelector('input[name="reg-mode"]:checked');
                var body = {
                    media_storage_path: document.getElementById('sc-media-path').value,
                    allow_text_posts: document.getElementById('sc-text').checked,
                    allow_image_posts: document.getElementById('sc-image').checked,
                    allow_video_posts: document.getElementById('sc-video').checked,
                    allow_file_posts: document.getElementById('sc-file').checked,
                    message_auto_delete_enabled: document.getElementById('sc-autodel').checked,
                    message_auto_delete_delay_hours: parseInt(document.getElementById('sc-del-hours').value, 10) || 168,
                    registration_mode: selectedMode ? selectedMode.value : cfg.registration_mode,
                    application_timeout_hours: parseInt(document.getElementById('sc-app-timeout').value, 10) || 48,
                    enforce_invite_email: document.getElementById('sc-enforce-invite-email').checked,
                    password_reset_email_enabled: document.getElementById('sc-reset-email').checked,
                    password_reset_signing_key_enabled: document.getElementById('sc-reset-key').checked,
                    proxy_user_system: document.getElementById('sc-proxy-system').checked,
                    proxy_rate_limit_pieces: parseInt(document.getElementById('sc-proxy-pieces').value, 10) || 1,
                    proxy_rate_limit_bytes: (parseInt(document.getElementById('sc-proxy-bytes').value, 10) || 12) * 1048576
                };
                if (cfg.encryption_key_configured) {
                    body.encryption_at_rest_enabled = document.getElementById('sc-enc').checked;
                }
                api.put('/api/admin/site-config', body).then(function () {
                    showMsg(section, 'Configuration saved.', 'success');
                }).catch(function (err) {
                    showMsg(section, err.message || 'Failed to save', 'error');
                });
            };
            section.appendChild(saveBtn);

        }).catch(function (err) {
            section.innerHTML = '<div class="error-box">' + escHtml(err.message || 'Failed to load config') + '</div>';
        });
    }

    // Theme Page 

    function renderTheme(feedArea) {
        feedArea.innerHTML = '';
        var page = el('div', { class: 'admin-page' });
        page.appendChild(renderSubNav(feedArea, 'theme'));

        var section = el('div', { class: 'admin-section', text: 'Loading…' });
        page.appendChild(section);
        feedArea.appendChild(page);

        api.get('/api/theme').then(function (theme) {
            section.innerHTML = '';
            var title = el('div', { class: 'admin-section-title', text: 'Theme & Branding' });
            section.appendChild(title);

            var feedback = el('div', { class: 'admin-feedback' });
            section.appendChild(feedback);

            // Live preview bar
            var preview = el('div', { class: 'theme-preview', id: 'theme-preview-bar', text: 'Preview, button text' });
            section.appendChild(preview);

            function colorRow(labelText, id, value) {
                var row = el('div', { class: 'theme-field' });
                row.appendChild(el('label', { text: labelText, for: id }));
                var inp = el('input', { type: 'color', id: id, value: value || '#6366f1' });
                inp.oninput = updatePreview;
                row.appendChild(inp);
                return row;
            }

            function textRow(labelText, id, value, placeholder) {
                var g = el('div', { class: 'form-group' });
                g.appendChild(el('label', { text: labelText }));
                var inp = el('input', { type: 'text', id: id, value: value || '', placeholder: placeholder || '' });
                g.appendChild(inp);
                return g;
            }

            // Colour fields
            var colorsWrap = el('div', { style: 'margin-bottom:16px' });
            colorsWrap.appendChild(colorRow('Accent / Button Colour', 't-accent', theme.color_accent));
            colorsWrap.appendChild(colorRow('Accent Hover', 't-accent-hover', theme.color_button_hover));
            colorsWrap.appendChild(colorRow('Background Layer 1', 't-bg1', theme.color_background));
            colorsWrap.appendChild(colorRow('Success Green', 't-success', theme.color_status_ready));
            colorsWrap.appendChild(colorRow('Warning Amber', 't-warning', theme.color_status_pending));
            colorsWrap.appendChild(colorRow('Danger Red', 't-danger', theme.color_status_removed));
            section.appendChild(colorsWrap);

            // Branding text
            section.appendChild(textRow('Site Title', 't-title', theme.site_title, 'Magnolia'));
            section.appendChild(textRow('Brand Text (header logo)', 't-brand', theme.brand_text, 'Magnolia'));
            section.appendChild(textRow('Hero Title', 't-hero', theme.hero_title, 'Welcome'));
            section.appendChild(textRow('Hero Subtitle', 't-hero-sub', theme.hero_subtitle || '', 'Optional tagline'));

            function updatePreview() {
                var accent = document.getElementById('t-accent').value;
                var bg1 = document.getElementById('t-bg1').value;
                preview.style.background = bg1;
                preview.style.color = '#fff';
                preview.innerHTML =
                    '<span style="background:' + accent + ';padding:6px 14px;border-radius:8px;font-size:12px;font-weight:600">Primary Button</span>' +
                    '<span style="color:' + accent + ';font-size:12px">Accent Text</span>' +
                    '<span style="font-size:12px;opacity:0.6">Background: ' + bg1 + '</span>';
            }
            updatePreview();

            var saveBtn = el('button', { class: 'btn btn-primary', text: 'Save Theme' });
            saveBtn.onclick = function () {
                var accent = document.getElementById('t-accent').value;
                var body = {
                    site_style: 'glassmorphism',
                    color_background: document.getElementById('t-bg1').value,
                    color_main: theme.color_main || '#e2e8f0',
                    color_accent: accent,
                    color_button: accent,
                    color_button_hover: document.getElementById('t-accent-hover').value,
                    color_status_ready: document.getElementById('t-success').value,
                    color_status_pending: document.getElementById('t-warning').value,
                    color_status_removed: document.getElementById('t-danger').value,
                    site_title: document.getElementById('t-title').value || 'Magnolia',
                    brand_icon: theme.brand_icon || '❦',
                    brand_text: document.getElementById('t-brand').value || 'Magnolia',
                    hero_title: document.getElementById('t-hero').value || 'Welcome',
                    hero_subtitle: document.getElementById('t-hero-sub').value || null
                };
                api.put('/api/admin/theme', body).then(function (updated) {
                    theme.applyTheme(updated);
                    showMsg(section, 'Theme saved and applied.', 'success');
                }).catch(function (err) {
                    showMsg(section, err.message || 'Failed to save theme', 'error');
                });
            };
            section.appendChild(saveBtn);
        });
    }

    // XSS helper

    // Email Settings Page 

    function renderEmailSettings(feedArea) {
        feedArea.innerHTML = '';
        var page = el('div', { class: 'admin-page' });
        page.appendChild(renderSubNav(feedArea, 'email'));

        var section = el('div', { class: 'admin-section', text: 'Loading…' });
        page.appendChild(section);
        feedArea.appendChild(page);

        api.get('/api/admin/email-settings').then(function (cfg) {
            section.innerHTML = '';
            section.appendChild(el('div', { class: 'admin-section-title', text: 'Email / SMTP Settings' }));

            var feedback = el('div', { class: 'admin-feedback' });
            section.appendChild(feedback);

            function field(labelText, inputEl, helpText) {
                var g = el('div', { class: 'form-group' });
                g.appendChild(el('label', { text: labelText }));
                g.appendChild(inputEl);
                if (helpText) g.appendChild(el('div', { class: 'form-help', text: helpText }));
                return g;
            }

            var hostInput = el('input', { type: 'text', id: 'em-host', value: cfg.smtp_host || '', placeholder: 'smtp.example.com' });
            var portInput = el('input', { type: 'number', id: 'em-port', value: cfg.smtp_port || 587, min: '1', max: '65535' });
            var userInput = el('input', { type: 'text', id: 'em-user', value: cfg.smtp_username || '', placeholder: 'user@example.com', autocomplete: 'off' });
            var passInput = el('input', { type: 'password', id: 'em-pass', value: '', placeholder: cfg.smtp_password_set ? '(unchanged)' : 'Enter password', autocomplete: 'new-password' });
            var fromInput = el('input', { type: 'text', id: 'em-from', value: cfg.smtp_from || '', placeholder: 'Magnolia <noreply@example.com>' });

            var secureSelect = el('select', { id: 'em-secure' });
            [['tls', 'STARTTLS (port 587)'], ['ssl', 'SSL/TLS (port 465)'], ['none', 'None (port 25)']].forEach(function (opt) {
                var o = el('option', { value: opt[0], text: opt[1] });
                if (cfg.smtp_secure === opt[0]) o.selected = true;
                secureSelect.appendChild(o);
            });

            section.appendChild(field('SMTP Host', hostInput));
            section.appendChild(field('SMTP Port', portInput));
            section.appendChild(field('Security', secureSelect));
            section.appendChild(field('Username', userInput));
            section.appendChild(field('Password', passInput, 'Leave blank to keep the current password.'));
            section.appendChild(field('From Address', fromInput, 'Used as the sender address. Can include a display name.'));

            var saveBtn = el('button', { class: 'btn btn-primary', text: 'Save Changes' });
            saveBtn.onclick = function () {
                var body = {
                    smtp_host: document.getElementById('em-host').value.trim(),
                    smtp_port: parseInt(document.getElementById('em-port').value, 10) || 587,
                    smtp_username: document.getElementById('em-user').value.trim(),
                    smtp_from: document.getElementById('em-from').value.trim(),
                    smtp_secure: document.getElementById('em-secure').value
                };
                var pass = document.getElementById('em-pass').value;
                if (pass) body.smtp_password = pass;

                api.put('/api/admin/email-settings', body).then(function (updated) {
                    cfg = updated;
                    // Refresh password placeholder state
                    passInput.placeholder = updated.smtp_password_set ? '(unchanged)' : 'Enter password';
                    passInput.value = '';
                    showMsg(section, 'Email settings saved.', 'success');
                }).catch(function (err) {
                    showMsg(section, err.message || 'Failed to save', 'error');
                });
            };
            section.appendChild(saveBtn);

        }).catch(function (err) {
            section.innerHTML = '<div class="error-box">' + escHtml(err.message || 'Failed to load email settings') + '</div>';
        });
    }

    // Federation Page 

    function renderFederation(feedArea) {
        feedArea.innerHTML = '';
        var page = el('div', { class: 'admin-page' });
        page.appendChild(renderSubNav(feedArea, 'federation'));

        var tabs = el('div', { class: 'admin-subnav admin-subnav--secondary' });
        var tabSettings = el('button', { class: 'admin-subnav-btn active', text: 'Settings' });
        var tabConnections = el('button', { class: 'admin-subnav-btn', text: 'Connections' });
        tabs.appendChild(tabSettings);
        tabs.appendChild(tabConnections);
        page.appendChild(tabs);

        var body = el('div', { class: 'admin-fed-body' });
        page.appendChild(body);
        feedArea.appendChild(page);

        function showTab(which) {
            if (which === 'settings') {
                tabSettings.classList.add('active');
                tabConnections.classList.remove('active');
                renderFedSettings(body);
            } else {
                tabSettings.classList.remove('active');
                tabConnections.classList.add('active');
                renderFedConnections(body);
            }
        }

        tabSettings.onclick = function () { showTab('settings'); };
        tabConnections.onclick = function () { showTab('connections'); };

        showTab('settings');
    }

    function renderFedSettings(body) {
        body.innerHTML = '';
        var section = el('div', { class: 'admin-section' });
        section.appendChild(el('div', { class: 'admin-section-title', html: '<span>Federation Settings</span>' }));
        var feedback = el('div', { class: 'admin-feedback' });
        section.appendChild(feedback);

        var form = el('div', { class: 'admin-fed-form' });
        section.appendChild(form);
        body.appendChild(section);

        api.get('/api/admin/federation/settings').then(function (d) {
            var s = d.settings || d;

            function chk(id, label, checked) {
                var wrap = el('div', { class: 'form-group form-group--inline' });
                var cb = el('input', { type: 'checkbox', id: id });
                cb.checked = !!checked;
                wrap.appendChild(cb);
                wrap.appendChild(el('label', { 'for': id, text: label }));
                return wrap;
            }
            function num(id, label, value, min, max) {
                var wrap = el('div', { class: 'form-group' });
                wrap.appendChild(el('label', { 'for': id, text: label }));
                var inp = el('input', { type: 'number', id: id, min: min, max: max });
                inp.value = value;
                wrap.appendChild(inp);
                return wrap;
            }

            var cbEnabled = chk('fed-enabled', 'Enable Federation', s.federation_enabled);
            var cbIncoming = chk('fed-incoming', 'Accept Incoming Connections', s.accept_incoming);
            var cbShareConn = chk('fed-share-conn', 'Share Connection List with Peers', s.share_connections);
            var fMaxConn = num('fed-max-conn', 'Max Connections', s.max_connections, 0, 1000);
            var fRelayDepth = num('fed-relay-depth', 'Relay Depth', s.relay_depth, 0, 10);

            form.appendChild(cbEnabled);
            form.appendChild(cbIncoming);
            form.appendChild(cbShareConn);
            form.appendChild(fMaxConn);
            form.appendChild(fRelayDepth);

            var saveBtn = el('button', { class: 'btn btn-primary', text: 'Save' });
            saveBtn.onclick = function () {
                var payload = {
                    federation_enabled: cbEnabled.querySelector('input').checked,
                    accept_incoming: cbIncoming.querySelector('input').checked,
                    share_connections: cbShareConn.querySelector('input').checked,
                    max_connections: parseInt(fMaxConn.querySelector('input').value, 10) || 0,
                    relay_depth: parseInt(fRelayDepth.querySelector('input').value, 10) || 0
                };
                api.put('/api/admin/federation/settings', payload).then(function () {
                    showMsg(section, 'Saved.', 'ok');
                }).catch(function (e) {
                    showMsg(section, 'Error: ' + e.message, 'error');
                });
            };
            form.appendChild(saveBtn);
        }).catch(function (e) {
            showMsg(section, 'Failed to load settings: ' + e.message, 'error');
        });
    }

    var _fedConnHubInterval = null;

    function renderFedConnections(body) {
        // Stop any previous polling interval from a prior render of this tab.
        if (_fedConnHubInterval) { clearInterval(_fedConnHubInterval); _fedConnHubInterval = null; }
        body.innerHTML = '';
        var section = el('div', { class: 'admin-section' });
        var titleRow = el('div', { class: 'admin-section-title' });
        titleRow.appendChild(el('span', { text: 'Server Connections' }));
        var addBtn = el('button', { class: 'btn btn-primary btn-small', text: '+ Connect' });
        titleRow.appendChild(addBtn);
        section.appendChild(titleRow);
        var feedback = el('div', { class: 'admin-feedback' });
        section.appendChild(feedback);
        var tableWrap = el('div', { class: 'admin-table-wrap', text: 'Loading…' });
        section.appendChild(tableWrap);
        body.appendChild(section);

        var discSection = el('div', { class: 'admin-section' });
        discSection.appendChild(el('div', { class: 'admin-section-title', html: '<span>Discovery Hints</span>' }));
        var discFeedback = el('div', { class: 'admin-feedback' });
        discSection.appendChild(discFeedback);
        var discWrap = el('div', { class: 'admin-table-wrap', text: 'Loading…' });
        discSection.appendChild(discWrap);
        body.appendChild(discSection);

        addBtn.onclick = function () {
            showConnectDialog(section, function () { loadConnections(); });
        };

        function statusBadge(status) {
            var map = {
                'pending_in': badge('Pending (incoming)', 'badge-warn'),
                'pending_out': badge('Pending (outgoing)', 'badge-neutral'),
                'active': badge('Active', 'badge-ok'),
                'rejected': badge('Rejected', 'badge-err'),
                'revoked': badge('Revoked', 'badge-err')
            };
            return map[status] || badge(escHtml(status), 'badge-neutral');
        }

        function wsBadge(wsState) {
            if (!wsState) return badge('-', 'badge-neutral');
            var map = {
                'connected': badge('WS Live', 'badge-ok'),
                'reconnecting': badge('Reconnecting', 'badge-warn'),
                'peer_offline': badge('Peer Offline', 'badge-err'),
                'gone': badge('Gone', 'badge-err')
            };
            return map[wsState] || badge(escHtml(wsState), 'badge-neutral');
        }

        var hubStatusByAddress = {};

        function loadHubStatus() {
            api.get('/api/admin/federation/hub-status').then(function (data) {
                hubStatusByAddress = {};
                var peers = data.peers || [];
                peers.forEach(function (p) {
                    hubStatusByAddress[p.address] = p;
                });
                // Update any visible WS status cells without re-rendering
                document.querySelectorAll('[data-ws-address]').forEach(function (cell) {
                    var addr = cell.getAttribute('data-ws-address');
                    var peer = hubStatusByAddress[addr];
                    cell.innerHTML = wsBadge(peer ? peer.state : null);
                    if (peer && peer.offline_since) {
                        cell.title = 'Offline since: ' + fmtDate(peer.offline_since);
                    } else {
                        cell.title = '';
                    }
                });
            }).catch(function () { /* silently ignore */ });
        }

        function startHubStatusPolling() {
            if (_fedConnHubInterval) clearInterval(_fedConnHubInterval);
            loadHubStatus();
            _fedConnHubInterval = setInterval(function () {
                // Stop polling if this body is no longer in the DOM
                if (!document.contains(body)) {
                    clearInterval(_fedConnHubInterval);
                    _fedConnHubInterval = null;
                    return;
                }
                loadHubStatus();
            }, 10000);
        }

        function loadConnections() {
            tableWrap.innerHTML = 'Loading…';
            api.get('/api/admin/federation/connections').then(function (data) {
                tableWrap.innerHTML = '';
                var list = data.connections || data || [];
                if (!list.length) {
                    tableWrap.innerHTML = '<div class="empty-state"><p>No connections.</p></div>';
                    return;
                }
                var table = el('table', { class: 'admin-table' });
                table.innerHTML =
                    '<thead><tr>' +
                    '<th>Address</th><th>Name</th><th>Status</th><th>WS</th><th>Connected</th><th>Actions</th>' +
                    '</tr></thead>';
                var tbody = el('tbody');
                list.forEach(function (c) {
                    var tr = el('tr');
                    var wsPeer = hubStatusByAddress[c.address];
                    tr.innerHTML =
                        '<td>' + escHtml(c.address) + '</td>' +
                        '<td>' + escHtml(c.display_name || '') + '</td>' +
                        '<td>' + statusBadge(c.status) + '</td>' +
                        '<td data-ws-address="' + escHtml(c.address) + '">' +
                        wsBadge(wsPeer ? wsPeer.state : null) +
                        '</td>' +
                        '<td>' + fmtDate(c.connected_at) + '</td>' +
                        '<td></td>';
                    var wsCell = tr.querySelector('[data-ws-address]');
                    if (wsPeer && wsPeer.offline_since) {
                        wsCell.title = 'Offline since: ' + fmtDate(wsPeer.offline_since);
                    }
                    var actions = tr.querySelector('td:last-child');

                    if (c.status === 'pending_in') {
                        var acceptBtn = el('button', { class: 'btn btn-small btn-primary', text: 'Accept' });
                        acceptBtn.onclick = function () {
                            api.post('/api/admin/federation/connections/' + c.id + '/accept', {}).then(function () {
                                showMsg(section, 'Connection accepted.', 'ok');
                                loadConnections();
                            }).catch(function (e) { showMsg(section, 'Error: ' + e.message, 'error'); });
                        };
                        var rejectBtn = el('button', { class: 'btn btn-small btn-danger', text: 'Reject' });
                        rejectBtn.onclick = function () {
                            api.post('/api/admin/federation/connections/' + c.id + '/reject', {}).then(function () {
                                showMsg(section, 'Connection rejected.', 'ok');
                                loadConnections();
                            }).catch(function (e) { showMsg(section, 'Error: ' + e.message, 'error'); });
                        };
                        actions.appendChild(acceptBtn);
                        actions.appendChild(rejectBtn);
                    }

                    if (c.status !== 'revoked' && c.status !== 'rejected') {
                        var editBtn = el('button', { class: 'btn btn-small', text: 'Edit' });
                        editBtn.onclick = (function (conn) {
                            return function () {
                                showEditConnectionDialog(conn, section, function () { loadConnections(); });
                            };
                        })(c);
                        actions.appendChild(editBtn);
                    }

                    var isTerminal = (c.status === 'pending_out' || c.status === 'rejected' || c.status === 'revoked');
                    var revokeLabel = isTerminal ? 'Remove' : 'Revoke';
                    var revokeConfirm = isTerminal
                        ? 'Remove connection record for ' + c.address + '?'
                        : 'Revoke active connection with ' + c.address + '?';
                    var revokeBtn = el('button', { class: 'btn btn-small btn-danger', text: revokeLabel });
                    revokeBtn.onclick = (function (cId, cAddr, confirmMsg) {
                        return function () {
                            if (!confirm(confirmMsg)) return;
                            api.del('/api/admin/federation/connections/' + cId).then(function () {
                                loadConnections();
                            }).catch(function (e) { showMsg(section, 'Error: ' + e.message, 'error'); });
                        };
                    })(c.id, c.address, revokeConfirm);
                    actions.appendChild(revokeBtn);

                    tbody.appendChild(tr);
                });
                table.appendChild(tbody);
                tableWrap.appendChild(table);
            }).catch(function (e) {
                tableWrap.innerHTML = '<div class="error-box">Failed to load: ' + escHtml(e.message) + '</div>';
            });
        }

        function loadDiscovery() {
            discWrap.innerHTML = 'Loading…';
            api.get('/api/admin/federation/discovery').then(function (data) {
                discWrap.innerHTML = '';
                var list = data.hints || data || [];
                if (!list.length) {
                    discWrap.innerHTML = '<div class="empty-state"><p>No discovery hints.</p></div>';
                    return;
                }
                var table = el('table', { class: 'admin-table' });
                table.innerHTML =
                    '<thead><tr><th>Address</th><th>Via</th><th>Seen</th><th>Actions</th></tr></thead>';
                var tbody = el('tbody');
                list.forEach(function (h) {
                    var tr = el('tr');
                    tr.innerHTML =
                        '<td>' + escHtml(h.address) + '</td>' +
                        '<td>' + escHtml(h.via_server || '') + '</td>' +
                        '<td>' + fmtDate(h.created_at) + '</td>' +
                        '<td></td>';
                    var actions = tr.querySelector('td:last-child');
                    var connectBtn = el('button', { class: 'btn btn-small btn-primary', text: 'Connect' });
                    connectBtn.onclick = function () {
                        showConnectDialog(section, function () { loadConnections(); }, h.address);
                    };
                    var dismissBtn = el('button', { class: 'btn btn-small', text: 'Dismiss' });
                    dismissBtn.onclick = function () {
                        api.del('/api/admin/federation/discovery/' + h.id).then(function () {
                            loadDiscovery();
                        }).catch(function (e) { showMsg(discSection, 'Error: ' + e.message, 'error'); });
                    };
                    actions.appendChild(connectBtn);
                    actions.appendChild(dismissBtn);
                    tbody.appendChild(tr);
                });
                table.appendChild(tbody);
                discWrap.appendChild(table);
            }).catch(function (e) {
                discWrap.innerHTML = '<div class="error-box">Failed to load: ' + escHtml(e.message) + '</div>';
            });
        }

        startHubStatusPolling();
        loadConnections();
        loadDiscovery();
    }

    function showConnectDialog(parent, onDone, prefillAddress) {
        var overlay = el('div', { class: 'dialog-overlay' });
        var dialog = el('div', { class: 'dialog' });
        dialog.appendChild(el('h3', { text: 'Connect to Server' }));

        var addrGroup = el('div', { class: 'form-group' });
        addrGroup.appendChild(el('label', { 'for': 'fed-connect-addr', text: 'Server Address (https://…)' }));
        var addrInp = el('input', { type: 'text', id: 'fed-connect-addr', placeholder: 'https://other.example' });
        if (prefillAddress) addrInp.value = prefillAddress;
        addrGroup.appendChild(addrInp);

        var nameGroup = el('div', { class: 'form-group' });
        nameGroup.appendChild(el('label', { 'for': 'fed-connect-name', text: 'Display Name (optional)' }));
        var nameInp = el('input', { type: 'text', id: 'fed-connect-name', placeholder: 'Friendly name for this server' });
        nameGroup.appendChild(nameInp);

        var shareGroup = el('div', { class: 'form-group form-group--inline' });
        var shareCb = el('input', { type: 'checkbox', id: 'fed-connect-share' });
        shareCb.checked = true;
        shareGroup.appendChild(shareCb);
        shareGroup.appendChild(el('label', { 'for': 'fed-connect-share', text: 'Share our connection list' }));

        var btnRow = el('div', { class: 'dialog-actions' });
        var cancelBtn = el('button', { class: 'btn', text: 'Cancel' });
        var submitBtn = el('button', { class: 'btn btn-primary', text: 'Send Request' });
        btnRow.appendChild(cancelBtn);
        btnRow.appendChild(submitBtn);

        var err = el('div', { class: 'admin-feedback' });
        dialog.appendChild(addrGroup);
        dialog.appendChild(nameGroup);
        dialog.appendChild(shareGroup);
        dialog.appendChild(err);
        dialog.appendChild(btnRow);
        overlay.appendChild(dialog);
        document.body.appendChild(overlay);

        cancelBtn.onclick = function () { document.body.removeChild(overlay); };

        submitBtn.onclick = function () {
            var addr = addrInp.value.trim();
            if (!addr) { err.className = 'admin-feedback error-box'; err.textContent = 'Address required.'; return; }
            submitBtn.disabled = true;
            var payload = { address: addr, we_share_connections: shareCb.checked };
            var name = nameInp.value.trim();
            if (name) payload.display_name = name;
            api.post('/api/admin/federation/connections', payload).then(function () {
                document.body.removeChild(overlay);
                showMsg(parent, 'Connection request sent.', 'ok');
                onDone();
            }).catch(function (e) {
                submitBtn.disabled = false;
                err.className = 'admin-feedback error-box';
                err.textContent = 'Error: ' + e.message;
            });
        };
    }

    function showEditConnectionDialog(conn, parent, onDone) {
        var overlay = el('div', { class: 'dialog-overlay' });
        var dialog = el('div', { class: 'dialog' });
        dialog.appendChild(el('h3', { text: 'Edit Connection' }));

        var nameGroup = el('div', { class: 'form-group' });
        nameGroup.appendChild(el('label', { 'for': 'fed-edit-name', text: 'Display Name' }));
        var nameInp = el('input', { type: 'text', id: 'fed-edit-name', placeholder: 'Friendly name' });
        nameInp.value = conn.display_name || '';
        nameGroup.appendChild(nameInp);

        var notesGroup = el('div', { class: 'form-group' });
        notesGroup.appendChild(el('label', { 'for': 'fed-edit-notes', text: 'Notes' }));
        var notesInp = el('textarea', { id: 'fed-edit-notes', rows: 3 });
        notesInp.value = conn.notes || '';
        notesGroup.appendChild(notesInp);

        var btnRow = el('div', { class: 'dialog-actions' });
        var cancelBtn = el('button', { class: 'btn', text: 'Cancel' });
        var saveBtn = el('button', { class: 'btn btn-primary', text: 'Save' });
        btnRow.appendChild(cancelBtn);
        btnRow.appendChild(saveBtn);

        var err = el('div', { class: 'admin-feedback' });
        dialog.appendChild(nameGroup);
        dialog.appendChild(notesGroup);
        dialog.appendChild(err);
        dialog.appendChild(btnRow);
        overlay.appendChild(dialog);
        document.body.appendChild(overlay);

        cancelBtn.onclick = function () { document.body.removeChild(overlay); };

        saveBtn.onclick = function () {
            saveBtn.disabled = true;
            api.put('/api/admin/federation/connections/' + conn.id, {
                display_name: nameInp.value.trim() || null,
                notes: notesInp.value.trim() || null
            }).then(function () {
                document.body.removeChild(overlay);
                showMsg(parent, 'Saved.', 'ok');
                onDone();
            }).catch(function (e) {
                saveBtn.disabled = false;
                err.className = 'admin-feedback error-box';
                err.textContent = 'Error: ' + e.message;
            });
        };
    }

    function escHtml(s) {
        if (!s) return '';
        return String(s)
            .replace(/&/g, '&amp;')
            .replace(/</g, '&lt;')
            .replace(/>/g, '&gt;')
            .replace(/"/g, '&quot;');
    }

    // Public API 

    // STUN/TURN Servers Page

    function stunStatusBadge(status) {
        if (status === 'ok') return badge('OK', 'status-active');
        if (status === 'unreachable') return badge('Unreachable', 'status-inactive');
        return badge('Unknown', 'status-pending');
    }

    function renderStunServers(feedArea) {
        feedArea.innerHTML = '';
        var page = el('div', { class: 'admin-page' });
        page.appendChild(renderSubNav(feedArea, 'stun'));

        var section = el('div', { class: 'admin-section' });
        var titleRow = el('div', { class: 'admin-section-title' });
        titleRow.appendChild(el('span', { text: 'STUN / TURN Servers' }));
        var addBtn = el('button', { class: 'btn btn-primary btn-small', text: '+ Add Server' });
        addBtn.onclick = function () { showAddStunDialog(section, function () { renderStunServers(feedArea); }); };
        titleRow.appendChild(addBtn);
        section.appendChild(titleRow);

        section.appendChild(el('div', {
            class: 'info-box',
            text: 'These servers are used for WebRTC peer connections. ' +
                  'The health-check service probes each server every 5 minutes and flags unreachable ones. ' +
                  'Unreachable servers are excluded from ICE config responses automatically.'
        }));

        var feedback = el('div', { class: 'admin-feedback' });
        section.appendChild(feedback);

        // Embedded TURN server card
        var embeddedCard = el('div', { class: 'embedded-turn-card' });
        section.appendChild(embeddedCard);
        api.get('/api/admin/embedded-turn').then(function (t) {
            if (!t.enabled) {
                embeddedCard.innerHTML =
                    '<div class="embedded-turn-header">' +
                    '<span class="embedded-turn-title">Embedded TURN Server</span>' +
                    badge('Disabled', 'status-inactive') +
                    '</div>' +
                    '<p class="embedded-turn-desc">Set <code>TURN_ENABLED=true</code>, <code>TURN_EXTERNAL_IP</code>, ' +
                    'and restart the server to enable the built-in TURN relay.</p>';
                return;
            }
            embeddedCard.innerHTML =
                '<div class="embedded-turn-header">' +
                '<span class="embedded-turn-title">Embedded TURN Server</span>' +
                badge('Running', 'status-active') +
                '</div>' +
                '<p class="embedded-turn-desc">Your server is acting as a STUN/TURN relay. ' +
                'Users can add these addresses to their WebRTC configuration, or you can add them to the list below. ' +
                'TURN credentials are generated automatically per-user - do not enter static credentials when adding to the list.</p>' +
                '<div class="embedded-turn-urls">' +
                '<div class="embedded-turn-row">' +
                '<span class="embedded-turn-label">STUN (no auth)</span>' +
                '<code class="embedded-turn-url" id="eturn-stun">' + escHtml(t.stun_url) + '</code>' +
                '<button class="btn btn-small" data-copy="eturn-stun">Copy</button>' +
                '</div>' +
                '<div class="embedded-turn-row">' +
                '<span class="embedded-turn-label">TURN / UDP</span>' +
                '<code class="embedded-turn-url" id="eturn-udp">' + escHtml(t.turn_url) + '</code>' +
                '<button class="btn btn-small" data-copy="eturn-udp">Copy</button>' +
                '</div>' +
                '<div class="embedded-turn-row">' +
                '<span class="embedded-turn-label">TURN / TCP</span>' +
                '<code class="embedded-turn-url" id="eturn-tcp">' + escHtml(t.turn_url_tcp) + '</code>' +
                '<button class="btn btn-small" data-copy="eturn-tcp">Copy</button>' +
                '</div>' +
                '</div>' +
                '<p class="embedded-turn-note">TURN credentials are time-limited and derived from the session - ' +
                'they are appended automatically to ICE config for logged-in users.</p>';
            embeddedCard.querySelectorAll('[data-copy]').forEach(function (btn) {
                btn.onclick = function () {
                    var id = btn.getAttribute('data-copy');
                    var text = document.getElementById(id).textContent;
                    navigator.clipboard.writeText(text).then(function () {
                        btn.textContent = 'Copied!';
                        setTimeout(function () { btn.textContent = 'Copy'; }, 1500);
                    });
                };
            });
        }).catch(function () { embeddedCard.style.display = 'none'; });

        var tableWrap = el('div', { class: 'admin-table-wrap', text: 'Loading…' });
        section.appendChild(tableWrap);
        page.appendChild(section);
        feedArea.appendChild(page);

        api.get('/api/admin/stun-servers').then(function (data) {
            tableWrap.innerHTML = '';
            if (!data || !data.length) {
                tableWrap.innerHTML = '<div class="empty-state"><p>No STUN/TURN servers configured.</p></div>';
                return;
            }
            var table = el('table', { class: 'admin-table' });
            table.innerHTML =
                '<thead><tr>' +
                '<th>URL</th><th>Username</th><th>Credential</th>' +
                '<th>Enabled</th><th>Status</th><th>Last Checked</th><th>Actions</th>' +
                '</tr></thead>';
            var tbody = el('tbody');
            data.forEach(function (s) {
                var tr = el('tr');
                tr.innerHTML =
                    '<td>' + escHtml(s.url) + '</td>' +
                    '<td>' + (s.username ? escHtml(s.username) : '<span style="opacity:.4">-</span>') + '</td>' +
                    '<td>' + (s.has_credential ? '<span style="opacity:.5">••••••</span>' : '<span style="opacity:.4">-</span>') + '</td>' +
                    '<td>' + (s.enabled ? badge('Yes', 'status-active') : badge('No', 'status-inactive')) + '</td>' +
                    '<td>' + stunStatusBadge(s.last_status) + '</td>' +
                    '<td style="font-size:11px;opacity:.6">' + (s.last_checked_at ? s.last_checked_at.replace('T', ' ').slice(0, 19) : '-') + '</td>' +
                    '<td></td>';
                var actTd = tr.querySelector('td:last-child');
                var editBtn = el('button', { class: 'btn btn-small', text: 'Edit' });
                editBtn.onclick = function () { showEditStunDialog(s, section, function () { renderStunServers(feedArea); }); };
                var delBtn = el('button', { class: 'btn btn-small btn-danger', text: 'Delete', style: 'margin-left:4px' });
                delBtn.onclick = function () {
                    if (!confirm('Delete ' + s.url + '?')) return;
                    api.del('/api/admin/stun-servers/' + s.id)
                        .then(function () { renderStunServers(feedArea); })
                        .catch(function (err) { showMsg(section, err.message || 'Delete failed', 'error'); });
                };
                actTd.appendChild(editBtn);
                actTd.appendChild(delBtn);
                tbody.appendChild(tr);
            });
            table.appendChild(tbody);
            tableWrap.appendChild(table);
        }).catch(function (err) {
            tableWrap.innerHTML = '<div class="error-box">' + escHtml(err.message) + '</div>';
        });
    }

    function showAddStunDialog(section, onDone) {
        var overlay = document.getElementById('dialog-overlay');
        var dialog = document.getElementById('dialog');
        overlay.style.display = 'flex';
        dialog.innerHTML =
            '<h3 style="margin-top:0">Add STUN/TURN Server</h3>' +
            '<div class="form-group"><label>URL</label>' +
            '<input id="stun-url" class="form-input" placeholder="stun:stun.example.com:3478"></div>' +
            '<div class="form-group"><label>Username <span style="opacity:.5">(TURN only)</span></label>' +
            '<input id="stun-user" class="form-input" placeholder="optional"></div>' +
            '<div class="form-group"><label>Credential <span style="opacity:.5">(TURN only)</span></label>' +
            '<input id="stun-cred" class="form-input" type="password" placeholder="optional"></div>' +
            '<div class="form-group"><label><input id="stun-enabled" type="checkbox" checked> Enabled</label></div>' +
            '<div id="stun-err" class="error-box" style="display:none"></div>' +
            '<div class="dialog-actions">' +
            '<button id="stun-cancel" class="btn btn-secondary">Cancel</button>' +
            '<button id="stun-save" class="btn btn-primary">Add</button>' +
            '</div>';
        document.getElementById('stun-cancel').onclick = function () { overlay.style.display = 'none'; };
        document.getElementById('stun-save').onclick = function () {
            var url = document.getElementById('stun-url').value.trim();
            if (!url) { document.getElementById('stun-err').textContent = 'URL is required.'; document.getElementById('stun-err').style.display = ''; return; }
            var body = {
                url: url,
                username: document.getElementById('stun-user').value.trim() || null,
                credential: document.getElementById('stun-cred').value || null,
                enabled: document.getElementById('stun-enabled').checked
            };
            api.post('/api/admin/stun-servers', body)
                .then(function () { overlay.style.display = 'none'; onDone(); showMsg(section, 'Server added.', 'success'); })
                .catch(function (err) { document.getElementById('stun-err').textContent = err.message || 'Failed'; document.getElementById('stun-err').style.display = ''; });
        };
    }

    function showEditStunDialog(s, section, onDone) {
        var overlay = document.getElementById('dialog-overlay');
        var dialog = document.getElementById('dialog');
        overlay.style.display = 'flex';
        dialog.innerHTML =
            '<h3 style="margin-top:0">Edit STUN/TURN Server</h3>' +
            '<div class="form-group"><label>URL</label>' +
            '<input id="stun-url" class="form-input" value="' + escHtml(s.url) + '"></div>' +
            '<div class="form-group"><label>Username <span style="opacity:.5">(TURN only)</span></label>' +
            '<input id="stun-user" class="form-input" value="' + escHtml(s.username || '') + '"></div>' +
            '<div class="form-group"><label>Credential <span style="opacity:.5">(leave blank to keep existing)</span></label>' +
            '<input id="stun-cred" class="form-input" type="password" placeholder="unchanged"></div>' +
            '<div class="form-group"><label><input id="stun-enabled" type="checkbox"' + (s.enabled ? ' checked' : '') + '> Enabled</label></div>' +
            '<div id="stun-err" class="error-box" style="display:none"></div>' +
            '<div class="dialog-actions">' +
            '<button id="stun-cancel" class="btn btn-secondary">Cancel</button>' +
            '<button id="stun-save" class="btn btn-primary">Save</button>' +
            '</div>';
        document.getElementById('stun-cancel').onclick = function () { overlay.style.display = 'none'; };
        document.getElementById('stun-save').onclick = function () {
            var url = document.getElementById('stun-url').value.trim();
            if (!url) { document.getElementById('stun-err').textContent = 'URL is required.'; document.getElementById('stun-err').style.display = ''; return; }
            var credVal = document.getElementById('stun-cred').value;
            var body = {
                url: url,
                username: document.getElementById('stun-user').value.trim() || null,
                credential: credVal || null,
                enabled: document.getElementById('stun-enabled').checked
            };
            api.patch('/api/admin/stun-servers/' + s.id, body)
                .then(function () { overlay.style.display = 'none'; onDone(); showMsg(section, 'Server updated.', 'success'); })
                .catch(function (err) { document.getElementById('stun-err').textContent = err.message || 'Failed'; document.getElementById('stun-err').style.display = ''; });
        };
    }

    // Proxy Accounts Page

    function generateHmacKey() {
        var bytes = new Uint8Array(32);
        crypto.getRandomValues(bytes);
        return Array.from(bytes).map(function (b) { return b.toString(16).padStart(2, '0'); }).join('');
    }

    function downloadText(filename, text) {
        var blob = new Blob([text], { type: 'text/plain' });
        var url = URL.createObjectURL(blob);
        var a = document.createElement('a');
        a.href = url;
        a.download = filename;
        a.click();
        URL.revokeObjectURL(url);
    }

    function renderProxies(feedArea) {
        feedArea.innerHTML = '';
        var page = el('div', { class: 'admin-page' });
        page.appendChild(renderSubNav(feedArea, 'proxies'));

        var section = el('div', { class: 'admin-section' });
        var titleRow = el('div', { class: 'admin-section-title' });
        titleRow.appendChild(el('span', { text: 'Proxy Accounts' }));
        var createBtn = el('button', { class: 'btn btn-primary btn-small', text: '+ Create Proxy' });
        createBtn.onclick = function () { showCreateProxyDialog(section, function () { loadProxyList(); }); };
        titleRow.appendChild(createBtn);
        section.appendChild(titleRow);

        var tableWrap = el('div', { class: 'table-wrap' });
        section.appendChild(tableWrap);
        page.appendChild(section);
        feedArea.appendChild(page);

        function loadProxyList() {
            tableWrap.innerHTML = 'Loading…';
            api.get('/api/admin/proxies').then(function (list) {
                tableWrap.innerHTML = '';
                if (!list.length) {
                    tableWrap.appendChild(el('div', { class: 'info-box', text: 'No proxy accounts yet.' }));
                    return;
                }
                var table = el('table', { class: 'admin-table' });
                var thead = el('thead');
                thead.innerHTML = '<tr><th>Username</th><th>Display Name</th><th>Paired User</th><th>Status</th><th>Pw</th><th>HMAC</th><th>E2E</th><th>Actions</th></tr>';
                table.appendChild(thead);
                var tbody = el('tbody');
                list.forEach(function (p) {
                    var tr = el('tr');
                    var avatarHtml = p.avatar_url
                        ? '<img src="' + escHtml(p.avatar_url) + '" style="width:28px;height:28px;border-radius:50%;object-fit:cover;vertical-align:middle;margin-right:6px">'
                        : '';
                    var nameTd = el('td');
                    nameTd.innerHTML = avatarHtml + escHtml(p.username) + '<br><span class="admin-uuid">' + escHtml(p.proxy_id) + '</span>';
                    tr.appendChild(nameTd);
                    tr.appendChild(el('td', { text: p.display_name || '' }));
                    var pairedTd = el('td');
                    pairedTd.innerHTML = p.paired_user_id
                        ? '<span class="admin-uuid">' + escHtml(p.paired_user_id) + '</span>'
                        : '<span style="opacity:.5">Unattached</span>';
                    tr.appendChild(pairedTd);
                    var statusTd = el('td');
                    statusTd.innerHTML = p.active ? badge('Active', 'status-active') : badge('Off', 'status-inactive');
                    tr.appendChild(statusTd);
                    var pwTd = el('td');
                    pwTd.innerHTML = p.has_password ? badge('✓', 'status-active') : badge('', 'status-inactive');
                    tr.appendChild(pwTd);
                    var hmacTd = el('td');
                    hmacTd.innerHTML = p.has_hmac_key
                        ? badge('✓', 'status-active') + '<span style="font-size:0.7rem;opacity:.6;margin-left:4px;font-family:monospace">' + escHtml(p.hmac_key_fingerprint || '') + '…</span>'
                        : badge('', 'status-inactive');
                    tr.appendChild(hmacTd);
                    var e2eTd = el('td');
                    e2eTd.innerHTML = p.has_e2e_key ? badge('✓', 'status-active') : badge('', 'status-inactive');
                    tr.appendChild(e2eTd);
                    var actTd = el('td', { style: 'white-space:nowrap' });

                    var editBtn = el('button', { class: 'btn btn-small', text: 'Edit' });
                    editBtn.onclick = (function (proxy) {
                        return function () { showEditProxyDialog(proxy, section, function () { loadProxyList(); }); };
                    })(p);

                    var hmacBtn = el('button', { class: 'btn btn-small', text: 'HMAC Key', style: 'margin-left:4px' });
                    hmacBtn.onclick = (function (proxy) {
                        return function () { showProxyHmacKeyDialog(proxy, section); };
                    })(p);

                    var rateLimitBtn = el('button', { class: 'btn btn-small', text: 'Rate Limit', style: 'margin-left:4px' });
                    rateLimitBtn.onclick = (function (proxy) {
                        return function () { showProxyRateLimitDialog(proxy, section); };
                    })(p);

                    var pwBtn = el('button', { class: 'btn btn-small', text: 'Password', style: 'margin-left:4px' });
                    pwBtn.onclick = (function (proxy) {
                        return function () { showSetProxyPasswordDialog(proxy.proxy_id, section); };
                    })(p);

                    var toggleBtn = el('button', { class: 'btn btn-small ' + (p.active ? 'btn-danger' : ''), text: p.active ? 'Disable' : 'Enable', style: 'margin-left:4px' });
                    toggleBtn.onclick = (function (proxy) {
                        return function () {
                            api.patch('/api/admin/proxies/' + proxy.proxy_id, { active: !proxy.active })
                                .then(function () { loadProxyList(); })
                                .catch(function (err) { showMsg(section, err.message || 'Failed', 'error'); });
                        };
                    })(p);

                    var delBtn = el('button', { class: 'btn btn-small btn-danger', text: 'Delete', style: 'margin-left:4px' });
                    delBtn.onclick = (function (proxy) {
                        return function () {
                            if (!confirm('Delete proxy @' + proxy.username + '? This cannot be undone.')) return;
                            api.del('/api/admin/proxies/' + proxy.proxy_id)
                                .then(function () { loadProxyList(); })
                                .catch(function (err) { showMsg(section, err.message || 'Delete failed', 'error'); });
                        };
                    })(p);

                    actTd.appendChild(editBtn);
                    actTd.appendChild(hmacBtn);
                    actTd.appendChild(rateLimitBtn);
                    actTd.appendChild(pwBtn);
                    actTd.appendChild(toggleBtn);
                    actTd.appendChild(delBtn);
                    tr.appendChild(actTd);
                    tbody.appendChild(tr);
                });
                table.appendChild(tbody);
                tableWrap.appendChild(table);
            }).catch(function (err) {
                tableWrap.innerHTML = '<div class="error-box">' + escHtml(err.message || 'Failed to load') + '</div>';
            });
        }

        loadProxyList();
    }

    function showCreateProxyDialog(section, onDone) {
        var overlay = document.getElementById('dialog-overlay');
        var dialog = document.getElementById('dialog');
        overlay.style.display = 'flex';
        dialog.innerHTML =
            '<h3 style="margin-top:0">Create Proxy Account</h3>' +
            '<div class="form-group"><label>Username</label>' +
            '<input id="px-username" class="form-input" placeholder="proxy_bot"></div>' +
            '<div class="form-group"><label>Pair to User ID <span style="opacity:.5">(optional)</span></label>' +
            '<input id="px-user-id" class="form-input" placeholder="leave blank for unattached"></div>' +
            '<div id="px-err" class="error-box" style="display:none"></div>' +
            '<div class="dialog-actions">' +
            '<button id="px-cancel" class="btn btn-secondary">Cancel</button>' +
            '<button id="px-save" class="btn btn-primary">Create</button>' +
            '</div>';
        document.getElementById('px-cancel').onclick = function () { overlay.style.display = 'none'; };
        document.getElementById('px-save').onclick = function () {
            var username = document.getElementById('px-username').value.trim();
            if (!username) {
                document.getElementById('px-err').textContent = 'Username is required.';
                document.getElementById('px-err').style.display = '';
                return;
            }
            var body = { username: username };
            var uid = document.getElementById('px-user-id').value.trim();
            if (uid) body.paired_user_id = uid;
            api.post('/api/admin/proxies', body)
                .then(function () { overlay.style.display = 'none'; onDone(); showMsg(section, 'Proxy created.', 'success'); })
                .catch(function (err) {
                    document.getElementById('px-err').textContent = err.message || 'Failed';
                    document.getElementById('px-err').style.display = '';
                });
        };
    }

    function showEditProxyDialog(proxy, section, onDone) {
        var overlay = document.getElementById('dialog-overlay');
        var dialog = document.getElementById('dialog');
        overlay.style.display = 'flex';

        // Build with DOM so we can attach file input events cleanly
        dialog.innerHTML = '';
        dialog.appendChild(el('h3', { text: 'Edit Proxy: @' + proxy.username, style: 'margin-top:0' }));

        // Avatar row
        var avatarRow = el('div', { style: 'display:flex;align-items:center;gap:12px;margin-bottom:14px' });
        var avatarImg = el('img', { style: 'width:48px;height:48px;border-radius:50%;object-fit:cover;background:var(--t-bg-alt,#222)' });
        if (proxy.avatar_url) { avatarImg.src = proxy.avatar_url; } else { avatarImg.style.display = 'none'; }
        var avatarFileInput = el('input', { type: 'file', accept: 'image/*', style: 'display:none' });
        var avatarUploadBtn = el('button', { class: 'btn btn-secondary btn-small', text: proxy.avatar_url ? 'Change Avatar' : 'Upload Avatar' });
        avatarUploadBtn.onclick = function () { avatarFileInput.click(); };
        avatarFileInput.onchange = function () {
            var file = avatarFileInput.files[0];
            if (!file) return;
            avatarUploadBtn.disabled = true;
            api.upload('/api/media', file).then(function (media) {
                return api.patch('/api/admin/proxies/' + proxy.proxy_id, { avatar_media_id: media.media_id });
            }).then(function (updated) {
                if (updated && updated.avatar_url) {
                    avatarImg.src = updated.avatar_url;
                    avatarImg.style.display = '';
                    proxy.avatar_url = updated.avatar_url;
                }
                avatarUploadBtn.textContent = 'Change Avatar';
                avatarUploadBtn.disabled = false;
            }).catch(function (err) {
                showMsg(section, err.message || 'Upload failed', 'error');
                avatarUploadBtn.disabled = false;
            });
        };
        avatarRow.appendChild(avatarImg);
        avatarRow.appendChild(avatarUploadBtn);
        avatarRow.appendChild(avatarFileInput);
        dialog.appendChild(avatarRow);

        var dnGroup = el('div', { class: 'form-group' });
        dnGroup.appendChild(el('label', { text: 'Display Name' }));
        var dnInput = el('input', { type: 'text', class: 'form-input', id: 'px-display-name', value: proxy.display_name || '' });
        dnGroup.appendChild(dnInput);
        dialog.appendChild(dnGroup);

        var bioGroup = el('div', { class: 'form-group' });
        bioGroup.appendChild(el('label', { text: 'Bio' }));
        var bioInput = el('textarea', { class: 'form-input', id: 'px-bio', rows: '3' });
        bioInput.value = proxy.bio || '';
        bioGroup.appendChild(bioInput);
        dialog.appendChild(bioGroup);

        var errBox = el('div', { id: 'px-err', class: 'error-box', style: 'display:none' });
        dialog.appendChild(errBox);

        var actions = el('div', { class: 'dialog-actions' });
        var cancelBtn = el('button', { class: 'btn btn-secondary', text: 'Cancel' });
        cancelBtn.onclick = function () { overlay.style.display = 'none'; onDone(); };
        var saveBtn = el('button', { class: 'btn btn-primary', text: 'Save' });
        saveBtn.onclick = function () {
            var body = {
                display_name: dnInput.value.trim() || null,
                bio: bioInput.value.trim() || null
            };
            api.patch('/api/admin/proxies/' + proxy.proxy_id, body)
                .then(function () { overlay.style.display = 'none'; onDone(); showMsg(section, 'Proxy updated.', 'success'); })
                .catch(function (err) { errBox.textContent = err.message || 'Failed'; errBox.style.display = ''; });
        };
        actions.appendChild(cancelBtn);
        actions.appendChild(saveBtn);
        dialog.appendChild(actions);
    }

    function showSetProxyPasswordDialog(proxyId, section) {
        var overlay = document.getElementById('dialog-overlay');
        var dialog = document.getElementById('dialog');
        overlay.style.display = 'flex';
        dialog.innerHTML =
            '<h3 style="margin-top:0">Set Proxy Password</h3>' +
            '<div class="form-group"><label>New Password</label>' +
            '<input id="px-pw" class="form-input" type="password" placeholder="Min 12 characters, Uppercase+Lowercase, 1 number, 1 symbol (?!$...)"></div>' +
            '<div id="px-err" class="error-box" style="display:none"></div>' +
            '<div class="dialog-actions">' +
            '<button id="px-cancel" class="btn btn-secondary">Cancel</button>' +
            '<button id="px-save" class="btn btn-primary">Set Password</button>' +
            '</div>';
        document.getElementById('px-cancel').onclick = function () { overlay.style.display = 'none'; };
        document.getElementById('px-save').onclick = function () {
            var pw = document.getElementById('px-pw').value;
            if (pw.length < 8) {
                document.getElementById('px-err').textContent = 'Password must be at least 12 characters, Uppercase+Lowercase, 1 number, 1 symbol (?!$...).';
                document.getElementById('px-err').style.display = '';
                return;
            }
            api.put('/api/admin/proxies/' + proxyId + '/password', { password: pw })
                .then(function () { overlay.style.display = 'none'; showMsg(section, 'Password set.', 'success'); })
                .catch(function (err) {
                    document.getElementById('px-err').textContent = err.message || 'Failed';
                    document.getElementById('px-err').style.display = '';
                });
        };
    }

    function showProxyHmacKeyDialog(proxy, section) {
        var overlay = document.getElementById('dialog-overlay');
        var dialog = document.getElementById('dialog');
        overlay.style.display = 'flex';

        dialog.innerHTML = '';
        dialog.appendChild(el('h3', { text: 'HMAC API Key  @' + proxy.username, style: 'margin-top:0' }));

        if (proxy.has_hmac_key) {
            dialog.appendChild(el('div', {
                class: 'info-box',
                style: 'font-family:monospace;margin-bottom:10px',
                text: 'Current fingerprint: ' + (proxy.hmac_key_fingerprint || '') + '…'
            }));
        }
        dialog.appendChild(el('div', { class: 'pref-desc', style: 'margin-bottom:12px', text: 'Generate a new 32-byte HMAC signing key. Download it before saving  it cannot be retrieved again. The server stores only the key for verification; keep the file secure.' }));

        var pendingKey = null;
        var keyBox = el('div', { style: 'display:none;font-family:monospace;font-size:0.78rem;word-break:break-all;background:var(--t-bg-alt,#1e1e1e);border:1px solid var(--t-border,#333);border-radius:6px;padding:10px;margin:8px 0;user-select:all' });
        dialog.appendChild(keyBox);

        var downloadBtn = el('button', { class: 'btn btn-secondary', text: 'Download Key File', style: 'display:none;margin-right:8px' });
        var saveServerBtn = el('button', { class: 'btn btn-primary', text: 'Save to Server', style: 'display:none;opacity:0.4;pointer-events:none' });

        downloadBtn.onclick = function () {
            if (!pendingKey) return;
            downloadText(
                'proxy-hmac-' + proxy.username + '-' + pendingKey.slice(0, 8) + '.txt',
                'HMAC Signing Key\n' +
                'Proxy: @' + proxy.username + '\n' +
                'Proxy ID: ' + proxy.proxy_id + '\n' +
                'Key (hex-64): ' + pendingKey + '\n' +
                'Fingerprint: ' + pendingKey.slice(0, 8) + '\n\n' +
                'Keep this file secure and private. Anyone with this key can sign requests as this proxy.'
            );
            saveServerBtn.style.opacity = '';
            saveServerBtn.style.pointerEvents = '';
        };

        saveServerBtn.onclick = function () {
            if (!pendingKey) return;
            saveServerBtn.disabled = true;
            api.put('/api/admin/proxies/' + proxy.proxy_id + '/hmac-key', { hmac_key: pendingKey })
                .then(function () {
                    overlay.style.display = 'none';
                    showMsg(section, 'HMAC key saved. Fingerprint: ' + pendingKey.slice(0, 8) + '…', 'success');
                })
                .catch(function (err) {
                    showMsg(section, err.message || 'Failed to save key', 'error');
                    saveServerBtn.disabled = false;
                });
        };

        var genBtn = el('button', { class: 'btn ' + (proxy.has_hmac_key ? 'btn-secondary' : 'btn-primary'), text: proxy.has_hmac_key ? 'Regenerate Key' : 'Generate Key' });
        genBtn.onclick = function () {
            pendingKey = generateHmacKey();
            keyBox.textContent = pendingKey;
            keyBox.style.display = '';
            downloadBtn.style.display = '';
            saveServerBtn.style.display = '';
            saveServerBtn.style.opacity = '0.4';
            saveServerBtn.style.pointerEvents = 'none';
        };
        dialog.appendChild(genBtn);
        dialog.appendChild(downloadBtn);
        dialog.appendChild(saveServerBtn);

        var cancelBtn = el('button', { class: 'btn btn-secondary', text: 'Close', style: 'margin-top:14px;display:block' });
        cancelBtn.onclick = function () { overlay.style.display = 'none'; };
        dialog.appendChild(cancelBtn);
    }

    function showProxyRateLimitDialog(proxy, section) {
        var overlay = document.getElementById('dialog-overlay');
        var dialog = document.getElementById('dialog');
        overlay.style.display = 'flex';
        dialog.innerHTML = '';
        dialog.appendChild(el('h3', { text: 'Rate Limit Override: @' + proxy.username, style: 'margin-top:0' }));
        dialog.appendChild(el('div', { class: 'pref-desc', style: 'margin-bottom:14px', text: 'Set per-proxy upload rate limits. Leave blank to use the server defaults. The effective limit is always the lower of the per-proxy override and the server default.' }));

        var errBox = el('div', { class: 'error-box', style: 'display:none;margin-bottom:8px' });
        dialog.appendChild(errBox);

        var piecesGroup = el('div', { class: 'form-group' });
        piecesGroup.appendChild(el('label', { text: 'Max uploads/minute (blank = server default)' }));
        var piecesInput = el('input', { type: 'number', class: 'form-input', id: 'px-rl-pieces', min: '1', placeholder: 'Server default' });
        piecesGroup.appendChild(piecesInput);
        dialog.appendChild(piecesGroup);

        var bytesGroup = el('div', { class: 'form-group' });
        bytesGroup.appendChild(el('label', { text: 'Max MB/minute (blank = server default)' }));
        var bytesInput = el('input', { type: 'number', class: 'form-input', id: 'px-rl-bytes', min: '1', placeholder: 'Server default' });
        bytesGroup.appendChild(bytesInput);
        dialog.appendChild(bytesGroup);

        // Load existing override
        api.get('/api/admin/proxies/' + proxy.proxy_id + '/rate-limit').then(function (rl) {
            if (rl.max_pieces_per_minute != null) piecesInput.value = rl.max_pieces_per_minute;
            if (rl.max_bytes_per_minute != null) bytesInput.value = Math.round(rl.max_bytes_per_minute / 1048576);
        }).catch(function () { /* non-fatal, just leave blank */ });

        var actions = el('div', { class: 'dialog-actions' });
        var cancelBtn = el('button', { class: 'btn btn-secondary', text: 'Cancel' });
        cancelBtn.onclick = function () { overlay.style.display = 'none'; };
        var saveBtn = el('button', { class: 'btn btn-primary', text: 'Save' });
        saveBtn.onclick = function () {
            var pieces = piecesInput.value.trim();
            var bytes = bytesInput.value.trim();
            var body = {
                max_pieces_per_minute: pieces ? (parseInt(pieces, 10) || null) : null,
                max_bytes_per_minute: bytes ? ((parseInt(bytes, 10) || null) * 1048576) : null
            };
            api.put('/api/admin/proxies/' + proxy.proxy_id + '/rate-limit', body)
                .then(function () {
                    overlay.style.display = 'none';
                    showMsg(section, 'Rate limit saved.', 'success');
                })
                .catch(function (err) {
                    errBox.textContent = err.message || 'Failed to save';
                    errBox.style.display = '';
                });
        };
        actions.appendChild(cancelBtn);
        actions.appendChild(saveBtn);
        dialog.appendChild(actions);
    }

    return {
        renderUsers: renderUsers,
        renderInvites: renderInvites,
        renderApplications: renderApplications,
        renderSiteConfig: renderSiteConfig,
        renderEmailSettings: renderEmailSettings,
        renderTheme: renderTheme,
        renderStunServers: renderStunServers,
        renderFederation: renderFederation,
        renderProxies: renderProxies
    };
})();
