// Profile UI — view and edit user profiles
var profile = (function () {

    // Avatar helper 
    // Returns HTML for an avatar element with image or initials fallback
    // size: 'sm' (28px), 'md' (36px), 'lg' (96px)
    function avatarHtml(avatarUrl, displayName, size) {
        var cls = 'avatar avatar-' + (size || 'sm');
        if (avatarUrl) {
            return '<div class="' + cls + '"><img src="' + escapeAttr(avatarUrl) + '" alt=""></div>';
        }
        var initials = getInitials(displayName || '?');
        var color = stringToColor(displayName || '?');
        return '<div class="' + cls + '" style="background:' + color + '"><span class="avatar-initials">' + escapeHtml(initials) + '</span></div>';
    }

    // if no profile image, put initials
    function getInitials(name) {
        if (!name) return '?';
        var parts = name.trim().split(/\s+/);
        if (parts.length >= 2) return (parts[0][0] + parts[1][0]).toUpperCase();
        return name.substring(0, 2).toUpperCase();
    }

    function stringToColor(str) {
        var hash = 0;
        for (var i = 0; i < str.length; i++) {
            hash = str.charCodeAt(i) + ((hash << 5) - hash);
        }
        var h = Math.abs(hash) % 360;
        return 'hsl(' + h + ', 45%, 55%)';
    }

    // Render profile page 

    async function renderProfile(userId) {
        var feedArea = document.getElementById('feed-area');
        if (!feedArea) return;
        feedArea.innerHTML = '<div class="loading-indicator">Loading profile...</div>';

        try {
            var prof = await api.get('/api/users/' + encodeURIComponent(userId) + '/profile');
            var isOwn = app.state.currentUser && app.state.currentUser.user_id === prof.user_id;
            var avatarUrl = prof.avatar_url;
            var name = prof.display_name || prof.email;

            var memberSince = '';
            try {
                var d = new Date(prof.created_at);
                memberSince = d.toLocaleDateString([], { year: 'numeric', month: 'long' });
            } catch (_) { }

            var html =
                '<div class="profile-page">' +
                '<div class="profile-card">' +
                '<div class="profile-header-section">' +
                avatarHtml(avatarUrl, name, 'lg') +
                '<div class="profile-info">' +
                '<h2 class="profile-name">' + escapeHtml(name) + '</h2>' +
                (prof.username ? '<div class="profile-username">@' + escapeHtml(prof.username) + '</div>' : '') +
                (prof.bio ? '<div class="profile-bio">' + escapeHtml(prof.bio) + '</div>' : '') +
                '</div>' +
                '</div>' +
                '<div class="profile-details">' +
                (prof.location ? '<div class="profile-detail"><span class="profile-detail-icon">&#128205;</span> ' + escapeHtml(prof.location) + '</div>' : '') +
                (prof.website && safeUrl(prof.website) ? '<div class="profile-detail"><span class="profile-detail-icon">&#128279;</span> <a href="' + escapeAttr(prof.website) + '" target="_blank" rel="noopener noreferrer">' + escapeHtml(prof.website) + '</a></div>' : '') +
                (memberSince ? '<div class="profile-detail"><span class="profile-detail-icon">&#128197;</span> Joined ' + memberSince + '</div>' : '') +
                '</div>' +
                (isOwn ? '<div class="profile-actions"><button class="btn btn-secondary btn-small" id="btn-edit-profile">Edit Profile</button></div>' : '') +
                '</div>' +
                '</div>';

            feedArea.innerHTML = html;

            if (isOwn) {
                document.getElementById('btn-edit-profile').onclick = function () {
                    renderEditProfileForm(prof);
                };
            }

            // Append posts section
            var profilePage = feedArea.querySelector('.profile-page');
            if (profilePage) {
                var postsSection = document.createElement('div');
                postsSection.className = 'profile-posts-section';
                postsSection.innerHTML = '<h3 class="profile-posts-title">Posts</h3><div class="profile-posts-list"><div class="loading-indicator">Loading posts\u2026</div></div>';
                profilePage.appendChild(postsSection);

                var postsList = postsSection.querySelector('.profile-posts-list');
                try {
                    var postsUrl = '/api/posts?author_id=' + encodeURIComponent(prof.user_id) + '&limit=20';
                    if (isOwn) postsUrl += '&include_drafts=true';
                    var postsData = await api.get(postsUrl);
                    var postItems = Array.isArray(postsData) ? postsData : (postsData.posts || []);
                    postsList.innerHTML = '';
                    if (!postItems.length) {
                        postsList.innerHTML = '<div class="empty-state">No posts yet.</div>';
                    } else {
                        postItems.forEach(function (post) {
                            if (typeof posts !== 'undefined' && typeof posts.renderPostCard === 'function') {
                                postsList.appendChild(posts.renderPostCard(post));
                            }
                        });
                    }
                } catch (_) {
                    postsList.innerHTML = '<div class="empty-state">Could not load posts.</div>';
                }
            }
        } catch (e) {
            feedArea.innerHTML = '<div class="empty-state">Failed to load profile</div>';
        }
    }

    // Edit profile form 

    function renderEditProfileForm(prof) {
        var feedArea = document.getElementById('feed-area');
        if (!feedArea) return;

        var currentAvatarId = null;
        // Extract media_id from avatar_url pattern /api/media/{id}/thumbnail
        if (prof.avatar_url) {
            var match = prof.avatar_url.match(/\/api\/media\/([^/]+)\/thumbnail/);
            if (match) currentAvatarId = match[1];
        }

        var html =
            '<div class="profile-page">' +
            '<div class="profile-card">' +
            '<h2 style="margin-bottom:20px">Edit Profile</h2>' +
            '<div class="profile-avatar-edit">' +
            '<div id="edit-avatar-preview">' + avatarHtml(prof.avatar_url, prof.display_name || prof.email, 'lg') + '</div>' +
            '<button class="btn btn-secondary btn-small" id="btn-change-avatar">Change Photo</button>' +
            (currentAvatarId ? '<button class="btn btn-small" id="btn-remove-avatar" style="color:#ef4444">Remove</button>' : '') +
            '<input type="file" id="avatar-file-input" accept="image/*" style="display:none">' +
            '</div>' +
            '<div class="form-group">' +
            '<label>Display Name</label>' +
            '<input type="text" class="form-input" id="edit-display-name" value="' + escapeAttr(prof.display_name || '') + '" maxlength="50" placeholder="Your name">' +
            '</div>' +
            '<div class="form-group">' +
            '<label>Username (case-sensitive) (primary login key) (uneditable)</label>' +
            '<input type="text" class="form-input" value="' + escapeAttr(prof.username || '') + '" maxlength="30" placeholder="username" disabled>' +
            '</div>' +
            '<div class="form-group">' +
            '<label>Bio</label>' +
            '<textarea class="form-input" id="edit-bio" rows="3" maxlength="500" placeholder="Tell us about yourself">' + escapeHtml(prof.bio || '') + '</textarea>' +
            '</div>' +
            '<div class="form-group">' +
            '<label>Location</label>' +
            '<input type="text" class="form-input" id="edit-location" value="' + escapeAttr(prof.location || '') + '" maxlength="100" placeholder="City, Country">' +
            '</div>' +
            '<div class="form-group">' +
            '<label>Website</label>' +
            '<input type="text" class="form-input" id="edit-website" value="' + escapeAttr(prof.website || '') + '" maxlength="200" placeholder="https://example.com">' +
            '</div>' +
            '<div id="edit-profile-error" class="error-box" style="display:none"></div>' +
            '<div class="profile-edit-actions">' +
            '<button class="btn btn-secondary btn-small" id="btn-cancel-edit">Cancel</button>' +
            '<button class="btn btn-primary btn-small" id="btn-save-profile">Save</button>' +
            '</div>' +
            '</div>' +
            '</div>';

        feedArea.innerHTML = html;

        var newAvatarId = currentAvatarId;

        // Change avatar
        var fileInput = document.getElementById('avatar-file-input');
        document.getElementById('btn-change-avatar').onclick = function () {
            fileInput.click();
        };
        fileInput.onchange = async function () {
            if (!fileInput.files.length) return;
            var file = fileInput.files[0];
            fileInput.value = '';
            if (!file.type.startsWith('image/')) {
                alert('Please select an image file.');
                return;
            }
            if (file.size > 50 * 1024 * 1024) {
                alert('Image must be smaller than 50 MB.');
                return;
            }
            try {
                var result = await api.upload('/api/media', file, { media_type: 'image' });
                newAvatarId = result.media_id;
                var previewUrl = result.thumbnail_url || ('/api/media/' + result.media_id + '/thumbnail');
                document.getElementById('edit-avatar-preview').innerHTML = avatarHtml(previewUrl, null, 'lg');
            } catch (e) {
                alert('Failed to upload: ' + e.message);
            }
        };

        // Remove avatar
        var removeBtn = document.getElementById('btn-remove-avatar');
        if (removeBtn) {
            removeBtn.onclick = function () {
                newAvatarId = null;
                document.getElementById('edit-avatar-preview').innerHTML = avatarHtml(null, prof.display_name || prof.email, 'lg');
            };
        }

        // Cancel
        document.getElementById('btn-cancel-edit').onclick = function () {
            renderProfile(prof.user_id);
        };

        // Save
        document.getElementById('btn-save-profile').onclick = async function () {
            var errorEl = document.getElementById('edit-profile-error');
            errorEl.style.display = 'none';

            var body = {
                display_name: document.getElementById('edit-display-name').value.trim() || null,
                username: document.getElementById('edit-username').value.trim() || null,
                bio: document.getElementById('edit-bio').value.trim() || null,
                location: document.getElementById('edit-location').value.trim() || null,
                website: document.getElementById('edit-website').value.trim() || null,
                avatar_media_id: newAvatarId || null
            };

            var saveBtn = document.getElementById('btn-save-profile');
            saveBtn.disabled = true;
            saveBtn.textContent = 'Saving...';

            try {
                var updated = await api.put('/api/profile', body);
                // Update local state
                if (app.state.currentUser) {
                    app.state.currentUser.display_name = updated.display_name;
                    app.state.currentUser.avatar_url = updated.avatar_url;
                    updateHeaderUser();
                }
                renderProfile(updated.user_id);
            } catch (e) {
                errorEl.textContent = e.message;
                errorEl.style.display = '';
                saveBtn.disabled = false;
                saveBtn.textContent = 'Save';
            }
        };
    }

    // Update header with user info 

    function updateHeaderUser() {
        var user = app.state.currentUser;
        if (!user) return;

        var headerRight = document.querySelector('.header-right');
        if (!headerRight) return;

        // Update or create avatar in header
        var existingAvatar = headerRight.querySelector('.header-avatar');
        var newAvatar = document.createElement('div');
        newAvatar.innerHTML = avatarHtml(user.avatar_url, user.display_name || user.email, 'sm');
        var avatarEl = newAvatar.firstChild;
        avatarEl.classList.add('header-avatar');
        avatarEl.style.cursor = 'pointer';
        avatarEl.onclick = function () {
            window.location.hash = 'profile/' + user.user_id;
        };

        if (existingAvatar) {
            existingAvatar.replaceWith(avatarEl);
        } else {
            var emailEl = document.getElementById('current-user-email');
            if (emailEl) headerRight.insertBefore(avatarEl, emailEl);
        }

        // Update display text
        var emailEl = document.getElementById('current-user-email');
        if (emailEl) {
            emailEl.textContent = user.display_name || user.email || '';
        }
    }
    
    function safeUrl(url) {
        try {
            var p = new URL(url);
            return (p.protocol === 'http:' || p.protocol === 'https:') ? url : null;
        } catch (_) { return null; }
    }

    return {
        renderProfile: renderProfile,
        avatarHtml: avatarHtml,
        updateHeaderUser: updateHeaderUser
    };
})();
