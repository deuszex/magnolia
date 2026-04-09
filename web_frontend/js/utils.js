// Shared HTML escaping utilities — loaded before all other scripts.

window.escapeHtml = function escapeHtml(str) {
    var div = document.createElement('div');
    div.textContent = str || '';
    return div.innerHTML;
};

// Escapes a string for safe use inside an HTML attribute value.
window.escapeAttr = function escapeAttr(str) {
    return (str || '')
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;')
        .replace(/'/g, '&#39;');
};
