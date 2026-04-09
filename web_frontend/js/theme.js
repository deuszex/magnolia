// User-facing theme module
// Fetches and applies the server theme, and lets users override accent + background locally.
var theme = (function () {

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

    function showMsg(container, msg, type) {
        var box = container.querySelector('.theme-feedback');
        if (!box) { box = el('div', { class: 'theme-feedback admin-feedback' }); container.prepend(box); }
        box.className = 'theme-feedback admin-feedback ' + (type === 'error' ? 'error-box' : 'success-box');
        box.textContent = msg;
        setTimeout(function () { box.textContent = ''; box.className = 'theme-feedback admin-feedback'; }, 4000);
    }

    // Apply server-side default theme (admin-configured)
    function applyTheme(t) {
        var root = document.documentElement;
        if (t.color_accent) root.style.setProperty('--c-accent', t.color_accent);
        if (t.color_button_hover) root.style.setProperty('--c-accent-hover', t.color_button_hover);
        if (t.color_background) root.style.setProperty('--c-bg-1', t.color_background);
        if (t.color_status_ready) root.style.setProperty('--c-success', t.color_status_ready);
        if (t.color_status_pending) root.style.setProperty('--c-warning', t.color_status_pending);
        if (t.color_status_removed) root.style.setProperty('--c-danger', t.color_status_removed);

        try {
            var hex = t.color_accent;
            var r = parseInt(hex.slice(1, 3), 16);
            var g = parseInt(hex.slice(3, 5), 16);
            var b = parseInt(hex.slice(5, 7), 16);
            root.style.setProperty('--c-accent-glow', 'rgba(' + r + ',' + g + ',' + b + ',0.28)');
            root.style.setProperty('--c-accent-light', 'rgba(' + r + ',' + g + ',' + b + ',0.12)');
        } catch (e) { }

        var logoEl = document.querySelector('.logo');
        if (logoEl && t.brand_text) logoEl.textContent = t.brand_text;
        if (t.site_title) document.title = t.site_title;
    }

    // Apply user-local overrides (localStorage)
    function applyUserThemePref(pref) {
        var root = document.documentElement;
        if (pref.accent) {
            root.style.setProperty('--c-accent', pref.accent);
            try {
                var hex = pref.accent;
                var r = parseInt(hex.slice(1, 3), 16);
                var g = parseInt(hex.slice(3, 5), 16);
                var b = parseInt(hex.slice(5, 7), 16);
                root.style.setProperty('--c-accent-glow', 'rgba(' + r + ',' + g + ',' + b + ',0.28)');
                root.style.setProperty('--c-accent-light', 'rgba(' + r + ',' + g + ',' + b + ',0.12)');
            } catch (e) { }
        }
        if (pref.bg1) root.style.setProperty('--c-bg-1', pref.bg1);
    }

    function loadUserThemePref() {
        try { return JSON.parse(localStorage.getItem('magnolia_theme') || '{}'); }
        catch (e) { return {}; }
    }

    function saveUserThemePref(pref) {
        localStorage.setItem('magnolia_theme', JSON.stringify(pref));
    }

    // Fetch server theme then apply user overrides on top
    function initTheme() {
        api.get('/api/theme').then(function (t) {
            applyTheme(t);
            var pref = loadUserThemePref();
            if (pref.accent || pref.bg1) applyUserThemePref(pref);
        }).catch(function () {
            var pref = loadUserThemePref();
            if (pref.accent || pref.bg1) applyUserThemePref(pref);
        });
    }

    // Render user appearance preferences page (#theme)
    function renderUserTheme(feedArea) {
        feedArea.innerHTML = '';
        var page = el('div', { class: 'pref-page' });
        var section = el('div', { class: 'pref-section' });
        section.appendChild(el('div', { class: 'pref-section-title', text: 'Appearance' }));

        var feedback = el('div', { class: 'theme-feedback admin-feedback' });
        section.appendChild(feedback);

        var saved = loadUserThemePref();
        var preview = el('div', { class: 'theme-preview', id: 'user-theme-preview' });
        section.appendChild(preview);

        var accentInput, bg1Input;

        function colorRow(labelText, id, value) {
            var row = el('div', { class: 'theme-field' });
            row.appendChild(el('label', { text: labelText, for: id }));
            var inp = el('input', { type: 'color', id: id, value: value });
            inp.oninput = updatePreview;
            row.appendChild(inp);
            return row;
        }

        var accentRow = colorRow('Accent Colour', 'up-accent',
            saved.accent || getComputedStyle(document.documentElement).getPropertyValue('--c-accent').trim() || '#6366f1');
        var bg1Row = colorRow('Background Layer 1', 'up-bg1',
            saved.bg1 || getComputedStyle(document.documentElement).getPropertyValue('--c-bg-1').trim() || '#080d1a');
        accentInput = accentRow.querySelector('input');
        bg1Input = bg1Row.querySelector('input');
        section.appendChild(accentRow);
        section.appendChild(bg1Row);

        function safeHex(val) {
            return /^#[0-9a-fA-F]{6}$/.test(val) ? val : '#000000';
        }

        function updatePreview() {
            var accent = safeHex(accentInput.value);
            var bg = safeHex(bg1Input.value);
            preview.style.background = bg;
            preview.style.color = '#fff';
            preview.innerHTML =
                '<span style="background:' + accent + ';padding:6px 14px;border-radius:8px;font-size:12px;font-weight:600">Accent</span>' +
                '<span style="font-size:12px;opacity:0.5">' + bg + '</span>';
        }
        updatePreview();

        var btnRow = el('div', { class: 'dialog-actions', style: 'justify-content:flex-start;gap:8px;margin-top:16px' });

        var applyBtn = el('button', { class: 'btn btn-primary', text: 'Apply & Save' });
        applyBtn.onclick = function () {
            var pref = { accent: accentInput.value, bg1: bg1Input.value };
            saveUserThemePref(pref);
            applyUserThemePref(pref);
            showMsg(section, 'Appearance saved.', 'success');
        };
        btnRow.appendChild(applyBtn);

        var resetBtn = el('button', { class: 'btn', text: 'Reset to Default' });
        resetBtn.onclick = function () {
            localStorage.removeItem('magnolia_theme');
            location.reload();
        };
        btnRow.appendChild(resetBtn);

        section.appendChild(btnRow);
        page.appendChild(section);
        feedArea.appendChild(page);
    }

    return {
        initTheme: initTheme,
        applyTheme: applyTheme,
        renderUserTheme: renderUserTheme
    };
})();
