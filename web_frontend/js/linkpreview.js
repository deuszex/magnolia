'use strict';

// Link preview module — URL linkification + OG preview cards
var linkPreview = (function () {
    // In-memory cache (supplements the server-side DB cache)
    var memCache = {};
    var CACHE_MS = 24 * 60 * 60 * 1000;

    function esc(s) {
        return String(s)
            .replace(/&/g, '&amp;')
            .replace(/</g, '&lt;')
            .replace(/>/g, '&gt;')
            .replace(/"/g, '&quot;');
    }

    // Convert plain text to HTML with clickable https:// URLs.
    // Non-URL text is HTML-escaped; URLs are wrapped in <a> tags.
    function linkify(text) {
        var result = '';
        var lastIndex = 0;
        var re = /https:\/\/[^\s<>"']+/g;
        var match;
        while ((match = re.exec(text)) !== null) {
            result += esc(text.slice(lastIndex, match.index));
            var url = match[0];
            result += '<a href="' + esc(url) + '" target="_blank" rel="noopener noreferrer">'
                + esc(url) + '</a>';
            lastIndex = match.index + url.length;
        }
        result += esc(text.slice(lastIndex));
        return result;
    }

    // Return the first https:// URL found in plain text, or null.
    function extractFirstUrl(text) {
        var m = /https:\/\/[^\s<>"']+/.exec(text);
        return m ? m[0] : null;
    }

    // Build a preview card DOM element from API response data.
    function renderCard(data) {
        var card = document.createElement('a');
        card.className = 'link-preview-card';
        card.href = data.url;
        card.target = '_blank';
        card.rel = 'noopener noreferrer';

        var html = '';
        if (data.image_url) {
            html += '<img class="link-preview-image" src="' + esc(data.image_url)
                + '" alt="" loading="lazy">';
        }
        html += '<div class="link-preview-body">'
            + '<div class="link-preview-domain">' + esc(data.domain) + '</div>';
        if (data.title) {
            html += '<div class="link-preview-title">' + esc(data.title) + '</div>';
        }
        if (data.description) {
            html += '<div class="link-preview-description">' + esc(data.description) + '</div>';
        }
        html += '</div>';
        card.innerHTML = html;
        return card;
    }

    // Fetch preview metadata for a URL (with in-memory cache).
    // Calls callback(err, data).
    function fetchPreview(url, callback) {
        var now = Date.now();
        if (memCache[url] && (now - memCache[url].ts) < CACHE_MS) {
            callback(null, memCache[url].data);
            return;
        }
        api.get('/api/link-preview?url=' + encodeURIComponent(url))
            .then(function (data) {
                memCache[url] = { data: data, ts: Date.now() };
                callback(null, data);
            })
            .catch(function (err) {
                callback(err);
            });
    }

    // Scan plain text for the first https:// URL, fetch its preview,
    // and append a preview card to container if there is useful metadata.
    function attachPreview(container, text) {
        var url = extractFirstUrl(text);
        if (!url) return;
        fetchPreview(url, function (err, data) {
            if (err || !data) return;
            if (!data.title && !data.description && !data.image_url) return;
            container.appendChild(renderCard(data));
        });
    }

    return { linkify: linkify, attachPreview: attachPreview };
})();
