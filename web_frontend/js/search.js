// Search UI — search bar, advanced filters, tag autocomplete
var search = (function () {
    var searchTags = [];
    var searchOffset = 0;
    var lastParams = null;
    var allTags = null; // cached tag list
    var debounceTimer = null;

    function init() {
        var input = document.getElementById('search-input');
        var btnSearch = document.getElementById('btn-search');
        var btnAdvanced = document.getElementById('btn-advanced-toggle');
        var btnApply = document.getElementById('btn-apply-search');
        var btnClear = document.getElementById('btn-clear-search');
        var tagInput = document.getElementById('search-tag-input');

        if (input) {
            input.onkeydown = function (e) {
                if (e.key === 'Enter') performSearch();
            };
            input.oninput = function () {
                clearTimeout(debounceTimer);
                debounceTimer = setTimeout(function () {
                    if (input.value.trim().length >= 2) performSearch();
                }, 500);
            };
        }

        if (btnSearch) btnSearch.onclick = performSearch;

        if (btnAdvanced) {
            btnAdvanced.onclick = function () {
                var panel = document.getElementById('search-advanced');
                var visible = panel.style.display !== 'none';
                panel.style.display = visible ? 'none' : '';
                btnAdvanced.innerHTML = visible ? '&#9660;' : '&#9650;';
            };
        }

        if (btnApply) btnApply.onclick = performSearch;
        if (btnClear) btnClear.onclick = clearSearch;

        if (tagInput) {
            tagInput.onkeydown = function (e) {
                if (e.key === 'Enter' || e.key === ',') {
                    e.preventDefault();
                    addSearchTag(tagInput.value.trim());
                    tagInput.value = '';
                    hideAutocomplete();
                }
                if (e.key === 'Escape') hideAutocomplete();
            };
            tagInput.oninput = function () {
                var val = tagInput.value.trim().toLowerCase();
                if (val.length >= 1) {
                    showAutocomplete(val);
                } else {
                    hideAutocomplete();
                }
            };
            tagInput.onblur = function () {
                // Small delay to allow click on autocomplete item
                setTimeout(hideAutocomplete, 200);
            };
        }
    }

    // Tag management 

    function addSearchTag(tag) {
        tag = tag.toLowerCase().trim();
        if (!tag || searchTags.indexOf(tag) !== -1) return;
        searchTags.push(tag);
        renderSearchTagChips();
    }

    function removeSearchTag(tag) {
        searchTags = searchTags.filter(function (t) { return t !== tag; });
        renderSearchTagChips();
    }

    function renderSearchTagChips() {
        var container = document.getElementById('search-tag-chips');
        if (!container) return;
        container.innerHTML = '';
        searchTags.forEach(function (t) {
            var chip = document.createElement('span');
            chip.className = 'tag-chip removable';
            chip.innerHTML = escapeHtml(t) + ' <span class="tag-remove">&times;</span>';
            chip.querySelector('.tag-remove').onclick = function () { removeSearchTag(t); };
            container.appendChild(chip);
        });
    }

    // Autocomplete 

    async function fetchTags() {
        if (allTags !== null) return allTags;
        try {
            var data = await api.get('/api/tags');
            allTags = data.tags || [];
            return allTags;
        } catch (_) {
            return [];
        }
    }

    async function showAutocomplete(prefix) {
        var tags = await fetchTags();
        var matches = tags.filter(function (t) {
            return t.name.indexOf(prefix) !== -1 && searchTags.indexOf(t.name) === -1;
        }).slice(0, 8);

        var container = document.getElementById('tag-autocomplete');
        if (!container) return;

        if (matches.length === 0) {
            container.style.display = 'none';
            return;
        }

        container.innerHTML = '';
        matches.forEach(function (t) {
            var item = document.createElement('div');
            item.className = 'autocomplete-item';
            item.innerHTML = '<span>' + escapeHtml(t.name) + '</span><span class="tag-count">' + t.count + '</span>';
            item.onmousedown = function (e) {
                e.preventDefault();
                addSearchTag(t.name);
                document.getElementById('search-tag-input').value = '';
                container.style.display = 'none';
            };
            container.appendChild(item);
        });
        container.style.display = '';
    }

    function hideAutocomplete() {
        var container = document.getElementById('tag-autocomplete');
        if (container) container.style.display = 'none';
    }

    // Search 

    function buildSearchParams() {
        var q = (document.getElementById('search-input').value || '').trim();
        var params = [];
        if (q) params.push('q=' + encodeURIComponent(q));
        if (searchTags.length > 0) params.push('tags=' + encodeURIComponent(searchTags.join(',')));
        if (document.getElementById('filter-images') && document.getElementById('filter-images').checked) params.push('has_images=true');
        if (document.getElementById('filter-videos') && document.getElementById('filter-videos').checked) params.push('has_videos=true');
        if (document.getElementById('filter-files') && document.getElementById('filter-files').checked) params.push('has_files=true');
        var fromDate = document.getElementById('filter-from-date');
        if (fromDate && fromDate.value) params.push('from_date=' + encodeURIComponent(fromDate.value + 'T00:00:00Z'));
        var toDate = document.getElementById('filter-to-date');
        if (toDate && toDate.value) params.push('to_date=' + encodeURIComponent(toDate.value + 'T23:59:59Z'));
        var author = document.getElementById('filter-author');
        if (author && author.value.trim()) params.push('author_id=' + encodeURIComponent(author.value.trim()));
        return params;
    }

    async function performSearch() {
        var params = buildSearchParams();
        if (params.length === 0) {
            // No filters — go back to feed
            posts.loadFeed(true);
            return;
        }

        searchOffset = 0;
        lastParams = params;
        params.push('limit=20');
        params.push('offset=0');

        try {
            var data = await api.get('/api/posts/search?' + params.join('&'));
            posts.renderSearchResults(data);
        } catch (e) {
            console.error('Search failed:', e);
        }
    }

    async function loadMoreResults() {
        if (!lastParams) return;
        searchOffset += 20;
        var params = lastParams.slice();
        params.push('limit=20');
        params.push('offset=' + searchOffset);

        try {
            var data = await api.get('/api/posts/search?' + params.join('&'));
            // Append results
            var container = document.getElementById('post-feed');
            // Remove existing load-more button
            var existing = container.querySelector('.load-more');
            if (existing) existing.remove();

            (data.posts || []).forEach(function (p) {
                container.appendChild(posts.renderPostCard ? renderPostCardExternal(p) : createPlaceholder());
            });

            if (data.has_more) {
                var loadMore = document.createElement('button');
                loadMore.className = 'btn btn-secondary load-more';
                loadMore.textContent = 'Load more results';
                loadMore.onclick = function () { loadMoreResults(); };
                container.appendChild(loadMore);
            }
        } catch (e) {
            console.error('Load more failed:', e);
        }
    }

    function renderPostCardExternal(p) {
        // Delegate to posts module
        if (typeof posts !== 'undefined' && posts.renderSearchResults) {
            var temp = document.createElement('div');
            // Use the same approach — build from posts module
            return buildCardFromPost(p);
        }
        return document.createElement('div');
    }

    function buildCardFromPost(post) {
        // Inline simplified card for load-more
        var card = document.createElement('div');
        card.className = 'post-card';

        var authorShort = post.author_id.substring(0, 12);
        var time = formatTime(post.created_at);
        var previewHtml = '';
        if (post.preview) {
            if (post.preview.content_type === 'text') {
                previewHtml = '<div class="post-text">' + escapeHtml(post.preview.content) + '</div>';
            } else {
                previewHtml = '<div class="post-text">[' + escapeHtml(post.preview.content_type) + ']</div>';
            }
        }
        var tagsHtml = '';
        if (post.tags && post.tags.length > 0) {
            tagsHtml = '<div class="post-tags">' +
                post.tags.map(function (t) {
                    return '<span class="tag-chip" data-tag="' + escapeAttr(t) + '">' + escapeHtml(t) + '</span>';
                }).join('') + '</div>';
        }
        card.innerHTML =
            '<div class="post-header"><span class="post-author">' + escapeHtml(authorShort) + '</span><span class="post-time">' + time + '</span></div>' +
            previewHtml + tagsHtml +
            '<div class="post-footer"><span class="post-comments">' + (post.comment_count || 0) + ' comments</span></div>';

        card.querySelectorAll('.tag-chip').forEach(function (chip) {
            chip.onclick = function (e) {
                e.stopPropagation();
                searchByTag(chip.dataset.tag);
            };
        });
        return card;
    }

    function clearSearch() {
        document.getElementById('search-input').value = '';
        searchTags = [];
        renderSearchTagChips();
        if (document.getElementById('filter-images')) document.getElementById('filter-images').checked = false;
        if (document.getElementById('filter-videos')) document.getElementById('filter-videos').checked = false;
        if (document.getElementById('filter-files')) document.getElementById('filter-files').checked = false;
        if (document.getElementById('filter-from-date')) document.getElementById('filter-from-date').value = '';
        if (document.getElementById('filter-to-date')) document.getElementById('filter-to-date').value = '';
        if (document.getElementById('filter-author')) document.getElementById('filter-author').value = '';
        lastParams = null;
        searchOffset = 0;
        posts.loadFeed(true);
    }

    function searchByTag(tag) {
        searchTags = [tag.toLowerCase()];
        renderSearchTagChips();
        document.getElementById('search-input').value = '';
        performSearch();
    }

    // Invalidate tag cache so next search refetches
    function invalidateTagCache() {
        allTags = null;
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

    function createPlaceholder() { return document.createElement('div'); }

    return {
        init: init,
        performSearch: performSearch,
        clearSearch: clearSearch,
        searchByTag: searchByTag,
        loadMoreResults: loadMoreResults,
        invalidateTagCache: invalidateTagCache
    };
})();
