// Posts UI — feed, post cards, create post form with media attachments, comments
var posts = (function () {
    var feedOffset = 0;
    var feedLoading = false;
    var feedHasMore = true;
    var currentTags = []; // tags for new post creation
    var pendingAttachments = []; // { file, media_id, media_type, url, thumbnail_url }

    // Feed 

    async function loadFeed(reset) {
        if (feedLoading) return;
        if (reset) {
            feedOffset = 0;
            feedHasMore = true;
            document.getElementById('post-feed').innerHTML = '';
        }
        if (!feedHasMore) return;

        feedLoading = true;
        document.getElementById('feed-loading').style.display = '';
        document.getElementById('feed-empty').style.display = 'none';

        try {
            var data = await api.get('/api/posts?limit=20&offset=' + feedOffset);
            var postList = data.posts || [];
            feedHasMore = data.has_more;
            feedOffset += postList.length;

            if (postList.length === 0 && feedOffset === 0) {
                document.getElementById('feed-empty').style.display = '';
            } else {
                var container = document.getElementById('post-feed');
                postList.forEach(function (p) {
                    container.appendChild(renderPostCard(p));
                });
            }
        } catch (e) {
            console.error('Failed to load feed:', e);
        }
        feedLoading = false;
        document.getElementById('feed-loading').style.display = 'none';
    }

    var MAX_VISIBLE_IMAGES = 6; // 3 columns x 2 rows

    function renderPostCard(post) {
        var card = document.createElement('div');
        card.className = 'post-card';
        card.dataset.postId = post.post_id;

        var authorName = post.author_name || post.author_id.substring(0, 12);
        var authorAvatarUrl = post.author_avatar_url || null;
        var time = formatTime(post.created_at);

        // Separate content types
        var contents = post.contents || [];
        var textItems = [];
        var imageItems = [];
        var videoItems = [];
        var fileItems = [];
        contents.forEach(function (c) {
            if (c.content_type === 'text') textItems.push(c);
            else if (c.content_type === 'image') imageItems.push(c);
            else if (c.content_type === 'video') videoItems.push(c);
            else if (c.content_type === 'file') fileItems.push(c);
        });

        // Render text
        var contentsHtml = '';
        var firstTextContent = null;
        textItems.forEach(function (c) {
            contentsHtml += '<div class="post-text">' + linkPreview.linkify(c.content) + '</div>';
            if (firstTextContent === null) firstTextContent = c.content;
        });
        contentsHtml += '<div class="post-link-preview"></div>';

        // Render image grid (thumbnails, max 6, overflow indicator)
        if (imageItems.length > 0) {
            var visibleCount = Math.min(imageItems.length, MAX_VISIBLE_IMAGES);
            var cols = visibleCount === 1 ? 1 : visibleCount === 2 ? 2 : 3;
            contentsHtml += '<div class="post-image-grid cols-' + cols + '">';
            for (var i = 0; i < visibleCount; i++) {
                var mediaRef = imageItems[i].content;
                var thumbSrc = mediaRef.startsWith('http') ? mediaRef
                    : ('/api/media/' + mediaRef + '/thumbnail');
                var isLast = (i === visibleCount - 1) && (imageItems.length > MAX_VISIBLE_IMAGES);
                var extraCount = imageItems.length - MAX_VISIBLE_IMAGES;
                contentsHtml += '<div class="post-image-cell' + (isLast ? ' has-overflow' : '') + '" data-img-index="' + i + '">' +
                    '<img src="' + escapeAttr(thumbSrc) + '" alt="post image" loading="lazy">' +
                    (isLast ? '<div class="image-overflow-overlay">+' + extraCount + '</div>' : '') +
                    '</div>';
            }
            contentsHtml += '</div>';
        }

        // Render videos
        videoItems.forEach(function (c) {
            var videoSrc = c.content.startsWith('http') ? c.content : ('/api/media/' + c.content + '/file');
            contentsHtml += '<div class="post-media"><video src="' + escapeAttr(videoSrc) + '" controls preload="metadata"></video></div>';
        });

        // Render files
        fileItems.forEach(function (c) {
            var fname = c.filename || 'File';
            var fsize = c.file_size ? formatSize(c.file_size) : '';
            var fileSrc = c.content.startsWith('http') ? c.content : ('/api/media/' + c.content + '/file');
            contentsHtml += '<a class="post-file-attachment" href="' + escapeAttr(fileSrc) + '" download="' + escapeAttr(fname) + '">' +
                '<span class="post-file-icon">&#128196;</span>' +
                '<span class="post-file-info"><span class="post-file-name">' + escapeHtml(fname) + '</span><span class="post-file-meta">' + fsize + '</span></span></a>';
        });

        var tagsHtml = '';
        if (post.tags && post.tags.length > 0) {
            tagsHtml = '<div class="post-tags">' +
                post.tags.map(function (t) {
                    return '<span class="tag-chip" data-tag="' + escapeAttr(t) + '">' + escapeHtml(t) + '</span>';
                }).join('') +
                '</div>';
        }

        var commentCount = post.comment_count || 0;
        var isOwn = app.state.currentUser && app.state.currentUser.user_id === post.author_id;

        var actionsHtml = '';
        if (isOwn) {
            actionsHtml = '<div class="post-actions-menu">' +
                '<button class="btn-post-action btn-post-edit" title="Edit">&#9998;</button>' +
                '<button class="btn-post-action btn-post-delete" title="Delete">&times;</button>' +
                '</div>';
        }

        var authorAvatarHtml = (typeof profile !== 'undefined') ? profile.avatarHtml(authorAvatarUrl, authorName, 'sm') : '';

        card.innerHTML =
            '<div class="post-header">' +
            '<a class="post-author-link" href="#profile/' + escapeAttr(post.author_id) + '">' +
            authorAvatarHtml +
            '<span class="post-author">' + escapeHtml(authorName) + '</span>' +
            '</a>' +
            '<span class="post-time">' + time + '</span>' +
            actionsHtml +
            '</div>' +
            contentsHtml +
            tagsHtml +
            '<div class="post-footer">' +
            '<button class="btn-comments-toggle">' + commentCount + ' comment' + (commentCount !== 1 ? 's' : '') + '</button>' +
            '</div>' +
            '<div class="post-comments-section" style="display:none">' +
            '<div class="comments-list"></div>' +
            '<div class="comments-loading" style="display:none">Loading comments...</div>' +
            '<button class="btn-load-more-comments" style="display:none">Load more comments</button>' +
            '<div class="comment-form">' +
            '<div class="comment-attachment-preview" style="display:none"></div>' +
            '<div class="comment-input-row">' +
            '<button class="btn-comment-attach" title="Attach file">&#128206;</button>' +
            '<input type="text" class="comment-input" placeholder="Write a comment..." autocomplete="off">' +
            '<button class="btn btn-primary btn-small btn-comment-submit">Post</button>' +
            '</div>' +
            '<input type="file" class="comment-file-input" style="display:none">' +
            '</div>' +
            '</div>';

        // Attach link preview card for first URL in post text
        if (firstTextContent) {
            linkPreview.attachPreview(card.querySelector('.post-link-preview'), firstTextContent);
        }

        // Click image cells to open lightbox
        if (imageItems.length > 0) {
            var allImageUrls = imageItems.map(function (c) {
                return c.content.startsWith('http') ? c.content : ('/api/media/' + c.content + '/file');
            });
            card.querySelectorAll('.post-image-cell').forEach(function (cell) {
                cell.onclick = function () {
                    var startIndex = parseInt(cell.dataset.imgIndex, 10) || 0;
                    openLightbox(allImageUrls, startIndex);
                };
            });
        }

        // Click tag to search
        card.querySelectorAll('.tag-chip').forEach(function (chip) {
            chip.onclick = function (e) {
                e.stopPropagation();
                if (typeof search !== 'undefined') {
                    search.searchByTag(chip.dataset.tag);
                }
            };
        });

        // Edit button
        var editBtn = card.querySelector('.btn-post-edit');
        if (editBtn) {
            editBtn.onclick = function (e) {
                e.stopPropagation();
                enterEditMode(card, post);
            };
        }

        // Delete button
        var deleteBtn = card.querySelector('.btn-post-delete');
        if (deleteBtn) {
            deleteBtn.onclick = function (e) {
                e.stopPropagation();
                deletePost(post.post_id, card);
            };
        }

        // Toggle comments section
        var toggleBtn = card.querySelector('.btn-comments-toggle');
        var commentsSection = card.querySelector('.post-comments-section');
        var commentsLoaded = false;
        toggleBtn.onclick = function () {
            var visible = commentsSection.style.display !== 'none';
            commentsSection.style.display = visible ? 'none' : '';
            if (!visible && !commentsLoaded) {
                commentsLoaded = true;
                loadComments(post.post_id, card, 0);
            }
        };

        // Load more comments
        var loadMoreBtn = card.querySelector('.btn-load-more-comments');
        loadMoreBtn.onclick = function () {
            var list = card.querySelector('.comments-list');
            var currentCount = list.querySelectorAll('.comment-item').length;
            loadComments(post.post_id, card, currentCount);
        };

        // Comment attachment state per card
        var pendingCommentAttachment = null;

        // Attach button
        var commentAttachBtn = card.querySelector('.btn-comment-attach');
        var commentFileInput = card.querySelector('.comment-file-input');
        var commentAttachPreview = card.querySelector('.comment-attachment-preview');
        if (commentAttachBtn && commentFileInput) {
            commentAttachBtn.onclick = function () { commentFileInput.click(); };
            commentFileInput.onchange = function () {
                if (commentFileInput.files.length > 0) {
                    uploadCommentAttachment(commentFileInput.files[0], commentAttachPreview, function (att) {
                        pendingCommentAttachment = att;
                    });
                }
                commentFileInput.value = '';
            };
        }

        // Submit comment
        var commentInput = card.querySelector('.comment-input');
        var submitBtn = card.querySelector('.btn-comment-submit');
        submitBtn.onclick = function () {
            submitComment(post.post_id, card, commentInput, pendingCommentAttachment, function () {
                pendingCommentAttachment = null;
                if (commentAttachPreview) { commentAttachPreview.style.display = 'none'; commentAttachPreview.innerHTML = ''; }
            });
        };
        commentInput.onkeydown = function (e) {
            if (e.key === 'Enter') {
                e.preventDefault();
                submitComment(post.post_id, card, commentInput, pendingCommentAttachment, function () {
                    pendingCommentAttachment = null;
                    if (commentAttachPreview) { commentAttachPreview.style.display = 'none'; commentAttachPreview.innerHTML = ''; }
                });
            }
        };

        return card;
    }

    // Edit Post 

    function enterEditMode(card, post) {
        var contents = post.contents || [];
        var textItems = [];
        var mediaItems = []; // images, videos, files — keep as-is
        contents.forEach(function (c) {
            if (c.content_type === 'text') textItems.push(c);
            else mediaItems.push(c);
        });

        var editTags = (post.tags || []).slice();
        var removedMedia = []; // media_ids to exclude on save
        var editAttachments = []; // newly uploaded during edit

        // Combine text items into one textarea value
        var textValue = textItems.map(function (c) { return c.content; }).join('\n');

        // Build edit form HTML
        var editHtml =
            '<div class="post-edit-form">' +
            '<textarea class="edit-post-text form-input" rows="3">' + escapeHtml(textValue) + '</textarea>' +
            '<div class="edit-media-list"></div>' +
            '<div class="edit-attachment-previews"></div>' +
            '<div class="create-post-media-bar">' +
            '<button class="btn-attach edit-btn-image" title="Add image">' +
            '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="3" width="18" height="18" rx="2"/><circle cx="8.5" cy="8.5" r="1.5"/><path d="M21 15l-5-5L5 21"/></svg> Photo</button>' +
            '<button class="btn-attach edit-btn-video" title="Add video">' +
            '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polygon points="23 7 16 12 23 17 23 7"/><rect x="1" y="5" width="15" height="14" rx="2"/></svg> Video</button>' +
            '<button class="btn-attach edit-btn-file" title="Add file">' +
            '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/></svg> File</button>' +
            '<input type="file" class="edit-file-input" style="display:none" multiple>' +
            '</div>' +
            '<div class="edit-tags-area">' +
            '<div class="edit-tag-chips tag-chips"></div>' +
            '<input type="text" class="edit-tag-input tag-input-small" placeholder="Add tags...">' +
            '</div>' +
            '<div class="edit-post-actions">' +
            '<label class="publish-toggle"><input type="checkbox" class="edit-publish"' + (post.is_published ? ' checked' : '') + '> Publish</label>' +
            '<button class="btn btn-secondary btn-small btn-edit-cancel">Cancel</button>' +
            '<button class="btn btn-primary btn-small btn-edit-save">Save</button>' +
            '</div>' +
            '</div>';

        card.innerHTML = editHtml;

        // Render existing media items with remove buttons
        var mediaList = card.querySelector('.edit-media-list');
        mediaItems.forEach(function (m) {
            var el = document.createElement('div');
            el.className = 'edit-media-item';
            el.dataset.mediaId = m.content;

            if (m.content_type === 'image') {
                el.innerHTML = '<img src="/api/media/' + escapeAttr(m.content) + '/thumbnail" alt="image">' +
                    '<button class="attachment-remove" title="Remove">&times;</button>';
            } else if (m.content_type === 'video') {
                el.innerHTML = '<div class="attachment-preview-file"><span class="file-name">' + escapeHtml(m.filename || 'Video') + '</span><span class="file-meta">Video</span></div>' +
                    '<button class="attachment-remove" title="Remove">&times;</button>';
            } else {
                el.innerHTML = '<div class="attachment-preview-file"><span class="file-name">' + escapeHtml(m.filename || 'File') + '</span><span class="file-meta">' + (m.file_size ? formatSize(m.file_size) : 'File') + '</span></div>' +
                    '<button class="attachment-remove" title="Remove">&times;</button>';
            }

            el.querySelector('.attachment-remove').onclick = function () {
                removedMedia.push(m.content);
                el.parentNode.removeChild(el);
            };
            mediaList.appendChild(el);
        });

        // Render tag chips
        function renderEditTags() {
            var container = card.querySelector('.edit-tag-chips');
            container.innerHTML = '';
            editTags.forEach(function (t) {
                var chip = document.createElement('span');
                chip.className = 'tag-chip removable';
                chip.innerHTML = escapeHtml(t) + ' <span class="tag-remove">&times;</span>';
                chip.querySelector('.tag-remove').onclick = function () {
                    editTags = editTags.filter(function (x) { return x !== t; });
                    renderEditTags();
                };
                container.appendChild(chip);
            });
        }
        renderEditTags();

        // Tag input
        var tagInput = card.querySelector('.edit-tag-input');
        tagInput.onkeydown = function (e) {
            if (e.key === 'Enter' || e.key === ',') {
                e.preventDefault();
                var val = tagInput.value.trim().toLowerCase();
                if (val && editTags.indexOf(val) === -1) {
                    editTags.push(val);
                    renderEditTags();
                }
                tagInput.value = '';
            }
        };

        // Media attachment buttons
        var editFileInput = card.querySelector('.edit-file-input');
        card.querySelector('.edit-btn-image').onclick = function () {
            editFileInput.accept = 'image/*';
            editFileInput.click();
        };
        card.querySelector('.edit-btn-video').onclick = function () {
            editFileInput.accept = 'video/*';
            editFileInput.click();
        };
        card.querySelector('.edit-btn-file').onclick = function () {
            editFileInput.accept = '*/*';
            editFileInput.click();
        };
        editFileInput.onchange = function () {
            Array.from(editFileInput.files).forEach(function (f) {
                uploadEditAttachment(f, card, editAttachments);
            });
            editFileInput.value = '';
        };

        // Cancel
        card.querySelector('.btn-edit-cancel').onclick = function () {
            // Re-render original card
            var newCard = renderPostCard(post);
            card.parentNode.replaceChild(newCard, card);
        };

        // Save
        card.querySelector('.btn-edit-save').onclick = function () {
            savePostEdit(post, card, textItems, mediaItems, removedMedia, editAttachments, editTags);
        };
    }

    async function uploadEditAttachment(file, card, editAttachments) {
        var preview = card.querySelector('.edit-attachment-previews');

        var mediaType = 'file';
        if (file.type.startsWith('image/')) mediaType = 'image';
        else if (file.type.startsWith('video/')) mediaType = 'video';

        var placeholder = document.createElement('div');
        placeholder.className = 'attachment-preview';
        placeholder.innerHTML = '<div class="attachment-preview-file"><span class="file-name">' + escapeHtml(file.name) + '</span><span class="file-meta attachment-uploading">Uploading...</span></div>';
        preview.appendChild(placeholder);

        try {
            var result = await api.upload('/api/media', file, { media_type: mediaType });
            var att = {
                media_id: result.media_id,
                media_type: mediaType,
                filename: file.name,
                mime_type: file.type,
                file_size: file.size
            };
            editAttachments.push(att);
            preview.removeChild(placeholder);
            preview.appendChild(renderEditAttachmentPreview(att, editAttachments));
        } catch (e) {
            placeholder.querySelector('.file-meta').textContent = 'Upload failed';
            placeholder.querySelector('.file-meta').style.color = '#ef4444';
            setTimeout(function () { if (placeholder.parentNode) placeholder.parentNode.removeChild(placeholder); }, 2000);
        }
    }

    function renderEditAttachmentPreview(att, editAttachments) {
        var el = document.createElement('div');
        el.className = 'attachment-preview';
        if (att.media_type === 'image') {
            el.innerHTML = '<img src="/api/media/' + escapeAttr(att.media_id) + '/thumbnail" alt="' + escapeAttr(att.filename) + '">' +
                '<button class="attachment-remove" title="Remove">&times;</button>';
        } else {
            el.innerHTML = '<div class="attachment-preview-file"><span class="file-name">' + escapeHtml(att.filename) + '</span><span class="file-meta">' + formatSize(att.file_size) + '</span></div>' +
                '<button class="attachment-remove" title="Remove">&times;</button>';
        }
        el.querySelector('.attachment-remove').onclick = function () {
            var idx = editAttachments.indexOf(att);
            if (idx !== -1) editAttachments.splice(idx, 1);
            el.parentNode.removeChild(el);
        };
        return el;
    }

    async function savePostEdit(post, card, textItems, mediaItems, removedMedia, editAttachments, editTags) {
        var textarea = card.querySelector('.edit-post-text');
        var text = textarea.value.trim();
        var publish = card.querySelector('.edit-publish').checked;

        var saveBtn = card.querySelector('.btn-edit-save');
        saveBtn.disabled = true;
        saveBtn.textContent = 'Saving...';

        try {
            var contents = [];
            var order = 0;

            // Text content
            if (text) {
                contents.push({ content_type: 'text', display_order: order++, content: text });
            }

            // Keep existing media that wasn't removed
            mediaItems.forEach(function (m) {
                if (removedMedia.indexOf(m.content) === -1) {
                    contents.push({
                        content_type: m.content_type,
                        display_order: order++,
                        content: m.content,
                        media_id: m.content,
                        filename: m.filename,
                        mime_type: m.mime_type
                    });
                }
            });

            // Add newly uploaded media
            editAttachments.forEach(function (att) {
                contents.push({
                    content_type: att.media_type,
                    display_order: order++,
                    content: att.media_id,
                    media_id: att.media_id,
                    filename: att.filename,
                    mime_type: att.mime_type
                });
            });

            if (contents.length === 0) {
                alert('Post must have at least one content item.');
                saveBtn.disabled = false;
                saveBtn.textContent = 'Save';
                return;
            }

            var updated = await api.put('/api/posts/' + post.post_id, {
                contents: contents,
                tags: editTags,
                publish: publish
            });

            // Re-render card with updated data
            var newCard = renderPostCard(updated);
            card.parentNode.replaceChild(newCard, card);
        } catch (e) {
            alert('Failed to save: ' + e.message);
            saveBtn.disabled = false;
            saveBtn.textContent = 'Save';
        }
    }

    async function deletePost(postId, card) {
        if (!confirm('Delete this post? This cannot be undone.')) return;
        try {
            await api.del('/api/posts/' + postId);
            card.style.transition = 'opacity 0.3s';
            card.style.opacity = '0';
            setTimeout(function () {
                if (card.parentNode) card.parentNode.removeChild(card);
            }, 300);
        } catch (e) {
            alert('Failed to delete post: ' + e.message);
        }
    }

    // Lightbox / Carousel 

    var lightboxEl = null;
    var lightboxImages = [];
    var lightboxIndex = 0;

    function openLightbox(imageUrls, startIndex) {
        lightboxImages = imageUrls;
        lightboxIndex = startIndex || 0;

        if (!lightboxEl) {
            lightboxEl = document.createElement('div');
            lightboxEl.className = 'lightbox-overlay';
            lightboxEl.innerHTML =
                '<button class="lightbox-close">&times;</button>' +
                '<button class="lightbox-nav lightbox-prev">&#8249;</button>' +
                '<img class="lightbox-img" src="" alt="">' +
                '<button class="lightbox-nav lightbox-next">&#8250;</button>' +
                '<div class="lightbox-counter"></div>';
            document.body.appendChild(lightboxEl);

            lightboxEl.querySelector('.lightbox-close').onclick = closeLightbox;
            lightboxEl.querySelector('.lightbox-prev').onclick = function () { navigateLightbox(-1); };
            lightboxEl.querySelector('.lightbox-next').onclick = function () { navigateLightbox(1); };
            lightboxEl.onclick = function (e) {
                if (e.target === lightboxEl) closeLightbox();
            };
            document.addEventListener('keydown', lightboxKeyHandler);
        }

        lightboxEl.style.display = 'flex';
        document.body.style.overflow = 'hidden';
        updateLightbox();
    }

    function closeLightbox() {
        if (lightboxEl) {
            lightboxEl.style.display = 'none';
            document.body.style.overflow = '';
        }
    }

    function navigateLightbox(dir) {
        lightboxIndex += dir;
        if (lightboxIndex < 0) lightboxIndex = lightboxImages.length - 1;
        if (lightboxIndex >= lightboxImages.length) lightboxIndex = 0;
        updateLightbox();
    }

    function updateLightbox() {
        if (!lightboxEl) return;
        var img = lightboxEl.querySelector('.lightbox-img');
        img.src = lightboxImages[lightboxIndex];
        var counter = lightboxEl.querySelector('.lightbox-counter');
        counter.textContent = (lightboxIndex + 1) + ' / ' + lightboxImages.length;
        // Hide nav buttons for single image
        lightboxEl.querySelector('.lightbox-prev').style.display = lightboxImages.length > 1 ? '' : 'none';
        lightboxEl.querySelector('.lightbox-next').style.display = lightboxImages.length > 1 ? '' : 'none';
        counter.style.display = lightboxImages.length > 1 ? '' : 'none';
    }

    function lightboxKeyHandler(e) {
        if (!lightboxEl || lightboxEl.style.display === 'none') return;
        if (e.key === 'Escape') closeLightbox();
        else if (e.key === 'ArrowLeft') navigateLightbox(-1);
        else if (e.key === 'ArrowRight') navigateLightbox(1);
    }

    // Comments 

    async function loadComments(postId, card, offset) {
        var list = card.querySelector('.comments-list');
        var loading = card.querySelector('.comments-loading');
        var loadMoreBtn = card.querySelector('.btn-load-more-comments');
        loading.style.display = '';

        try {
            var data = await api.get('/api/posts/' + postId + '/comments?limit=10&offset=' + offset);
            var comments = data.comments || [];

            comments.forEach(function (c) {
                list.appendChild(renderCommentItem(c, postId, card));
            });

            loadMoreBtn.style.display = data.has_more ? '' : 'none';
        } catch (e) {
            console.error('Failed to load comments:', e);
        }
        loading.style.display = 'none';
    }

    function renderCommentItem(comment, postId, card) {
        var el = document.createElement('div');
        el.className = 'comment-item';
        el.dataset.commentId = comment.comment_id;

        if (comment.is_deleted) {
            el.innerHTML = '<div class="comment-deleted">[deleted]</div>';
            return el;
        }

        var authorShort = comment.author_display_name || comment.author_id.substring(0, 12);
        var commentAvatarUrl = comment.author_avatar_url || null;
        var time = formatTime(comment.created_at);
        var replyCount = comment.reply_count || 0;

        var contentHtml = '';
        if (comment.content_type === 'text') {
            contentHtml = '<span class="comment-text">' + linkPreview.linkify(comment.content) + '</span>';
        } else if (comment.content_type === 'image' && comment.media_url) {
            contentHtml = '<div class="comment-media"><img src="' + escapeAttr(comment.media_url) + '" alt="comment image"></div>';
        } else if (comment.content_type === 'video' && comment.media_url) {
            contentHtml = '<div class="comment-media"><video src="' + escapeAttr(comment.media_url) + '" controls preload="metadata"></video></div>';
        } else if (comment.content_type === 'file' && comment.media_url) {
            contentHtml = '<a class="comment-file-link" href="' + escapeAttr(comment.media_url) + '" download="' + escapeAttr(comment.filename || 'file') + '">&#128196; ' + escapeHtml(comment.filename || 'File') + '</a>';
        } else {
            contentHtml = '<span class="comment-text">' + escapeHtml(comment.content) + '</span>';
        }

        var isOwn = app.state.currentUser && app.state.currentUser.user_id === comment.author_id;
        var commentAvatarHtml = (typeof profile !== 'undefined') ? profile.avatarHtml(commentAvatarUrl, authorShort, 'sm') : '';

        // comment-item is display:flex — only TWO direct children:
        // 1. the avatar (flex-shrink:0) 2. comment-body (flex:1, column)
        // Actions / replies / reply-form go INSIDE comment-body so they stack
        // vertically, not horizontally alongside the avatar.
        el.innerHTML =
            commentAvatarHtml +
            '<div class="comment-body">' +
            '<div class="comment-header">' +
            '<a class="comment-author" href="#profile/' + escapeAttr(comment.author_id) + '">' + escapeHtml(authorShort) + '</a>' +
            '<span class="comment-time">' + time + '</span>' +
            (isOwn ? '<button class="btn-comment-delete" title="Delete">&times;</button>' : '') +
            '</div>' +
            contentHtml +
            '<div class="comment-actions">' +
            '<button class="btn-reply">' +
            (replyCount > 0 ? replyCount + ' repl' + (replyCount === 1 ? 'y' : 'ies') : 'Reply') +
            '</button>' +
            '</div>' +
            '<div class="comment-replies" style="display:none"></div>' +
            '<div class="comment-reply-form" style="display:none">' +
            '<div class="reply-attachment-preview" style="display:none"></div>' +
            '<div class="comment-input-row">' +
            '<button class="btn-comment-attach btn-reply-attach" title="Attach file">&#128206;</button>' +
            '<input type="text" class="comment-input reply-input" placeholder="Write a reply..." autocomplete="off">' +
            '<button class="btn btn-primary btn-small btn-reply-submit">Reply</button>' +
            '</div>' +
            '<input type="file" class="reply-file-input" style="display:none">' +
            '</div>' +
            '</div>';

        // Attach link preview for text comments
        if (comment.content_type === 'text') {
            linkPreview.attachPreview(el.querySelector('.comment-body'), comment.content);
        }

        // Click comment image to enlarge
        var commentImg = el.querySelector('.comment-media img');
        if (commentImg) {
            commentImg.onclick = function () {
                openLightbox([commentImg.src], 0);
            };
        }

        // Delete button
        var deleteBtn = el.querySelector('.btn-comment-delete');
        if (deleteBtn) {
            deleteBtn.onclick = function (e) {
                e.stopPropagation();
                deleteComment(comment.comment_id, el, postId, card);
            };
        }

        // Reply toggle
        var replyBtn = el.querySelector('.btn-reply');
        var repliesDiv = el.querySelector('.comment-replies');
        var replyForm = el.querySelector('.comment-reply-form');
        var repliesLoaded = false;
        replyBtn.onclick = function () {
            var visible = replyForm.style.display !== 'none';
            replyForm.style.display = visible ? 'none' : '';
            if (!visible && !repliesLoaded && replyCount > 0) {
                repliesLoaded = true;
                loadReplies(comment.comment_id, repliesDiv);
            }
            if (!visible) {
                repliesDiv.style.display = '';
                replyForm.querySelector('.reply-input').focus();
            } else {
                repliesDiv.style.display = 'none';
            }
        };

        // Reply attachment state
        var pendingReplyAttachment = null;
        var replyAttachBtn = el.querySelector('.btn-reply-attach');
        var replyFileInput = el.querySelector('.reply-file-input');
        var replyAttachPreview = el.querySelector('.reply-attachment-preview');
        if (replyAttachBtn && replyFileInput) {
            replyAttachBtn.onclick = function () { replyFileInput.click(); };
            replyFileInput.onchange = function () {
                if (replyFileInput.files.length > 0) {
                    uploadCommentAttachment(replyFileInput.files[0], replyAttachPreview, function (att) {
                        pendingReplyAttachment = att;
                    });
                }
                replyFileInput.value = '';
            };
        }

        // Submit reply
        var replyInput = el.querySelector('.reply-input');
        var replySubmitBtn = el.querySelector('.btn-reply-submit');
        replySubmitBtn.onclick = function () {
            submitReply(postId, comment.comment_id, replyInput, repliesDiv, card, pendingReplyAttachment, function () {
                pendingReplyAttachment = null;
                if (replyAttachPreview) { replyAttachPreview.style.display = 'none'; replyAttachPreview.innerHTML = ''; }
            });
        };
        replyInput.onkeydown = function (e) {
            if (e.key === 'Enter') {
                e.preventDefault();
                submitReply(postId, comment.comment_id, replyInput, repliesDiv, card, pendingReplyAttachment, function () {
                    pendingReplyAttachment = null;
                    if (replyAttachPreview) { replyAttachPreview.style.display = 'none'; replyAttachPreview.innerHTML = ''; }
                });
            }
        };

        return el;
    }

    async function loadReplies(commentId, repliesDiv) {
        try {
            var data = await api.get('/api/comments/' + commentId + '/replies?limit=50&offset=0');
            var replies = data.comments || [];
            replies.forEach(function (r) {
                var replyEl = document.createElement('div');
                replyEl.className = 'comment-item comment-reply';
                replyEl.dataset.commentId = r.comment_id;

                if (r.is_deleted) {
                    replyEl.innerHTML = '<div class="comment-deleted">[deleted]</div>';
                } else {
                    var authorShort = r.author_display_name || r.author_id.substring(0, 12);
                    var time = formatTime(r.created_at);
                    var isOwn = app.state.currentUser && app.state.currentUser.user_id === r.author_id;

                    var replyContentHtml = renderCommentContentHtml(r);

                    replyEl.innerHTML =
                        '<div class="comment-header">' +
                        '<a class="comment-author" href="#profile/' + escapeAttr(r.author_id) + '">' + escapeHtml(authorShort) + '</a>' +
                        '<span class="comment-time">' + time + '</span>' +
                        (isOwn ? '<button class="btn-comment-delete" title="Delete">&times;</button>' : '') +
                        '</div>' +
                        '<div class="comment-body">' + replyContentHtml + '</div>';

                    if (r.content_type === 'text') {
                        linkPreview.attachPreview(replyEl.querySelector('.comment-body'), r.content);
                    }

                    // Click reply image to enlarge
                    var replyImg = replyEl.querySelector('.comment-media img');
                    if (replyImg) {
                        replyImg.onclick = function () { openLightbox([replyImg.src], 0); };
                    }

                    var delBtn = replyEl.querySelector('.btn-comment-delete');
                    if (delBtn) {
                        delBtn.onclick = function (e) {
                            e.stopPropagation();
                            api.del('/api/comments/' + r.comment_id).then(function () {
                                replyEl.innerHTML = '<div class="comment-deleted">[deleted]</div>';
                            }).catch(function (err) {
                                console.error('Failed to delete reply:', err);
                            });
                        };
                    }
                }
                repliesDiv.appendChild(replyEl);
            });
        } catch (e) {
            console.error('Failed to load replies:', e);
        }
    }

    async function submitComment(postId, card, input, pendingAttachment, clearAttachmentFn) {
        var text = input.value.trim();
        if (!text && !pendingAttachment) return;

        input.disabled = true;
        try {
            var body;
            if (pendingAttachment) {
                body = {
                    content_type: pendingAttachment.media_type,
                    content: pendingAttachment.media_id,
                    filename: pendingAttachment.filename
                };
            } else {
                body = { content_type: 'text', content: text };
            }

            var comment = await api.post('/api/posts/' + postId + '/comments', body);
            input.value = '';
            if (clearAttachmentFn) clearAttachmentFn();
            var list = card.querySelector('.comments-list');
            list.appendChild(renderCommentItem(comment, postId, card));

            // Update comment count in footer
            var toggleBtn = card.querySelector('.btn-comments-toggle');
            var count = list.querySelectorAll('.comment-item').length;
            toggleBtn.textContent = count + ' comment' + (count !== 1 ? 's' : '');
        } catch (e) {
            console.error('Failed to post comment:', e);
        }
        input.disabled = false;
        input.focus();
    }

    async function submitReply(postId, parentCommentId, input, repliesDiv, card, pendingAttachment, clearAttachmentFn) {
        var text = input.value.trim();
        if (!text && !pendingAttachment) return;

        input.disabled = true;
        try {
            var body;
            if (pendingAttachment) {
                body = {
                    content_type: pendingAttachment.media_type,
                    content: pendingAttachment.media_id,
                    filename: pendingAttachment.filename,
                    parent_comment_id: parentCommentId
                };
            } else {
                body = {
                    content_type: 'text',
                    content: text,
                    parent_comment_id: parentCommentId
                };
            }

            var reply = await api.post('/api/posts/' + postId + '/comments', body);
            input.value = '';
            if (clearAttachmentFn) clearAttachmentFn();

            // Add reply to the replies div
            var replyEl = document.createElement('div');
            replyEl.className = 'comment-item comment-reply';
            replyEl.dataset.commentId = reply.comment_id;
            var authorShort = reply.author_display_name || reply.author_id.substring(0, 12);
            var time = formatTime(reply.created_at);
            var replyContentHtml = renderCommentContentHtml(reply);
            replyEl.innerHTML =
                '<div class="comment-header">' +
                '<a class="comment-author" href="#profile/' + escapeAttr(reply.author_id) + '">' + escapeHtml(authorShort) + '</a>' +
                '<span class="comment-time">' + time + '</span>' +
                '<button class="btn-comment-delete" title="Delete">&times;</button>' +
                '</div>' +
                '<div class="comment-body">' + replyContentHtml + '</div>';

            if (reply.content_type === 'text') {
                linkPreview.attachPreview(replyEl.querySelector('.comment-body'), reply.content);
            }

            // Click reply image to enlarge
            var newReplyImg = replyEl.querySelector('.comment-media img');
            if (newReplyImg) {
                newReplyImg.onclick = function () { openLightbox([newReplyImg.src], 0); };
            }

            var delBtn = replyEl.querySelector('.btn-comment-delete');
            if (delBtn) {
                delBtn.onclick = function (e) {
                    e.stopPropagation();
                    api.del('/api/comments/' + reply.comment_id).then(function () {
                        replyEl.innerHTML = '<div class="comment-deleted">[deleted]</div>';
                    }).catch(function (err) {
                        console.error('Failed to delete reply:', err);
                    });
                };
            }

            repliesDiv.appendChild(replyEl);
            repliesDiv.style.display = '';
        } catch (e) {
            console.error('Failed to post reply:', e);
        }
        input.disabled = false;
        input.focus();
    }

    /// Render the body content HTML for a comment or reply
    function renderCommentContentHtml(c) {
        if (c.content_type === 'text') {
            return '<span class="comment-text">' + linkPreview.linkify(c.content) + '</span>';
        } else if (c.content_type === 'image' && c.media_url) {
            return '<div class="comment-media"><img src="' + escapeAttr(c.media_url) + '" alt="comment image"></div>';
        } else if (c.content_type === 'video' && c.media_url) {
            return '<div class="comment-media"><video src="' + escapeAttr(c.media_url) + '" controls preload="metadata"></video></div>';
        } else if (c.content_type === 'file' && c.media_url) {
            return '<a class="comment-file-link" href="' + escapeAttr(c.media_url) + '" download="' + escapeAttr(c.filename || 'file') + '">&#128196; ' + escapeHtml(c.filename || 'File') + '</a>';
        }
        return '<span class="comment-text">' + linkPreview.linkify(c.content) + '</span>';
    }

    /// Upload a file for comment/reply attachment
    async function uploadCommentAttachment(file, previewEl, onSuccess) {
        if (!previewEl) return;

        var mediaType = 'file';
        if (file.type.startsWith('image/')) mediaType = 'image';
        else if (file.type.startsWith('video/')) mediaType = 'video';

        previewEl.style.display = '';
        previewEl.innerHTML = '<span class="file-name">' + escapeHtml(file.name) + '</span><span class="attachment-uploading">Uploading...</span>';

        try {
            var result = await api.upload('/api/media', file, { media_type: mediaType });
            var att = {
                media_id: result.media_id,
                media_type: mediaType,
                filename: file.name
            };

            var html = '';
            if (mediaType === 'image' && result.thumbnail_url) {
                html += '<img src="' + escapeAttr(result.thumbnail_url) + '" alt="" class="comment-attach-thumb">';
            }
            html += '<span class="file-name">' + escapeHtml(file.name) + '</span>';
            html += '<button class="attachment-remove" title="Remove">&times;</button>';
            previewEl.innerHTML = html;

            previewEl.querySelector('.attachment-remove').onclick = function () {
                previewEl.style.display = 'none';
                previewEl.innerHTML = '';
                onSuccess(null);
            };

            onSuccess(att);
        } catch (e) {
            previewEl.innerHTML = '<span style="color:#ef4444">Upload failed</span>';
            setTimeout(function () { previewEl.style.display = 'none'; previewEl.innerHTML = ''; }, 2000);
        }
    }

    async function deleteComment(commentId, el, postId, card) {
        try {
            await api.del('/api/comments/' + commentId);
            el.innerHTML = '<div class="comment-deleted">[deleted]</div>';
            el.className = 'comment-item';
        } catch (e) {
            console.error('Failed to delete comment:', e);
        }
    }

    // Infinite scroll 

    function initInfiniteScroll() {
        var feedArea = document.getElementById('feed-area');
        if (!feedArea) return;
        feedArea.addEventListener('scroll', function () {
            if (feedArea.scrollHeight - feedArea.scrollTop - feedArea.clientHeight < 200) {
                if (activeTab === 'federated') {
                    loadFederatedFeed(false);
                } else {
                    loadFeed(false);
                }
            }
        });
    }

    // Create post 

    function initCreatePost() {
        currentTags = [];
        pendingAttachments = [];

        var btn = document.getElementById('btn-create-post');
        if (btn) btn.onclick = submitPost;

        var tagInput = document.getElementById('post-tag-input');
        if (tagInput) {
            tagInput.onkeydown = function (e) {
                if (e.key === 'Enter' || e.key === ',') {
                    e.preventDefault();
                    addPostTag(tagInput.value.trim());
                    tagInput.value = '';
                }
            };
        }

        // Media attachment buttons
        var fileInput = document.getElementById('post-file-input');
        var btnImage = document.getElementById('btn-attach-image');
        var btnVideo = document.getElementById('btn-attach-video');
        var btnFile = document.getElementById('btn-attach-file');

        if (btnImage) btnImage.onclick = function () {
            fileInput.accept = 'image/*';
            fileInput.multiple = true;
            fileInput.click();
        };
        if (btnVideo) btnVideo.onclick = function () {
            fileInput.accept = 'video/*';
            fileInput.multiple = true;
            fileInput.click();
        };
        if (btnFile) btnFile.onclick = function () {
            fileInput.accept = '*/*';
            fileInput.multiple = true;
            fileInput.click();
        };

        if (fileInput) {
            fileInput.onchange = function () {
                var files = Array.from(fileInput.files);
                files.forEach(function (f) { uploadAttachment(f); });
                fileInput.value = '';
            };
        }
    }

    async function uploadAttachment(file) {
        var preview = document.getElementById('post-attachment-previews');
        if (!preview) return;

        // Determine media type from MIME
        var mediaType = 'file';
        if (file.type.startsWith('image/')) mediaType = 'image';
        else if (file.type.startsWith('video/')) mediaType = 'video';

        // Show uploading indicator
        var placeholder = document.createElement('div');
        placeholder.className = 'attachment-preview';
        placeholder.innerHTML = '<div class="attachment-preview-file"><span class="file-name">' + escapeHtml(file.name) + '</span><span class="file-meta attachment-uploading">Uploading...</span></div>';
        preview.appendChild(placeholder);

        try {
            var result = await api.upload('/api/media', file, { media_type: mediaType });
            var att = {
                file: file,
                media_id: result.media_id,
                media_type: mediaType,
                url: result.url,
                thumbnail_url: result.thumbnail_url,
                filename: file.name,
                file_size: file.size
            };
            pendingAttachments.push(att);
            // Replace placeholder with proper preview
            preview.removeChild(placeholder);
            preview.appendChild(renderAttachmentPreview(att));
        } catch (e) {
            placeholder.querySelector('.file-meta').textContent = 'Upload failed';
            placeholder.querySelector('.file-meta').style.color = '#ef4444';
            setTimeout(function () { if (placeholder.parentNode) placeholder.parentNode.removeChild(placeholder); }, 2000);
        }
    }

    function renderAttachmentPreview(att) {
        var el = document.createElement('div');
        el.className = 'attachment-preview';

        if (att.media_type === 'image') {
            el.innerHTML = '<img src="' + escapeAttr(att.url || att.thumbnail_url) + '" alt="' + escapeAttr(att.filename) + '">' +
                '<button class="attachment-remove" title="Remove">&times;</button>';
        } else {
            el.innerHTML = '<div class="attachment-preview-file">' +
                '<span class="file-name">' + escapeHtml(att.filename) + '</span>' +
                '<span class="file-meta">' + (att.media_type === 'video' ? 'Video' : 'File') + ' &middot; ' + formatSize(att.file_size) + '</span></div>' +
                '<button class="attachment-remove" title="Remove">&times;</button>';
        }

        el.querySelector('.attachment-remove').onclick = function () {
            pendingAttachments = pendingAttachments.filter(function (a) { return a.media_id !== att.media_id; });
            el.parentNode.removeChild(el);
        };

        return el;
    }

    // Tag management 

    function addPostTag(tag) {
        tag = tag.toLowerCase().trim();
        if (!tag || currentTags.indexOf(tag) !== -1) return;
        currentTags.push(tag);
        renderPostTagChips();
    }

    function removePostTag(tag) {
        currentTags = currentTags.filter(function (t) { return t !== tag; });
        renderPostTagChips();
    }

    function renderPostTagChips() {
        var container = document.getElementById('post-tag-chips');
        if (!container) return;
        container.innerHTML = '';
        currentTags.forEach(function (t) {
            var chip = document.createElement('span');
            chip.className = 'tag-chip removable';
            chip.innerHTML = escapeHtml(t) + ' <span class="tag-remove">&times;</span>';
            chip.querySelector('.tag-remove').onclick = function () { removePostTag(t); };
            container.appendChild(chip);
        });
    }

    async function submitPost() {
        var textarea = document.getElementById('post-text');
        var text = textarea.value.trim();

        // Must have at least text or attachments
        if (!text && pendingAttachments.length === 0) return;

        var publish = document.getElementById('post-publish').checked;
        var btn = document.getElementById('btn-create-post');
        btn.disabled = true;
        btn.textContent = 'Posting...';

        try {
            var contents = [];
            var order = 0;

            // Add text content first if present
            if (text) {
                contents.push({ content_type: 'text', display_order: order++, content: text });
            }

            // Add media attachments
            pendingAttachments.forEach(function (att) {
                contents.push({
                    content_type: att.media_type,
                    display_order: order++,
                    content: att.media_id,
                    media_id: att.media_id,
                    filename: att.filename,
                    mime_type: att.file ? att.file.type : null
                });
            });

            await api.post('/api/posts', {
                contents: contents,
                publish: publish,
                tags: currentTags
            });
            textarea.value = '';
            currentTags = [];
            pendingAttachments = [];
            renderPostTagChips();
            var previews = document.getElementById('post-attachment-previews');
            if (previews) previews.innerHTML = '';
            loadFeed(true);
            if (typeof search !== 'undefined') search.invalidateTagCache();
        } catch (e) {
            alert('Failed to create post: ' + e.message);
        }

        btn.disabled = false;
        btn.textContent = 'Post';
    }

    // Search results rendering 

    function renderSearchResults(data) {
        var container = document.getElementById('post-feed');
        container.innerHTML = '';

        document.getElementById('feed-empty').style.display = 'none';

        if (!data.posts || data.posts.length === 0) {
            container.innerHTML = '<div class="empty-state"><p>No posts match your search.</p></div>';
            return;
        }

        data.posts.forEach(function (p) {
            container.appendChild(renderPostCard(p));
        });

        if (data.has_more) {
            var loadMore = document.createElement('button');
            loadMore.className = 'btn btn-secondary load-more';
            loadMore.textContent = 'Load more results';
            loadMore.onclick = function () {
                if (typeof search !== 'undefined') {
                    search.loadMoreResults();
                }
            };
            container.appendChild(loadMore);
        }
    }

    // escapeHt

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

    // Federated feed 

    var fedOffset = 0;
    var fedLoading = false;
    var fedHasMore = true;

    async function loadFederatedFeed(reset) {
        if (fedLoading) return;
        if (reset) {
            fedOffset = 0;
            fedHasMore = true;
            var container = document.getElementById('post-feed');
            if (container) container.innerHTML = '';
        }
        if (!fedHasMore) return;

        fedLoading = true;
        var loadingEl = document.getElementById('feed-loading');
        var emptyEl = document.getElementById('feed-empty');
        if (loadingEl) loadingEl.style.display = '';
        if (emptyEl) emptyEl.style.display = 'none';

        try {
            var data = await api.get('/api/posts/federated?limit=20&offset=' + fedOffset);
            var postList = data.posts || [];
            fedHasMore = data.has_more;
            fedOffset += postList.length;

            if (postList.length === 0 && fedOffset === 0) {
                if (emptyEl) {
                    emptyEl.textContent = 'No federated posts yet. Connect to other servers to see their content here.';
                    emptyEl.style.display = '';
                }
            } else {
                var container = document.getElementById('post-feed');
                postList.forEach(function (p) {
                    container.appendChild(renderFederatedPostCard(p));
                });
            }
        } catch (e) {
            console.error('Failed to load federated feed:', e);
        }
        fedLoading = false;
        if (loadingEl) loadingEl.style.display = 'none';
    }

    function renderFederatedPostCard(post) {
        var card = renderPostCard(post);

        // Prepend source-server badge below the header
        if (post.source_server) {
            var badge = document.createElement('div');
            badge.className = 'fed-source-badge';
            badge.textContent = 'from ' + post.source_server;
            var header = card.querySelector('.post-header');
            if (header && header.nextSibling) {
                card.insertBefore(badge, header.nextSibling);
            } else {
                card.insertBefore(badge, card.firstChild);
            }
        }

        // Federated posts are read-only — remove edit/delete actions
        var actionsMenu = card.querySelector('.post-actions-menu');
        if (actionsMenu) actionsMenu.remove();

        return card;
    }

    // Feed tab switching 

    var activeTab = 'local'; // 'local' | 'federated'

    function initFeedTabs() {
        var tabBar = document.getElementById('feed-tab-bar');
        if (!tabBar) return;
        if (tabBar.dataset.tabsInit) return;
        tabBar.dataset.tabsInit = '1';

        // Sync activeTab to whichever button is visually marked active in the DOM.
        var activeBtn = tabBar.querySelector('[data-feed-tab].active');
        if (activeBtn) activeTab = activeBtn.dataset.feedTab;

        tabBar.addEventListener('click', function (e) {
            var btn = e.target.closest('[data-feed-tab]');
            if (!btn) return;
            var tab = btn.dataset.feedTab;
            if (tab === activeTab) return;
            activeTab = tab;

            tabBar.querySelectorAll('[data-feed-tab]').forEach(function (b) {
                b.classList.toggle('active', b.dataset.feedTab === tab);
            });

            // Show/hide create-post box — only on local tab
            var createPost = document.getElementById('create-post');
            if (createPost) createPost.style.display = (tab === 'local') ? '' : 'none';

            if (tab === 'federated') {
                loadFederatedFeed(true);
            } else {
                loadFeed(true);
            }
        });
    }

    return {
        loadFeed: loadFeed,
        loadFederatedFeed: loadFederatedFeed,
        initInfiniteScroll: initInfiniteScroll,
        initCreatePost: initCreatePost,
        initFeedTabs: initFeedTabs,
        renderSearchResults: renderSearchResults,
        openLightbox: openLightbox
    };
})();
